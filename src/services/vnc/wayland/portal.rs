//! Asynchronous XDG portal request/session lifecycle, owned by the selected source.
use super::Cancel;
use futures::{FutureExt, StreamExt};
use std::{
    collections::HashMap,
    future::Future,
    io,
    os::fd::OwnedFd,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use zbus::{
    Connection, MessageStream,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};
pub(super) type Dict = HashMap<String, OwnedValue>;
const DESKTOP: &str = "/org/freedesktop/portal/desktop";
const SC: &str = "org.freedesktop.portal.ScreenCast";
const RD: &str = "org.freedesktop.portal.RemoteDesktop";
static NEXT: AtomicU64 = AtomicU64::new(1);
pub(super) struct Selected {
    pub node: u32,
    pub serial: Option<u64>,
    pub logical: Option<(i32, i32)>,
}
pub(super) struct Portal {
    pub runtime: tokio::runtime::Runtime,
    conn: Connection,
    owner: String,
    session: OwnedObjectPath,
    pending: Option<String>,
    closed: MessageStream,
    cancel: Cancel,
    control: bool,
    devices: u32,
    ended: bool,
}
async fn wait<T>(
    cancel: &Cancel,
    future: impl Future<Output = zbus::Result<T>>,
    limit: Duration,
) -> io::Result<T> {
    let deadline = tokio::time::sleep(limit);
    tokio::pin!(deadline);
    tokio::pin!(future);
    loop {
        if cancel() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Portal sharing cancelled",
            ));
        }
        tokio::select! {biased; _=&mut deadline=>return Err(io::Error::new(io::ErrorKind::TimedOut,"Portal request timed out")),v=&mut future=>return v.map_err(|_|io::Error::other("Selected desktop portal request failed")), _=tokio::time::sleep(Duration::from_millis(20))=>{}}
    }
}
fn options(token: &str) -> HashMap<&str, Value<'_>> {
    HashMap::from([("handle_token", Value::from(token))])
}
impl Portal {
    pub(super) fn open(control: bool, cancel: Cancel) -> io::Result<(Self, Selected, OwnedFd)> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        let conn = runtime.block_on(wait(
            &cancel,
            async {
                zbus::connection::Builder::session()?
                    .method_timeout(Duration::from_secs(2))
                    .max_queued(8)
                    .build()
                    .await
            },
            Duration::from_secs(3),
        ))?;
        let owner: String = runtime.block_on(wait(
            &cancel,
            async {
                conn.call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "GetNameOwner",
                    &("org.freedesktop.portal.Desktop",),
                )
                .await?
                .body()
                .deserialize()
            },
            Duration::from_secs(3),
        ))?;
        let unique = conn
            .unique_name()
            .ok_or_else(|| io::Error::other("Portal bus has no unique name"))?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let token = format!("harbor_{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let session = OwnedObjectPath::try_from(format!(
            "/org/freedesktop/portal/desktop/session/{unique}/{token}"
        ))
        .map_err(|_| io::Error::other("Invalid portal session"))?;
        let closed = runtime.block_on(wait(
            &cancel,
            MessageStream::for_match_rule(
                zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .sender(owner.as_str())
                    .map_err(io::Error::other)?
                    .path(session.as_str())
                    .map_err(io::Error::other)?
                    .interface("org.freedesktop.portal.Session")
                    .map_err(io::Error::other)?
                    .member("Closed")
                    .map_err(io::Error::other)?
                    .build(),
                &conn,
                Some(2),
            ),
            Duration::from_secs(3),
        ))?;
        let mut portal = Self {
            runtime,
            conn,
            owner,
            session,
            pending: None,
            closed,
            cancel,
            control,
            devices: 0,
            ended: false,
        };
        let mut opts = options(&token);
        opts.insert("session_handle_token", Value::from(token.as_str()));
        let result = portal.request(
            if control { RD } else { SC },
            "CreateSession",
            &token,
            &(opts,),
        )?;
        let returned = String::try_from(
            result
                .get("session_handle")
                .ok_or_else(|| io::Error::other("Portal did not return a session"))?
                .try_clone()
                .map_err(io::Error::other)?,
        )
        .map_err(io::Error::other)?;
        if returned != portal.session.as_str() {
            return Err(io::Error::other("Portal returned an unexpected session"));
        }
        let source_types = portal.property(SC, "AvailableSourceTypes")? & 3;
        if source_types == 0 {
            return Err(io::Error::other(
                "Portal has no supported monitor or window sources",
            ));
        }
        if control && portal.property(RD, "AvailableDeviceTypes")? & 3 != 3 {
            return Err(io::Error::other(
                "Portal does not support both keyboard and pointer control",
            ));
        }
        if control {
            let token = portal.token();
            let mut opts = options(&token);
            opts.insert("types", Value::from(3u32));
            portal.request(RD, "SelectDevices", &token, &(portal.session.clone(), opts))?;
        }
        let token = portal.token();
        let mut opts = options(&token);
        opts.insert("types", Value::from(source_types));
        opts.insert("multiple", Value::from(false));
        // No restore token, no persistent grant, no automatic dialog acceptance.
        portal.request(SC, "SelectSources", &token, &(portal.session.clone(), opts))?;
        let token = portal.token();
        let opts = options(&token);
        let mut result = portal.request(
            if control { RD } else { SC },
            "Start",
            &token,
            &(portal.session.clone(), "", opts),
        )?;
        if control {
            portal.devices = u32::try_from(
                result
                    .remove("devices")
                    .ok_or_else(|| io::Error::other("Portal omitted granted devices"))?,
            )
            .map_err(io::Error::other)?;
            if portal.devices & 3 != 3 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Portal did not grant keyboard and pointer control",
                ));
            }
        }
        let streams: Vec<(u32, Dict)> = result
            .remove("streams")
            .ok_or_else(|| io::Error::other("Portal omitted selected streams"))?
            .try_into()
            .map_err(io::Error::other)?;
        if streams.len() != 1 {
            return Err(io::Error::other(
                "Choose exactly one portal display or window",
            ));
        }
        let (node, mut metadata) = streams.into_iter().next().unwrap();
        if metadata.len() > 32 || node == u32::MAX {
            return Err(io::Error::other("Invalid selected portal stream"));
        }
        let serial = metadata
            .remove("pipewire-serial")
            .map(u64::try_from)
            .transpose()
            .map_err(io::Error::other)?;
        let logical = metadata
            .remove("logical_size")
            .or_else(|| metadata.remove("size"))
            .map(<(i32, i32)>::try_from)
            .transpose()
            .map_err(io::Error::other)?;
        if control && logical.is_none() {
            return Err(io::Error::other(
                "Portal omitted logical dimensions required for pointer control",
            ));
        }
        if logical.is_some_and(|(w, h)| w <= 0 || h <= 0) {
            return Err(io::Error::other("Invalid portal logical size"));
        }
        let msg = portal.runtime.block_on(wait(
            &portal.cancel,
            portal.conn.call_method(
                Some(portal.owner.as_str()),
                DESKTOP,
                Some(SC),
                "OpenPipeWireRemote",
                &(portal.session.clone(), HashMap::<&str, Value<'_>>::new()),
            ),
            Duration::from_secs(3),
        ))?;
        let fd: zbus::zvariant::OwnedFd = msg.body().deserialize().map_err(io::Error::other)?;
        portal.ensure_active()?;
        Ok((
            portal,
            Selected {
                node,
                serial,
                logical,
            },
            fd.into(),
        ))
    }
    fn property(&self, interface: &str, property: &str) -> io::Result<u32> {
        let message = self.runtime.block_on(wait(
            &self.cancel,
            self.conn.call_method(
                Some(self.owner.as_str()),
                DESKTOP,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(interface, property),
            ),
            Duration::from_secs(2),
        ))?;
        let value: OwnedValue = message.body().deserialize().map_err(io::Error::other)?;
        u32::try_from(value).map_err(io::Error::other)
    }
    fn token(&self) -> String {
        format!("harbor_{}", NEXT.fetch_add(1, Ordering::Relaxed))
    }
    fn request<B: serde::Serialize + zbus::zvariant::DynamicType>(
        &mut self,
        interface: &str,
        method: &str,
        token: &str,
        body: &B,
    ) -> io::Result<Dict> {
        let unique = self
            .conn
            .unique_name()
            .unwrap()
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        let path = format!("/org/freedesktop/portal/desktop/request/{unique}/{token}");
        self.pending = Some(path.clone());
        let result = self.runtime.block_on(wait(
            &self.cancel,
            async {
                let rule = zbus::MatchRule::builder()
                    .msg_type(zbus::message::Type::Signal)
                    .sender(self.owner.as_str())?
                    .path(path.as_str())?
                    .interface("org.freedesktop.portal.Request")?
                    .member("Response")?
                    .build();
                let mut replies = MessageStream::for_match_rule(rule, &self.conn, Some(2)).await?;
                let reply = self
                    .conn
                    .call_method(
                        Some(self.owner.as_str()),
                        DESKTOP,
                        Some(interface),
                        method,
                        body,
                    )
                    .await?;
                let handle: OwnedObjectPath = reply.body().deserialize()?;
                if handle.as_str() != path {
                    return Err(zbus::Error::Failure(
                        "Unexpected portal request handle".into(),
                    ));
                }
                let message = replies
                    .next()
                    .await
                    .ok_or_else(|| zbus::Error::Failure("Portal response stream ended".into()))??;
                if message.body().len() > 65536 {
                    return Err(zbus::Error::Failure("Oversized portal response".into()));
                }
                message.body().deserialize::<(u32, Dict)>()
            },
            Duration::from_secs(120),
        ));
        // Requests are removed by the portal after a Response. On errors keep the
        // predicted handle until Drop, including cancellation before method reply.
        let (code, values) = result
            .map_err(|_| io::Error::other("Portal request failed, cancelled or timed out"))?;
        self.pending = None;
        if code != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Desktop sharing was denied or cancelled",
            ));
        }
        if values.len() > 32 {
            return Err(io::Error::other("Oversized portal result"));
        }
        self.ensure_active()?;
        Ok(values)
    }
    pub(super) fn ensure_active(&mut self) -> io::Result<()> {
        if self.ended
            || (self.cancel)()
            || self
                .runtime
                .block_on(async { self.closed.next().now_or_never().is_some() })
        {
            self.ended = true;
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Portal desktop sharing ended",
            ));
        }
        Ok(())
    }
    pub(super) fn notify<B: serde::Serialize + zbus::zvariant::DynamicType>(
        &mut self,
        method: &str,
        args: &B,
    ) -> io::Result<()> {
        self.ensure_active()?;
        if !self.control || self.devices & 3 != 3 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Portal control was not granted",
            ));
        }
        self.runtime.block_on(wait(
            &self.cancel,
            self.conn
                .call_method(Some(self.owner.as_str()), DESKTOP, Some(RD), method, args),
            Duration::from_secs(2),
        ))?;
        Ok(())
    }
    pub(super) fn session(&self) -> OwnedObjectPath {
        self.session.clone()
    }
}
impl Drop for Portal {
    fn drop(&mut self) {
        // Cleanup ignores the cancelled generation but has its own bounded deadline.
        let _ = self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(600), async {
                let pending = self.pending.take();
                let request = async {
                    if let Some(path) = pending {
                        let _ = self
                            .conn
                            .call_method(
                                Some(self.owner.as_str()),
                                path,
                                Some("org.freedesktop.portal.Request"),
                                "Close",
                                &(),
                            )
                            .await;
                    }
                };
                let session = async {
                    let _ = self
                        .conn
                        .call_method(
                            Some(self.owner.as_str()),
                            self.session.clone(),
                            Some("org.freedesktop.portal.Session"),
                            "Close",
                            &(),
                        )
                        .await;
                };
                // A stalled Request.Close must not prevent Session.Close from being sent.
                futures::join!(request, session);
            })
            .await
        });
    }
}
