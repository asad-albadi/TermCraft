use harbor_workspace::terminal::Screen;
#[test]
#[ignore = "long-session benchmark; run explicitly to record machine and memory"]
fn benchmark_360000_lines_with_a_retained_memory_budget() {
    fn rss_bytes() -> Option<usize> {
        std::fs::read_to_string("/proc/self/status")
            .ok()?
            .lines()
            .find(|line| line.starts_with("VmRSS:"))?
            .split_whitespace()
            .nth(1)?
            .parse::<usize>()
            .ok()
            .map(|kb| kb * 1024)
    }
    let mut screen = Screen::new(110, 30, 360000);
    let before_rss = rss_bytes();
    let start = std::time::Instant::now();
    let chunk = b"owned line containing needle and terminal text\r\n".repeat(100);
    for _ in 0..3600 {
        screen.feed(&chunk);
    }
    let feed = start.elapsed();
    let first = std::time::Instant::now();
    let matches = screen.search_count("needle");
    let initial_search = first.elapsed();
    let indexed = screen.stats().indexed_history_lines;
    let mut samples = Vec::new();
    for _ in 0..1000 {
        let now = std::time::Instant::now();
        assert_eq!(screen.search_count("needle"), matches);
        samples.push(now.elapsed().as_nanos());
    }
    samples.sort_unstable();
    let now = std::time::Instant::now();
    screen.feed(b"owned new needle\r\n");
    let _ = screen.search_count("needle");
    let incremental = now.elapsed();
    assert_eq!(screen.stats().indexed_history_lines - indexed, 1);
    let stats = screen.stats();
    assert!(stats.history_bytes <= stats.history_budget);
    assert!(
        screen.history().len() + screen.rows >= 360000,
        "Ordinary output should retain the configured history under the budget"
    );
    if let (Some(before), Some(after)) = (before_rss, rss_bytes()) {
        assert!(
            after.saturating_sub(before) <= stats.history_budget + 32 * 1024 * 1024,
            "Retained history exceeded its storage budget plus allocator allowance: baseline={before}, after={after}, budget={}",
            stats.history_budget
        );
    }
    let rss = std::fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .find(|line| line.starts_with("VmRSS:"))
        .unwrap_or("RSS unavailable")
        .to_owned();
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find(|line| line.starts_with("model name"))
        .unwrap_or("CPU unavailable")
        .to_owned();
    println!(
        "T01 benchmark: profile={} cols=110 rows=30 input_lines=360000 retained_lines={} retained_bytes={} budget_bytes={} discarded={} feed_ms={} initial_search_us={} cached_search_p50_ns={} cached_search_p95_ns={} incremental_feed_and_search_us={} {} {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        screen.history().len(),
        stats.history_bytes,
        stats.history_budget,
        stats.discarded_lines,
        feed.as_millis(),
        initial_search.as_micros(),
        samples[500],
        samples[950],
        incremental.as_micros(),
        rss,
        cpu
    );
}
