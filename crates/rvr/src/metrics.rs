//! Metrics collection and reporting using metrics-rs.
//!
//! Provides a unified approach to recording execution and test metrics
//! with support for terminal output and future integration with observability tools.

use std::collections::HashMap;
use std::sync::Arc;

use metrics::{
    Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit, counter,
    describe_counter, describe_gauge, describe_histogram, gauge, histogram,
};
use parking_lot::RwLock;

use crate::{PerfCounters, RunResult};

/// Test result status for metrics recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    Pass,
    Fail,
    Skip,
}

// ============================================================================
// Metric descriptions
// ============================================================================

/// Initialize metric descriptions.
///
/// Call this once at startup to register metric descriptions.
pub fn init() {
    // Counters (cumulative)
    describe_counter!(
        "rvr_guest_instructions_total",
        Unit::Count,
        "Total guest RISC-V instructions retired"
    );
    describe_counter!(
        "rvr_host_cycles_total",
        Unit::Count,
        "Total host CPU cycles"
    );
    describe_counter!(
        "rvr_host_instructions_total",
        Unit::Count,
        "Total host CPU instructions"
    );
    describe_counter!(
        "rvr_host_branches_total",
        Unit::Count,
        "Total host branch instructions"
    );
    describe_counter!(
        "rvr_host_branch_misses_total",
        Unit::Count,
        "Total host branch mispredictions"
    );
    describe_counter!("rvr_tests_passed_total", Unit::Count, "Total tests passed");
    describe_counter!("rvr_tests_failed_total", Unit::Count, "Total tests failed");
    describe_counter!(
        "rvr_tests_skipped_total",
        Unit::Count,
        "Total tests skipped"
    );

    // Gauges (point-in-time values)
    describe_gauge!(
        "rvr_execution_time_seconds",
        Unit::Seconds,
        "Execution wall-clock time"
    );
    describe_gauge!(
        "rvr_guest_speed_mips",
        Unit::Count,
        "Guest instruction speed in MIPS"
    );
    describe_gauge!("rvr_host_ipc", Unit::Count, "Host instructions per cycle");
    describe_gauge!(
        "rvr_host_branch_miss_rate",
        Unit::Count,
        "Host branch miss rate (0-1)"
    );
    describe_gauge!(
        "rvr_overhead_ratio",
        Unit::Count,
        "VM time / host time ratio"
    );

    // Histograms (distribution)
    describe_histogram!(
        "rvr_run_duration_seconds",
        Unit::Seconds,
        "Execution duration distribution"
    );
}

// ============================================================================
// Metric recording functions
// ============================================================================

/// Record execution metrics after a run.
///
/// # Arguments
/// * `arch` - Architecture label (e.g., "rv32i", "rv64e", "host")
/// * `result` - Execution result containing instret, time, and MIPS
/// * `perf` - Optional hardware performance counters
pub fn record_run(arch: &str, result: &RunResult, perf: Option<&PerfCounters>) {
    let labels = [("arch", arch.to_string())];

    counter!("rvr_guest_instructions_total", &labels).absolute(result.instret);
    gauge!("rvr_execution_time_seconds", &labels).set(result.time_secs);
    gauge!("rvr_guest_speed_mips", &labels).set(result.mips);
    histogram!("rvr_run_duration_seconds", &labels).record(result.time_secs);

    if let Some(p) = perf {
        if let Some(c) = p.cycles {
            counter!("rvr_host_cycles_total", &labels).absolute(c);
        }
        if let Some(i) = p.instructions {
            counter!("rvr_host_instructions_total", &labels).absolute(i);
        }
        if let Some(b) = p.branches {
            counter!("rvr_host_branches_total", &labels).absolute(b);
        }
        if let Some(m) = p.branch_misses {
            counter!("rvr_host_branch_misses_total", &labels).absolute(m);
        }
        if let Some(ipc) = p.ipc() {
            gauge!("rvr_host_ipc", &labels).set(ipc);
        }
        if let Some(rate) = p.branch_miss_rate() {
            // Convert from percentage to fraction (0-1)
            gauge!("rvr_host_branch_miss_rate", &labels).set(rate / 100.0);
        }
    }
}

/// Record overhead ratio (vm_time / host_time).
pub fn record_overhead(arch: &str, vm_time: f64, host_time: f64) {
    if host_time > 0.0 {
        let labels = [("arch", arch.to_string())];
        gauge!("rvr_overhead_ratio", &labels).set(vm_time / host_time);
    }
}

/// Record a single test result.
pub fn record_test(name: &str, status: TestStatus) {
    let labels = [("test", name.to_string())];
    match status {
        TestStatus::Pass => counter!("rvr_tests_passed_total", &labels).increment(1),
        TestStatus::Fail => counter!("rvr_tests_failed_total", &labels).increment(1),
        TestStatus::Skip => counter!("rvr_tests_skipped_total", &labels).increment(1),
    }
}

/// Record test summary totals.
pub fn record_test_summary(passed: u64, failed: u64, skipped: u64) {
    counter!("rvr_tests_passed_total").absolute(passed);
    counter!("rvr_tests_failed_total").absolute(failed);
    counter!("rvr_tests_skipped_total").absolute(skipped);
}

// ============================================================================
// CLI Recorder for terminal output
// ============================================================================

/// Storage for counter values.
#[derive(Default)]
struct CounterStorage {
    values: RwLock<HashMap<String, u64>>,
}

/// Storage for gauge values.
#[derive(Default)]
struct GaugeStorage {
    values: RwLock<HashMap<String, f64>>,
}

/// Storage for histogram values.
#[derive(Default)]
struct HistogramStorage {
    values: RwLock<HashMap<String, Vec<f64>>>,
}

/// A simple counter handle for the CLI recorder.
struct CliCounter {
    key: String,
    storage: Arc<CounterStorage>,
}

impl metrics::CounterFn for CliCounter {
    fn increment(&self, value: u64) {
        let mut values = self.storage.values.write();
        *values.entry(self.key.clone()).or_insert(0) += value;
    }

    fn absolute(&self, value: u64) {
        let mut values = self.storage.values.write();
        values.insert(self.key.clone(), value);
    }
}

/// A simple gauge handle for the CLI recorder.
struct CliGauge {
    key: String,
    storage: Arc<GaugeStorage>,
}

impl metrics::GaugeFn for CliGauge {
    fn increment(&self, value: f64) {
        let mut values = self.storage.values.write();
        *values.entry(self.key.clone()).or_insert(0.0) += value;
    }

    fn decrement(&self, value: f64) {
        let mut values = self.storage.values.write();
        *values.entry(self.key.clone()).or_insert(0.0) -= value;
    }

    fn set(&self, value: f64) {
        let mut values = self.storage.values.write();
        values.insert(self.key.clone(), value);
    }
}

/// A simple histogram handle for the CLI recorder.
struct CliHistogram {
    key: String,
    storage: Arc<HistogramStorage>,
}

impl metrics::HistogramFn for CliHistogram {
    fn record(&self, value: f64) {
        let mut values = self.storage.values.write();
        values.entry(self.key.clone()).or_default().push(value);
    }
}

/// CLI recorder that stores metrics for terminal output.
///
/// This recorder collects metrics in memory and can print them
/// in a human-readable format for CLI usage.
pub struct CliRecorder {
    counters: Arc<CounterStorage>,
    gauges: Arc<GaugeStorage>,
    histograms: Arc<HistogramStorage>,
}

impl CliRecorder {
    /// Create a new CLI recorder.
    pub fn new() -> Self {
        Self {
            counters: Arc::new(CounterStorage::default()),
            gauges: Arc::new(GaugeStorage::default()),
            histograms: Arc::new(HistogramStorage::default()),
        }
    }

    /// Install this recorder as the global metrics recorder.
    ///
    /// Returns a handle that can be used to retrieve metrics later.
    pub fn install(self) -> Option<CliRecorderHandle> {
        let counters = Arc::clone(&self.counters);
        let gauges = Arc::clone(&self.gauges);
        let histograms = Arc::clone(&self.histograms);

        metrics::set_global_recorder(self).ok()?;

        Some(CliRecorderHandle {
            counters,
            gauges,
            histograms,
        })
    }
}

impl Default for CliRecorder {
    fn default() -> Self {
        Self::new()
    }
}

fn key_to_string(key: &Key) -> String {
    let name = key.name();
    let labels = key.labels();
    if labels.len() == 0 {
        name.to_string()
    } else {
        let label_str: Vec<String> = labels
            .map(|l| format!("{}={}", l.key(), l.value()))
            .collect();
        format!("{}{{{}}}", name, label_str.join(","))
    }
}

impl Recorder for CliRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}
    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        Counter::from_arc(Arc::new(CliCounter {
            key: key_to_string(key),
            storage: Arc::clone(&self.counters),
        }))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        Gauge::from_arc(Arc::new(CliGauge {
            key: key_to_string(key),
            storage: Arc::clone(&self.gauges),
        }))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::from_arc(Arc::new(CliHistogram {
            key: key_to_string(key),
            storage: Arc::clone(&self.histograms),
        }))
    }
}

/// Handle for accessing recorded metrics after installing the CLI recorder.
pub struct CliRecorderHandle {
    counters: Arc<CounterStorage>,
    gauges: Arc<GaugeStorage>,
    histograms: Arc<HistogramStorage>,
}

impl CliRecorderHandle {
    /// Get a counter value by key.
    pub fn get_counter(&self, key: &str) -> Option<u64> {
        self.counters.values.read().get(key).copied()
    }

    /// Get a gauge value by key.
    pub fn get_gauge(&self, key: &str) -> Option<f64> {
        self.gauges.values.read().get(key).copied()
    }

    /// Get histogram values by key.
    pub fn get_histogram(&self, key: &str) -> Option<Vec<f64>> {
        self.histograms.values.read().get(key).cloned()
    }

    /// Get all counter values.
    pub fn all_counters(&self) -> HashMap<String, u64> {
        self.counters.values.read().clone()
    }

    /// Get all gauge values.
    pub fn all_gauges(&self) -> HashMap<String, f64> {
        self.gauges.values.read().clone()
    }

    /// Get all histogram values.
    pub fn all_histograms(&self) -> HashMap<String, Vec<f64>> {
        self.histograms.values.read().clone()
    }

    /// Print all collected metrics in a human-readable format.
    pub fn print_summary(&self) {
        let counters = self.counters.values.read();
        let gauges = self.gauges.values.read();
        let histograms = self.histograms.values.read();

        if counters.is_empty() && gauges.is_empty() && histograms.is_empty() {
            println!("No metrics collected.");
            return;
        }

        println!();
        println!("## Metrics Summary");
        println!();

        if !counters.is_empty() {
            println!("### Counters");
            let mut keys: Vec<_> = counters.keys().collect();
            keys.sort();
            for key in keys {
                if let Some(value) = counters.get(key) {
                    println!("  {}: {}", key, value);
                }
            }
            println!();
        }

        if !gauges.is_empty() {
            println!("### Gauges");
            let mut keys: Vec<_> = gauges.keys().collect();
            keys.sort();
            for key in keys {
                if let Some(value) = gauges.get(key) {
                    println!("  {}: {:.6}", key, value);
                }
            }
            println!();
        }

        if !histograms.is_empty() {
            println!("### Histograms");
            let mut keys: Vec<_> = histograms.keys().collect();
            keys.sort();
            for key in keys {
                if let Some(values) = histograms.get(key)
                    && !values.is_empty()
                {
                    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
                    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    let sum: f64 = values.iter().sum();
                    let avg = sum / values.len() as f64;
                    println!(
                        "  {}: count={}, min={:.6}, max={:.6}, avg={:.6}",
                        key,
                        values.len(),
                        min,
                        max,
                        avg
                    );
                }
            }
            println!();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metrics::Label;

    #[test]
    fn test_key_to_string() {
        // Test with no labels
        let key = Key::from_name("test_metric");
        assert_eq!(key_to_string(&key), "test_metric");

        // Test with labels
        let key = Key::from_parts("test_metric", vec![Label::new("arch", "rv64i")]);
        assert_eq!(key_to_string(&key), "test_metric{arch=rv64i}");

        let key = Key::from_parts(
            "test_metric",
            vec![Label::new("arch", "rv64i"), Label::new("mode", "fast")],
        );
        assert_eq!(key_to_string(&key), "test_metric{arch=rv64i,mode=fast}");
    }

    #[test]
    fn test_cli_recorder_storage() {
        let recorder = CliRecorder::new();
        let counters = Arc::clone(&recorder.counters);
        let gauges = Arc::clone(&recorder.gauges);

        // Test counter
        let counter = CliCounter {
            key: "test_counter".to_string(),
            storage: counters,
        };
        metrics::CounterFn::increment(&counter, 5);
        assert_eq!(counter.storage.values.read().get("test_counter"), Some(&5));
        metrics::CounterFn::absolute(&counter, 10);
        assert_eq!(counter.storage.values.read().get("test_counter"), Some(&10));

        // Test gauge
        let gauge = CliGauge {
            key: "test_gauge".to_string(),
            storage: gauges,
        };
        metrics::GaugeFn::set(&gauge, 1.23);
        assert_eq!(gauge.storage.values.read().get("test_gauge"), Some(&1.23));
    }
}
