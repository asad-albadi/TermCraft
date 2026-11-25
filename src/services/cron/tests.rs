use super::*;
#[test]
fn numeric_calendar_and_rejected_extensions() {
    let cron = Schedule::parse("*/15 9-17 * * 1-5").unwrap();
    assert!(cron.matches(1_704_099_600)); // 2024-01-01 09:00 UTC Monday
    let either = Schedule::parse("0 9 2 * MON").unwrap();
    assert!(either.matches(1_704_099_600));
    assert!(either.matches(1_704_186_000));
    assert!(!either.matches(1_704_272_400));
    for bad in [
        "@hourly",
        "0 0 0 * * *",
        "0 0 L * *",
        "60 * * * *",
        "*/0 * * * *",
    ] {
        assert!(Schedule::parse(bad).is_err(), "{bad}");
    }
}
use std::sync::atomic::{AtomicU64, Ordering};
struct Clock(AtomicU64);
impl CronClock for Clock {
    fn unix_seconds(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
}
#[cfg(unix)]
fn job(root: &std::path::Path, script: &str) -> CronJob {
    CronJob {
        id: "fixture".into(),
        schedule: "* * * * *".into(),
        enabled: true,
        program: "/bin/sh".into(),
        arguments: vec!["-c".into(), script.into()],
        directory: root.into(),
        max_seconds: 5,
    }
}
fn wait(mut condition: impl FnMut() -> bool) {
    let end = std::time::Instant::now() + Duration::from_secs(6);
    while !condition() {
        assert!(std::time::Instant::now() < end, "timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(unix)]
#[test]
fn real_due_process_once_disable_restart_and_corrupt_preserve() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.json");
    let j = job(dir.path(), "printf x >> ran");
    save_jobs(&path, vec![j.clone()]).unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut bad = j.clone();
    bad.schedule = "@hourly".into();
    assert!(save_jobs(&path, vec![bad]).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let clock = Arc::new(Clock(AtomicU64::new(1_704_099_600)));
    let supervisor = crate::services::ServiceSupervisor::default();
    let mut config = CronServiceConfig::new(path.clone());
    config.clock = clock.clone();
    let id = supervisor.start_cron(config.clone()).unwrap();
    wait(|| dir.path().join("ran").exists());
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(std::fs::read(dir.path().join("ran")).unwrap(), b"x");
    supervisor.stop(id).unwrap();
    wait(|| supervisor.status(id).unwrap().state == crate::services::ServiceState::Stopped);
    let id = supervisor.start_cron(config.clone()).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(std::fs::read(dir.path().join("ran")).unwrap(), b"x");
    // Jump several minutes: only the current minute runs, no catch-up replay.
    clock.0.fetch_add(600, Ordering::Release);
    wait(|| std::fs::read(dir.path().join("ran")).unwrap().len() == 2);
    clock.0.fetch_sub(120, Ordering::Release);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(std::fs::read(dir.path().join("ran")).unwrap().len(), 2);
    assert_eq!(
        supervisor.status(id).unwrap().cron.as_ref().unwrap().jobs[0].next_run,
        Some(1_704_100_260)
    );
    supervisor.stop(id).unwrap();
    wait(|| supervisor.status(id).unwrap().state == crate::services::ServiceState::Stopped);
    let mut disabled = j;
    disabled.enabled = false;
    save_jobs(&path, vec![disabled]).unwrap();
    clock.0.fetch_add(600, Ordering::Release);
    let id = supervisor.start_cron(config.clone()).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(std::fs::read(dir.path().join("ran")).unwrap().len(), 2);
    supervisor.stop(id).unwrap();
    wait(|| supervisor.status(id).unwrap().state == crate::services::ServiceState::Stopped);
    let last = std::fs::read(&path).unwrap();
    std::fs::write(&path, b"{broken").unwrap();
    assert!(load_jobs(&path).is_err());
    assert!(supervisor.start_cron(config).is_err());
    std::fs::write(&path, last).unwrap();
    assert!(!load_jobs(&path).unwrap()[0].enabled);
}
#[cfg(unix)]
#[test]
fn stop_and_deadline_kill_owned_descendants_leave_unrelated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.json");
    let mut unrelated = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    save_jobs(
        &path,
        vec![job(dir.path(), "sleep 30 & echo $! > child; wait")],
    )
    .unwrap();
    let supervisor = crate::services::ServiceSupervisor::default();
    let id = supervisor
        .start_cron(CronServiceConfig::new(path.clone()))
        .unwrap();
    wait(|| dir.path().join("child").exists());
    let pid = std::fs::read_to_string(dir.path().join("child"))
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    supervisor.stop(id).unwrap();
    wait(|| supervisor.status(id).unwrap().state == crate::services::ServiceState::Stopped);
    // Linux can retain an orphan zombie until its system reaper runs; it must no longer execute.
    wait(|| {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| s.split_whitespace().nth(2) == Some("Z"))
            .unwrap_or(true)
    });
    assert!(unrelated.try_wait().unwrap().is_none());
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
    let mut j = job(dir.path(), "sleep 30");
    j.id = "deadline".into();
    j.max_seconds = 1;
    save_jobs(&path, vec![j]).unwrap();
    let id = supervisor.start_cron(CronServiceConfig::new(path)).unwrap();
    wait(|| {
        supervisor.status(id).unwrap().cron.as_ref().unwrap().jobs[0].last_event
            == "Runtime limit reached"
    });
    supervisor.stop(id).unwrap();
}
#[test]
fn utc_day_rules_leap_and_dst_dates() {
    // UTC has no skipped/repeated wall times at DST transitions.
    let every = Schedule::parse("30 1 * * *").unwrap();
    assert_eq!(every.next_after(1_730_593_800), Some(1_730_597_400));
    assert_eq!(
        Schedule::parse("0 0 29 2 *")
            .unwrap()
            .next_after(1_704_067_200),
        Some(1_709_164_800)
    );
    assert!(
        Schedule::parse("0 0 30 2 *")
            .unwrap()
            .next_after(1_704_067_200)
            .is_none()
    );
}
#[test]
#[cfg(unix)]
fn local_dst_gaps_and_repeated_instants() {
    const FLAG: &str = "TERMCRAFT_CRON_DST_FIXTURE";
    if std::env::var_os(FLAG).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "services::cron::tests::local_dst_gaps_and_repeated_instants",
                "--nocapture",
            ])
            .env(FLAG, "1")
            .env("TZ", "America/New_York")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        return;
    }
    let repeated = Schedule::parse("30 1 * * *").unwrap();
    assert_eq!(
        repeated.next_in(1_730_611_799, CronTimezone::SystemLocal),
        Some(1_730_611_800)
    );
    assert_eq!(
        repeated.next_in(1_730_611_800, CronTimezone::SystemLocal),
        Some(1_730_615_400)
    );
    let gap = Schedule::parse("30 2 * * *").unwrap();
    assert_eq!(
        gap.next_in(1_709_969_400, CronTimezone::SystemLocal),
        Some(1_710_138_600)
    );
    assert!(Schedule::parse("0 9 * JAN-MAR MON-FRI").is_ok());
}
#[cfg(windows)]
#[test]
fn native_windows_local_schedule_reaches_the_next_minute() {
    // Windows Local reads the system timezone. Changing a child process's TZ
    // cannot install a particular historical timezone fixture there.
    let every_minute = Schedule::parse("* * * * *").unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(
        every_minute.next_in(now, CronTimezone::SystemLocal),
        Some((now / 60 + 1) * 60)
    );
}
#[cfg(unix)]
#[test]
fn same_store_has_one_owner_and_rejects_live_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.json");
    let j = job(dir.path(), "printf x >> once");
    save_jobs(&path, vec![j.clone()]).unwrap();
    let supervisor = crate::services::ServiceSupervisor::default();
    let config = CronServiceConfig::new(path.clone());
    let first = supervisor.start_cron(config.clone()).unwrap();
    assert!(
        supervisor.start_cron(config).is_err(),
        "same store must have one active scheduler"
    );
    assert!(
        save_jobs(&path, vec![j]).is_err(),
        "active scheduler owns the persisted watermark"
    );
    supervisor.stop(first).unwrap();
}
#[cfg(unix)]
#[test]
fn concurrency_cap_and_service_lifetime_are_real_process_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.json");
    let jobs = (0..6)
        .map(|n| {
            let mut j = job(dir.path(), "printf x >> starts; sleep 30");
            j.id = format!("job-{n}");
            j
        })
        .collect();
    save_jobs(&path, jobs).unwrap();
    let supervisor = crate::services::ServiceSupervisor::default();
    let mut config = CronServiceConfig::new(path);
    config.lifetime = Some(Duration::from_secs(1));
    let id = supervisor.start_cron(config).unwrap();
    wait(|| std::fs::read(dir.path().join("starts")).is_ok_and(|v| v.len() == 4));
    let status = supervisor.status(id).unwrap();
    let jobs = &status.cron.as_ref().unwrap().jobs;
    assert_eq!(
        jobs.iter()
            .filter(|j| j.last_event == "Skipped: process limit")
            .count(),
        2
    );
    wait(|| supervisor.status(id).unwrap().state == crate::services::ServiceState::Stopped);
    assert_eq!(std::fs::read(dir.path().join("starts")).unwrap(), b"xxxx");
    assert!(supervisor.status(id).unwrap().log.len() <= 32);
}
#[cfg(unix)]
#[test]
fn nonzero_exit_is_visible_without_output_or_command_logging() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.json");
    save_jobs(
        &path,
        vec![job(dir.path(), "printf SECRET_FIXTURE_OUTPUT; exit 17")],
    )
    .unwrap();
    let supervisor = crate::services::ServiceSupervisor::default();
    let id = supervisor.start_cron(CronServiceConfig::new(path)).unwrap();
    wait(|| {
        supervisor.status(id).unwrap().cron.as_ref().unwrap().jobs[0].last_event == "Process exited"
    });
    let status = supervisor.status(id).unwrap();
    assert!(
        status.cron.as_ref().unwrap().jobs[0]
            .last_result
            .as_ref()
            .unwrap()
            .contains("17")
    );
    assert!(!format!("{status:?}").contains("SECRET_FIXTURE_OUTPUT"));
    supervisor.stop(id).unwrap();
}
#[cfg(unix)]
#[test]
fn durable_commit_barrier_precedes_launch_and_failure_prevents_execution() {
    use std::sync::{Mutex, mpsc};
    struct Barrier {
        entered: mpsc::SyncSender<()>,
        release: Mutex<mpsc::Receiver<bool>>,
    }
    impl CommitBarrier for Barrier {
        fn finish(&self, parent: &Path) -> std::io::Result<()> {
            self.entered.send(()).unwrap();
            if !self.release.lock().unwrap().recv().unwrap() {
                return Err(std::io::Error::other("injected directory sync failure"));
            }
            NativeBarrier.finish(parent)
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.json");
    save_jobs(&path, vec![job(dir.path(), "printf x >> committed")]).unwrap();
    let supervisor = crate::services::ServiceSupervisor::default();
    let clock = Arc::new(Clock(AtomicU64::new(1_704_099_600)));
    for succeed in [false, true] {
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let mut config = CronServiceConfig::new(path.clone());
        config.clock = clock.clone();
        config.barrier = Arc::new(Barrier {
            entered: entered_tx,
            release: Mutex::new(release_rx),
        });
        let id = supervisor.start_cron(config).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        // The actual private file was staged, synced and renamed; its parent commit is still held.
        assert_eq!(
            load(&path).unwrap().entries[0].last_minute,
            Some(clock.unix_seconds() / 60)
        );
        assert!(!dir.path().join("committed").exists());
        release_tx.send(succeed).unwrap();
        if succeed {
            wait(|| dir.path().join("committed").exists());
            assert_eq!(std::fs::read(dir.path().join("committed")).unwrap(), b"x");
            supervisor.stop(id).unwrap();
            wait(|| supervisor.status(id).unwrap().state == crate::services::ServiceState::Stopped);
        } else {
            wait(|| supervisor.status(id).unwrap().state == crate::services::ServiceState::Failed);
            assert!(!dir.path().join("committed").exists());
            clock.0.fetch_add(60, Ordering::Release);
        }
    }
}

#[test]
fn releasing_store_owner_unlocks_inherited_open_description() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.json");
    let owner = store_lock(&path).unwrap();
    // dup and fork share the same open file description. Hold the duplicate
    // through owner teardown to reproduce a concurrent pre-exec child.
    let inherited = owner.try_clone().unwrap();
    assert!(
        store_lock(&path).is_err(),
        "live owner must remain exclusive"
    );
    drop(owner);
    let replacement = store_lock(&path).expect("retired owner must explicitly release its lock");
    drop(inherited);
    assert!(
        store_lock(&path).is_err(),
        "old child teardown must not unlock the new owner"
    );
    drop(replacement);
    assert!(store_lock(&path).is_ok());
}
