use crate::model::{Session, validate_host, validate_user};
/// Single explicit ProxyJump endpoint. Never run external proxy commands.
pub(super) fn jump_session(target: &Session) -> Result<Session, String> {
    if target.jump.contains(',') {
        return Err("Choose one SSH gateway; jump chains are not supported by this field.".into());
    }
    let (user, address) = match target.jump.rsplit_once('@') {
        Some((u, h)) => (u.to_owned(), h),
        None => (target.user.clone(), target.jump.as_str()),
    };
    validate_user(&user)?;
    let (host, port) = if let Some(rest) = address.strip_prefix('[') {
        let (host, after) = rest
            .split_once(']')
            .ok_or("Unclosed IPv6 gateway address")?;
        let port = if after.is_empty() {
            22
        } else {
            after
                .strip_prefix(':')
                .ok_or("Invalid gateway port")?
                .parse::<u16>()
                .map_err(|_| "Invalid gateway port")?
        };
        (host.to_owned(), port)
    } else if address.matches(':').count() == 1 {
        let (host, port) = address.rsplit_once(':').unwrap();
        (
            host.to_owned(),
            port.parse::<u16>().map_err(|_| "Invalid gateway port")?,
        )
    } else {
        (address.to_owned(), 22)
    };
    validate_host(&host)?;
    if port == 0 {
        return Err("Gateway port must be between 1 and 65535".into());
    }
    let mut gateway = target.clone();
    gateway.host = host;
    gateway.port = port;
    gateway.user = user;
    gateway.jump.clear();
    gateway.x11 = false;
    gateway.command.clear();
    Ok(gateway)
}
