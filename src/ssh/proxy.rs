//! Bounded HTTP CONNECT and SOCKS5 TCP proxies. Authentication never becomes process arguments.
use super::{ChallengeCredential, auth::Challenges};
use crate::{
    credentials::CredentialBinding,
    model::{ProxyKind, ProxySettings, Session},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{net::IpAddr, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use zeroize::Zeroizing;
pub(super) async fn connect(
    proxy: Option<&ProxySettings>,
    target: &Session,
    timeout: Duration,
    prompts: &Challenges,
    owner_profile: &str,
) -> Result<TcpStream, String> {
    let host = target.host.trim_start_matches('[').trim_end_matches(']');
    let Some(proxy) = proxy else {
        return tcp(host, target.port, timeout).await;
    };
    match proxy.kind {
        ProxyKind::HttpConnect => {
            http(proxy, host, target.port, timeout, prompts, owner_profile).await
        }
        ProxyKind::Socks5 => socks(proxy, host, target.port, timeout, prompts, owner_profile).await,
    }
}
async fn tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, String> {
    tokio::time::timeout(
        timeout,
        TcpStream::connect((host.trim_start_matches('[').trim_end_matches(']'), port)),
    )
    .await
    .map_err(|_| {
        format!(
            "TCP connection to {host}:{port} timed out after {} seconds",
            timeout.as_secs()
        )
    })?
    .map_err(|error| format!("TCP connection to {host}:{port} failed: {error}"))
}
async fn credentials(
    stream: &mut TcpStream,
    proxy: &ProxySettings,
    prompts: &Challenges,
    profile: &str,
) -> Result<Vec<Zeroizing<String>>, String> {
    let credential = ChallengeCredential {
        reference: proxy.credential_ref.clone(),
        binding: CredentialBinding::Proxy {
            kind: proxy.kind,
            host: proxy.host.clone(),
            port: proxy.port,
            profile: profile.into(),
        },
    };
    let mut peek = [0; 1];
    let title = format!(
        "{:?} proxy {}:{} · Sign in before SSH verification",
        proxy.kind, proxy.host, proxy.port
    );
    let values = tokio::select! {biased;values=prompts.proxy_values(title,credential)=>values?,_ =stream.peek(&mut peek)=>{prompts.lifecycle.cancel();return Err("Proxy connection closed or changed while waiting for sign-in.".into())}};
    if values.len() != 2 || values[0].is_empty() {
        return Err("Proxy sign-in requires a username and password response.".into());
    }
    Ok(values)
}
async fn http(
    proxy: &ProxySettings,
    host: &str,
    port: u16,
    timeout: Duration,
    prompts: &Challenges,
    profile: &str,
) -> Result<TcpStream, String> {
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let mut auth = None;
    for attempt in 0..2 {
        let mut stream = tcp(&proxy.host, proxy.port, timeout).await?;
        if proxy.authenticate || attempt == 1 {
            auth = Some(credentials(&mut stream, proxy, prompts, profile).await?);
        }
        let mut request = Zeroizing::new(format!(
            "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n"
        ));
        if let Some(values) = &auth {
            if values[0].contains(':') || values[0].len() + values[1].len() > 8192 {
                return Err("Proxy Basic credentials exceed the supported limits.".into());
            }
            let joined = Zeroizing::new(format!("{}:{}", values[0].as_str(), values[1].as_str()));
            let encoded = Zeroizing::new(STANDARD.encode(joined.as_bytes()));
            request.push_str("Proxy-Authorization: Basic ");
            request.push_str(&encoded);
            request.push_str("\r\n");
        }
        request.push_str("\r\n");
        tokio::time::timeout(timeout, stream.write_all(request.as_bytes()))
            .await
            .map_err(|_| "Proxy request timed out")?
            .map_err(|_| "Could not send proxy request")?;
        drop(request);
        let header = tokio::time::timeout(timeout, read_http_header(&mut stream))
            .await
            .map_err(|_| "Proxy response timed out")??;
        let text = String::from_utf8_lossy(&header);
        let mut line = text
            .lines()
            .next()
            .ok_or("Empty proxy response")?
            .split_whitespace();
        if !matches!(line.next(), Some("HTTP/1.1" | "HTTP/1.0")) {
            return Err("Invalid HTTP proxy response.".into());
        }
        let code = line
            .next()
            .ok_or("Missing proxy response status")?
            .parse::<u16>()
            .map_err(|_| "Invalid proxy response status")?;
        if (200..300).contains(&code) {
            return Ok(stream);
        }
        let basic = text.lines().skip(1).any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.eq_ignore_ascii_case("proxy-authenticate")
                    && value.trim().to_ascii_lowercase().starts_with("basic")
            })
        });
        if code == 407 && basic && auth.is_none() && attempt == 0 {
            continue;
        }
        return Err(
            "HTTP proxy denied the connection or requested unsupported authentication.".into(),
        );
    }
    Err("HTTP proxy authentication failed.".into())
}
async fn read_http_header(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 16384 {
            return Err("HTTP proxy response headers exceed 16 KiB.".into());
        }
        let byte = stream
            .read_u8()
            .await
            .map_err(|_| "HTTP proxy closed before completing its response")?;
        header.push(byte);
    }
    Ok(header)
}
async fn socks(
    proxy: &ProxySettings,
    host: &str,
    port: u16,
    timeout: Duration,
    prompts: &Challenges,
    profile: &str,
) -> Result<TcpStream, String> {
    let mut stream = tcp(&proxy.host, proxy.port, timeout).await?;
    let methods: &[u8] = if proxy.authenticate {
        &[5, 1, 2]
    } else {
        &[5, 2, 0, 2]
    };
    timed_write(&mut stream, methods, timeout).await?;
    let mut choice = [0; 2];
    timed_read(&mut stream, &mut choice, timeout).await?;
    if choice[0] != 5 {
        return Err("Invalid SOCKS proxy version.".into());
    }
    match choice[1] {
        0 if !proxy.authenticate => {}
        2 => {
            let values = credentials(&mut stream, proxy, prompts, profile).await?;
            if values.iter().any(|v| v.len() > 255) {
                return Err("SOCKS credentials must fit 255 bytes per field.".into());
            }
            let mut auth = Zeroizing::new(vec![1, values[0].len() as u8]);
            auth.extend_from_slice(values[0].as_bytes());
            auth.push(values[1].len() as u8);
            auth.extend_from_slice(values[1].as_bytes());
            timed_write(&mut stream, &auth, timeout).await?;
            let mut response = [0; 2];
            timed_read(&mut stream, &mut response, timeout).await?;
            if response != [1, 0] {
                return Err("SOCKS proxy authentication failed.".into());
            }
        }
        _ => return Err("SOCKS proxy requested unsupported authentication.".into()),
    }
    let mut request = vec![5, 1, 0];
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            request.push(1);
            request.extend_from_slice(&ip.octets());
        }
        Ok(IpAddr::V6(ip)) => {
            request.push(4);
            request.extend_from_slice(&ip.octets());
        }
        Err(_) => {
            if host.len() > 255 {
                return Err("SOCKS destination hostname exceeds 255 bytes.".into());
            }
            request.extend_from_slice(&[3, host.len() as u8]);
            request.extend_from_slice(host.as_bytes());
        }
    }
    request.extend_from_slice(&port.to_be_bytes());
    timed_write(&mut stream, &request, timeout).await?;
    let mut response = [0; 4];
    timed_read(&mut stream, &mut response, timeout).await?;
    if response[0] != 5 || response[1] != 0 || response[2] != 0 {
        return Err("SOCKS proxy denied the destination.".into());
    }
    let count = match response[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut len = [0];
            timed_read(&mut stream, &mut len, timeout).await?;
            len[0] as usize
        }
        _ => return Err("SOCKS proxy returned an invalid address type.".into()),
    };
    let mut address = vec![0; count + 2];
    timed_read(&mut stream, &mut address, timeout).await?;
    Ok(stream)
}
async fn timed_write(stream: &mut TcpStream, data: &[u8], timeout: Duration) -> Result<(), String> {
    tokio::time::timeout(timeout, stream.write_all(data))
        .await
        .map_err(|_| "Proxy write timed out")?
        .map_err(|_| "Proxy write failed".into())
}
async fn timed_read(
    stream: &mut TcpStream,
    data: &mut [u8],
    timeout: Duration,
) -> Result<(), String> {
    tokio::time::timeout(timeout, stream.read_exact(data))
        .await
        .map_err(|_| "Proxy read timed out")?
        .map(|_| ())
        .map_err(|_| "Proxy read failed".into())
}
