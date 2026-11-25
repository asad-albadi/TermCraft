//! Bounded ownership of optional Windows serial counter probes.
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

pub(super) type Origin = [u8; 24];
const INVALID: u32 = 0xc000000d;
const LIMIT: usize = 16;
static LIVE: AtomicUsize = AtomicUsize::new(0);
const DEADLINE: Duration = Duration::from_millis(100);
pub(super) const POLL: Duration = Duration::from_millis(5);

pub(super) fn completed_count(
    origin: Option<&Origin>,
    code: u32,
    output: &mut [u8],
    count: u32,
) -> Result<u32, u32> {
    if code == super::control::GET_STATS {
        relative(origin.ok_or(super::UNSUPPORTED)?, output, count)
    } else {
        super::control::completed_count(code, count)
    }
}
fn relative(origin: &Origin, output: &mut [u8], count: u32) -> Result<u32, u32> {
    if count != 24 || output.len() != 24 {
        return Err(INVALID);
    }
    for (value, start) in output.chunks_exact_mut(4).zip(origin.chunks_exact(4)) {
        let current = u32::from_le_bytes(value[..].try_into().unwrap());
        let start = u32::from_le_bytes(start.try_into().unwrap());
        value.copy_from_slice(&current.wrapping_sub(start).to_le_bytes());
    }
    Ok(count)
}

// Cancellation relinquishes the result, never ownership of the native request.
pub(super) fn retire(
    mut should_cancel: impl FnMut() -> bool,
    cancel: impl FnOnce(),
    mut terminal: impl FnMut() -> Option<Result<u32, ()>>,
    mut pause: impl FnMut(),
) -> bool {
    let mut cancel = Some(cancel);
    loop {
        if should_cancel()
            && let Some(request) = cancel.take()
        {
            request();
        }
        if let Some(result) = terminal() {
            return cancel.is_some() && result == Ok(24);
        }
        pause();
    }
}

struct Permit(Arc<AtomicBool>);
impl Permit {
    fn reserve(binding: &Arc<AtomicBool>) -> Option<Self> {
        if binding.swap(true, Ordering::AcqRel) {
            return None;
        }
        if LIVE
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
                (live < LIMIT).then_some(live + 1)
            })
            .is_err()
        {
            binding.store(false, Ordering::Release);
            return None;
        }
        Some(Self(binding.clone()))
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::AcqRel);
        self.0.store(false, Ordering::Release);
    }
}

pub(super) struct Probe {
    cancel: Arc<AtomicBool>,
    result: Receiver<Option<Origin>>,
    deadline: Instant,
}
impl Probe {
    pub(super) fn start(
        binding: &Arc<AtomicBool>,
        read: impl FnOnce(&AtomicBool, Instant) -> Option<Origin> + Send + 'static,
    ) -> Option<Self> {
        let permit = Permit::reserve(binding)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let deadline = Instant::now() + DEADLINE;
        let (sender, result) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("rdp-serial-stats".into())
            .spawn(move || {
                let origin = if flag.load(Ordering::Acquire) || Instant::now() >= deadline {
                    None
                } else {
                    read(&flag, deadline)
                };
                drop(permit);
                let _ = sender.send(origin);
            })
            .ok()?;
        Some(Self {
            cancel,
            result,
            deadline,
        })
    }
    pub(super) fn poll(&self) -> Option<Option<Origin>> {
        if Instant::now() >= self.deadline {
            self.cancel.store(true, Ordering::Release);
            return Some(None);
        }
        match self.result.try_recv() {
            Ok(origin) => Some(origin),
            Err(TryRecvError::Disconnected) => Some(None),
            Err(TryRecvError::Empty) => None,
        }
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn words(values: [u32; 6]) -> Origin {
        let mut bytes = [0; 24];
        for (word, value) in bytes.chunks_exact_mut(4).zip(values) {
            word.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn reopening_preserves_each_pending_requests_origin_and_wraps_all_fields() {
        let a = words([u32::MAX, 20, 30, 40, 50, 60]);
        let b = words([2, 23, 33, 43, 53, 63]);
        let mut old = b;
        let mut new = b;
        relative(&b, &mut new, 24).unwrap();
        relative(&a, &mut old, 24).unwrap();
        assert_eq!(old, words([3; 6]));
        assert_eq!(new, words([0; 6]));
        let mut wrapped = words([0; 6]);
        relative(&words([u32::MAX; 6]), &mut wrapped, 24).unwrap();
        assert_eq!(wrapped, words([1; 6]));
        for count in [0, 23, 25] {
            assert_eq!(relative(&a, &mut new, count), Err(INVALID));
        }
    }

    #[test]
    fn only_exact_uncanceled_terminal_native_success_can_publish() {
        for (outcome, valid) in [
            (Ok(24), true),
            (Ok(23), false),
            (Ok(25), false),
            (Err(()), false),
        ] {
            assert_eq!(
                retire(|| false, || panic!(), || Some(outcome), || panic!()),
                valid
            );
            let polls = Cell::new(0);
            let cancels = Cell::new(0);
            let ok = retire(
                || polls.get() > 0,
                || cancels.set(cancels.get() + 1),
                || (polls.get() == 3).then_some(outcome),
                || polls.set(polls.get() + 1),
            );
            assert!(!ok);
            assert_eq!((polls.get(), cancels.get()), (3, 1));
        }
    }

    #[test]
    fn timeout_releases_open_but_holds_binding_and_global_permit_until_terminal() {
        let binding = Arc::new(AtomicBool::new(false));
        let (started, entered) = mpsc::channel();
        let (resume, finish) = mpsc::channel();
        let (signaled, observed) = mpsc::channel();
        let mut probe = Probe::start(&binding, move |cancel, _| {
            started.send(()).unwrap();
            finish.recv().unwrap();
            signaled.send(cancel.load(Ordering::Acquire)).unwrap();
            Some([1; 24])
        })
        .unwrap();
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(probe.result.try_recv().is_err());
        probe.deadline = Instant::now();
        assert_eq!(probe.poll(), Some(None));
        drop(probe);
        assert!(Permit::reserve(&binding).is_none());
        let permits: Vec<_> = (1..LIMIT)
            .map(|_| Permit::reserve(&Arc::default()).unwrap())
            .collect();
        assert!(Permit::reserve(&Arc::default()).is_none());
        resume.send(()).unwrap();
        assert!(observed.recv_timeout(Duration::from_secs(5)).unwrap());
        let end = Instant::now() + Duration::from_secs(5);
        while binding.load(Ordering::Acquire) {
            assert!(Instant::now() < end);
            thread::yield_now();
        }
        assert!(Permit::reserve(&binding).is_some());
        drop(permits);
    }
}
