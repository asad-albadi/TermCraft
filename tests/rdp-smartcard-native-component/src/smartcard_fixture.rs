#[path = "../../../engines/rdp/src/smartcard/framed.rs"]
mod framed;
#[path = "../../../src/desktop/loader_env.rs"]
mod loader_env;
#[path = "../../../engines/rdp/src/smartcard/native_worker.rs"]
mod native_worker;
#[path = "../../../engines/rdp/src/smartcard/ndr.rs"]
mod ndr;
#[path = "../../../engines/rdp/src/smartcard/native_pcsc.rs"]
mod pcsc;
#[path = "../../../engines/rdp/src/smartcard/session.rs"]
mod session;
pub(super) fn fixture_parent() -> ! {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut worker = native_worker::Worker::start(Some("Reader 日本語 🗝"))
            .await
            .unwrap()
            .unwrap();
        let mut body = vec![1, 0x10, 8, 0, 0xcc, 0xcc, 0xcc, 0xcc];
        for word in [8u32, 0, 0, 0] {
            body.extend(word.to_le_bytes());
        }
        let mut packet = b"rDRI".to_vec();
        for word in [1u32, 0, 1, 14, 0, 1024, body.len() as u32, 0x90014] {
            packet.extend(word.to_le_bytes());
        }
        packet.extend([0; 20]);
        packet.extend(body);
        worker.enqueue(&packet).unwrap();
        let marker = std::env::var_os("D02_NATIVE_ESTABLISH_STALL").unwrap();
        let start = std::time::Instant::now();
        while !std::path::Path::new(&marker).exists() {
            assert!(start.elapsed() < std::time::Duration::from_secs(2));
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        println!("{}", worker.pid());
        use std::io::Write;
        std::io::stdout().flush().unwrap();
        // Model abrupt parent death: deliberately bypass Worker::drop.
        std::process::exit(0);
    });
    unreachable!()
}
pub(super) fn fixture_entry() -> Result<(), ()> {
    native_worker::fixture_child()
}
#[cfg(test)]
#[path = "transport_tests.rs"]
mod transport_tests;
