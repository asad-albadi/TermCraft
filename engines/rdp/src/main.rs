#![cfg_attr(windows, windows_subsystem = "windows")]
mod clipboard;
mod devices;
mod diagnostics;
mod display;
mod smartcard;
use ironrdp_cliprdr as clipapi;
use ironrdp_core as clipcore;
#[path = "../../../src/desktop/options.rs"]
mod rdp_options;
// Headless RDP worker. Only the private, bounded stdio protocol is accepted.
mod config;
mod tls;
#[path = "../../../src/desktop/wire.rs"]
mod wire;
use ironrdp_pdu::input::{
    fast_path::{FastPathInputEvent, KeyboardFlags},
    mouse::{MousePdu, PointerFlags},
};
use ironrdp_session::{ActiveStage, ActiveStageBuilder, ActiveStageOutput, image::DecodedImage};
use ironrdp_tokio::{FramedWrite, NetworkClient, TokioFramed};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, watch};
use wire::{Event, Input};
struct Output(Mutex<std::io::Stdout>);
impl Output {
    fn event(&self, event: Event) -> Result<(), String> {
        wire::write_json(
            &mut *self.0.lock().unwrap_or_else(|p| p.into_inner()),
            &event,
        )
        .map_err(|_| "Desktop parent output pipe closed".into())
    }
    fn frame(&self, image: &DecodedImage, revision: u64) -> Result<(), String> {
        wire::dimensions(image.width(), image.height())?;
        let mut bytes = zeroize::Zeroizing::new(Vec::with_capacity(image.data().len() + 12));
        bytes.extend_from_slice(&image.width().to_be_bytes());
        bytes.extend_from_slice(&image.height().to_be_bytes());
        bytes.extend_from_slice(&revision.to_be_bytes());
        bytes.extend_from_slice(image.data());
        for alpha in bytes[12..].iter_mut().skip(3).step_by(4) {
            *alpha = 255;
        }
        wire::write_packet(
            &mut *self.0.lock().unwrap_or_else(|p| p.into_inner()),
            2,
            &bytes,
        )
        .map_err(|_| "Desktop parent frame pipe closed".into())
    }
}
#[derive(Default)]
struct NoExternalAuthNetwork {
    requested: bool,
}
impl NetworkClient for NoExternalAuthNetwork {
    async fn send(
        &mut self,
        _: &ironrdp_connector::sspi::generator::NetworkRequest,
    ) -> ironrdp_connector::ConnectorResult<Vec<u8>> {
        self.requested = true;
        Err(ironrdp_connector::general_err!(
            "Kerberos network transport is unavailable; use NTLM NLA or an explicit compatibility client"
        ))
    }
}
fn main() {
    #[cfg(any(windows, target_os = "macos"))]
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--printer-worker")
    {
        if devices::printer_child().is_err() {
            std::process::exit(1)
        }
        return;
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    if std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == "--pcsc-worker")
    {
        if smartcard::child().is_err() {
            std::process::exit(1);
        }
        return;
    }
    let out = Arc::new(Output(Mutex::new(std::io::stdout())));
    let message = match start(out.clone()) {
        Ok(()) => "Remote desktop ended".into(),
        Err(message) => message,
    };
    let _ = out.event(Event::Disconnected { message });
}
fn start(out: Arc<Output>) -> Result<(), String> {
    out.event(Event::Ready {
        version: wire::VERSION,
    })?;
    let (kind, bytes) = wire::read_packet(&mut std::io::stdin())
        .map_err(|_| "Desktop parent did not send configuration")?;
    if kind != 1 {
        return Err("Invalid initial desktop configuration packet".into());
    }
    let initial: Input =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid desktop configuration")?;
    let config = match &initial {
        Input::Configure(config) => config.clone(),
        _ => return Err("Desktop worker requires initial configuration".into()),
    };
    if config.version != wire::VERSION
        || config.host.is_empty()
        || config.host.len() > 255
        || config.port == 0
        || config.timeout_secs == 0
        || config.timeout_secs > 3600
    {
        return Err("Desktop configuration is out of range".into());
    }
    wire::dimensions(config.width, config.height)?;
    config
        .options
        .negotiate(&rdp_options::RdpCapabilities::embedded())?;
    let (tx, mut input) = mpsc::channel(32);
    let (cancel, mut cancelled) = watch::channel(false);
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        loop {
            let Ok((kind, bytes)) = wire::read_packet(&mut stdin) else {
                break;
            };
            if kind != 1 {
                break;
            }
            let Ok(message) = serde_json::from_slice::<Input>(&bytes) else {
                break;
            };
            if tx.blocking_send(message).is_err() {
                break;
            }
        }
        cancel.send_replace(true);
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "Cannot start RDP runtime")?;
    runtime.block_on(async {tokio::select! {result=connect(config,&mut input,&out)=>result,_=cancelled.changed()=>Err("Desktop parent closed the connection".into())}})
}
async fn answer(
    input: &mut mpsc::Receiver<Input>,
    id: u64,
    trust: bool,
    resize: &mut display::PendingResize,
) -> Result<Input, String> {
    let value = loop {
        let message = input
            .recv()
            .await
            .ok_or("Desktop parent closed the sign-in prompt")?;
        if let Input::Resize { width, height } = message {
            resize.request(width, height)?;
        } else {
            break message;
        }
    };
    match &value {
        Input::Trust { id: got, .. } if trust && *got == id => Ok(value),
        Input::Authenticate {
            id: got,
            user,
            password,
            domain,
        } if !trust
            && *got == id
            && !user.is_empty()
            && user.len() <= 4096
            && password.len() <= 16384
            && domain.len() <= 4096 =>
        {
            Ok(value)
        }
        Input::Close => Err("Desktop connection closed".into()),
        _ => Err("Desktop sign-in answer is stale or invalid".into()),
    }
}
async fn connect(
    config: wire::Config,
    input: &mut mpsc::Receiver<Input>,
    out: &Output,
) -> Result<(), String> {
    let timeout = Duration::from_secs(config.timeout_secs as u64);
    let mut pin = config.remembered_certificate.clone();
    let mut confirmation = None;
    let mut next_id = 1u64;
    let mut resize = display::PendingResize::default();
    let serial = devices::BoundSerialMappings::new(&config.options.serial_mappings)?;
    let printer = devices::BoundPrinterMapping::new(config.options.printer_mapping.as_ref())?;
    for attempt in 0..3 {
        printer.check_ready().await?;
        out.event(Event::Status {
            message: format!("Connecting to {}:{}", config.host, config.port),
        })?;
        let mut tcp = tokio::time::timeout(timeout, async {
            if let Some(address) = config.connect_to {
                if !address.ip().is_loopback() {
                    return Err(std::io::Error::other(
                        "RDP carrier must be a local owned relay",
                    ));
                }
                tokio::net::TcpStream::connect(address).await
            } else {
                tokio::net::TcpStream::connect((
                    config.host.trim_start_matches('[').trim_end_matches(']'),
                    config.port,
                ))
                .await
            }
        })
        .await
        .map_err(|_| "RDP TCP connection timed out")?
        .map_err(|_| "RDP TCP connection failed")?;
        tcp.set_nodelay(true)
            .map_err(|_| "Could not enable low-latency RDP transport")?;
        if config.connect_to.is_some() {
            let token = config
                .carrier_token
                .ok_or("RDP carrier has no private rendezvous token")?;
            use tokio::io::AsyncWriteExt;
            tokio::time::timeout(timeout, tcp.write_all(&token))
                .await
                .map_err(|_| "RDP carrier authentication timed out")?
                .map_err(|_| "RDP carrier closed before authentication")?;
        }
        let local = tcp
            .local_addr()
            .map_err(|_| "Cannot read RDP socket address")?;
        let mut connector =
            ironrdp_connector::ClientConnector::new(config::connector(&config), local);
        if let Some(cluster) = config::cluster(&config.options) {
            connector = connector.with_cluster_data(cluster);
        }
        connector.attach_static_channel(display::channel());
        devices::Channel::attach_with_smartcard(&mut connector, &config.options, &serial, &printer)
            .await?;
        let clip_actions = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let clip_overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let local_clipboard = Arc::new(Mutex::new(None));
        if config.options.clipboard != rdp_options::ClipboardPolicy::Disabled {
            let actions = clip_actions.clone();
            let overflow = clip_overflow.clone();
            connector.attach_static_channel(clipapi::CliprdrClient::new(Box::new(
                clipboard::Backend {
                    to_remote: config.options.clipboard.to_remote(),
                    from_remote: config.options.clipboard.from_remote(),
                    local: local_clipboard.clone(),
                    emit: Arc::new(move |action| {
                        let mut queue = actions.lock().unwrap();
                        if queue.len() >= 32 {
                            overflow.store(true, std::sync::atomic::Ordering::Release);
                        } else {
                            queue.push_back(action);
                        }
                    }),
                },
            )));
        }
        let mut framed = TokioFramed::new(tcp);
        let upgrade = tokio::time::timeout(
            timeout,
            ironrdp_tokio::connect_begin(&mut framed, &mut connector),
        )
        .await
        .map_err(|_| "RDP negotiation timed out")?
        .map_err(|_| "RDP security negotiation failed; this connection requires NLA")?;
        let (verifier, tls_config) = tls::Verifier::make(&config, pin.clone())?;
        let name = rustls::pki_types::ServerName::try_from(
            config
                .host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_owned(),
        )
        .map_err(|_| "Invalid RDP TLS hostname")?;
        let result = tokio::time::timeout(
            timeout,
            tokio_rustls::TlsConnector::from(tls_config)
                .connect(name, framed.into_inner_no_leftover()),
        )
        .await
        .map_err(|_| "RDP TLS handshake timed out")?;
        let mut stream = match result {
            Ok(stream) => stream,
            Err(_) => {
                let observed = verifier
                    .seen
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone()
                    .ok_or("RDP TLS handshake failed before receiving a certificate")?;
                if attempt >= 2 || pin.as_ref() == Some(&observed) {
                    return Err("RDP TLS handshake failed after certificate verification".into());
                }
                let id = next_id;
                next_id += 1;
                out.event(Event::Certificate{id,host:config.host.clone(),port:config.port,fingerprint:observed.clone(),previous:config.remembered_certificate.clone(),changed:config.remembered_certificate.as_ref().is_some_and(|old|old!=&observed),reason:if config.remembered_certificate.is_some(){"The server certificate differs from the remembered certificate. Verify its new fingerprint before proceeding.".into()}else{"The certificate could not be validated against the system/custom certificate authorities and hostname. Verify its fingerprint before proceeding.".into()}})?;
                let response = answer(input, id, true, &mut resize).await?;
                let decision = match &response {
                    Input::Trust { decision, .. } => *decision,
                    _ => unreachable!(),
                };
                if decision == wire::TrustDecision::Reject {
                    return Err("RDP certificate was rejected".into());
                }
                pin = Some(observed.clone());
                confirmation = Some((observed, decision == wire::TrustDecision::Remember));
                continue;
            }
        };
        let cert = stream
            .get_ref()
            .1
            .peer_certificates()
            .and_then(|certs| certs.first())
            .ok_or("RDP TLS peer did not provide a certificate")?;
        let fingerprint = tls::fingerprint(cert);
        if let Some((approved, remember)) = confirmation.take() {
            if approved != fingerprint {
                return Err("RDP certificate changed during sign-in".into());
            }
            out.event(Event::TrustConfirmed {
                fingerprint: fingerprint.clone(),
                remember,
            })?;
        }
        use x509_cert::der::Decode;
        let cert =
            x509_cert::Certificate::from_der(cert).map_err(|_| "Invalid RDP TLS certificate")?;
        let public = cert
            .tbs_certificate
            .subject_public_key_info
            .subject_public_key
            .as_bytes()
            .ok_or("Invalid RDP server public key")?
            .to_vec();
        let upgraded = ironrdp_tokio::mark_as_upgraded(upgrade, &mut connector);
        let id = next_id;
        out.event(Event::Authentication {
            id,
            host: config.host.clone(),
            port: config.port,
            fingerprint,
            user: config.user.clone(),
        })?;
        use tokio::io::AsyncReadExt;
        let mut probe = [0u8; 1];
        let mut response = tokio::select! {biased;
            peer=stream.read(&mut probe)=>return Err(match peer{Ok(0)|Err(_)=>"RDP server closed while waiting for sign-in",Ok(_)=>"RDP server sent unexpected data before sign-in"}.into()),
            response=answer(input,id,false,&mut resize)=>response?,
        };
        if let Input::Authenticate {
            user,
            password,
            domain,
            ..
        } = &mut response
        {
            connector.config.credentials = ironrdp_connector::Credentials::UsernamePassword {
                username: std::mem::take(user),
                password: std::mem::take(password),
            };
            connector.config.domain = if domain.is_empty() {
                None
            } else {
                Some(std::mem::take(domain))
            };
        }
        drop(response);
        let mut framed = TokioFramed::new(stream);
        let mut auth_network = NoExternalAuthNetwork::default();
        // Run the same negotiated CredSSP exchange explicitly, so a TLS EOF
        // during sign-in is distinguished from one after NLA succeeded.
        let deadline = tokio::time::Instant::now() + timeout;
        let ironrdp_connector::ClientConnectorState::Credssp {
            selected_protocol, ..
        } = connector.state
        else {
            return Err("RDP sign-in protocol failed because NLA was not negotiated.".into());
        };
        let (sequence, request) = ironrdp_connector::credssp::CredsspSequence::init(
            connector.config.credentials.clone(),
            connector.config.domain.as_deref(),
            selected_protocol,
            config.host.clone().into(),
            public,
            None,
        )
        .map_err(|error| diagnostics::Stage::SignIn.failure(&error))?;
        tokio::time::timeout_at(deadline, ironrdp_tokio::perform_credssp(
            &mut framed, &mut auth_network, &mut ironrdp_core::WriteBuf::new(), sequence, request,
        )).await.map_err(|_| diagnostics::Stage::SignIn.timeout())?.map_err(|error| {
            if auth_network.requested {
                "This endpoint requires Kerberos network authentication; choose installed-client compatibility until that provider is available".into()
            } else {
                diagnostics::Stage::SignIn.failure(&error)
            }
        })?;
        connector.mark_credssp_as_done();
        // The TLS public key was consumed by CredSSP above. Finalization skips
        // the completed CredSSP stage and retains IronRDP's redirection handling.
        let result = tokio::time::timeout_at(deadline, ironrdp_tokio::connect_finalize(
            upgraded, connector, &mut framed, &mut auth_network, config.host.clone().into(), Vec::new(), None,
        )).await.map_err(|_| diagnostics::Stage::DesktopSetup.timeout())?
          .map_err(|error| diagnostics::Stage::DesktopSetup.failure(&error))?
          .into_connected().map_err(|_| "RDP server requested a different destination; open it as a separate session so its certificate and credentials can be verified")?;
        if (config.options.printer_mapping.is_some() || config.options.smartcard_reader.is_some())
            && result
                .static_channels
                .get_channel_id_by_type::<devices::Channel>()
                .is_none()
        {
            let device = if config.options.smartcard_reader.is_some() {
                "smartcard"
            } else {
                "printer"
            };
            return Err(format!(
                "Server did not negotiate the requested {device} channel"
            ));
        }
        wire::dimensions(result.desktop_size.width, result.desktop_size.height)?;
        let mut image = DecodedImage::new(
            ironrdp_graphics::image_processing::PixelFormat::RgbA32,
            result.desktop_size.width,
            result.desktop_size.height,
        );
        let activation_factory =
            ironrdp_connector::connection_activation::ConnectionActivationFactory::new(
                config::connector(&config),
                result.io_channel_id,
                result.user_channel_id,
            );
        let mut input_flags = result.input_flags;
        drop(result.activation_factory);
        let mut active = ActiveStageBuilder {
            static_channels: result.static_channels,
            user_channel_id: result.user_channel_id,
            io_channel_id: result.io_channel_id,
            message_channel_id: result.message_channel_id,
            share_id: result.share_id,
            compression_type: result.compression_type,
            enable_server_pointer: result.enable_server_pointer,
            pointer_software_rendering: result.pointer_software_rendering,
        }
        .build();
        out.event(Event::Connected {
            width: image.width(),
            height: image.height(),
        })?;
        let (mut reader, mut writer) = ironrdp_tokio::split_tokio_framed(framed);
        let mut tick = tokio::time::interval(Duration::from_millis(16));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut dirty = true;
        let mut resize_in_flight = None;
        let mut fit_locally = false;
        let mut revision = 0u64;
        let mut held = HeldInput::default();
        let mut system_cursor = true;
        loop {
            if clip_overflow.load(std::sync::atomic::Ordering::Acquire) {
                return Err("RDP clipboard event queue exceeded its bound".into());
            }
            loop {
                let action = clip_actions.lock().unwrap().pop_front();
                let Some(action) = action else {
                    break;
                };
                let messages = match action {
                    clipboard::Action::Ready => {
                        out.event(Event::ClipboardReady)?;
                        continue;
                    }
                    clipboard::Action::Text(text) => {
                        out.event(Event::Clipboard { text })?;
                        continue;
                    }
                    action => {
                        let channel = active
                            .get_svc_processor_mut::<clipapi::CliprdrClient>()
                            .ok_or("Server did not negotiate the requested clipboard channel")?;
                        if !channel.is_ready() && !matches!(&action, clipboard::Action::Offer) {
                            out.event(Event::Status{message:"Clipboard channel is not ready; try paste again after negotiation".into()})?;
                            continue;
                        }
                        match action {
                            clipboard::Action::Offer => {
                                let formats = if config.options.clipboard.to_remote()
                                    && local_clipboard.lock().unwrap().is_some()
                                {
                                    clipboard::formats().to_vec()
                                } else {
                                    Vec::new()
                                };
                                channel.initiate_copy(&formats)
                            }
                            clipboard::Action::Request => channel
                                .initiate_paste(clipapi::pdu::ClipboardFormatId::CF_UNICODETEXT),
                            clipboard::Action::Data(data) => channel.submit_format_data(data),
                            _ => unreachable!(),
                        }
                        .map_err(|_| "RDP clipboard negotiation failed")?
                    }
                };
                let bytes = active
                    .process_svc_processor_messages(messages)
                    .map_err(|_| "RDP clipboard encoding failed")?;
                tokio::time::timeout(timeout, writer.write_all(&bytes))
                    .await
                    .map_err(|_| "RDP clipboard output timed out")?
                    .map_err(|_| "RDP clipboard transport closed")?;
            }
            if let Some(channel) = active.get_svc_processor_mut::<devices::Channel>() {
                let messages = channel.poll()?;
                if let Some(message) = channel.take_notice() {
                    out.event(Event::Status {
                        message: message.into(),
                    })?;
                }
                if !messages.is_empty() {
                    let bytes = zeroize::Zeroizing::new(
                        active
                            .process_svc_processor_messages(ironrdp_svc::SvcProcessorMessages::<
                                devices::Channel,
                            >::new(
                                messages
                            ))
                            .map_err(|_| "RDP device response encoding failed")?,
                    );
                    tokio::time::timeout(timeout, writer.write_all(&bytes))
                        .await
                        .map_err(|_| "RDP device output timed out")?
                        .map_err(|_| "RDP device transport closed")?;
                }
            }
            // The server can enter activation before its Deactivate All reaches
            // us. Sending fast-path input in that window can silently lose it.
            // Keep it in the bounded FIFO while the resize is in flight.
            if resize_in_flight.is_some_and(|deadline| tokio::time::Instant::now() >= deadline) {
                // Display Control has no rejection response. An endpoint can
                // decline a layout and continue serving the original desktop.
                resize_in_flight = None;
                fit_locally = true;
                out.event(Event::Status {
                    message: "The server kept its desktop size; fitting the view locally".into(),
                })?;
            }
            if let Some(result) =
                resize.poll(|width, height| active.encode_resize(width, height, None, None))
            {
                match result {
                    Ok(bytes) => {
                        tokio::time::timeout(timeout, writer.write_all(&bytes))
                            .await
                            .map_err(|_| "RDP resize output timed out")?
                            .map_err(|_| "RDP resize transport closed")?;
                        resize_in_flight =
                            Some(tokio::time::Instant::now() + Duration::from_secs(1));
                    }
                    Err(_) => out.event(Event::Status {
                        message: "The server could not apply the requested display size".into(),
                    })?,
                }
            }
            let outputs = tokio::select! {
                packet = reader.read_pdu() => {
                    let (action, mut data) = packet.map_err(|_| "RDP server closed its transport")?;
                    let result = active.process(&mut image, action, &data, reader.last_read_at());
                    if config.options.smartcard_reader.is_some() {
                        use zeroize::Zeroize;
                        data.as_mut().zeroize();
                    }
                    result.map_err(|_| "RDP server sent an unsupported or invalid display update")?
                },
                message=input.recv(), if resize_in_flight.is_none()=>{
                    let message=message.ok_or("Desktop parent closed input")?;if matches!(&message,Input::Close){return Ok(());}
                    if let Input::Resize {width, height} = message {
                        if !fit_locally && (width, height) != (image.width(), image.height()) {
                            resize.request(width, height)?;
                        } else {
                            resize = display::PendingResize::default();
                        }
                        continue;
                    }
                    if let Input::Clipboard{text}=&message {
                        if !config.options.clipboard.to_remote()||text.len()>clipboard::MAX_TEXT{return Err("Clipboard upload is disabled or exceeds its text limit".into());}
                        *local_clipboard.lock().unwrap()=Some(zeroize::Zeroizing::new(text.clone()));
                        clip_actions.lock().unwrap().push_back(clipboard::Action::Offer);
                    }
                    let events=inputs(&message,&mut held,&image)?;
                    encode_input(&mut active,&mut image,&events,input_flags)?
                },
                _=tick.tick()=>{if dirty{revision=revision.checked_add(1).ok_or("Desktop frame revisions exhausted")?;out.frame(&image,revision)?;dirty=false;}continue;}
            };
            for output in outputs {
                match output {
                    ActiveStageOutput::ResponseFrame(bytes) => {
                        let bytes = zeroize::Zeroizing::new(bytes);
                        tokio::time::timeout(timeout, writer.write_all(&bytes))
                            .await
                            .map_err(|_| "RDP output timed out")?
                            .map_err(|_| "RDP output transport closed")?
                    }
                    ActiveStageOutput::GraphicsUpdate(_) => dirty = true,
                    ActiveStageOutput::PointerPosition { x, y } => out.event(Event::Cursor {
                        x,
                        y,
                        visible: system_cursor,
                    })?,
                    ActiveStageOutput::PointerHidden => {
                        system_cursor = false;
                        out.event(Event::Cursor {
                            x: 0,
                            y: 0,
                            visible: false,
                        })?;
                    }
                    ActiveStageOutput::PointerDefault => {
                        system_cursor = true;
                        out.event(Event::Cursor {
                            x: 0,
                            y: 0,
                            visible: true,
                        })?;
                    }
                    ActiveStageOutput::Terminate(_) => return Ok(()),
                    ActiveStageOutput::DeactivateAll => {
                        tokio::time::timeout(timeout,async {
                        let mut sequence=activation_factory.create();let mut buffer=ironrdp_core::WriteBuf::new();
                        loop {
                            match sequence.connection_activation_state() {
                                ironrdp_connector::connection_activation::ConnectionActivationState::Finalized{desktop_size,share_id,input_flags:flags,static_channel_chunk_size,enable_server_pointer,pointer_software_rendering,window_support_level,..}=>{
                                    wire::dimensions(desktop_size.width,desktop_size.height)?;
                                    if !active.reactivate(activation_factory.io_channel_id(),activation_factory.user_channel_id(),share_id,enable_server_pointer,pointer_software_rendering,static_channel_chunk_size){return Err("Invalid RDP reactivation channel size".to_string());}
                                    active.set_window_support_level(window_support_level);input_flags=flags;
                                    if image.width()!=desktop_size.width||image.height()!=desktop_size.height{image=DecodedImage::new(ironrdp_graphics::image_processing::PixelFormat::RgbA32,desktop_size.width,desktop_size.height);}
                                    out.event(Event::Connected{width:image.width(),height:image.height()})?;resize_in_flight=None;fit_locally=false;dirty=true;break Ok(());
                                },
                                ironrdp_connector::connection_activation::ConnectionActivationState::Redirected{..}=>return Err("RDP server requested redirection during reactivation".into()),
                                _=>{}
                            }
                            let written=ironrdp_tokio::single_sequence_step_read(&mut reader,&mut sequence,&mut buffer).await.map_err(|_|"RDP reactivation negotiation failed")?;
                            if written.size().is_some(){writer.write_all(buffer.filled()).await.map_err(|_|"RDP reactivation output closed")?;}
                        }
                    }).await.map_err(|_|"RDP display reactivation timed out")??;
                    }
                    _ => {}
                }
            }
        }
    }
    Err("RDP certificate resolution retry limit reached".into())
}
#[derive(Default)]
struct HeldInput {
    keys: BTreeSet<(u8, bool)>,
    buttons: BTreeSet<u8>,
    position: (u16, u16),
}
fn inputs(
    input: &Input,
    held: &mut HeldInput,
    image: &DecodedImage,
) -> Result<Vec<FastPathInputEvent>, String> {
    let mut result = Vec::new();
    match input {
        Input::Pointer {
            x,
            y,
            button,
            pressed,
        } => {
            if (*x >= image.width() || *y >= image.height()) && button.is_some() && *pressed {
                return Ok(result);
            }
            let (x, y) = ((*x).min(image.width() - 1), (*y).min(image.height() - 1));
            held.position = (x, y);
            if let Some(button) = button {
                if *pressed {
                    held.buttons.insert(*button);
                } else {
                    held.buttons.remove(button);
                }
            }
            let mut flags = match button {
                None => PointerFlags::MOVE,
                Some(1) => PointerFlags::LEFT_BUTTON,
                Some(2) => PointerFlags::RIGHT_BUTTON,
                Some(3) => PointerFlags::MIDDLE_BUTTON_OR_WHEEL,
                _ => return Err("Invalid desktop pointer button".into()),
            };
            if button.is_some() && *pressed {
                flags |= PointerFlags::DOWN;
            }
            result.push(FastPathInputEvent::MouseEvent(MousePdu {
                flags,
                number_of_wheel_rotation_units: 0,
                x_position: x,
                y_position: y,
            }));
        }
        Input::Scroll {
            x,
            y,
            delta,
            horizontal,
        } => {
            if !(-256..=255).contains(delta) {
                return Err("Invalid remote scroll event".into());
            }
            if *delta != 0 {
                result.push(FastPathInputEvent::MouseEvent(MousePdu {
                    flags: if *horizontal {
                        PointerFlags::HORIZONTAL_WHEEL
                    } else {
                        PointerFlags::VERTICAL_WHEEL
                    },
                    number_of_wheel_rotation_units: *delta,
                    x_position: (*x).min(image.width() - 1),
                    y_position: (*y).min(image.height() - 1),
                }));
            }
        }
        Input::Key {
            code,
            extended,
            pressed,
        } => {
            if *pressed {
                if held.keys.len() >= 128 && !held.keys.contains(&(*code, *extended)) {
                    return Err("Too many remote keys held".into());
                }
                held.keys.insert((*code, *extended));
            } else {
                held.keys.remove(&(*code, *extended));
            }
            let mut flags = KeyboardFlags::empty();
            if *extended {
                flags |= KeyboardFlags::EXTENDED;
            }
            if !pressed {
                flags |= KeyboardFlags::RELEASE;
            }
            result.push(FastPathInputEvent::KeyboardEvent(flags, *code));
        }
        Input::Text { value } => {
            if value.len() > 4096 {
                return Err("Remote text input exceeds 4 KiB".into());
            }
            for value in value.encode_utf16() {
                result.push(FastPathInputEvent::UnicodeKeyboardEvent(
                    KeyboardFlags::empty(),
                    value,
                ));
                result.push(FastPathInputEvent::UnicodeKeyboardEvent(
                    KeyboardFlags::RELEASE,
                    value,
                ));
            }
        }
        Input::ReleaseAll => {
            for button in std::mem::take(&mut held.buttons) {
                let flags = match button {
                    1 => PointerFlags::LEFT_BUTTON,
                    2 => PointerFlags::RIGHT_BUTTON,
                    _ => PointerFlags::MIDDLE_BUTTON_OR_WHEEL,
                };
                result.push(FastPathInputEvent::MouseEvent(MousePdu {
                    flags,
                    number_of_wheel_rotation_units: 0,
                    x_position: held.position.0.min(image.width() - 1),
                    y_position: held.position.1.min(image.height() - 1),
                }));
            }
            for (code, extended) in std::mem::take(&mut held.keys) {
                let mut flags = KeyboardFlags::RELEASE;
                if extended {
                    flags |= KeyboardFlags::EXTENDED;
                }
                result.push(FastPathInputEvent::KeyboardEvent(flags, code));
            }
        }
        Input::Clipboard { .. } => {}
        _ => return Err("Unexpected desktop input for this connection stage".into()),
    }
    Ok(result)
}

fn encode_input(
    active: &mut ActiveStage,
    image: &mut DecodedImage,
    events: &[FastPathInputEvent],
    flags: ironrdp_pdu::rdp::capability_sets::InputFlags,
) -> Result<Vec<ActiveStageOutput>, String> {
    use ironrdp_pdu::{
        input::{self, InputEvent, InputEventPdu},
        rdp::{capability_sets::InputFlags, headers::ShareDataPdu},
    };
    if flags.intersects(InputFlags::FASTPATH_INPUT | InputFlags::FASTPATH_INPUT_2) {
        return active
            .process_fastpath_input(image, events)
            .map_err(|_| "Could not encode fast-path RDP input".into());
    }
    let mut output = Vec::new();
    for chunk in events.chunks(128) {
        let mut slow = Vec::new();
        for event in chunk {
            slow.push(match event {
                FastPathInputEvent::KeyboardEvent(flags, code) => {
                    let mut converted = input::scan_code::KeyboardFlags::empty();
                    if flags.contains(KeyboardFlags::EXTENDED) {
                        converted |= input::scan_code::KeyboardFlags::EXTENDED;
                    }
                    if flags.contains(KeyboardFlags::RELEASE) {
                        converted |= input::scan_code::KeyboardFlags::RELEASE;
                    }
                    InputEvent::ScanCode(input::ScanCodePdu {
                        flags: converted,
                        key_code: u16::from(*code),
                    })
                }
                FastPathInputEvent::UnicodeKeyboardEvent(flags, code) => {
                    let converted = if flags.contains(KeyboardFlags::RELEASE) {
                        input::unicode::KeyboardFlags::RELEASE
                    } else {
                        input::unicode::KeyboardFlags::empty()
                    };
                    InputEvent::Unicode(input::UnicodePdu {
                        flags: converted,
                        unicode_code: *code,
                    })
                }
                FastPathInputEvent::MouseEvent(mouse) => InputEvent::Mouse(*mouse),
                FastPathInputEvent::MouseEventEx(mouse) => InputEvent::MouseX(*mouse),
                FastPathInputEvent::MouseEventRel(mouse) => InputEvent::MouseRel(*mouse),
                FastPathInputEvent::SyncEvent(flags) => InputEvent::Sync(input::SyncPdu {
                    flags: input::sync::SyncToggleFlags::from_bits_retain(u32::from(flags.bits())),
                }),
                _ => return Err("Unsupported slow-path input event".into()),
            });
        }
        let mut buffer = ironrdp_core::WriteBuf::new();
        active
            .encode_static(&mut buffer, ShareDataPdu::Input(InputEventPdu(slow)))
            .map_err(|_| "Could not encode slow-path RDP input")?;
        output.push(ActiveStageOutput::ResponseFrame(buffer.filled().to_vec()));
    }
    // Use the stage's public cursor bookkeeping, discarding fast-path wire frames.
    for update in active
        .process_fastpath_input(image, events)
        .map_err(|_| "Could not update remote cursor")?
    {
        if !matches!(update, ActiveStageOutput::ResponseFrame(_)) {
            output.push(update);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod devices_tests;
