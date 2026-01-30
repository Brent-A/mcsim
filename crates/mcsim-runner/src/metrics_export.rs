//! Metrics export functionality for simulation results.
//!
//! This module provides an in-memory metrics recorder that collects metrics
//! during simulation and can export them in JSON or Prometheus format.
//!
//! # Flexible Breakdown Export
//!
//! The new [`export_with_specs`] function allows exporting metrics with configurable
//! breakdowns using the [`MetricSpec`](crate::metric_spec::MetricSpec) syntax:
//!
//! ```text
//! mcsim.radio.rx_packets/node/packet_type
//! ```

use crate::metric_spec::{
    BreakdownLabel, CounterValue, GaugeValue, HistogramSummary as SpecHistogramSummary,
    HistogramValue, MetricSpec, MetricValue, MetricsExport,
};
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ============================================================================
// JSON Sorting Utilities
// ============================================================================

/// Recursively sort all JSON objects by key to ensure consistent output order.
fn sort_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            // Sort object keys and recursively sort values
            let mut sorted_entries: Vec<_> = map.iter().collect();
            sorted_entries.sort_by(|a, b| a.0.cmp(b.0));
            
            let mut sorted_map = serde_json::Map::new();
            for (key, val) in sorted_entries {
                sorted_map.insert(key.clone(), sort_json_value(val));
            }
            serde_json::Value::Object(sorted_map)
        }
        serde_json::Value::Array(arr) => {
            // Recursively sort array elements
            serde_json::Value::Array(arr.iter().map(sort_json_value).collect())
        }
        other => other.clone(),
    }
}

// ============================================================================
// Metrics Snapshot Types
// ============================================================================

/// Per-node metric values.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct NodeMetrics {
    /// Counter metrics for this node.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub counters: HashMap<String, u64>,
    /// Gauge metrics for this node.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub gauges: HashMap<String, f64>,
    /// Histogram metrics for this node.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub histograms: HashMap<String, HistogramSummary>,
}

/// Collected metric data for export.
#[derive(Debug, serde::Serialize)]
pub struct MetricsSnapshot {
    /// Timestamp when metrics were collected.
    pub timestamp: String,
    /// Counter metrics (name -> value) - aggregated across all nodes.
    pub counters: HashMap<String, u64>,
    /// Gauge metrics (name -> value) - aggregated across all nodes.
    pub gauges: HashMap<String, f64>,
    /// Histogram metrics (name -> summary stats) - aggregated across all nodes.
    pub histograms: HashMap<String, HistogramSummary>,
    /// Per-node breakdown of metrics.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub nodes: HashMap<String, NodeMetrics>,
}

/// Summary statistics for a histogram metric.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistogramSummary {
    /// Number of samples recorded.
    pub count: u64,
    /// Sum of all samples.
    pub sum: f64,
    /// Minimum value.
    pub min: f64,
    /// Maximum value.
    pub max: f64,
    /// Mean value.
    pub mean: f64,
    /// 50th percentile (median).
    pub p50: f64,
    /// 90th percentile.
    pub p90: f64,
    /// 99th percentile.
    pub p99: f64,
}

// ============================================================================
// Export Functions
// ============================================================================

/// Export metrics as JSON.
pub fn export_json<W: Write>(snapshot: &MetricsSnapshot, writer: &mut W) -> std::io::Result<()> {
    // Sort snapshot for consistent output order
    let sorted_snapshot = serde_json::to_value(snapshot)?;
    let sorted = sort_json_value(&sorted_snapshot);
    serde_json::to_writer_pretty(&mut *writer, &sorted)?;
    writeln!(writer)?;
    Ok(())
}

/// Export metrics with configurable breakdowns as JSON.
///
/// This function exports metrics according to the provided specifications,
/// allowing flexible breakdown by various labels (node, node_type, group, packet_type).
///
/// # Arguments
///
/// * `export` - The metrics export containing metrics with their breakdowns.
/// * `writer` - The writer to output JSON to.
pub fn export_json_with_specs<W: Write>(export: &MetricsExport, writer: &mut W) -> std::io::Result<()> {
    // Sort export for consistent output order
    let sorted_export = serde_json::to_value(export)?;
    let sorted = sort_json_value(&sorted_export);
    serde_json::to_writer_pretty(&mut *writer, &sorted)?;
    writeln!(writer)?;
    Ok(())
}

/// Export metrics in Prometheus text exposition format.
pub fn export_prometheus<W: Write>(
    snapshot: &MetricsSnapshot,
    writer: &mut W,
) -> std::io::Result<()> {
    // Counters - aggregated (no labels)
    for (name, value) in &snapshot.counters {
        let prom_name = name.replace('.', "_");
        writeln!(writer, "# TYPE {} counter", prom_name)?;
        writeln!(writer, "{} {}", prom_name, value)?;
    }

    // Counters - per node
    for (node_name, node_metrics) in &snapshot.nodes {
        for (name, value) in &node_metrics.counters {
            let prom_name = name.replace('.', "_");
            writeln!(writer, "{}{{node=\"{}\"}} {}", prom_name, node_name, value)?;
        }
    }

    // Gauges - aggregated
    for (name, value) in &snapshot.gauges {
        let prom_name = name.replace('.', "_");
        writeln!(writer, "# TYPE {} gauge", prom_name)?;
        writeln!(writer, "{} {}", prom_name, value)?;
    }

    // Gauges - per node
    for (node_name, node_metrics) in &snapshot.nodes {
        for (name, value) in &node_metrics.gauges {
            let prom_name = name.replace('.', "_");
            writeln!(writer, "{}{{node=\"{}\"}} {}", prom_name, node_name, value)?;
        }
    }

    // Histograms (as summary with quantiles) - aggregated
    for (name, summary) in &snapshot.histograms {
        let prom_name = name.replace('.', "_");
        writeln!(writer, "# TYPE {} summary", prom_name)?;
        writeln!(writer, "{}_count {}", prom_name, summary.count)?;
        writeln!(writer, "{}_sum {}", prom_name, summary.sum)?;
        writeln!(writer, "{}{{quantile=\"0.5\"}} {}", prom_name, summary.p50)?;
        writeln!(writer, "{}{{quantile=\"0.9\"}} {}", prom_name, summary.p90)?;
        writeln!(writer, "{}{{quantile=\"0.99\"}} {}", prom_name, summary.p99)?;
    }

    // Histograms - per node
    for (node_name, node_metrics) in &snapshot.nodes {
        for (name, summary) in &node_metrics.histograms {
            let prom_name = name.replace('.', "_");
            writeln!(writer, "{}_count{{node=\"{}\"}} {}", prom_name, node_name, summary.count)?;
            writeln!(writer, "{}_sum{{node=\"{}\"}} {}", prom_name, node_name, summary.sum)?;
            writeln!(writer, "{}{{node=\"{}\",quantile=\"0.5\"}} {}", prom_name, node_name, summary.p50)?;
            writeln!(writer, "{}{{node=\"{}\",quantile=\"0.9\"}} {}", prom_name, node_name, summary.p90)?;
            writeln!(writer, "{}{{node=\"{}\",quantile=\"0.99\"}} {}", prom_name, node_name, summary.p99)?;
        }
    }

    Ok(())
}

/// Export metrics in CSV format with hierarchical paths.
///
/// The first column is "path" which represents the hierarchical breakdown path.
/// Each metric becomes a column. Rows represent different breakdown paths.
///
/// # Path Format
///
/// - `/` - Root (total/aggregated value)
/// - `/nodename` - Per-node breakdown
/// - `/repeater/repeatername` - Nested breakdowns (e.g., by_node_type/by_node)
///
/// # Arguments
///
/// * `export` - The metrics export containing metrics with their breakdowns.
/// * `writer` - The writer to output CSV to.
pub fn export_csv_with_specs<W: Write>(export: &MetricsExport, writer: &mut W) -> std::io::Result<()> {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    // Collect all unique paths and their metric values
    // We use BTreeMap/BTreeSet for deterministic ordering
    let mut all_paths: BTreeSet<String> = BTreeSet::new();
    let mut metric_names: BTreeSet<String> = BTreeSet::new();
    let mut data: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();

    // Process each metric
    for (metric_name, metric_value) in &export.metrics {
        metric_names.insert(metric_name.clone());
        collect_csv_rows(metric_name, metric_value, "/".to_string(), &mut all_paths, &mut data);
    }

    // Write header row
    write!(writer, "path")?;
    for metric_name in &metric_names {
        write!(writer, ",{}", escape_csv_field(metric_name))?;
    }
    writeln!(writer)?;

    // Write data rows
    for path in &all_paths {
        write!(writer, "{}", escape_csv_field(path))?;
        for metric_name in &metric_names {
            let value = data
                .get(path)
                .and_then(|m| m.get(metric_name))
                .map(|s| s.as_str())
                .unwrap_or("");
            write!(writer, ",{}", value)?;
        }
        writeln!(writer)?;
    }

    Ok(())
}

/// Escape a CSV field if it contains special characters.
fn escape_csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Recursively collect CSV rows from a metric value's breakdown hierarchy.
fn collect_csv_rows(
    metric_name: &str,
    value: &MetricValue,
    path: String,
    all_paths: &mut std::collections::BTreeSet<String>,
    data: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) {
    // Add the current path with its value
    all_paths.insert(path.clone());
    let path_data = data.entry(path.clone()).or_default();

    // Get the total value as a string
    let total_str = match value {
        MetricValue::Counter(c) => c.total.to_string(),
        MetricValue::Gauge(g) => format_float(g.total),
        MetricValue::Histogram(h) => {
            // For histograms, use the mean as the primary value
            format_float(h.total.mean)
        }
    };
    path_data.insert(metric_name.to_string(), total_str);

    // Process breakdowns recursively
    match value {
        MetricValue::Counter(c) => {
            collect_counter_breakdowns(metric_name, c, &path, all_paths, data);
        }
        MetricValue::Gauge(g) => {
            collect_gauge_breakdowns(metric_name, g, &path, all_paths, data);
        }
        MetricValue::Histogram(h) => {
            collect_histogram_breakdowns(metric_name, h, &path, all_paths, data);
        }
    }
}

/// Collect breakdown rows for a counter value.
fn collect_counter_breakdowns(
    metric_name: &str,
    value: &CounterValue,
    parent_path: &str,
    all_paths: &mut std::collections::BTreeSet<String>,
    data: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) {
    // Process each label's breakdown
    for (_label_name, label_values) in &value.labels {
        for (name, child) in label_values {
            let child_path = format_child_path(parent_path, name);
            collect_counter_breakdown_recursive(metric_name, child, child_path, all_paths, data);
        }
    }
}

/// Recursively collect counter breakdown rows.
fn collect_counter_breakdown_recursive(
    metric_name: &str,
    value: &CounterValue,
    path: String,
    all_paths: &mut std::collections::BTreeSet<String>,
    data: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) {
    all_paths.insert(path.clone());
    let path_data = data.entry(path.clone()).or_default();
    path_data.insert(metric_name.to_string(), value.total.to_string());
    collect_counter_breakdowns(metric_name, value, &path, all_paths, data);
}

/// Collect breakdown rows for a gauge value.
fn collect_gauge_breakdowns(
    metric_name: &str,
    value: &GaugeValue,
    parent_path: &str,
    all_paths: &mut std::collections::BTreeSet<String>,
    data: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) {
    // Process each label's breakdown
    for (_label_name, label_values) in &value.labels {
        for (name, child) in label_values {
            let child_path = format_child_path(parent_path, name);
            collect_gauge_breakdown_recursive(metric_name, child, child_path, all_paths, data);
        }
    }
}

/// Recursively collect gauge breakdown rows.
fn collect_gauge_breakdown_recursive(
    metric_name: &str,
    value: &GaugeValue,
    path: String,
    all_paths: &mut std::collections::BTreeSet<String>,
    data: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) {
    all_paths.insert(path.clone());
    let path_data = data.entry(path.clone()).or_default();
    path_data.insert(metric_name.to_string(), format_float(value.total));
    collect_gauge_breakdowns(metric_name, value, &path, all_paths, data);
}

/// Collect breakdown rows for a histogram value.
fn collect_histogram_breakdowns(
    metric_name: &str,
    value: &HistogramValue,
    parent_path: &str,
    all_paths: &mut std::collections::BTreeSet<String>,
    data: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) {
    // Process each label's breakdown
    for (_label_name, label_values) in &value.labels {
        for (name, child) in label_values {
            let child_path = format_child_path(parent_path, name);
            collect_histogram_breakdown_recursive(metric_name, child, child_path, all_paths, data);
        }
    }
}

/// Recursively collect histogram breakdown rows.
fn collect_histogram_breakdown_recursive(
    metric_name: &str,
    value: &HistogramValue,
    path: String,
    all_paths: &mut std::collections::BTreeSet<String>,
    data: &mut std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
) {
    all_paths.insert(path.clone());
    let path_data = data.entry(path.clone()).or_default();
    path_data.insert(metric_name.to_string(), format_float(value.total.mean));
    collect_histogram_breakdowns(metric_name, value, &path, all_paths, data);
}

/// Format a child path by appending a name to the parent path.
fn format_child_path(parent_path: &str, name: &str) -> String {
    if parent_path == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", parent_path, name)
    }
}

/// Format a float value for CSV output.
fn format_float(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{:.0}", value)
    } else {
        format!("{}", value)
    }
}

// ============================================================================
// In-Memory Recorder
// ============================================================================

/// Thread-safe storage for a single counter value.
#[derive(Debug, Default)]
struct CounterState {
    value: AtomicU64,
}

impl CounterState {
    fn increment(&self, value: u64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Thread-safe storage for a single gauge value.
/// Uses AtomicU64 to store f64 bits.
#[derive(Debug, Default)]
struct GaugeState {
    value: AtomicU64,
}

impl GaugeState {
    fn set(&self, value: f64) {
        self.value
            .store(value.to_bits(), Ordering::Relaxed);
    }

    fn increment(&self, value: f64) {
        loop {
            let current = self.value.load(Ordering::Relaxed);
            let current_f64 = f64::from_bits(current);
            let new_value = current_f64 + value;
            if self
                .value
                .compare_exchange_weak(
                    current,
                    new_value.to_bits(),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                break;
            }
        }
    }

    fn decrement(&self, value: f64) {
        self.increment(-value);
    }

    fn get(&self) -> f64 {
        f64::from_bits(self.value.load(Ordering::Relaxed))
    }
}

/// Maximum number of samples to retain in reservoir for percentile calculation.
/// This bounds memory usage while maintaining statistically accurate percentiles
/// via reservoir sampling.
const HISTOGRAM_RESERVOIR_SIZE: usize = 10_000;

/// Thread-safe storage for histogram samples using reservoir sampling.
/// 
/// Uses Algorithm R (Vitter, 1985) to maintain a representative sample of
/// at most `HISTOGRAM_RESERVOIR_SIZE` elements while seeing an arbitrary
/// number of samples. This bounds memory usage to O(HISTOGRAM_RESERVOIR_SIZE)
/// while providing statistically accurate percentile estimates.
#[derive(Debug)]
struct HistogramState {
    /// Reservoir of samples for percentile calculation.
    reservoir: RwLock<Vec<f64>>,
    /// Total count of samples seen (may exceed reservoir size).
    count: AtomicU64,
    /// Running sum of all samples for accurate mean calculation.
    sum: RwLock<f64>,
    /// Minimum value seen.
    min: RwLock<f64>,
    /// Maximum value seen.
    max: RwLock<f64>,
    /// RNG state for reservoir sampling (simple xorshift).
    rng_state: AtomicU64,
}

impl Default for HistogramState {
    fn default() -> Self {
        Self {
            reservoir: RwLock::new(Vec::with_capacity(HISTOGRAM_RESERVOIR_SIZE)),
            count: AtomicU64::new(0),
            sum: RwLock::new(0.0),
            min: RwLock::new(f64::MAX),
            max: RwLock::new(f64::MIN),
            // Initialize RNG with a non-zero seed based on address
            rng_state: AtomicU64::new(0x12345678_9ABCDEF0),
        }
    }
}

impl HistogramState {
    /// Simple xorshift64 PRNG for reservoir sampling.
    fn next_random(&self) -> u64 {
        loop {
            let mut state = self.rng_state.load(Ordering::Relaxed);
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            if self.rng_state.compare_exchange_weak(
                self.rng_state.load(Ordering::Relaxed),
                state,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ).is_ok() {
                return state;
            }
        }
    }

    fn record(&self, value: f64) {
        let n = self.count.fetch_add(1, Ordering::Relaxed) + 1;
        
        // Update sum, min, max
        {
            let mut sum = self.sum.write();
            *sum += value;
        }
        {
            let mut min = self.min.write();
            if value < *min {
                *min = value;
            }
        }
        {
            let mut max = self.max.write();
            if value > *max {
                *max = value;
            }
        }
        
        // Reservoir sampling (Algorithm R)
        let mut reservoir = self.reservoir.write();
        if (n as usize) <= HISTOGRAM_RESERVOIR_SIZE {
            // Fill reservoir until full
            reservoir.push(value);
        } else {
            // Randomly replace elements with decreasing probability
            let j = (self.next_random() % n) as usize;
            if j < HISTOGRAM_RESERVOIR_SIZE {
                reservoir[j] = value;
            }
        }
    }

    fn summary(&self) -> HistogramSummary {
        let count = self.count.load(Ordering::Relaxed);
        if count == 0 {
            return HistogramSummary {
                count: 0,
                sum: 0.0,
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                p50: 0.0,
                p90: 0.0,
                p99: 0.0,
            };
        }

        let sum = *self.sum.read();
        let min = *self.min.read();
        let max = *self.max.read();
        let mean = sum / count as f64;

        // Calculate percentiles from reservoir
        let reservoir = self.reservoir.read();
        let mut sorted: Vec<f64> = reservoir.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let percentile = |p: f64| -> f64 {
            if sorted.is_empty() {
                return 0.0;
            }
            let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
            sorted[idx.min(sorted.len() - 1)]
        };

        HistogramSummary {
            count,
            sum,
            min: if min == f64::MAX { 0.0 } else { min },
            max: if max == f64::MIN { 0.0 } else { max },
            mean,
            p50: percentile(50.0),
            p90: percentile(90.0),
            p99: percentile(99.0),
        }
    }

    /// Get a clone of the current reservoir samples.
    fn samples(&self) -> Vec<f64> {
        self.reservoir.read().clone()
    }

    /// Clear all samples and reset statistics.
    fn clear(&self) {
        self.reservoir.write().clear();
        self.count.store(0, Ordering::Relaxed);
        *self.sum.write() = 0.0;
        *self.min.write() = f64::MAX;
        *self.max.write() = f64::MIN;
    }
}

/// Shared state for the in-memory recorder.
#[derive(Debug, Default)]
struct RecorderState {
    /// Counters keyed by full key string (metric name + labels).
    counters: RwLock<HashMap<String, Arc<CounterState>>>,
    /// Gauges keyed by full key string (metric name + labels).
    gauges: RwLock<HashMap<String, Arc<GaugeState>>>,
    /// Histograms keyed by full key string (metric name + labels).
    histograms: RwLock<HashMap<String, Arc<HistogramState>>>,
    /// Mapping from full key to (metric_name, labels) for breakdown.
    key_metadata: RwLock<HashMap<String, KeyMetadata>>,
    /// Metric specs for label filtering. When set, only labels matching the
    /// breakdown specifications will be stored, reducing memory usage.
    label_specs: RwLock<Vec<MetricSpec>>,
}

/// Metadata about a metric key for breakdown purposes.
#[derive(Debug, Clone)]
struct KeyMetadata {
    /// The metric name without labels.
    name: String,
    /// Labels as key-value pairs.
    labels: Vec<(String, String)>,
}

impl KeyMetadata {
    /// Get the "node" label value if present.
    fn node(&self) -> Option<&str> {
        self.label_value("node")
    }

    /// Get the "node_type" label value if present.
    fn node_type(&self) -> Option<&str> {
        self.label_value("node_type")
    }

    /// Get the "groups" label value if present (comma-separated).
    fn groups(&self) -> Vec<&str> {
        self.label_value("groups")
            .map(|v| v.split(',').collect())
            .unwrap_or_default()
    }

    /// Get the "route_type" label value if present.
    fn route_type(&self) -> Option<&str> {
        self.label_value("route_type")
    }

    /// Get the "payload_type" label value if present.
    fn payload_type(&self) -> Option<&str> {
        self.label_value("payload_type")
    }

    /// Get the "payload_hash" label value if present.
    fn payload_hash(&self) -> Option<&str> {
        self.label_value("payload_hash")
    }

    /// Get any label value by key.
    fn label_value(&self, key: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Get the value for a breakdown label.
    fn breakdown_value(&self, label: BreakdownLabel) -> Option<&str> {
        match label {
            BreakdownLabel::Node => self.node(),
            BreakdownLabel::NodeType => self.node_type(),
            BreakdownLabel::Group => None, // Groups handled specially (multiple values)
            BreakdownLabel::RouteType => self.route_type(),
            BreakdownLabel::PayloadType => self.payload_type(),
            BreakdownLabel::PayloadHash => self.payload_hash(),
        }
    }

    /// Get all breakdown values for a label (handles multi-value case like groups).
    fn breakdown_values(&self, label: BreakdownLabel) -> Vec<&str> {
        match label {
            BreakdownLabel::Group => self.groups(),
            _ => self.breakdown_value(label).into_iter().collect(),
        }
    }

    /// Get all breakdown labels that have values in this metadata.
    fn available_breakdown_labels(&self) -> Vec<BreakdownLabel> {
        let mut labels = Vec::new();
        if self.node().is_some() {
            labels.push(BreakdownLabel::Node);
        }
        if self.node_type().is_some() {
            labels.push(BreakdownLabel::NodeType);
        }
        if !self.groups().is_empty() {
            labels.push(BreakdownLabel::Group);
        }
        if self.route_type().is_some() {
            labels.push(BreakdownLabel::RouteType);
        }
        if self.payload_type().is_some() {
            labels.push(BreakdownLabel::PayloadType);
        }
        if self.payload_hash().is_some() {
            labels.push(BreakdownLabel::PayloadHash);
        }
        labels
    }
}

/// Build a unique key string from a metrics Key (including labels).
fn key_to_string(key: &Key) -> String {
    let labels: Vec<String> = key
        .labels()
        .map(|l| format!("{}={}", l.key(), l.value()))
        .collect();
    
    if labels.is_empty() {
        key.name().to_string()
    } else {
        format!("{}|{}", key.name(), labels.join(","))
    }
}

/// Build a key string with labels filtered based on metric specs.
///
/// If specs are provided, only labels that match a breakdown in any matching spec
/// will be included. Labels not mentioned in any matching spec are dropped.
/// If no specs match or specs is empty, all labels are included.
fn key_to_string_filtered(key: &Key, specs: &[MetricSpec]) -> String {
    let metric_name = key.name();
    
    // Find all specs that match this metric
    let matching_specs: Vec<&MetricSpec> = specs
        .iter()
        .filter(|s| s.matches(metric_name))
        .collect();
    
    // If no specs match, keep all labels (default behavior)
    if matching_specs.is_empty() {
        return key_to_string(key);
    }
    
    // Collect all allowed labels from all matching specs
    let mut allowed_labels: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut has_breakdown_all = false;
    
    for spec in &matching_specs {
        if spec.breakdown_all {
            // If any spec has breakdown_all, include all labels
            has_breakdown_all = true;
            break;
        }
        for breakdown in &spec.breakdowns {
            allowed_labels.insert(breakdown.label_key());
        }
    }
    
    // If breakdown_all, keep all labels
    if has_breakdown_all {
        return key_to_string(key);
    }
    
    // Filter labels to only include allowed ones
    let labels: Vec<String> = key
        .labels()
        .filter(|l| allowed_labels.contains(l.key()))
        .map(|l| format!("{}={}", l.key(), l.value()))
        .collect();
    
    if labels.is_empty() {
        metric_name.to_string()
    } else {
        format!("{}|{}", metric_name, labels.join(","))
    }
}

/// Build filtered metadata from a metrics Key, respecting label specs.
fn key_metadata_filtered(key: &Key, specs: &[MetricSpec]) -> KeyMetadata {
    let metric_name = key.name();
    
    // Find all specs that match this metric
    let matching_specs: Vec<&MetricSpec> = specs
        .iter()
        .filter(|s| s.matches(metric_name))
        .collect();
    
    // If no specs match, keep all labels
    if matching_specs.is_empty() {
        return key_metadata(key);
    }
    
    // Collect all allowed labels from all matching specs
    let mut allowed_labels: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut has_breakdown_all = false;
    
    for spec in &matching_specs {
        if spec.breakdown_all {
            has_breakdown_all = true;
            break;
        }
        for breakdown in &spec.breakdowns {
            allowed_labels.insert(breakdown.label_key());
        }
    }
    
    // If breakdown_all, keep all labels
    if has_breakdown_all {
        return key_metadata(key);
    }
    
    // Filter labels
    let labels: Vec<(String, String)> = key
        .labels()
        .filter(|l| allowed_labels.contains(l.key()))
        .map(|l| (l.key().to_string(), l.value().to_string()))
        .collect();
    
    KeyMetadata {
        name: metric_name.to_string(),
        labels,
    }
}

/// Extract metadata from a metrics Key.
fn key_metadata(key: &Key) -> KeyMetadata {
    KeyMetadata {
        name: key.name().to_string(),
        labels: key
            .labels()
            .map(|l| (l.key().to_string(), l.value().to_string()))
            .collect(),
    }
}

impl RecorderState {
    /// Clear all recorded metrics.
    ///
    /// This resets all counters to 0, all gauges to 0, and clears all histogram samples.
    /// The metric keys and metadata are preserved so new recordings use the same handles.
    fn clear(&self) {
        // Reset all counters to 0
        for counter in self.counters.read().values() {
            counter.value.store(0, Ordering::Relaxed);
        }
        
        // Reset all gauges to 0
        for gauge in self.gauges.read().values() {
            gauge.value.store(0.0f64.to_bits(), Ordering::Relaxed);
        }
        
        // Clear all histogram samples
        for histogram in self.histograms.read().values() {
            histogram.clear();
        }
    }

    fn get_or_create_counter(&self, key: &Key) -> Arc<CounterState> {
        let specs = self.label_specs.read();
        let key_str = key_to_string_filtered(key, &specs);
        
        {
            let counters = self.counters.read();
            if let Some(counter) = counters.get(&key_str) {
                return counter.clone();
            }
        }

        // Store metadata for this key (also filtered)
        {
            let mut metadata = self.key_metadata.write();
            metadata.entry(key_str.clone()).or_insert_with(|| key_metadata_filtered(key, &specs));
        }

        let mut counters = self.counters.write();
        counters
            .entry(key_str)
            .or_insert_with(|| Arc::new(CounterState::default()))
            .clone()
    }

    fn get_or_create_gauge(&self, key: &Key) -> Arc<GaugeState> {
        let specs = self.label_specs.read();
        let key_str = key_to_string_filtered(key, &specs);
        
        {
            let gauges = self.gauges.read();
            if let Some(gauge) = gauges.get(&key_str) {
                return gauge.clone();
            }
        }

        // Store metadata for this key (also filtered)
        {
            let mut metadata = self.key_metadata.write();
            metadata.entry(key_str.clone()).or_insert_with(|| key_metadata_filtered(key, &specs));
        }

        let mut gauges = self.gauges.write();
        gauges
            .entry(key_str)
            .or_insert_with(|| Arc::new(GaugeState::default()))
            .clone()
    }

    fn get_or_create_histogram(&self, key: &Key) -> Arc<HistogramState> {
        let specs = self.label_specs.read();
        let key_str = key_to_string_filtered(key, &specs);
        
        {
            let histograms = self.histograms.read();
            if let Some(histogram) = histograms.get(&key_str) {
                return histogram.clone();
            }
        }

        // Store metadata for this key (also filtered)
        {
            let mut metadata = self.key_metadata.write();
            metadata.entry(key_str.clone()).or_insert_with(|| key_metadata_filtered(key, &specs));
        }

        let mut histograms = self.histograms.write();
        histograms
            .entry(key_str)
            .or_insert_with(|| Arc::new(HistogramState::default()))
            .clone()
    }

    fn snapshot(&self) -> MetricsSnapshot {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let metadata = self.key_metadata.read();

        // Build aggregated metrics and per-node breakdown
        let mut agg_counters: HashMap<String, u64> = HashMap::new();
        let mut agg_gauges: HashMap<String, f64> = HashMap::new();
        let mut agg_histograms: HashMap<String, Vec<f64>> = HashMap::new();
        let mut nodes: HashMap<String, NodeMetrics> = HashMap::new();

        // Process counters
        for (key_str, counter) in self.counters.read().iter() {
            let value = counter.get();
            if let Some(meta) = metadata.get(key_str) {
                // Aggregate by metric name
                *agg_counters.entry(meta.name.clone()).or_insert(0) += value;
                
                // Per-node breakdown
                if let Some(node) = meta.node() {
                    let node_metrics = nodes.entry(node.to_string()).or_default();
                    *node_metrics.counters.entry(meta.name.clone()).or_insert(0) += value;
                }
            }
        }

        // Process gauges (use last value for aggregation - gauges don't sum)
        for (key_str, gauge) in self.gauges.read().iter() {
            let value = gauge.get();
            if let Some(meta) = metadata.get(key_str) {
                // For gauges, we'll use the last value (or could average - using last here)
                agg_gauges.insert(meta.name.clone(), value);
                
                // Per-node breakdown
                if let Some(node) = meta.node() {
                    let node_metrics = nodes.entry(node.to_string()).or_default();
                    node_metrics.gauges.insert(meta.name.clone(), value);
                }
            }
        }

        // Process histograms - collect all samples for aggregated summary
        for (key_str, histogram) in self.histograms.read().iter() {
            if let Some(meta) = metadata.get(key_str) {
                // Collect samples for aggregation
                let samples = histogram.samples();
                agg_histograms
                    .entry(meta.name.clone())
                    .or_default()
                    .extend(samples.clone());
                
                // Per-node breakdown
                if let Some(node) = meta.node() {
                    let node_metrics = nodes.entry(node.to_string()).or_default();
                    node_metrics.histograms.insert(meta.name.clone(), histogram.summary());
                }
            }
        }

        // Convert aggregated histogram samples to summaries
        let histograms: HashMap<String, HistogramSummary> = agg_histograms
            .into_iter()
            .map(|(name, samples)| {
                let summary = compute_histogram_summary(&samples);
                (name, summary)
            })
            .collect();

        MetricsSnapshot {
            timestamp,
            counters: agg_counters,
            gauges: agg_gauges,
            histograms,
            nodes,
        }
    }

    /// Take a snapshot with configurable breakdowns based on metric specs.
    fn snapshot_with_specs(&self, specs: &[MetricSpec]) -> MetricsExport {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let metadata = self.key_metadata.read();

        // Collect raw data keyed by (metric_name, labels)
        let mut counter_data: HashMap<String, Vec<(KeyMetadata, u64)>> = HashMap::new();
        let mut gauge_data: HashMap<String, Vec<(KeyMetadata, f64)>> = HashMap::new();
        let mut histogram_data: HashMap<String, Vec<(KeyMetadata, Vec<f64>)>> = HashMap::new();

        // Collect counters
        for (key_str, counter) in self.counters.read().iter() {
            if let Some(meta) = metadata.get(key_str) {
                counter_data
                    .entry(meta.name.clone())
                    .or_default()
                    .push((meta.clone(), counter.get()));
            }
        }

        // Collect gauges
        for (key_str, gauge) in self.gauges.read().iter() {
            if let Some(meta) = metadata.get(key_str) {
                gauge_data
                    .entry(meta.name.clone())
                    .or_default()
                    .push((meta.clone(), gauge.get()));
            }
        }

        // Collect histograms
        for (key_str, histogram) in self.histograms.read().iter() {
            if let Some(meta) = metadata.get(key_str) {
                histogram_data
                    .entry(meta.name.clone())
                    .or_default()
                    .push((meta.clone(), histogram.samples()));
            }
        }

        // Build result based on specs
        let mut metrics: HashMap<String, MetricValue> = HashMap::new();

        // Determine which metrics match which specs
        let all_metric_names: std::collections::HashSet<_> = counter_data
            .keys()
            .chain(gauge_data.keys())
            .chain(histogram_data.keys())
            .collect();

        // If no specs provided, default to all metrics with totals only
        let default_spec;
        let specs = if specs.is_empty() {
            default_spec = vec![MetricSpec::parse("*").unwrap()];
            &default_spec[..]
        } else {
            specs
        };

        for metric_name in all_metric_names {
            // Find ALL matching specs for this metric
            let matching_specs: Vec<_> = specs.iter().filter(|s| s.matches(metric_name)).collect();
            if matching_specs.is_empty() {
                continue;
            }

            // Process based on metric type, merging all breakdown paths
            if let Some(data) = counter_data.get(metric_name) {
                let mut value = CounterValue::new(data.iter().map(|(_, v)| v).sum());
                for spec in &matching_specs {
                    let breakdowns = if spec.breakdown_all {
                        collect_available_labels_counter(data)
                    } else {
                        spec.breakdowns.clone()
                    };
                    apply_counter_breakdowns(&mut value, data, &breakdowns, 0);
                }
                metrics.insert(metric_name.clone(), MetricValue::Counter(value));
            } else if let Some(data) = gauge_data.get(metric_name) {
                let total = if data.is_empty() {
                    0.0
                } else {
                    data.iter().map(|(_, v)| v).sum::<f64>() / data.len() as f64
                };
                let mut value = GaugeValue::new(total);
                for spec in &matching_specs {
                    let breakdowns = if spec.breakdown_all {
                        collect_available_labels_gauge(data)
                    } else {
                        spec.breakdowns.clone()
                    };
                    apply_gauge_breakdowns(&mut value, data, &breakdowns, 0);
                }
                metrics.insert(metric_name.clone(), MetricValue::Gauge(value));
            } else if let Some(data) = histogram_data.get(metric_name) {
                let all_samples: Vec<f64> = data.iter().flat_map(|(_, s)| s.clone()).collect();
                let mut value = HistogramValue::new(compute_spec_histogram_summary(&all_samples));
                for spec in &matching_specs {
                    let breakdowns = if spec.breakdown_all {
                        collect_available_labels_histogram(data)
                    } else {
                        spec.breakdowns.clone()
                    };
                    apply_histogram_breakdowns(&mut value, data, &breakdowns, 0);
                }
                metrics.insert(metric_name.clone(), MetricValue::Histogram(value));
            }
        }

        MetricsExport { timestamp, metrics }
    }
}

// ============================================================================
// Apply Breakdowns (for merging multiple specs)
// ============================================================================

/// Collect all available breakdown labels from counter data.
fn collect_available_labels_counter(data: &[(KeyMetadata, u64)]) -> Vec<BreakdownLabel> {
    let mut labels = std::collections::HashSet::new();
    for (meta, _) in data {
        for label in meta.available_breakdown_labels() {
            labels.insert(label);
        }
    }
    // Return in the preferred nesting order: network, group, node_type, node, route_type, payload_type, payload_hash
    let mut result: Vec<_> = labels.into_iter().collect();
    result.sort_by_key(|l| l.sort_order());
    result
}

/// Collect all available breakdown labels from gauge data.
fn collect_available_labels_gauge(data: &[(KeyMetadata, f64)]) -> Vec<BreakdownLabel> {
    let mut labels = std::collections::HashSet::new();
    for (meta, _) in data {
        for label in meta.available_breakdown_labels() {
            labels.insert(label);
        }
    }
    // Return in the preferred nesting order: network, group, node_type, node, route_type, payload_type, payload_hash
    let mut result: Vec<_> = labels.into_iter().collect();
    result.sort_by_key(|l| l.sort_order());
    result
}

/// Collect all available breakdown labels from histogram data.
fn collect_available_labels_histogram(data: &[(KeyMetadata, Vec<f64>)]) -> Vec<BreakdownLabel> {
    let mut labels = std::collections::HashSet::new();
    for (meta, _) in data {
        for label in meta.available_breakdown_labels() {
            labels.insert(label);
        }
    }
    // Return in the preferred nesting order: network, group, node_type, node, route_type, payload_type, payload_hash
    let mut result: Vec<_> = labels.into_iter().collect();
    result.sort_by_key(|l| l.sort_order());
    result
}

/// Apply counter breakdowns to an existing value (for merging multiple specs).
fn apply_counter_breakdowns(
    value: &mut CounterValue,
    data: &[(KeyMetadata, u64)],
    breakdowns: &[BreakdownLabel],
    depth: usize,
) {
    if depth >= breakdowns.len() {
        return;
    }

    let label = breakdowns[depth];
    let mut grouped: HashMap<String, Vec<(KeyMetadata, u64)>> = HashMap::new();

    for (meta, v) in data {
        let keys = meta.breakdown_values(label);
        if keys.is_empty() {
            grouped.entry("_unknown_".to_string()).or_default().push((meta.clone(), *v));
        } else {
            for key in keys {
                grouped.entry(key.to_string()).or_default().push((meta.clone(), *v));
            }
        }
    }

    // Skip if no real data for this breakdown
    if grouped.len() == 1 && grouped.contains_key("_unknown_") {
        return;
    }
    grouped.remove("_unknown_");

    if !grouped.is_empty() {
        let breakdown_map = value.get_or_create_breakdown(label);
        for (key, sub_data) in grouped {
            let sub_total: u64 = sub_data.iter().map(|(_, v)| v).sum();
            let sub_value = breakdown_map
                .entry(key)
                .or_insert_with(|| Box::new(CounterValue::new(sub_total)));
            // Recursively apply remaining breakdowns
            apply_counter_breakdowns(sub_value, &sub_data, breakdowns, depth + 1);
        }
    }
}

/// Apply gauge breakdowns to an existing value (for merging multiple specs).
fn apply_gauge_breakdowns(
    value: &mut GaugeValue,
    data: &[(KeyMetadata, f64)],
    breakdowns: &[BreakdownLabel],
    depth: usize,
) {
    if depth >= breakdowns.len() {
        return;
    }

    let label = breakdowns[depth];
    let mut grouped: HashMap<String, Vec<(KeyMetadata, f64)>> = HashMap::new();

    for (meta, v) in data {
        let keys = meta.breakdown_values(label);
        if keys.is_empty() {
            grouped.entry("_unknown_".to_string()).or_default().push((meta.clone(), *v));
        } else {
            for key in keys {
                grouped.entry(key.to_string()).or_default().push((meta.clone(), *v));
            }
        }
    }

    if grouped.len() == 1 && grouped.contains_key("_unknown_") {
        return;
    }
    grouped.remove("_unknown_");

    if !grouped.is_empty() {
        let breakdown_map = value.get_or_create_breakdown(label);
        for (key, sub_data) in grouped {
            let sub_total = if sub_data.is_empty() {
                0.0
            } else {
                sub_data.iter().map(|(_, v)| v).sum::<f64>() / sub_data.len() as f64
            };
            let sub_value = breakdown_map
                .entry(key)
                .or_insert_with(|| Box::new(GaugeValue::new(sub_total)));
            apply_gauge_breakdowns(sub_value, &sub_data, breakdowns, depth + 1);
        }
    }
}

/// Apply histogram breakdowns to an existing value (for merging multiple specs).
fn apply_histogram_breakdowns(
    value: &mut HistogramValue,
    data: &[(KeyMetadata, Vec<f64>)],
    breakdowns: &[BreakdownLabel],
    depth: usize,
) {
    if depth >= breakdowns.len() {
        return;
    }

    let label = breakdowns[depth];
    let mut grouped: HashMap<String, Vec<(KeyMetadata, Vec<f64>)>> = HashMap::new();

    for (meta, samples) in data {
        let keys = meta.breakdown_values(label);
        if keys.is_empty() {
            grouped
                .entry("_unknown_".to_string())
                .or_default()
                .push((meta.clone(), samples.clone()));
        } else {
            for key in keys {
                grouped
                    .entry(key.to_string())
                    .or_default()
                    .push((meta.clone(), samples.clone()));
            }
        }
    }

    if grouped.len() == 1 && grouped.contains_key("_unknown_") {
        return;
    }
    grouped.remove("_unknown_");

    if !grouped.is_empty() {
        let breakdown_map = value.get_or_create_breakdown(label);
        for (key, sub_data) in grouped {
            let all_samples: Vec<f64> = sub_data.iter().flat_map(|(_, s)| s.clone()).collect();
            let sub_value = breakdown_map
                .entry(key)
                .or_insert_with(|| Box::new(HistogramValue::new(compute_spec_histogram_summary(&all_samples))));
            apply_histogram_breakdowns(sub_value, &sub_data, breakdowns, depth + 1);
        }
    }
}

/// Compute histogram summary for spec export format.
fn compute_spec_histogram_summary(samples: &[f64]) -> SpecHistogramSummary {
    if samples.is_empty() {
        return SpecHistogramSummary::default();
    }

    let mut sorted: Vec<f64> = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let count = sorted.len() as u64;
    let sum: f64 = sorted.iter().sum();
    let min = sorted.first().copied().unwrap_or(0.0);
    let max = sorted.last().copied().unwrap_or(0.0);
    let mean = sum / count as f64;

    let percentile = |p: f64| -> f64 {
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    };

    SpecHistogramSummary {
        count,
        sum,
        min,
        max,
        mean,
        p50: percentile(50.0),
        p90: percentile(90.0),
        p99: percentile(99.0),
    }
}

/// Compute histogram summary from a vector of samples.
fn compute_histogram_summary(samples: &[f64]) -> HistogramSummary {
    if samples.is_empty() {
        return HistogramSummary {
            count: 0,
            sum: 0.0,
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            p50: 0.0,
            p90: 0.0,
            p99: 0.0,
        };
    }

    let mut sorted: Vec<f64> = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let count = sorted.len() as u64;
    let sum: f64 = sorted.iter().sum();
    let min = sorted.first().copied().unwrap_or(0.0);
    let max = sorted.last().copied().unwrap_or(0.0);
    let mean = sum / count as f64;

    let percentile = |p: f64| -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    };

    HistogramSummary {
        count,
        sum,
        min,
        max,
        mean,
        p50: percentile(50.0),
        p90: percentile(90.0),
        p99: percentile(99.0),
    }
}

/// In-memory metrics recorder that collects metrics for later export.
///
/// This recorder implements the `metrics::Recorder` trait and stores all
/// metrics in memory for snapshot export at the end of simulation.
#[derive(Debug, Clone)]
pub struct InMemoryRecorder {
    state: Arc<RecorderState>,
}

impl InMemoryRecorder {
    /// Create a new in-memory recorder.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RecorderState::default()),
        }
    }

    /// Configure label filtering based on metric specifications.
    ///
    /// When specs are configured, the recorder will only store labels that are
    /// explicitly requested in the breakdown specifications. This significantly
    /// reduces memory usage for long-running simulations by avoiding unbounded
    /// growth from labels like `payload_hash`.
    ///
    /// # Arguments
    ///
    /// * `specs` - Metric specifications defining which labels to track.
    ///             Labels not mentioned in any matching spec's breakdowns will be dropped.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let recorder = InMemoryRecorder::new();
    /// let specs = vec![
    ///     MetricSpec::parse("mcsim.radio.*/node").unwrap(),  // Only track 'node' label
    ///     MetricSpec::parse("mcsim.dm.*").unwrap(),          // Track totals only
    /// ];
    /// recorder.set_label_filter(specs);
    /// ```
    pub fn set_label_filter(&self, specs: Vec<MetricSpec>) {
        *self.state.label_specs.write() = specs;
    }

    /// Take a snapshot of all current metric values.
    pub fn snapshot(&self) -> MetricsSnapshot {
        self.state.snapshot()
    }

    /// Take a snapshot with configurable breakdowns based on metric specs.
    ///
    /// # Arguments
    ///
    /// * `specs` - Metric specifications defining which metrics to include and how to break them down.
    ///             If empty, defaults to all metrics with totals only.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let spec = MetricSpec::parse("mcsim.radio.*/node").unwrap();
    /// let export = recorder.snapshot_with_specs(&[spec]);
    /// ```
    pub fn snapshot_with_specs(&self, specs: &[MetricSpec]) -> MetricsExport {
        self.state.snapshot_with_specs(specs)
    }

    /// Clear all recorded metrics.
    ///
    /// This is used for metrics warmup - clearing metrics after the warmup period
    /// so that only steady-state metrics are reported.
    pub fn clear(&self) {
        self.state.clear();
    }
}

impl Default for InMemoryRecorder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Counter Implementation
// ============================================================================

/// A counter handle backed by in-memory storage.
struct InMemoryCounter {
    state: Arc<CounterState>,
}

impl metrics::CounterFn for InMemoryCounter {
    fn increment(&self, value: u64) {
        self.state.increment(value);
    }

    fn absolute(&self, value: u64) {
        self.state.value.store(value, Ordering::Relaxed);
    }
}

// ============================================================================
// Gauge Implementation
// ============================================================================

/// A gauge handle backed by in-memory storage.
struct InMemoryGauge {
    state: Arc<GaugeState>,
}

impl metrics::GaugeFn for InMemoryGauge {
    fn increment(&self, value: f64) {
        self.state.increment(value);
    }

    fn decrement(&self, value: f64) {
        self.state.decrement(value);
    }

    fn set(&self, value: f64) {
        self.state.set(value);
    }
}

// ============================================================================
// Histogram Implementation
// ============================================================================

/// A histogram handle backed by in-memory storage.
struct InMemoryHistogram {
    state: Arc<HistogramState>,
}

impl metrics::HistogramFn for InMemoryHistogram {
    fn record(&self, value: f64) {
        self.state.record(value);
    }
}

// ============================================================================
// Recorder Implementation
// ============================================================================

impl Recorder for InMemoryRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
        // Descriptions are optional for in-memory storage
    }

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
        // Descriptions are optional for in-memory storage
    }

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
        // Descriptions are optional for in-memory storage
    }

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        let state = self.state.get_or_create_counter(key);
        Counter::from_arc(Arc::new(InMemoryCounter { state }))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        let state = self.state.get_or_create_gauge(key);
        Gauge::from_arc(Arc::new(InMemoryGauge { state }))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        let state = self.state.get_or_create_histogram(key);
        Histogram::from_arc(Arc::new(InMemoryHistogram { state }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_increment() {
        let recorder = InMemoryRecorder::new();
        let key = Key::from_static_name("test.counter");
        let counter = recorder.state.get_or_create_counter(&key);
        counter.increment(5);
        counter.increment(3);
        assert_eq!(counter.get(), 8);
    }

    #[test]
    fn test_gauge_operations() {
        let recorder = InMemoryRecorder::new();
        let key = Key::from_static_name("test.gauge");
        let gauge = recorder.state.get_or_create_gauge(&key);
        gauge.set(10.0);
        assert!((gauge.get() - 10.0).abs() < f64::EPSILON);
        gauge.increment(5.0);
        assert!((gauge.get() - 15.0).abs() < f64::EPSILON);
        gauge.decrement(3.0);
        assert!((gauge.get() - 12.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_histogram_summary() {
        let recorder = InMemoryRecorder::new();
        let key = Key::from_static_name("test.histogram");
        let histogram = recorder.state.get_or_create_histogram(&key);

        for i in 1..=100 {
            histogram.record(i as f64);
        }

        let summary = histogram.summary();
        assert_eq!(summary.count, 100);
        assert!((summary.min - 1.0).abs() < f64::EPSILON);
        assert!((summary.max - 100.0).abs() < f64::EPSILON);
        assert!((summary.mean - 50.5).abs() < f64::EPSILON);
        // Percentiles may vary slightly due to rounding in index calculation
        assert!((summary.p50 - 50.0).abs() < 2.0);
        assert!((summary.p90 - 90.0).abs() < 2.0);
        assert!((summary.p99 - 99.0).abs() < 2.0);
    }

    #[test]
    fn test_snapshot() {
        let recorder = InMemoryRecorder::new();

        let counter_key = Key::from_static_name("test.counter");
        let counter = recorder.state.get_or_create_counter(&counter_key);
        counter.increment(42);

        let gauge_key = Key::from_static_name("test.gauge");
        let gauge = recorder.state.get_or_create_gauge(&gauge_key);
        gauge.set(3.14);

        let histogram_key = Key::from_static_name("test.histogram");
        let histogram = recorder.state.get_or_create_histogram(&histogram_key);
        histogram.record(1.0);
        histogram.record(2.0);
        histogram.record(3.0);

        let snapshot = recorder.snapshot();

        assert_eq!(snapshot.counters.get("test.counter"), Some(&42));
        assert!((snapshot.gauges.get("test.gauge").unwrap() - 3.14).abs() < f64::EPSILON);
        assert!(snapshot.histograms.contains_key("test.histogram"));
    }

    #[test]
    fn test_export_json() {
        let recorder = InMemoryRecorder::new();
        let key = Key::from_static_name("test.counter");
        let counter = recorder.state.get_or_create_counter(&key);
        counter.increment(10);

        let snapshot = recorder.snapshot();
        let mut output = Vec::new();
        export_json(&snapshot, &mut output).unwrap();

        let json_str = String::from_utf8(output).unwrap();
        assert!(json_str.contains("test.counter"));
        assert!(json_str.contains("10"));
    }

    #[test]
    fn test_export_prometheus() {
        let recorder = InMemoryRecorder::new();
        let counter_key = Key::from_static_name("test.counter");
        let counter = recorder.state.get_or_create_counter(&counter_key);
        counter.increment(10);

        let gauge_key = Key::from_static_name("test.gauge");
        let gauge = recorder.state.get_or_create_gauge(&gauge_key);
        gauge.set(5.5);

        let snapshot = recorder.snapshot();
        let mut output = Vec::new();
        export_prometheus(&snapshot, &mut output).unwrap();

        let prom_str = String::from_utf8(output).unwrap();
        assert!(prom_str.contains("# TYPE test_counter counter"));
        assert!(prom_str.contains("test_counter 10"));
        assert!(prom_str.contains("# TYPE test_gauge gauge"));
        assert!(prom_str.contains("test_gauge 5.5"));
    }

    #[test]
    fn test_per_node_breakdown() {
        use metrics::Label;
        
        let recorder = InMemoryRecorder::new();
        
        // Create metrics with node labels
        let node1_key = Key::from_parts("test.packets", vec![
            Label::new("node", "Node1"),
            Label::new("node_type", "repeater"),
        ]);
        let counter1 = recorder.state.get_or_create_counter(&node1_key);
        counter1.increment(10);

        let node2_key = Key::from_parts("test.packets", vec![
            Label::new("node", "Node2"),
            Label::new("node_type", "client"),
        ]);
        let counter2 = recorder.state.get_or_create_counter(&node2_key);
        counter2.increment(5);

        let snapshot = recorder.snapshot();

        // Check aggregated total
        assert_eq!(snapshot.counters.get("test.packets"), Some(&15));

        // Check per-node breakdown
        assert!(snapshot.nodes.contains_key("Node1"));
        assert!(snapshot.nodes.contains_key("Node2"));
        assert_eq!(
            snapshot.nodes.get("Node1").unwrap().counters.get("test.packets"),
            Some(&10)
        );
        assert_eq!(
            snapshot.nodes.get("Node2").unwrap().counters.get("test.packets"),
            Some(&5)
        );
    }
}
