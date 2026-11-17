//! Local macOS metrics. Native reads run on one sleeping worker; the UI only clones a snapshot.
//!
//! CPU, network and disk counters are differenced only across consecutive valid samples.
//! Pausing, failures, device changes and sleep all establish a fresh baseline.
use eframe::egui;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime},
};

const INTERVAL: Duration = Duration::from_secs(2);
const MAX_SAMPLE_GAP_NS: u64 = 10_000_000_000;
const SLEEP_TOLERANCE_NS: u64 = 250_000_000;

#[derive(Clone, Debug, Default)]
pub struct Gpu {
    pub name: String,
    /// Percentage, 0..=100. None means the driver does not expose utilization.
    pub usage: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct Network {
    pub interface: String,
    pub received_bytes_per_sec: Option<f64>,
    pub sent_bytes_per_sec: Option<f64>,
    pub received_bytes: u64,
    pub sent_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkStatus {
    #[default]
    Unknown,
    /// One physical interface is active. Rates may still be warming up.
    Online,
    /// No configured, active physical interface was found; no Internet probe is made.
    Offline,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub supported: bool,
    pub sampled_at: Option<SystemTime>,
    pub paused: bool,
    pub cpu_usage: Option<f64>,
    pub cpu_name: String,
    pub logical_cores: u32,
    pub memory_total: u64,
    pub memory_used: u64,
    pub memory_compressed: u64,
    pub memory_valid: bool,
    pub swap_used: u64,
    pub swap_valid: bool,
    pub disk_total: u64,
    pub disk_available: u64,
    pub disk_valid: bool,
    /// Combined physical block-driver I/O; APFS volumes are not counted separately.
    pub disk_read_bytes_per_sec: Option<f64>,
    pub disk_write_bytes_per_sec: Option<f64>,
    pub gpus: Vec<Gpu>,
    pub network: Option<Network>,
    pub network_status: NetworkStatus,
    pub error: Option<String>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            supported: cfg!(target_os = "macos"),
            sampled_at: None,
            paused: true,
            cpu_usage: None,
            cpu_name: String::new(),
            logical_cores: 0,
            memory_total: 0,
            memory_used: 0,
            memory_compressed: 0,
            memory_valid: false,
            swap_used: 0,
            swap_valid: false,
            disk_total: 0,
            disk_available: 0,
            disk_valid: false,
            disk_read_bytes_per_sec: None,
            disk_write_bytes_per_sec: None,
            gpus: Vec::new(),
            network: None,
            network_status: NetworkStatus::Unknown,
            error: None,
        }
    }
}

impl Snapshot {
    fn clear_rates(&mut self) {
        self.cpu_usage = None;
        self.disk_read_bytes_per_sec = None;
        self.disk_write_bytes_per_sec = None;
        if let Some(network) = &mut self.network {
            network.received_bytes_per_sec = None;
            network.sent_bytes_per_sec = None;
        }
    }
}

/// Call update with `page_visible && !screen_locked`. Disabled workers wait without polling.
pub struct Monitor {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    spawn_failed: bool,
}

impl Default for Monitor {
    fn default() -> Self {
        Self { shared: Arc::new(Shared::default()), worker: None, spawn_failed: false }
    }
}

impl Monitor {
    pub fn update(&mut self, enabled: bool, ctx: &egui::Context) {
        let enabled = enabled && cfg!(target_os = "macos");
        self.shared.set_enabled(enabled, Some(ctx.clone()));
        if !enabled {
            self.spawn_failed = false;
            return;
        }
        if self.worker.is_none() && !self.spawn_failed {
            match spawn_worker(self.shared.clone(), INTERVAL, read_native) {
                Ok(worker) => self.worker = Some(worker),
                Err(error) => {
                    self.spawn_failed = true;
                    let mut state = self.shared.lock();
                    state.snapshot.paused = true;
                    state.snapshot.error = Some(format!("无法启动系统状态采集：{error}"));
                }
            }
        }
    }

    pub fn snapshot(&self) -> Snapshot { self.shared.lock().snapshot.clone() }

    /// Request an immediate sample if enabled. Does not create work while paused.
    pub fn refresh(&mut self) {
        self.spawn_failed = false;
        let mut state = self.shared.lock();
        state.refresh = state.refresh.wrapping_add(1);
        self.shared.changed.notify_all();
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        {
            let mut state = self.shared.lock();
            state.stop = true;
            self.shared.changed.notify_all();
        }
        // No commands, networking or run-loop dispatch occur in the native sampler.
        // Wake the sleeping thread and join it so repeated page lifetimes cannot leak workers.
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
}

#[derive(Default)]
struct State {
    enabled: bool,
    stop: bool,
    epoch: u64,
    refresh: u64,
    context: Option<egui::Context>,
    snapshot: Snapshot,
}

#[derive(Default)]
struct Shared { state: Mutex<State>, changed: Condvar }

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn set_enabled(&self, enabled: bool, context: Option<egui::Context>) {
        let mut state = self.lock();
        if let Some(context) = context { state.context = Some(context); }
        if state.enabled != enabled {
            state.enabled = enabled;
            state.epoch = state.epoch.wrapping_add(1);
            state.snapshot.paused = !enabled;
            state.snapshot.clear_rates();
            self.changed.notify_all();
        }
    }
}

fn spawn_worker(
    shared: Arc<Shared>, interval: Duration,
    mut sample: impl FnMut() -> Result<RawSample, String> + Send + 'static,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new().name("qingyu-mac-stats".into()).spawn(move || {
        let mut processor = Processor::default();
        let mut epoch = 0;
        let mut refresh = 0;
        let mut deadline = Instant::now();
        loop {
            let mut state = shared.lock();
            while !state.stop {
                if !state.enabled {
                    processor.reset();
                    state = shared.changed.wait(state).unwrap_or_else(|error| error.into_inner());
                    continue;
                }
                if state.epoch != epoch {
                    processor.reset();
                    epoch = state.epoch;
                    deadline = Instant::now();
                }
                let now = Instant::now();
                if state.refresh != refresh || now >= deadline { break; }
                let (next, _) = shared.changed.wait_timeout(state, deadline.saturating_duration_since(now))
                    .unwrap_or_else(|error| error.into_inner());
                state = next;
            }
            if state.stop { break; }
            let sampling_epoch = state.epoch;
            refresh = state.refresh;
            drop(state);

            let snapshot = processor.process(sample());
            deadline = Instant::now() + interval;
            let context = {
                let mut state = shared.lock();
                // A pause or re-enable during an in-flight read invalidates that read.
                if state.stop || !state.enabled || state.epoch != sampling_epoch { None }
                else {
                    state.snapshot = snapshot;
                    shared.changed.notify_all();
                    state.context.clone()
                }
            };
            if let Some(context) = context { context.request_repaint(); }
        }
    })
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct RawSample {
    continuous_ns: u64,
    awake_ns: u64,
    cpu_ticks: Option<[u32; 4]>, // user, system, idle, nice
    ticks_per_second: u64,
    cpu_name: String,
    logical_cores: u32,
    memory: Option<RawMemory>,
    swap_used: Option<u64>,
    disk_space: Option<RawDiskSpace>,
    disk_io: Option<Vec<RawDiskIo>>,
    gpus: Vec<RawGpu>,
    network: Option<RawNetwork>,
    network_status: NetworkStatus,
    errors: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct RawMemory { total: u64, used: u64, compressed: u64 }
#[derive(Clone, Debug, Default, Deserialize)]
struct RawDiskSpace { total: u64, available: u64 }
#[derive(Clone, Debug, Default, Deserialize)]
struct RawDiskIo { id: u64, read_bytes: u64, write_bytes: u64 }
#[derive(Clone, Debug, Default, Deserialize)]
struct RawGpu { name: String, usage: Option<f64> }
#[derive(Clone, Debug, Default, Deserialize)]
struct RawNetwork { interface: String, identity: String, received_bytes: u64, sent_bytes: u64 }

#[derive(Default)]
struct Processor { previous: Option<RawSample> }

impl Processor {
    fn reset(&mut self) { self.previous = None; }

    fn process(&mut self, result: Result<RawSample, String>) -> Snapshot {
        let raw = match result {
            Ok(raw) => raw,
            Err(error) => {
                self.reset();
                return Snapshot { sampled_at: Some(SystemTime::now()), paused: false,
                    error: Some(error), network_status: NetworkStatus::Unavailable, ..Default::default() };
            }
        };
        let previous = self.previous.as_ref();
        let interval = previous.and_then(|previous| sample_interval(previous, &raw));
        let mut snapshot = Snapshot {
            sampled_at: Some(SystemTime::now()), paused: false,
            cpu_name: raw.cpu_name.clone(), logical_cores: raw.logical_cores,
            gpus: raw.gpus.iter().map(|gpu| Gpu { name: gpu.name.clone(),
                usage: gpu.usage.filter(|value| value.is_finite() && (0.0..=100.0).contains(value)) }).collect(),
            network_status: raw.network_status,
            error: (!raw.errors.is_empty()).then(|| raw.errors.join("；")),
            ..Default::default()
        };
        if let Some(memory) = &raw.memory {
            snapshot.memory_valid = memory.total > 0 && memory.used <= memory.total && memory.compressed <= memory.total;
            if snapshot.memory_valid {
                snapshot.memory_total = memory.total;
                snapshot.memory_used = memory.used;
                snapshot.memory_compressed = memory.compressed;
            }
        }
        if let Some(swap) = raw.swap_used { snapshot.swap_used = swap; snapshot.swap_valid = true; }
        if let Some(space) = &raw.disk_space {
            snapshot.disk_valid = space.total > 0 && space.available <= space.total;
            if snapshot.disk_valid { snapshot.disk_total = space.total; snapshot.disk_available = space.available; }
        }
        if let (Some(previous), Some(seconds)) = (previous, interval) {
            snapshot.cpu_usage = cpu_usage(previous, &raw, seconds);
            if let Some((read, write)) = disk_rates(previous.disk_io.as_deref(), raw.disk_io.as_deref(), seconds) {
                snapshot.disk_read_bytes_per_sec = Some(read);
                snapshot.disk_write_bytes_per_sec = Some(write);
            }
        }
        if let Some(network) = &raw.network {
            let rates = previous.zip(interval).and_then(|(previous, seconds)| {
                let previous = previous.network.as_ref()?;
                if previous.identity != network.identity || previous.interface != network.interface { return None; }
                Some((byte_rate(previous.received_bytes, network.received_bytes, seconds)?,
                    byte_rate(previous.sent_bytes, network.sent_bytes, seconds)?))
            });
            snapshot.network = Some(Network {
                interface: network.interface.clone(), received_bytes: network.received_bytes, sent_bytes: network.sent_bytes,
                received_bytes_per_sec: rates.map(|rates| rates.0), sent_bytes_per_sec: rates.map(|rates| rates.1),
            });
        }
        self.previous = Some(raw);
        snapshot
    }
}

fn sample_interval(previous: &RawSample, current: &RawSample) -> Option<f64> {
    let elapsed = current.continuous_ns.checked_sub(previous.continuous_ns)?;
    let awake = current.awake_ns.checked_sub(previous.awake_ns)?;
    if elapsed == 0 || elapsed > MAX_SAMPLE_GAP_NS || elapsed.saturating_sub(awake) > SLEEP_TOLERANCE_NS { return None; }
    Some(elapsed as f64 / 1_000_000_000.0)
}

fn cpu_usage(previous: &RawSample, current: &RawSample, seconds: f64) -> Option<f64> {
    if current.logical_cores == 0 || current.logical_cores != previous.logical_cores || current.ticks_per_second == 0 { return None; }
    let previous = previous.cpu_ticks?;
    let current_ticks = current.cpu_ticks?;
    // The kernel exports unsigned 32-bit cumulative counters, which can wrap on long uptimes.
    let deltas = std::array::from_fn::<_, 4, _>(|index| current_ticks[index].wrapping_sub(previous[index]) as u64);
    let total: u64 = deltas.iter().sum();
    // Reject reset/corrupt counters instead of turning a rollback into a huge busy sample.
    let plausible = seconds * current.logical_cores as f64 * current.ticks_per_second as f64 * 4.0 + current.logical_cores as f64;
    if total == 0 || total as f64 > plausible { return None; }
    Some(((deltas[0] + deltas[1] + deltas[3]) as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

fn byte_rate(previous: u64, current: u64, seconds: f64) -> Option<f64> {
    if !seconds.is_finite() || seconds <= 0.0 { return None; }
    let rate = current.checked_sub(previous)? as f64 / seconds;
    rate.is_finite().then_some(rate)
}

fn disk_rates(previous: Option<&[RawDiskIo]>, current: Option<&[RawDiskIo]>, seconds: f64) -> Option<(f64, f64)> {
    let previous = previous?;
    let current = current?;
    if previous.is_empty() || previous.len() != current.len() || !seconds.is_finite() || seconds <= 0.0 { return None; }
    let count = previous.len();
    let previous: BTreeMap<_, _> = previous.iter().map(|disk| (disk.id, disk)).collect();
    let current: BTreeMap<_, _> = current.iter().map(|disk| (disk.id, disk)).collect();
    if previous.len() != count || current.len() != count { return None; }
    let mut read = 0_u128;
    let mut write = 0_u128;
    for (id, disk) in current {
        let previous = previous.get(&id)?;
        read += disk.read_bytes.checked_sub(previous.read_bytes)? as u128;
        write += disk.write_bytes.checked_sub(previous.write_bytes)? as u128;
    }
    let rates = (read as f64 / seconds, write as f64 / seconds);
    (seconds > 0.0 && rates.0.is_finite() && rates.1.is_finite()).then_some(rates)
}

#[cfg(target_os = "macos")]
fn read_native() -> Result<RawSample, String> {
    use std::ffi::{c_char, c_void, CStr};
    unsafe extern "C" {
        fn qy_stats_sample_json() -> *mut c_char;
        fn qy_stats_free(value: *mut c_void);
    }
    struct Owned(*mut c_char);
    impl Drop for Owned { fn drop(&mut self) { unsafe { qy_stats_free(self.0.cast()); } } }
    let pointer = unsafe { qy_stats_sample_json() };
    if pointer.is_null() { return Err("无法读取系统状态".into()); }
    let owned = Owned(pointer);
    let bytes = unsafe { CStr::from_ptr(owned.0) }.to_bytes();
    serde_json::from_slice(bytes).map_err(|error| format!("系统状态数据无效：{error}"))
}

#[cfg(not(target_os = "macos"))]
fn read_native() -> Result<RawSample, String> { Err("仅支持 macOS".into()) }

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::{AtomicBool, Ordering}, mpsc};

    fn sample(second: u64, counter: u64) -> RawSample {
        RawSample {
            continuous_ns: second * 1_000_000_000, awake_ns: second * 1_000_000_000,
            cpu_ticks: Some([counter as u32, counter as u32, counter as u32 * 2, 0]),
            ticks_per_second: 100, logical_cores: 4, cpu_name: "Test CPU".into(),
            memory: Some(RawMemory { total: 16_000, used: 9_000, compressed: 100 }), swap_used: Some(0),
            disk_space: Some(RawDiskSpace { total: 50_000, available: 20_000 }),
            disk_io: Some(vec![RawDiskIo { id: 1, read_bytes: counter * 10, write_bytes: counter * 20 }]),
            network: Some(RawNetwork { interface: "en0".into(), identity: "en0:1:mac:epoch".into(),
                received_bytes: counter * 100, sent_bytes: counter * 200 }),
            network_status: NetworkStatus::Online, ..Default::default()
        }
    }

    fn no_rates(snapshot: &Snapshot) {
        assert_eq!(snapshot.cpu_usage, None);
        assert_eq!(snapshot.disk_read_bytes_per_sec, None);
        assert_eq!(snapshot.disk_write_bytes_per_sec, None);
        if let Some(network) = &snapshot.network {
            assert_eq!(network.received_bytes_per_sec, None);
            assert_eq!(network.sent_bytes_per_sec, None);
        }
    }

    #[test]
    fn first_sample_is_a_baseline_and_next_sample_has_real_rates() {
        let mut processor = Processor::default();
        let first = processor.process(Ok(sample(1, 100)));
        no_rates(&first);
        assert!(first.memory_valid && first.swap_valid && first.disk_valid);
        assert_eq!(first.network_status, NetworkStatus::Online);
        let next = processor.process(Ok(sample(3, 200)));
        assert_eq!(next.cpu_usage, Some(50.0));
        assert_eq!(next.disk_read_bytes_per_sec, Some(500.0));
        assert_eq!(next.network.unwrap().received_bytes_per_sec, Some(5_000.0));
    }

    #[test]
    fn failures_reset_only_affected_counters_and_do_not_publish_false_zeroes() {
        let mut processor = Processor::default();
        processor.process(Ok(sample(1, 100)));
        let mut failed = sample(3, 200);
        failed.cpu_ticks = None; failed.disk_io = None; failed.network = None;
        failed.memory = None; failed.swap_used = None; failed.disk_space = None;
        failed.network_status = NetworkStatus::Unavailable;
        let failed = processor.process(Ok(failed));
        no_rates(&failed);
        assert!(!failed.memory_valid && !failed.swap_valid && !failed.disk_valid);
        no_rates(&processor.process(Ok(sample(5, 300))));
        processor.process(Err("native failure".into()));
        no_rates(&processor.process(Ok(sample(7, 400))));
        assert_eq!(processor.process(Ok(sample(9, 500))).cpu_usage, Some(50.0));
    }

    #[test]
    fn sleep_pause_and_backwards_clocks_establish_fresh_baselines() {
        let mut processor = Processor::default();
        processor.process(Ok(sample(1, 100)));
        let mut after_sleep = sample(3, 200);
        after_sleep.awake_ns = 1_100_000_000;
        no_rates(&processor.process(Ok(after_sleep)));
        no_rates(&processor.process(Ok(sample(30, 300))));
        no_rates(&processor.process(Ok(sample(29, 400))));
        processor.reset();
        no_rates(&processor.process(Ok(sample(31, 500))));
    }

    #[test]
    fn interface_disk_changes_and_counter_rollbacks_cannot_spike() {
        let mut processor = Processor::default();
        processor.process(Ok(sample(1, 100)));
        let mut replaced = sample(3, 900_000);
        replaced.network.as_mut().unwrap().identity = "en0:1:new-mac:epoch".into();
        replaced.disk_io.as_mut().unwrap()[0].id = 2;
        let snapshot = processor.process(Ok(replaced));
        no_rates(&snapshot);
        let mut rollback = sample(5, 1);
        rollback.disk_io.as_mut().unwrap()[0].id = 2;
        rollback.network.as_mut().unwrap().identity = "en0:1:new-mac:epoch".into();
        no_rates(&processor.process(Ok(rollback)));
        let mut offline = sample(7, 100);
        offline.network = None; offline.network_status = NetworkStatus::Offline;
        let offline = processor.process(Ok(offline));
        assert!(offline.network.is_none());
        assert_eq!(offline.network_status, NetworkStatus::Offline);
        let connected = processor.process(Ok(sample(9, 200)));
        assert_eq!(connected.network.unwrap().received_bytes_per_sec, None);
    }

    #[test]
    fn cpu_unsigned_wrap_is_valid_but_rollback_and_zero_ticks_are_not() {
        let mut previous = sample(1, 100);
        let mut current = sample(3, 200);
        previous.cpu_ticks = Some([u32::MAX - 9, 100, 100, 0]);
        current.cpu_ticks = Some([10, 110, 130, 0]);
        assert_eq!(cpu_usage(&previous, &current, 2.0), Some(50.0));
        current.cpu_ticks = previous.cpu_ticks;
        assert_eq!(cpu_usage(&previous, &current, 2.0), None);
        current.cpu_ticks = Some([u32::MAX - 10, 99, 99, 0]);
        assert_eq!(cpu_usage(&previous, &current, 2.0), None);
    }

    fn wait_sample(shared: &Shared, after: Option<SystemTime>) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut state = shared.lock();
        while state.snapshot.sampled_at.is_none() || state.snapshot.sampled_at == after {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "worker did not publish a sample");
            let (next, _) = shared.changed.wait_timeout(state, remaining).unwrap();
            state = next;
        }
        state.snapshot.clone()
    }

    #[test]
    fn paused_workers_do_no_work_refresh_resumes_and_drop_joins() {
        struct Ended(Arc<AtomicBool>);
        impl Drop for Ended { fn drop(&mut self) { self.0.store(true, Ordering::SeqCst); } }
        let ended = Arc::new(AtomicBool::new(false));
        let sentinel = Ended(ended.clone());
        let (tx, rx) = mpsc::channel();
        let mut monitor = Monitor::default();
        let mut counter = 0;
        monitor.worker = Some(spawn_worker(monitor.shared.clone(), Duration::from_secs(3600), move || {
            let _keep_alive = &sentinel;
            counter += 1; tx.send(counter).unwrap(); Ok(sample(counter * 2, counter * 100))
        }).unwrap());
        monitor.refresh();
        assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
        monitor.shared.set_enabled(true, None);
        let first = wait_sample(&monitor.shared, None);
        no_rates(&first); assert_eq!(rx.recv().unwrap(), 1);
        monitor.refresh();
        let second = wait_sample(&monitor.shared, first.sampled_at);
        assert_eq!(second.cpu_usage, Some(50.0)); assert_eq!(rx.recv().unwrap(), 2);
        monitor.shared.set_enabled(false, None);
        monitor.refresh();
        assert!(monitor.snapshot().paused);
        assert!(rx.recv_timeout(Duration::from_millis(30)).is_err());
        monitor.shared.set_enabled(true, None);
        let third = wait_sample(&monitor.shared, second.sampled_at);
        no_rates(&third); assert_eq!(rx.recv().unwrap(), 3);
        drop(monitor);
        assert!(ended.load(Ordering::SeqCst), "worker must be joined on drop");
    }

    #[test]
    fn disabling_and_reenabling_during_a_read_discards_that_read() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut monitor = Monitor::default();
        let mut counter = 0;
        monitor.worker = Some(spawn_worker(monitor.shared.clone(), Duration::from_secs(3600), move || {
            counter += 1;
            if counter == 1 { entered_tx.send(()).unwrap(); release_rx.recv().unwrap(); }
            Ok(sample(counter * 2, counter * 100))
        }).unwrap());
        monitor.shared.set_enabled(true, None);
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        monitor.shared.set_enabled(false, None);
        monitor.shared.set_enabled(true, None);
        release_tx.send(()).unwrap();
        let snapshot = wait_sample(&monitor.shared, None);
        no_rates(&snapshot);
        assert_eq!(snapshot.network.unwrap().received_bytes, 20_000);
        drop(monitor);
    }

    #[test]
    fn disk_devices_are_differenced_once_and_invalid_gpu_values_remain_unknown() {
        let previous = vec![RawDiskIo { id: 2, read_bytes: 20, write_bytes: 30 },
            RawDiskIo { id: 1, read_bytes: 10, write_bytes: 20 }];
        let current = vec![RawDiskIo { id: 1, read_bytes: 30, write_bytes: 60 },
            RawDiskIo { id: 2, read_bytes: 40, write_bytes: 70 }];
        assert_eq!(disk_rates(Some(&previous), Some(&current), 2.0), Some((20.0, 40.0)));
        assert_eq!(disk_rates(Some(&previous), Some(&current), f64::INFINITY), None);
        assert_eq!(disk_rates(Some(&previous), Some(&[current[0].clone(), current[0].clone()]), 2.0), None);
        let mut raw = sample(1, 100);
        raw.gpus = vec![RawGpu { name: "Unknown driver".into(), usage: Some(f64::NAN) }];
        assert_eq!(Processor::default().process(Ok(raw)).gpus[0].usage, None);
        assert_eq!(byte_rate(1, 2, 0.0), None);
        assert_eq!(byte_rate(2, 1, 2.0), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_payload_uses_the_same_schema_as_the_portable_processor() {
        let raw = read_native().expect("native JSON must decode without hardware-specific assumptions");
        assert!(raw.continuous_ns > 0 && raw.awake_ns > 0);
        let snapshot = Processor::default().process(Ok(raw));
        no_rates(&snapshot);
        assert!(snapshot.supported);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unsupported_platforms_do_not_start_a_worker() {
        let mut monitor = Monitor::default();
        monitor.update(true, &egui::Context::default());
        assert!(monitor.worker.is_none());
        assert!(!monitor.snapshot().supported);
    }
}
