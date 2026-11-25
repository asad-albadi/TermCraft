const INVALID: u32 = 0xc000000d;
fn u32_at(p: &[u8], at: usize) -> Result<u32, u32> {
    Ok(u32::from_le_bytes(
        p.get(at..at + 4).ok_or(INVALID)?.try_into().unwrap(),
    ))
}

fn word(p: &mut Vec<u8>, n: u32) {
    p.extend(n.to_le_bytes());
}

fn header(id: u16) -> Vec<u8> {
    let mut p = vec![0x72, 0x44];
    p.extend(id.to_le_bytes());
    p
}

fn length_data(data: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    word(&mut out, data.len() as u32);
    out.extend(data);
    out
}

fn complete(request: &[u8], result: Result<Vec<u8>, u32>) -> Vec<u8> {
    match result {
        Ok(body) => {
            let mut p = completion_header(request, 0);
            p.extend(body);
            p
        }
        Err(error) => failure(request, error),
    }
}

fn completion_header(request: &[u8], status: u32) -> Vec<u8> {
    let mut p = header(0x4943);
    p.extend(&request[4..8]);
    p.extend(&request[12..16]);
    word(&mut p, status);
    p
}

fn failure(request: &[u8], status: u32) -> Vec<u8> {
    let mut p = completion_header(request, status);
    word(&mut p, 0);
    if matches!(u32_at(request, 16), Ok(0) | Ok(4)) {
        p.push(0);
    }
    p
}
