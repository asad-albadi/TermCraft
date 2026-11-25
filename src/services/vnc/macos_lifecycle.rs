//! OS-independent completion ownership seam; contains no framework calls.
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
pub(super) type Cancel = Arc<dyn Fn() -> bool + Send + Sync>;
pub(super) struct Permit(&'static AtomicBool);
impl Permit {
    pub(super) fn acquire(gate: &'static AtomicBool) -> io::Result<Arc<Self>> {
        gate.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                io::Error::other("A previous macOS capture is still pending; wait or relaunch")
            })?;
        Ok(Arc::new(Self(gate)))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
pub(super) fn wait<T>(rx: mpsc::Receiver<io::Result<T>>, cancel: &Cancel) -> io::Result<T> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if cancel() {
            return Err(io::Error::other("macOS display sharing cancelled"));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::other("ScreenCaptureKit response timed out"));
        }
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err(io::Error::other("ScreenCaptureKit callback ended")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_callback_retains_permit_without_rust_source_pointer() {
        let gate = Box::leak(Box::new(AtomicBool::new(false)));
        let permit = Permit::acquire(gate).unwrap();
        let (tx, rx) = mpsc::sync_channel::<io::Result<Vec<u8>>>(1);
        let callback = move || {
            let _held = &permit;
            assert!(tx.try_send(Ok(vec![1])).is_err());
        };
        assert!(wait(rx, &(Arc::new(|| true) as Cancel)).is_err());
        assert!(Permit::acquire(gate).is_err());
        callback();
        drop(callback);
        assert!(Permit::acquire(gate).is_ok());
    }
    #[test]
    fn bounded_completion_and_disconnect() {
        let (tx, rx) = mpsc::sync_channel(1);
        tx.try_send(Ok(3)).unwrap();
        assert!(tx.try_send(Ok(4)).is_err());
        assert_eq!(wait(rx, &(Arc::new(|| false) as Cancel)).unwrap(), 3);
        let (tx, rx) = mpsc::sync_channel::<io::Result<()>>(1);
        drop(tx);
        assert!(wait(rx, &(Arc::new(|| false) as Cancel)).is_err());
    }
}
