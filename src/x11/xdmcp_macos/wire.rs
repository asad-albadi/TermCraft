//! X.Org XDMCP v1, IPv4, no manager authentication, MIT-MAGIC-COOKIE-1 X11 authorization.
use std::net::Ipv4Addr;
use zeroize::Zeroizing;

pub(super) const MAX_PACKET: usize = 1024;
pub(super) const COOKIE_NAME: &[u8] = b"MIT-MAGIC-COOKIE-1";
pub(super) const WILLING: u16 = 5;
pub(super) const UNWILLING: u16 = 6;
pub(super) const ACCEPT: u16 = 8;
pub(super) const DECLINE: u16 = 9;
pub(super) const REFUSE: u16 = 11;
pub(super) const FAILED: u16 = 12;
pub(super) const ALIVE: u16 = 14;

#[derive(Clone)]
pub(super) struct Accepted {
    pub id: u32,
    pub cookie: Zeroizing<[u8; 16]>,
}

fn packet(op: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + body.len());
    out.extend(1u16.to_be_bytes());
    out.extend(op.to_be_bytes());
    out.extend((body.len() as u16).to_be_bytes());
    out.extend(body);
    out
}

fn push_field(out: &mut Vec<u8>, value: &[u8]) {
    out.extend((value.len() as u16).to_be_bytes());
    out.extend(value);
}

pub(super) fn query(indirect: bool) -> Vec<u8> {
    packet(if indirect { 3 } else { 2 }, &[0])
}

pub(super) fn request(display: u16, addr: Ipv4Addr) -> Vec<u8> {
    let mut out = display.to_be_bytes().to_vec();
    out.extend([1, 0, 0, 1]); // One FamilyInternet type and one corresponding address.
    push_field(&mut out, &addr.octets());
    push_field(&mut out, b""); // Authentication name and data are empty.
    push_field(&mut out, b"");
    out.push(1);
    push_field(&mut out, COOKIE_NAME);
    push_field(&mut out, b""); // No manufacturer identifier.
    packet(7, &out)
}

pub(super) fn manage(display: u16, session: u32) -> Vec<u8> {
    let mut out = session.to_be_bytes().to_vec();
    out.extend(display.to_be_bytes());
    push_field(&mut out, b"");
    packet(10, &out)
}

pub(super) fn keepalive(display: u16, session: u32) -> Vec<u8> {
    let mut out = display.to_be_bytes().to_vec();
    out.extend(session.to_be_bytes());
    packet(13, &out)
}

pub(super) fn decode(bytes: &[u8]) -> Option<(u16, &[u8])> {
    let ([0, 1, hi, lo, len_hi, len_lo], rest) = bytes.split_first_chunk::<6>()? else {
        return None;
    };
    (bytes.len() <= MAX_PACKET && usize::from(u16::from_be_bytes([*len_hi, *len_lo])) == rest.len())
        .then(|| (u16::from_be_bytes([*hi, *lo]), rest))
}

fn field<'a>(bytes: &mut &'a [u8]) -> Option<&'a [u8]> {
    let (len, tail) = bytes.split_first_chunk::<2>()?;
    let (value, rest) = tail.split_at_checked(u16::from_be_bytes(*len).into())?;
    *bytes = rest;
    Some(value)
}

fn id(bytes: &mut &[u8]) -> Option<u32> {
    let (value, tail) = bytes.split_first_chunk::<4>()?;
    *bytes = tail;
    Some(u32::from_be_bytes(*value))
}

pub(super) fn willing(mut body: &[u8]) -> Option<()> {
    let auth = field(&mut body)?;
    fields(body, 2)?;
    auth.is_empty().then_some(())
}

pub(super) fn fields(mut body: &[u8], count: usize) -> Option<()> {
    for _ in 0..count {
        field(&mut body)?;
    }
    body.is_empty().then_some(())
}

pub(super) fn accept(mut body: &[u8]) -> Option<Accepted> {
    let id = id(&mut body)?;
    let auth = field(&mut body)?;
    let data = field(&mut body)?;
    let protocol = field(&mut body)?;
    let cookie: [u8; 16] = field(&mut body)?.try_into().ok()?;
    (id != 0 && auth.is_empty() && data.is_empty() && protocol == COOKIE_NAME && body.is_empty())
        .then_some(Accepted {
            id,
            cookie: Zeroizing::new(cookie),
        })
}

pub(super) fn declined(mut body: &[u8]) -> Option<()> {
    // Status, authentication name, authentication data.
    field(&mut body)?;
    fields(body, 2)
}

pub(super) fn manage_failed(op: u16, mut body: &[u8], session: u32) -> bool {
    id(&mut body) == Some(session)
        && match op {
            REFUSE => body.is_empty(),
            FAILED => fields(body, 1).is_some(),
            _ => false,
        }
}

pub(super) fn alive(mut body: &[u8], session: u32) -> Option<bool> {
    let (running, rest) = body.split_first()?;
    body = rest;
    let reply = id(&mut body)?;
    if !body.is_empty() {
        return None;
    }
    match (*running, reply) {
        (1, id) if id == session => Some(true),
        (0, id) if id == 0 || id == session => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_requires_complete_cookie_and_no_unsolicited_authentication() {
        let mut body = 0x1234u32.to_be_bytes().to_vec();
        push_field(&mut body, b"");
        push_field(&mut body, b"");
        push_field(&mut body, COOKIE_NAME);
        push_field(&mut body, &[0x7d; 16]);
        assert_eq!(*accept(&body).unwrap().cookie, [0x7d; 16]);
        assert!(
            body.iter()
                .enumerate()
                .all(|(n, _)| accept(&body[..n]).is_none())
        );
        body.push(0);
        assert!(accept(&body).is_none());
        body.pop();
        body[10] = b'?';
        assert!(accept(&body).is_none());
    }

    #[test]
    fn datagram_framing_rejects_truncation_suffix_and_oversize() {
        let willing = packet(WILLING, &[0; 6]);
        assert_eq!(decode(&willing), Some((WILLING, &[0; 6][..])));
        let mut wrong = willing.clone();
        wrong[1] = 2;
        assert!(decode(&wrong).is_none());
        assert!(decode(&willing[..willing.len() - 1]).is_none());
        assert!(decode(&[willing, vec![0]].concat()).is_none());
        assert!(decode(&packet(WILLING, &[0; MAX_PACKET])).is_none());
    }

    #[test]
    fn terminal_packets_are_correlated_to_current_session() {
        assert!(manage_failed(REFUSE, &[0, 0, 0, 4], 4));
        assert!(!manage_failed(REFUSE, &[0, 0, 0, 4], 3));
        assert!(!manage_failed(FAILED, &[0, 0, 0, 4], 4));
        assert_eq!(alive(&[1, 0, 0, 0, 4], 4), Some(true));
        assert_eq!(alive(&[1, 0, 0, 0, 4], 3), None);
        assert_eq!(alive(&[0, 0, 0, 0, 0], 4), Some(false));
    }
}
