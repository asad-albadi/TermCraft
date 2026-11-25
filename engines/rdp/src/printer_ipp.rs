//! Bounded subset of the CUPS IPP responses used by printer redirection.
use std::{collections::BTreeMap, io};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

pub(super) const RAW_FORMAT: &str = "application/vnd.cups-raw";
pub(super) const GET_PRINTER_ATTRIBUTES: u16 = 0x000b;
pub(super) const PRINT_JOB: u16 = 0x0002;
const MAX_HTTP_HEADER: u64 = 8 * 1024;
const MAX_IPP_REPLY: usize = 16 * 1024;

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid CUPS HTTP/IPP reply")
}

pub(super) fn request(operation: u16, resource: &str, user: &str) -> Vec<u8> {
    // Use the operation as the request ID. Each request has its own HTTP connection.
    let mut bytes = vec![2, 0];
    bytes.extend(operation.to_be_bytes());
    bytes.extend(u32::from(operation).to_be_bytes());
    bytes.push(1);
    for (tag, name, value) in [
        (0x47, "attributes-charset", "utf-8"),
        (0x48, "attributes-natural-language", "en"),
        (0x45, "printer-uri", &format!("ipp://localhost{resource}")),
        (0x42, "requesting-user-name", user),
    ] {
        attribute(&mut bytes, tag, name, value);
    }
    match operation {
        PRINT_JOB => {
            attribute(&mut bytes, 0x42, "job-name", "TermCraft RDP print job");
            attribute(&mut bytes, 0x49, "document-format", RAW_FORMAT);
        }
        GET_PRINTER_ATTRIBUTES => {
            attribute(&mut bytes, 0x49, "document-format", RAW_FORMAT);
            attribute(
                &mut bytes,
                0x44,
                "requested-attributes",
                "printer-uri-supported",
            );
            attribute(&mut bytes, 0x44, "", "printer-is-accepting-jobs");
            attribute(&mut bytes, 0x44, "", "document-format-supported");
            attribute(&mut bytes, 0x44, "", "printer-uuid");
        }
        _ => unreachable!(),
    }
    bytes.push(3);
    bytes
}

fn attribute(bytes: &mut Vec<u8>, tag: u8, name: &str, value: &str) {
    bytes.push(tag);
    for text in [name, value] {
        bytes.extend((text.len() as u16).to_be_bytes());
        bytes.extend(text.as_bytes());
    }
}

pub(super) async fn read_response(
    stream: impl tokio::io::AsyncRead + Unpin,
) -> io::Result<Vec<u8>> {
    let mut reader = BufReader::new(stream);
    let mut header = Vec::new();
    let mut bounded = (&mut reader).take(MAX_HTTP_HEADER);
    loop {
        if bounded.read_until(b'\n', &mut header).await? == 0 {
            return Err(invalid());
        }
        if header.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let header = std::str::from_utf8(&header).map_err(|_| invalid())?;
    let mut lines = header.split("\r\n").filter(|line| !line.is_empty());
    let mut status = lines.next().ok_or_else(invalid)?.split_ascii_whitespace();
    if !matches!(status.next(), Some("HTTP/1.0" | "HTTP/1.1")) || status.next() != Some("200") {
        return Err(invalid());
    }
    let mut length = None;
    let mut content_type = None;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or_else(invalid)?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() {
                return Err(invalid());
            }
            length = Some(value.parse::<usize>().map_err(|_| invalid())?);
        } else if name.eq_ignore_ascii_case("Content-Type") {
            if content_type.replace(value).is_some() {
                return Err(invalid());
            }
        } else if name.eq_ignore_ascii_case("Transfer-Encoding") {
            return Err(invalid()); // HTTP/1.0 request, no transfer coding.
        }
    }
    if !content_type.is_some_and(|v| v.eq_ignore_ascii_case("application/ipp")) {
        return Err(invalid());
    }
    let length = length
        .filter(|len| (9..=MAX_IPP_REPLY).contains(len))
        .ok_or_else(invalid)?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await?;
    Ok(body)
}

type Attribute<'a> = (u8, Vec<&'a [u8]>);
type Attributes<'a> = BTreeMap<&'a str, Attribute<'a>>;

pub(super) struct Response<'a> {
    pub status: u16,
    attributes: Attributes<'a>,
}

impl<'a> Response<'a> {
    #[cfg(any(target_os = "macos", test))]
    pub(super) fn queue_uuid(&self) -> io::Result<&[u8]> {
        let uuid = one(&self.attributes, "printer-uuid", 0x45)?;
        if uuid.len() > 128 || !uuid.starts_with(b"urn:uuid:") || uuid.len() < 20 {
            return Err(invalid());
        }
        Ok(uuid)
    }
    pub(super) fn parse(bytes: &'a [u8], operation: u16) -> io::Result<Self> {
        let mut rest = bytes;
        let header = take(&mut rest, 8).ok_or_else(invalid)?;
        if header[..2] != [2, 0] || header[4..8] != u32::from(operation).to_be_bytes() {
            return Err(invalid());
        }
        let status = u16::from_be_bytes([header[2], header[3]]);
        if take(&mut rest, 1) != Some(&[1]) {
            return Err(invalid());
        }
        let operation_attributes = read_group(&mut rest)?;
        if one(&operation_attributes, "attributes-charset", 0x47)? != b"utf-8"
            || !keyword(one(
                &operation_attributes,
                "attributes-natural-language",
                0x48,
            )?)
        {
            return Err(invalid());
        }
        if rest.first() == Some(&5) {
            rest = &rest[1..];
            read_group(&mut rest)?;
        }
        let expected_group = if operation == PRINT_JOB { 2 } else { 4 };
        let attributes = if rest.first() == Some(&expected_group) {
            rest = &rest[1..];
            read_group(&mut rest)?
        } else {
            BTreeMap::new()
        };
        if rest != [3] {
            return Err(invalid());
        }
        Ok(Self { status, attributes })
    }

    pub(super) fn confirm_job(&self, resource: &str) -> io::Result<()> {
        if self.status > 0x00ff {
            return Err(io::Error::other("CUPS rejected the print job"));
        }
        let id = integer(one(&self.attributes, "job-id", 0x21)?)?;
        let job_uri = one(&self.attributes, "job-uri", 0x45)?;
        let state = integer(one(&self.attributes, "job-state", 0x23)?)?;
        let reasons = values(&self.attributes, "job-state-reasons", 0x44)?;
        if id <= 0
            || uri_path(job_uri) != Some(format!("/jobs/{id}").as_str())
            || !matches!(state, 3..=9)
            || reasons.iter().any(|reason| !keyword(reason))
        {
            return Err(invalid());
        }
        if self.attributes.contains_key("job-printer-uri")
            && !uri_path(one(&self.attributes, "job-printer-uri", 0x45)?)
                .is_some_and(|path| path.eq_ignore_ascii_case(resource))
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub(super) fn check_queue(&self, resource: &str) -> Result<(), &'static str> {
        if self.status == 0x0406 {
            return Err("does not exist");
        }
        if self.status > 0x00ff {
            return Err("was rejected by CUPS");
        }
        if !values(&self.attributes, "printer-uri-supported", 0x45).is_ok_and(|uris| {
            uris.iter()
                .any(|uri| uri_path(uri).is_some_and(|path| path.eq_ignore_ascii_case(resource)))
        }) {
            return Err("did not identify the selected queue");
        }
        if one(&self.attributes, "printer-is-accepting-jobs", 0x22).ok() != Some(&[1]) {
            return Err("is not accepting print jobs");
        }
        if !values(&self.attributes, "document-format-supported", 0x49).is_ok_and(|formats| {
            formats
                .iter()
                .any(|format| format.eq_ignore_ascii_case(RAW_FORMAT.as_bytes()))
        }) {
            return Err("does not support raw print data");
        }
        Ok(())
    }
}

fn read_group<'a>(rest: &mut &'a [u8]) -> io::Result<Attributes<'a>> {
    let mut attributes = BTreeMap::new();
    let mut last = None;
    while let Some(&tag @ 0x10..=0xff) = rest.first() {
        *rest = &rest[1..];
        let name = field(rest).ok_or_else(invalid)?;
        let value = field(rest).ok_or_else(invalid)?;
        if !valid_value(tag, value) {
            return Err(invalid());
        }
        if name.is_empty() {
            let (previous_tag, values): &mut Attribute<'_> = attributes
                .get_mut(last.ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            if *previous_tag != tag {
                return Err(invalid());
            }
            values.push(value);
        } else {
            if !keyword(name) {
                return Err(invalid());
            }
            let name = std::str::from_utf8(name).map_err(|_| invalid())?;
            if attributes.insert(name, (tag, vec![value])).is_some() {
                return Err(invalid());
            }
            last = Some(name);
        }
    }
    Ok(attributes)
}

fn valid_value(tag: u8, value: &[u8]) -> bool {
    match tag {
        0x10..=0x16 => value.is_empty(), // Out-of-band values.
        0x21 | 0x23 => value.len() == 4,
        0x22 => matches!(value, [0] | [1]),
        0x30 => true, // octetString
        0x31 => value.len() == 11,
        0x32 => value.len() == 9,
        0x33 => value.len() == 8,
        0x41 | 0x42 => std::str::from_utf8(value).is_ok(),
        0x44 | 0x46..=0x48 => keyword(value),
        0x45 | 0x49 => !value.is_empty() && value.iter().all(u8::is_ascii_graphic),
        // No collection or extension syntax is requested for either operation.
        _ => false,
    }
}

fn keyword(value: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(b))
}

fn uri_path(value: &[u8]) -> Option<&str> {
    let uri = std::str::from_utf8(value).ok()?;
    let address = uri
        .strip_prefix("ipp://")
        .or_else(|| uri.strip_prefix("ipps://"))?;
    let (authority, path) = address.split_at(address.find('/')?);
    let port = if let Some(address) = authority.strip_prefix('[') {
        let (host, suffix) = address.split_once(']')?;
        host.parse::<std::net::Ipv6Addr>().ok()?;
        if suffix.is_empty() {
            None
        } else {
            Some(suffix.strip_prefix(':')?)
        }
    } else {
        let (host, port) = authority
            .split_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)));
        if !keyword(host.as_bytes()) {
            return None;
        }
        port
    };
    if port.is_some_and(|port| port.parse::<std::num::NonZeroU16>().is_err())
        || path.contains(['?', '#', '\\'])
    {
        return None;
    }
    Some(path)
}

fn values<'a, 'b>(
    attributes: &'b Attributes<'a>,
    name: &str,
    tag: u8,
) -> io::Result<&'b [&'a [u8]]> {
    let (actual, values) = attributes.get(name).ok_or_else(invalid)?;
    if *actual != tag {
        return Err(invalid());
    }
    Ok(values)
}

fn one<'a>(attributes: &Attributes<'a>, name: &str, tag: u8) -> io::Result<&'a [u8]> {
    let [value] = values(attributes, name, tag)? else {
        return Err(invalid());
    };
    Ok(value)
}

fn integer(value: &[u8]) -> io::Result<i32> {
    Ok(i32::from_be_bytes(value.try_into().map_err(|_| invalid())?))
}

fn field<'a>(rest: &mut &'a [u8]) -> Option<&'a [u8]> {
    let length = u16::from_be_bytes(take(rest, 2)?.try_into().ok()?);
    take(rest, usize::from(length))
}

fn take<'a>(rest: &mut &'a [u8], length: usize) -> Option<&'a [u8]> {
    let (head, tail) = rest.split_at_checked(length)?;
    *rest = tail;
    Some(head)
}

#[cfg(all(test, unix))]
mod retained_tests {
    use super::*;
    use tokio::{io::AsyncWriteExt, net::UnixStream};
    fn ready(uuid: &str) -> Vec<u8> {
        let mut p = vec![2, 0, 0, 0, 0, 0, 0, 11, 1];
        attribute(&mut p, 0x47, "attributes-charset", "utf-8");
        attribute(&mut p, 0x48, "attributes-natural-language", "en");
        p.push(4);
        attribute(
            &mut p,
            0x45,
            "printer-uri-supported",
            "ipp://localhost/printers/Office",
        );
        p.extend([0x22, 0, 25]);
        p.extend(b"printer-is-accepting-jobs");
        p.extend([0, 1, 1]);
        attribute(&mut p, 0x49, "document-format-supported", RAW_FORMAT);
        attribute(&mut p, 0x45, "printer-uuid", uuid);
        p.push(3);
        p
    }
    #[test]
    fn persistent_stream_keeps_queue_uuid_and_reply_boundaries() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async{
            let (mut client,mut server)=UnixStream::pair().unwrap();
            for uuid in ["urn:uuid:11111111-1111-1111-1111-111111111111","urn:uuid:22222222-2222-2222-2222-222222222222"]{
                let body=ready(uuid);
                let header=format!("HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nContent-Length: {}\r\n\r\n",body.len());
                server.write_all(header.as_bytes()).await.unwrap();server.write_all(&body).await.unwrap();
                let body=read_response(&mut client).await.unwrap();
                let reply=Response::parse(&body,GET_PRINTER_ATTRIBUTES).unwrap();
                reply.check_queue("/printers/Office").unwrap();
                assert_eq!(reply.queue_uuid().unwrap(),uuid.as_bytes());
            }
        });
    }
    #[test]
    fn missing_or_malformed_queue_uuid_fails_closed() {
        for uuid in ["", "http://other/printers/Office", "urn:uuid:short"] {
            let bytes = ready(uuid);
            let result = Response::parse(&bytes, GET_PRINTER_ATTRIBUTES);
            assert!(result.is_err() || result.unwrap().queue_uuid().is_err());
        }
    }
}
