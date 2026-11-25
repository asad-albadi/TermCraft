//! Borrowed system agents and a separately owned, in-process agent runtime.
use super::{Secret, keys::KeyManager};
use interprocess::local_socket::{
    GenericNamespaced, ListenerOptions, ToNsName,
    tokio::{Stream, prelude::*},
    traits::StreamCommon,
};
use russh::keys::{
    agent::{
        Constraint,
        client::{AgentClient, AgentStream},
    },
    ssh_key::{HashAlg, PublicKey},
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};
pub type Client = AgentClient<Box<dyn AgentStream + Send + Unpin>>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentEndpoint {
    Unix(PathBuf),
    NamedPipe(PathBuf),
    Pageant,
    OwnedName(String),
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentTarget {
    #[default]
    System,
    Owned,
    Pageant,
}
#[derive(Clone, Debug)]
pub struct Identity {
    pub fingerprint: String,
    pub algorithm: String,
    pub comment: String,
    pub public_key: String,
}
#[derive(Clone, Debug)]
pub struct Listing {
    pub identities: Vec<Identity>,
    pub omitted: usize,
}
struct Owned {
    endpoint: AgentEndpoint,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Owned {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
pub struct AgentManager {
    system: Option<AgentEndpoint>,
    owned: Option<Owned>,
}
#[cfg(windows)]
#[path = "agent_windows.rs"]
mod windows;
impl AgentEndpoint {
    pub fn system() -> Option<Self> {
        #[cfg(unix)]
        {
            std::env::var_os("SSH_AUTH_SOCK")
                .filter(|s| !s.is_empty())
                .map(|s| Self::Unix(s.into()))
        }
        #[cfg(windows)]
        {
            Some(Self::NamedPipe(
                std::env::var_os("SSH_AUTH_SOCK")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from(r"\\.\pipe\openssh-ssh-agent")),
            ))
        }
    }
    async fn connect_for_add(&self) -> Result<Client, String> {
        #[cfg(windows)]
        if let Self::NamedPipe(path) = self {
            return tokio::time::timeout(
                Duration::from_secs(5),
                windows::private_service_pipe(path),
            )
            .await
            .map_err(|_| "Agent service verification timed out".to_owned())?
            .map(|pipe| AgentClient::connect(pipe).dynamic());
        }
        self.connect().await
    }
    pub async fn connect(&self) -> Result<Client, String> {
        let operation = async {
            match self {
                Self::Unix(path) => {
                    #[cfg(unix)]
                    {
                        if !path.is_absolute() {
                            return Err("Agent socket path must be absolute.".into());
                        }
                        let stream = tokio::net::UnixStream::connect(path)
                            .await
                            .map_err(|_| "Could not connect to the system agent")?;
                        if stream
                            .peer_cred()
                            .map_err(|_| "Could not authenticate the agent socket")?
                            .uid()
                            != unsafe { libc::geteuid() }
                        {
                            return Err("Agent belongs to another user.".into());
                        }
                        Ok(AgentClient::connect(stream).dynamic())
                    }
                    #[cfg(not(unix))]
                    {
                        let _ = path;
                        Err("Unix agent sockets are unavailable on this platform.".into())
                    }
                }
                Self::NamedPipe(path) => {
                    #[cfg(windows)]
                    {
                        if !path
                            .to_string_lossy()
                            .to_lowercase()
                            .starts_with(r"\\.\pipe\")
                        {
                            return Err("Only local agent named pipes are supported.".into());
                        }
                        AgentClient::connect_named_pipe(path)
                            .await
                            .map(|c| c.dynamic())
                            .map_err(|_| "Could not connect to the Windows OpenSSH agent.".into())
                    }
                    #[cfg(not(windows))]
                    {
                        let _ = path;
                        Err("Windows agent pipes are unavailable on this platform.".into())
                    }
                }
                Self::Pageant => {
                    #[cfg(windows)]
                    {
                        AgentClient::connect_pageant()
                            .await
                            .map(|c| c.dynamic())
                            .map_err(|_| "Could not connect to Pageant.".into())
                    }
                    #[cfg(not(windows))]
                    {
                        Err("Pageant is available only on Windows.".into())
                    }
                }
                Self::OwnedName(name) => {
                    let name = name
                        .as_str()
                        .to_ns_name::<GenericNamespaced>()
                        .map_err(|_| "Invalid owned-agent endpoint")?;
                    let stream = Stream::connect(name)
                        .await
                        .map_err(|_| "Owned agent is not running")?;
                    #[cfg(unix)]
                    if stream.peer_creds().ok().and_then(|p| p.euid())
                        != Some(unsafe { libc::geteuid() })
                    {
                        return Err("Owned agent peer belongs to another user.".into());
                    }
                    Ok(AgentClient::connect(stream).dynamic())
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(5), operation)
            .await
            .map_err(|_| "Agent connection timed out".to_owned())?
    }
}
impl AgentManager {
    pub fn new(system: Option<AgentEndpoint>) -> Self {
        Self {
            system,
            owned: None,
        }
    }
    pub fn from_system() -> Self {
        Self::new(AgentEndpoint::system())
    }
    pub fn owned_endpoint(&self) -> Option<AgentEndpoint> {
        self.owned
            .as_ref()
            .filter(|o| o.thread.as_ref().is_some_and(|t| !t.is_finished()))
            .map(|o| o.endpoint.clone())
    }
    pub fn endpoint(&self, target: AgentTarget) -> Result<AgentEndpoint, String> {
        match target {
            AgentTarget::System => self
                .system
                .clone()
                .ok_or("No system agent is configured.".into()),
            AgentTarget::Owned => self
                .owned_endpoint()
                .ok_or("Start the TermCraft-owned agent first.".into()),
            AgentTarget::Pageant => {
                if cfg!(windows) {
                    Ok(AgentEndpoint::Pageant)
                } else {
                    Err("Pageant is available only on Windows.".into())
                }
            }
        }
    }
    pub fn start_owned(&mut self) -> Result<AgentEndpoint, String> {
        if self.owned.is_some() {
            return Err(
                "A TermCraft-owned agent already exists; stop it before restarting.".into(),
            );
        }
        let mut random = [0u8; 32];
        getrandom::fill(&mut random).map_err(|_| "Secure randomness is unavailable")?;
        let name = format!(
            "termcraft-agent-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let endpoint = AgentEndpoint::OwnedName(name.clone());
        let (tx, rx) = mpsc::sync_channel(1);
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let thread=std::thread::Builder::new().name("owned-ssh-agent".into()).spawn(move||{
   let runtime=match tokio::runtime::Builder::new_current_thread().enable_all().build(){Ok(r)=>r,Err(_)=>{let _=tx.send(Err("Could not start agent runtime".to_owned()));return}};
   runtime.block_on(async move{
    let name=match name.to_ns_name::<GenericNamespaced>(){Ok(n)=>n,Err(_)=>{let _=tx.send(Err("Invalid agent endpoint".into()));return}};
    let options=ListenerOptions::new().name(name);
    #[cfg(unix)]let options={use interprocess::os::unix::local_socket::ListenerOptionsExt;options.mode(0o600)};
    #[cfg(windows)]let options={use interprocess::os::windows::{local_socket::ListenerOptionsExt,security_descriptor::SecurityDescriptor};let value=widestring::U16CString::from_str("D:P(A;;GA;;;OW)").unwrap();match SecurityDescriptor::deserialize(&value){Ok(sd)=>options.security_descriptor(sd),Err(_)=>{let _=tx.send(Err("Could not protect agent endpoint".into()));return}}};
    let listener=match options.create_tokio(){Ok(l)=>l,Err(_)=>{let _=tx.send(Err("Could not listen on the owned agent endpoint".into()));return}};
    let active=Arc::new(AtomicUsize::new(0));
    let incoming=futures::stream::unfold((listener,active),|(listener,active)|async move{
     loop{let stream=match listener.accept().await{Ok(s)=>s,Err(e)=>return Some((Err(e),(listener,active)))};
      #[cfg(unix)]if stream.peer_creds().ok().and_then(|p|p.euid())!=Some(unsafe{libc::geteuid()}){continue}
      if active.fetch_update(Ordering::AcqRel,Ordering::Acquire,|n|if n<32{Some(n+1)}else{None}).is_err(){continue}
      return Some((Ok(Counted{stream,active:active.clone()}),(listener,active)));
     }
    });
    let _=tx.send(Ok(()));tokio::select!{_ =stopped=>{},_ =russh::keys::agent::server::serve(Box::pin(incoming),())=>{}}
   });
   // Runtime drop aborts every owned connection and drops the in-memory key store.
  }).map_err(|_|"Could not start owned agent thread")?;
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                self.owned = Some(Owned {
                    endpoint: endpoint.clone(),
                    stop: Some(stop),
                    thread: Some(thread),
                });
                Ok(endpoint)
            }
            Ok(Err(e)) => {
                let _ = stop.send(());
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = stop.send(());
                let _ = thread.join();
                Err("Owned agent did not start.".into())
            }
        }
    }
    pub fn stop_owned(&mut self) -> Result<(), String> {
        if let Some(owned) = self.owned.take() {
            drop(owned);
        }
        Ok(())
    }
    pub async fn list(&self, target: AgentTarget) -> Result<Listing, String> {
        let mut client = self.endpoint(target)?.connect().await?;
        let identities = tokio::time::timeout(Duration::from_secs(5), client.request_identities())
            .await
            .map_err(|_| "Agent listing timed out")?
            .map_err(|_| "Agent rejected identity listing")?;
        let omitted = identities.len().saturating_sub(256);
        let identities = identities
            .into_iter()
            .take(256)
            .map(|identity| {
                let public = identity.public_key();
                let comment = match &identity {
                    russh::keys::agent::AgentIdentity::PublicKey { comment, .. }
                    | russh::keys::agent::AgentIdentity::Certificate { comment, .. } => comment
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(1024)
                        .collect(),
                };
                Identity {
                    fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
                    algorithm: public.algorithm().to_string(),
                    comment,
                    public_key: public.to_openssh().unwrap_or_default(),
                }
            })
            .collect();
        Ok(Listing {
            identities,
            omitted,
        })
    }
    pub async fn add(
        &self,
        target: AgentTarget,
        path: &Path,
        passphrase: Option<Secret>,
        lifetime: Option<Duration>,
    ) -> Result<Identity, String> {
        let listing = self.list(target).await?;
        if listing.identities.len() + listing.omitted >= 256 {
            return Err("Agent identity limit reached; remove identities first.".into());
        }
        let key = KeyManager::load_private(path, passphrase.as_ref())?;
        drop(passphrase);
        let mut public = key.public_key().clone();
        public.set_comment(
            public
                .comment()
                .as_str_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .take(1024)
                .collect::<String>(),
        );
        let info = Identity {
            fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
            algorithm: public.algorithm().to_string(),
            comment: public
                .comment()
                .as_str_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .take(1024)
                .collect(),
            public_key: public.to_openssh().map_err(|_| "Invalid public key")?,
        };
        let constraints = if let Some(lifetime) = lifetime {
            let seconds =
                u32::try_from(lifetime.as_secs()).map_err(|_| "Identity lifetime is too large")?;
            if seconds == 0 {
                return Err("Identity lifetime must be at least one second.".into());
            }
            vec![Constraint::KeyLifetime { seconds }]
        } else {
            vec![]
        };
        let mut client = self.endpoint(target)?.connect_for_add().await?;
        tokio::time::timeout(
            Duration::from_secs(5),
            client.add_identity(&key, &constraints),
        )
        .await
        .map_err(|_| "Agent add timed out")?
        .map_err(|_| "Agent rejected this identity")?;
        Ok(info)
    }
    pub async fn remove(&self, target: AgentTarget, public: &str) -> Result<(), String> {
        let public = PublicKey::from_openssh(public).map_err(|_| "Invalid public identity")?;
        let mut client = self.endpoint(target)?.connect().await?;
        tokio::time::timeout(Duration::from_secs(5), client.remove_identity(&public))
            .await
            .map_err(|_| "Agent remove timed out")?
            .map_err(|_| "Agent could not remove this identity".into())
    }
}
struct Counted {
    stream: Stream,
    active: Arc<AtomicUsize>,
}
impl Drop for Counted {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}
impl tokio::io::AsyncRead for Counted {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_read(cx, buf)
    }
}
impl tokio::io::AsyncWrite for Counted {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_write(cx, buf)
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_flush(cx)
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.get_mut().stream).poll_shutdown(cx)
    }
}
