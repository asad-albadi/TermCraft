//! App-owned scheduler: user-authored commands only, never OS crontabs.
use super::supervisor::Context;
use chrono::{Datelike, Local, LocalResult, NaiveDate, TimeZone as _, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const MAX_JOBS: usize = 32;
const MAX_FILE: u64 = 256 * 1024;
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum CronTimezone {
    #[default]
    Utc,
    SystemLocal,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CronJob {
    pub id: String,
    pub schedule: String,
    pub enabled: bool,
    pub program: PathBuf,
    pub arguments: Vec<String>,
    pub directory: PathBuf,
    pub max_seconds: u32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    job: CronJob,
    #[serde(default)]
    last_minute: Option<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    version: u8,
    #[serde(default)]
    timezone: CronTimezone,
    entries: Vec<Entry>,
}
pub trait CronClock: Send + Sync {
    fn unix_seconds(&self) -> u64;
}
struct SystemClock;
impl CronClock for SystemClock {
    fn unix_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}
#[derive(Clone)]
pub struct CronServiceConfig {
    pub store: PathBuf,
    pub clock: Arc<dyn CronClock>,
    pub lifetime: Option<Duration>,
    barrier: Arc<dyn CommitBarrier>,
}
impl CronServiceConfig {
    pub fn new(store: PathBuf) -> Self {
        Self {
            store,
            clock: Arc::new(SystemClock),
            lifetime: None,
            barrier: Arc::new(NativeBarrier),
        }
    }
}
#[derive(Clone, Debug)]
pub struct CronJobStatus {
    pub id: String,
    pub enabled: bool,
    pub next_run: Option<u64>,
    pub last_event: &'static str,
    pub last_result: Option<String>,
}
#[derive(Clone, Debug)]
pub struct CronStatus {
    pub timezone: CronTimezone,
    pub jobs: Vec<CronJobStatus>,
}
#[derive(Clone)]
pub struct Schedule {
    fields: [BTreeSet<u32>; 5],
    unrestricted: [bool; 2],
}
impl Schedule {
    pub fn parse(text: &str) -> Result<Self, String> {
        let parts: Vec<_> = text.split_whitespace().collect();
        if parts.len() != 5 || text.len() > 256 {
            return Err("Use five cron fields: minute hour day month weekday.".into());
        }
        let bounds = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];
        let mut fields: [BTreeSet<u32>; 5] = Default::default();
        for (i, part) in parts.iter().enumerate() {
            let value = |s: &str| -> Result<u32, String> {
                let upper = s.to_ascii_uppercase();
                let named = if i == 3 {
                    [
                        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT",
                        "NOV", "DEC",
                    ]
                    .iter()
                    .position(|n| *n == upper)
                    .map(|v| v as u32 + 1)
                } else if i == 4 {
                    ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"]
                        .iter()
                        .position(|n| *n == upper)
                        .map(|v| v as u32)
                } else {
                    None
                };
                named
                    .or_else(|| {
                        s.bytes()
                            .all(|c| c.is_ascii_digit())
                            .then(|| s.parse().ok())
                            .flatten()
                    })
                    .ok_or_else(|| {
                        "Unsupported cron value; use numbers or month/weekday names.".into()
                    })
            };
            for item in part.split(',') {
                let pair: Vec<_> = item.split('/').collect();
                if pair.len() > 2 {
                    return Err("Invalid cron step.".into());
                }
                let step = if pair.len() == 2 {
                    pair[1].parse::<u32>().map_err(|_| "Invalid cron step.")?
                } else {
                    1
                };
                if step == 0 || step > bounds[i].1 - bounds[i].0 + 1 {
                    return Err("Cron step is outside its field range.".into());
                }
                let (start, end) = if pair[0] == "*" {
                    bounds[i]
                } else if let Some((a, b)) = pair[0].split_once('-') {
                    (value(a)?, value(b)?)
                } else {
                    let n = value(pair[0])?;
                    (n, if pair.len() == 2 { bounds[i].1 } else { n })
                };
                if start < bounds[i].0 || end > bounds[i].1 || start > end {
                    return Err("Cron value is outside its field range.".into());
                }
                for n in (start..=end).step_by(step as usize) {
                    fields[i].insert(if i == 4 && n == 7 { 0 } else { n });
                }
            }
        }
        Ok(Self {
            fields,
            unrestricted: [parts[2].starts_with('*'), parts[4].starts_with('*')],
        })
    }
    fn day_matches(&self, date: NaiveDate) -> bool {
        let dom = self.fields[2].contains(&date.day());
        let dow = self.fields[4].contains(&date.weekday().num_days_from_sunday());
        self.fields[3].contains(&date.month())
            && if self.unrestricted[0] || self.unrestricted[1] {
                dom && dow
            } else {
                dom || dow
            }
    }
    pub fn matches(&self, seconds: u64) -> bool {
        self.matches_zone(seconds, CronTimezone::Utc)
    }
    fn matches_zone(&self, seconds: u64, zone: CronTimezone) -> bool {
        let Some(dt) = i64::try_from(seconds)
            .ok()
            .and_then(|s| Utc.timestamp_opt(s, 0).single())
        else {
            return false;
        };
        let (date, h, m) = match zone {
            CronTimezone::Utc => (dt.date_naive(), dt.hour(), dt.minute()),
            CronTimezone::SystemLocal => {
                let d = dt.with_timezone(&Local);
                (d.date_naive(), d.hour(), d.minute())
            }
        };
        self.day_matches(date) && self.fields[0].contains(&m) && self.fields[1].contains(&h)
    }
    pub fn next_after(&self, seconds: u64) -> Option<u64> {
        self.next_in(seconds, CronTimezone::Utc)
    }
    pub fn next_in(&self, seconds: u64, zone: CronTimezone) -> Option<u64> {
        let dt = Utc
            .timestamp_opt(i64::try_from(seconds).ok()?, 0)
            .single()?;
        let mut date = match zone {
            CronTimezone::Utc => dt.date_naive(),
            CronTimezone::SystemLocal => dt.with_timezone(&Local).date_naive(),
        };
        for _ in 0..=366 * 8 {
            if self.day_matches(date) {
                let mut best = None;
                for h in &self.fields[1] {
                    for m in &self.fields[0] {
                        let n = date.and_hms_opt(*h, *m, 0)?;
                        let candidates = match zone {
                            CronTimezone::Utc => vec![Utc.from_utc_datetime(&n).timestamp()],
                            CronTimezone::SystemLocal => match Local.from_local_datetime(&n) {
                                LocalResult::None => vec![],
                                LocalResult::Single(d) => vec![d.timestamp()],
                                LocalResult::Ambiguous(a, b) => vec![a.timestamp(), b.timestamp()],
                            },
                        };
                        for s in candidates {
                            if s >= 0 && s as u64 > seconds {
                                best = Some(best.map_or(s as u64, |b: u64| b.min(s as u64)));
                            }
                        }
                    }
                }
                if best.is_some() {
                    return best;
                }
            }
            date = date.succ_opt()?;
        }
        None
    }
}
fn validate(jobs: &[CronJob]) -> Result<(), String> {
    if jobs.len() > MAX_JOBS {
        return Err("At most 32 managed jobs are supported.".into());
    }
    let mut ids = BTreeSet::new();
    for j in jobs {
        Schedule::parse(&j.schedule)?;
        if j.id.trim().is_empty()
            || j.id.len() > 80
            || j.id.chars().any(char::is_control)
            || !ids.insert(&j.id)
        {
            return Err("Job names must be unique, nonempty and at most 80 characters.".into());
        }
        if !j.program.is_absolute()
            || !j.directory.is_absolute()
            || !j.program.is_file()
            || !j.directory.is_dir()
        {
            return Err("Choose an existing absolute executable and working folder.".into());
        }
        if j.program.to_str().is_none()
            || j.arguments.len() > 64
            || j.arguments
                .iter()
                .any(|a| a.len() > 8192 || a.contains('\0'))
            || !(1..=86400).contains(&j.max_seconds)
        {
            return Err("Use at most 64 arguments and a runtime limit of 1–86400 seconds.".into());
        }
    }
    Ok(())
}
fn load(path: &Path) -> Result<FileConfig, String> {
    use std::io::Read;
    let file = crate::platform::private_file::open(path, false)
        .map_err(|_| "Could not read the managed cron file.")?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the managed cron file.")?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("Cron file is too large.".into());
    }
    let data: FileConfig = serde_json::from_slice(&bytes)
        .map_err(|_| "Invalid cron file; existing configuration was preserved.")?;
    if data.version != 1 {
        return Err("Unsupported cron file version.".into());
    }
    validate(
        &data
            .entries
            .iter()
            .map(|e| e.job.clone())
            .collect::<Vec<_>>(),
    )?;
    Ok(data)
}
// A platform commit boundary, kept separate so failures can be exercised after a real rename.
trait CommitBarrier: Send + Sync {
    fn finish(&self, target: &Path) -> std::io::Result<()>;
}
struct NativeBarrier;
impl CommitBarrier for NativeBarrier {
    fn finish(&self, target: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            let parent = target
                .parent()
                .ok_or_else(|| std::io::Error::other("Missing cron folder"))?;
            std::fs::File::open(parent)?.sync_all()?;
            #[cfg(target_os = "macos")]
            {
                // F_FULLFSYNC asks the device to flush after filesystem metadata has been synced.
                use std::os::fd::AsRawFd;
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(target)?;
                if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_FULLFSYNC) } == -1 {
                    return Err(std::io::Error::last_os_error());
                }
            }
        }
        // Windows replacement itself uses MOVEFILE_WRITE_THROUGH below.
        #[cfg(windows)]
        let _ = target;
        Ok(())
    }
}
fn replace_staged(temporary: tempfile::NamedTempFile, target: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        temporary.persist(target).map_err(|e| e.error)?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let staged = temporary.into_temp_path();
        let source: Vec<_> = staged.as_os_str().encode_wide().chain(Some(0)).collect();
        let destination: Vec<_> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        if source[..source.len() - 1].contains(&0)
            || destination[..destination.len() - 1].contains(&0)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cron path contains NUL",
            ));
        }
        // Same-directory staged file: no cross-volume copy fallback and no delayed reboot move.
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}
fn persist(path: &Path, data: &FileConfig) -> Result<(), String> {
    persist_with_barrier(path, data, &NativeBarrier)
}
fn persist_with_barrier(
    path: &Path,
    data: &FileConfig,
    barrier: &dyn CommitBarrier,
) -> Result<(), String> {
    use std::io::Write;
    let bytes =
        serde_json::to_vec_pretty(data).map_err(|_| "Could not encode cron configuration.")?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("Cron file is too large.".into());
    }
    let parent = path.parent().ok_or("Choose a cron storage folder.")?;
    std::fs::create_dir_all(parent).map_err(|_| "Could not create cron storage.")?;
    let mut temporary = tempfile::Builder::new()
        .make_in(parent, crate::platform::private_file::create_new)
        .map_err(|_| "Could not stage cron configuration.")?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| "Could not write cron configuration.")?;
    replace_staged(temporary, path).map_err(|_| "Could not durably replace cron configuration.")?;
    barrier
        .finish(path)
        .map_err(|_| "Could not durably commit the cron directory; no command was launched.")?;
    Ok(())
}
pub fn load_jobs(path: &Path) -> Result<Vec<CronJob>, String> {
    Ok(load(path)?.entries.into_iter().map(|e| e.job).collect())
}
pub fn load_timezone(path: &Path) -> Result<CronTimezone, String> {
    Ok(load(path)?.timezone)
}
pub fn save_jobs(path: &Path, jobs: Vec<CronJob>) -> Result<(), String> {
    let zone = if path.exists() {
        load(path)?.timezone
    } else {
        CronTimezone::Utc
    };
    save_jobs_with_timezone(path, jobs, zone)
}
struct StoreLock(std::fs::File);
impl std::ops::Deref for StoreLock {
    type Target = std::fs::File;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Drop for StoreLock {
    fn drop(&mut self) {
        // A concurrent fork may hold this open file description until exec.
        // Release ownership now, rather than waiting for its last copy to close.
        let _ = self.0.unlock();
    }
}
fn store_lock(path: &Path) -> Result<StoreLock, String> {
    let parent = path.parent().ok_or("Choose a cron storage folder.")?;
    std::fs::create_dir_all(parent).map_err(|_| "Could not create cron storage.")?;
    let file = crate::platform::private_file::open(&path.with_extension("cron-lock"), true)
        .map_err(|_| "Could not secure the cron lock file.")?;
    file.try_lock().map_err(|_| "This cron file is already owned by an active scheduler or save; stop it before editing.")?;
    Ok(StoreLock(file))
}
pub fn save_jobs_with_timezone(
    path: &Path,
    jobs: Vec<CronJob>,
    timezone: CronTimezone,
) -> Result<(), String> {
    validate(&jobs)?;
    let _lock = store_lock(path)?;
    let old = if path.exists() {
        Some(load(path)?)
    } else {
        None
    };
    let entries = jobs
        .into_iter()
        .map(|job| {
            let last_minute = old
                .as_ref()
                .and_then(|d| d.entries.iter().find(|e| e.job.id == job.id))
                .and_then(|e| e.last_minute);
            Entry { job, last_minute }
        })
        .collect();
    persist(
        path,
        &FileConfig {
            version: 1,
            timezone,
            entries,
        },
    )
}
struct Running {
    index: usize,
    process: crate::process::Process,
    began: Instant,
}
pub(super) fn run(
    config: CronServiceConfig,
    context: &Arc<Context>,
    ready: std::sync::mpsc::SyncSender<Result<(), String>>,
) {
    let _lock = match store_lock(&config.store) {
        Ok(lock) => lock,
        Err(e) => {
            context.cron_failed(e.clone());
            let _ = ready.send(Err(e));
            return;
        }
    };
    if config
        .lifetime
        .is_some_and(|d| d < Duration::from_secs(1) || d > Duration::from_secs(86400))
    {
        let e = "Scheduler lifetime must be 1 second to 24 hours.".to_string();
        context.cron_failed(e.clone());
        let _ = ready.send(Err(e));
        return;
    }
    let result = load(&config.store);
    let mut data = match result {
        Ok(d) => d,
        Err(e) => {
            context.cron_failed(e.clone());
            let _ = ready.send(Err(e));
            return;
        }
    };
    let schedules: Vec<_> = data
        .entries
        .iter()
        .map(|e| Schedule::parse(&e.job.schedule).unwrap())
        .collect();
    let mut state = CronStatus {
        timezone: data.timezone,
        jobs: data
            .entries
            .iter()
            .map(|e| CronJobStatus {
                id: e.job.id.clone(),
                enabled: e.job.enabled,
                next_run: None,
                last_result: None,
                last_event: if e.job.enabled { "Waiting" } else { "Disabled" },
            })
            .collect(),
    };
    let began = Instant::now();
    let mut last_tick = None;
    let mut running: Vec<Running> = vec![];
    context.cron_started(state.clone());
    let _ = ready.send(Ok(()));
    while !context.cancelled() && !config.lifetime.is_some_and(|d| began.elapsed() >= d) {
        let now = config.clock.unix_seconds();
        let minute = now / 60;
        running.retain_mut(|r| {
            // Discard output; never put command text or output in logs. The process backend queue is bounded.
            for _ in 0..256 {
                if r.process.rx.try_recv().is_err() {
                    break;
                }
            }
            r.process.poll();
            let timeout = r.began.elapsed()
                >= Duration::from_secs(data.entries[r.index].job.max_seconds as u64);
            if r.process.exited || timeout {
                state.jobs[r.index].last_result = Some(if timeout {
                    "Stopped at runtime limit".into()
                } else {
                    r.process.exit_message.clone()
                });
                state.jobs[r.index].last_event = if timeout {
                    "Runtime limit reached"
                } else {
                    "Process exited"
                };
                context.log("Cron process finished.");
                false
            } else {
                true
            }
        });
        if last_tick != Some(minute) {
            last_tick = Some(minute);
            let mut admitted = running.len();
            for (i, entry) in data.entries.iter_mut().enumerate() {
                state.jobs[i].next_run = if entry.job.enabled {
                    schedules[i].next_in(
                        entry
                            .last_minute
                            .map(|last| last.saturating_mul(60).saturating_add(59))
                            .unwrap_or(now)
                            .max(now),
                        data.timezone,
                    )
                } else {
                    None
                };
                if !entry.job.enabled
                    || entry.last_minute.is_some_and(|last| minute <= last)
                    || !schedules[i].matches_zone(now, data.timezone)
                {
                    continue;
                }
                entry.last_minute = Some(minute);
                state.jobs[i].last_event = if running.iter().any(|r| r.index == i) {
                    "Skipped: previous run active"
                } else if admitted >= 4 {
                    "Skipped: process limit"
                } else {
                    admitted += 1;
                    "Due"
                };
            }
            // Durable intent before launch: a crash may omit a run, but never replays that recorded minute.
            if let Err(e) = persist_with_barrier(&config.store, &data, config.barrier.as_ref()) {
                context.cron_failed(e);
                return;
            }
            for (i, entry) in data.entries.iter().enumerate() {
                if state.jobs[i].last_event != "Due" {
                    continue;
                }
                match crate::process::Process::spawn_with_environment_cancellable(
                    entry.job.program.to_str().unwrap(),
                    &entry.job.arguments,
                    Some(&entry.job.directory),
                    80,
                    24,
                    &[],
                    context.stop_flag(),
                ) {
                    Ok(process) => {
                        running.push(Running {
                            index: i,
                            process,
                            began: Instant::now(),
                        });
                        state.jobs[i].last_event = "Running";
                        state.jobs[i].last_result = None;
                        context.log("Cron process started.");
                    }
                    Err(_) => {
                        state.jobs[i].last_event = "Command could not start";
                        context.log("Cron command startup failed.");
                    }
                }
            }
        }
        context.cron_update(state.clone());
        std::thread::sleep(super::supervisor::POLL);
    }
    drop(running);
    for s in &mut state.jobs {
        if s.last_event == "Running" {
            s.last_event = "Stopped";
        }
    }
    context.cron_update(state);
    drop(_lock);
    context.cron_stopped();
}
#[cfg(test)]
mod tests;
