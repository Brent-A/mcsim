//! Metric specification parsing and breakdown configuration.
//!
//! This module provides parsing for metric selection syntax like:
//! - `mcsim.radio.rx_packets` - Get total only
//! - `mcsim.radio.rx_packets/node` - Breakdown by node
//! - `mcsim.radio.*/packet_type/node` - All radio metrics, by packet_type then node
//! - `mcsim.radio.rx_packets/*` - Breakdown by all available labels
//!
//! # Syntax
//!
//! ```text
//! <metric-pattern>[/<breakdown1>[/<breakdown2>...]]
//! <metric-pattern>/*
//! ```
//!
//! Where:
//! - `<metric-pattern>` is a glob pattern (supports `*` wildcard)
//! - `<breakdown>` is a label name to group by (node, node_type, group, packet_type)
//! - `*` as a breakdown means "break down by all available labels in the data"

use std::collections::HashMap;

/// Known breakdown labels that can be used for aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BreakdownLabel {
    /// Break down by individual node name.
    Node,
    /// Break down by node type (repeater, client, etc.).
    NodeType,
    /// Break down by group membership.
    Group,
    /// Break down by route type (flood, direct, transport_flood, transport_direct).
    RouteType,
    /// Break down by payload type.
    PayloadType,
    /// Break down by payload hash (unique packet identifier).
    PayloadHash,
}

impl BreakdownLabel {
    /// Parse a breakdown label from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "node" => Some(Self::Node),
            "node_type" | "nodetype" | "type" => Some(Self::NodeType),
            "group" | "groups" => Some(Self::Group),
            "route_type" | "routetype" | "route" => Some(Self::RouteType),
            "payload_type" | "payloadtype" => Some(Self::PayloadType),
            "payload_hash" | "payloadhash" | "hash" => Some(Self::PayloadHash),
            _ => None,
        }
    }

    /// Get the label key name as used in metrics.
    pub fn label_key(&self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::NodeType => "node_type",
            Self::Group => "groups",
            Self::RouteType => "route_type",
            Self::PayloadType => "payload_type",
            Self::PayloadHash => "payload_hash",
        }
    }

    /// Get the sort order for this breakdown label when using wildcard breakdowns.
    ///
    /// The order is:
    /// 1. group
    /// 2. node_type
    /// 3. node
    /// 4. route_type
    /// 5. payload_type
    /// 6. payload_hash
    pub fn sort_order(&self) -> u8 {
        match self {
            Self::Group => 1,
            Self::NodeType => 2,
            Self::Node => 3,
            Self::RouteType => 4,
            Self::PayloadType => 5,
            Self::PayloadHash => 6,
        }
    }
}

/// A parsed metric specification.
#[derive(Debug, Clone)]
pub struct MetricSpec {
    /// Glob pattern for metric names (e.g., "mcsim.radio.*").
    pub pattern: String,
    /// Ordered list of breakdown labels.
    pub breakdowns: Vec<BreakdownLabel>,
    /// If true, break down by all available labels found in the metric data.
    /// When set, the `breakdowns` field is ignored and all labels present
    /// in the metric data will be used for breakdown.
    pub breakdown_all: bool,
}

impl MetricSpec {
    /// Parse a metric specification string.
    ///
    /// # Examples
    ///
    /// ```
    /// use mcsim_runner::metric_spec::MetricSpec;
    ///
    /// let spec = MetricSpec::parse("mcsim.radio.rx_packets/node").unwrap();
    /// assert_eq!(spec.pattern, "mcsim.radio.rx_packets");
    /// assert_eq!(spec.breakdowns.len(), 1);
    /// ```
    pub fn parse(s: &str) -> Result<Self, MetricSpecError> {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.is_empty() {
            return Err(MetricSpecError::EmptySpec);
        }

        let pattern = parts[0].trim().to_string();
        if pattern.is_empty() {
            return Err(MetricSpecError::EmptyPattern);
        }

        // Check for wildcard breakdown (/* means all available labels)
        // Must be the only breakdown specified
        if parts.len() == 2 && parts[1].trim() == "*" {
            return Ok(Self {
                pattern,
                breakdowns: Vec::new(),
                breakdown_all: true,
            });
        }

        let mut breakdowns = Vec::new();
        let breakdown_all = false;
        for (i, part) in parts.iter().skip(1).enumerate() {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            // Check for wildcard in breakdown position
            if part == "*" {
                return Err(MetricSpecError::WildcardBreakdownNotAlone);
            }
            match BreakdownLabel::from_str(part) {
                Some(label) => {
                    if breakdowns.contains(&label) {
                        return Err(MetricSpecError::DuplicateBreakdown(part.to_string()));
                    }
                    breakdowns.push(label);
                }
                None => {
                    return Err(MetricSpecError::UnknownBreakdown {
                        label: part.to_string(),
                        position: i + 1,
                    });
                }
            }
        }

        Ok(Self { pattern, breakdowns, breakdown_all })
    }

    /// Check if a metric name matches this spec's pattern.
    pub fn matches(&self, metric_name: &str) -> bool {
        glob_match(&self.pattern, metric_name)
    }
}

/// Errors that can occur when parsing a metric specification.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MetricSpecError {
    #[error("empty metric specification")]
    EmptySpec,
    #[error("empty metric pattern")]
    EmptyPattern,
    #[error("unknown breakdown label: '{label}' at position {position}")]
    UnknownBreakdown { label: String, position: usize },
    #[error("duplicate breakdown label: '{0}'")]
    DuplicateBreakdown(String),
    #[error("wildcard breakdown '*' must be the only breakdown (use 'metric/*', not 'metric/*/label')")]
    WildcardBreakdownNotAlone,
}

/// Simple glob matching supporting '*' as a wildcard for any sequence.
fn glob_match(pattern: &str, text: &str) -> bool {
    // Handle simple cases first
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == text;
    }

    // Split pattern by '*' and match segments
    let segments: Vec<&str> = pattern.split('*').collect();

    let mut text_pos = 0;

    for (i, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }

        let is_first = i == 0;
        let is_last = i == segments.len() - 1;

        if is_first {
            // First segment must match at the start
            if !text.starts_with(segment) {
                return false;
            }
            text_pos = segment.len();
        } else if is_last {
            // Last segment must match at the end
            if !text.ends_with(segment) {
                return false;
            }
            // Ensure we haven't passed it
            let end_start = text.len().saturating_sub(segment.len());
            if text_pos > end_start {
                return false;
            }
        } else {
            // Middle segment: find it anywhere after current position
            if let Some(found) = text[text_pos..].find(segment) {
                text_pos += found + segment.len();
            } else {
                return false;
            }
        }
    }

    true
}

// ============================================================================
// Metric Value Types for Nested Output
// ============================================================================

/// A metric value that can be a simple value or have nested breakdowns.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(untagged)]
pub enum MetricValue {
    /// A simple counter value.
    Counter(CounterValue),
    /// A simple gauge value.
    Gauge(GaugeValue),
    /// A histogram summary.
    Histogram(HistogramValue),
}

/// Counter value with optional breakdowns.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CounterValue {
    /// The total/aggregated value.
    pub total: u64,
    /// Breakdowns by label name. Each key is a label name (e.g., "node", "node_type")
    /// and the value is a map from label values to nested CounterValues.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, HashMap<String, Box<CounterValue>>>,
}

impl CounterValue {
    /// Create a new counter value with just a total.
    pub fn new(total: u64) -> Self {
        Self {
            total,
            labels: HashMap::new(),
        }
    }

    /// Set a breakdown map for a given label.
    pub fn set_breakdown(&mut self, label: BreakdownLabel, values: HashMap<String, Box<CounterValue>>) {
        self.labels.insert(label.label_key().to_string(), values);
    }

    /// Get a mutable reference to a breakdown map, creating it if needed.
    pub fn get_or_create_breakdown(&mut self, label: BreakdownLabel) -> &mut HashMap<String, Box<CounterValue>> {
        self.labels
            .entry(label.label_key().to_string())
            .or_insert_with(HashMap::new)
    }
}

/// Gauge value with optional breakdowns.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GaugeValue {
    /// The total/aggregated value.
    pub total: f64,
    /// Breakdowns by label name. Each key is a label name (e.g., "node", "node_type")
    /// and the value is a map from label values to nested GaugeValues.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, HashMap<String, Box<GaugeValue>>>,
}

impl GaugeValue {
    /// Create a new gauge value with just a total.
    pub fn new(total: f64) -> Self {
        Self {
            total,
            labels: HashMap::new(),
        }
    }

    /// Get a mutable reference to a breakdown map, creating it if needed.
    pub fn get_or_create_breakdown(&mut self, label: BreakdownLabel) -> &mut HashMap<String, Box<GaugeValue>> {
        self.labels
            .entry(label.label_key().to_string())
            .or_insert_with(HashMap::new)
    }
}

/// Histogram summary statistics.
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

impl Default for HistogramSummary {
    fn default() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            p50: 0.0,
            p90: 0.0,
            p99: 0.0,
        }
    }
}

/// Histogram value with optional breakdowns.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistogramValue {
    /// The total/aggregated summary.
    #[serde(flatten)]
    pub total: HistogramSummary,
    /// Breakdowns by label name. Each key is a label name (e.g., "node", "node_type")
    /// and the value is a map from label values to nested HistogramValues.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub labels: HashMap<String, HashMap<String, Box<HistogramValue>>>,
}

impl HistogramValue {
    /// Create a new histogram value with just a total summary.
    pub fn new(total: HistogramSummary) -> Self {
        Self {
            total,
            labels: HashMap::new(),
        }
    }

    /// Get a mutable reference to a breakdown map, creating it if needed.
    pub fn get_or_create_breakdown(&mut self, label: BreakdownLabel) -> &mut HashMap<String, Box<HistogramValue>> {
        self.labels
            .entry(label.label_key().to_string())
            .or_insert_with(HashMap::new)
    }
}

// ============================================================================
// New Snapshot Format
// ============================================================================

/// Collected metric data for export with flexible breakdowns.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsExport {
    /// Timestamp when metrics were collected.
    pub timestamp: String,
    /// All metrics with their values and breakdowns.
    pub metrics: HashMap<String, MetricValue>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_metric() {
        let spec = MetricSpec::parse("mcsim.radio.rx_packets").unwrap();
        assert_eq!(spec.pattern, "mcsim.radio.rx_packets");
        assert!(spec.breakdowns.is_empty());
    }

    #[test]
    fn test_parse_metric_with_breakdown() {
        let spec = MetricSpec::parse("mcsim.radio.rx_packets/node").unwrap();
        assert_eq!(spec.pattern, "mcsim.radio.rx_packets");
        assert_eq!(spec.breakdowns, vec![BreakdownLabel::Node]);
    }

    #[test]
    fn test_parse_metric_with_multiple_breakdowns() {
        let spec = MetricSpec::parse("mcsim.radio.*/payload_type/node").unwrap();
        assert_eq!(spec.pattern, "mcsim.radio.*");
        assert_eq!(spec.breakdowns, vec![BreakdownLabel::PayloadType, BreakdownLabel::Node]);
    }

    #[test]
    fn test_parse_breakdown_aliases() {
        let spec = MetricSpec::parse("mcsim.radio.*/node_type").unwrap();
        assert_eq!(spec.breakdowns, vec![BreakdownLabel::NodeType]);

        let spec = MetricSpec::parse("mcsim.radio.*/type").unwrap();
        assert_eq!(spec.breakdowns, vec![BreakdownLabel::NodeType]);

        let spec = MetricSpec::parse("mcsim.radio.*/groups").unwrap();
        assert_eq!(spec.breakdowns, vec![BreakdownLabel::Group]);
    }

    #[test]
    fn test_parse_duplicate_breakdown_error() {
        let result = MetricSpec::parse("mcsim.radio.*/node/node");
        assert!(matches!(result, Err(MetricSpecError::DuplicateBreakdown(_))));
    }

    #[test]
    fn test_parse_unknown_breakdown_error() {
        let result = MetricSpec::parse("mcsim.radio.*/foobar");
        assert!(matches!(result, Err(MetricSpecError::UnknownBreakdown { .. })));
    }

    #[test]
    fn test_glob_match_exact() {
        assert!(glob_match("mcsim.radio.rx_packets", "mcsim.radio.rx_packets"));
        assert!(!glob_match("mcsim.radio.rx_packets", "mcsim.radio.tx_packets"));
    }

    #[test]
    fn test_glob_match_wildcard() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("mcsim.*", "mcsim.radio.rx_packets"));
        assert!(glob_match("mcsim.radio.*", "mcsim.radio.rx_packets"));
        assert!(glob_match("mcsim.radio.*", "mcsim.radio.tx_packets"));
        assert!(!glob_match("mcsim.radio.*", "mcsim.dm.sent"));
    }

    #[test]
    fn test_glob_match_middle_wildcard() {
        assert!(glob_match("mcsim.*.rx_packets", "mcsim.radio.rx_packets"));
        assert!(glob_match("mcsim.*.rx_packets", "mcsim.packet.rx_packets"));
        assert!(!glob_match("mcsim.*.rx_packets", "mcsim.radio.tx_packets"));
    }

    #[test]
    fn test_spec_matches() {
        let spec = MetricSpec::parse("mcsim.radio.*").unwrap();
        assert!(spec.matches("mcsim.radio.rx_packets"));
        assert!(spec.matches("mcsim.radio.tx_packets"));
        assert!(!spec.matches("mcsim.dm.sent"));
    }

    #[test]
    fn test_counter_value_serialization() {
        let mut value = CounterValue::new(100);
        let mut by_node = HashMap::new();
        by_node.insert("Node1".to_string(), Box::new(CounterValue::new(60)));
        by_node.insert("Node2".to_string(), Box::new(CounterValue::new(40)));
        value.labels.insert("node".to_string(), by_node);

        let json = serde_json::to_string_pretty(&value).unwrap();
        assert!(json.contains("\"total\": 100"));
        assert!(json.contains("\"labels\""));
        assert!(json.contains("\"node\""));
        assert!(json.contains("\"Node1\""));
    }

    #[test]
    fn test_parse_wildcard_breakdown() {
        let spec = MetricSpec::parse("mcsim.radio.rx_packets/*").unwrap();
        assert_eq!(spec.pattern, "mcsim.radio.rx_packets");
        assert!(spec.breakdowns.is_empty());
        assert!(spec.breakdown_all);
    }

    #[test]
    fn test_parse_wildcard_breakdown_with_pattern() {
        let spec = MetricSpec::parse("mcsim.radio.*/*").unwrap();
        assert_eq!(spec.pattern, "mcsim.radio.*");
        assert!(spec.breakdowns.is_empty());
        assert!(spec.breakdown_all);
    }

    #[test]
    fn test_parse_wildcard_breakdown_not_alone() {
        // Wildcard must be the only breakdown
        let result = MetricSpec::parse("mcsim.radio.*/*/node");
        assert!(matches!(result, Err(MetricSpecError::WildcardBreakdownNotAlone)));

        let result = MetricSpec::parse("mcsim.radio.*/node/*");
        assert!(matches!(result, Err(MetricSpecError::WildcardBreakdownNotAlone)));
    }

    #[test]
    fn test_simple_metric_no_wildcard_breakdown() {
        let spec = MetricSpec::parse("mcsim.radio.rx_packets").unwrap();
        assert!(!spec.breakdown_all);
    }

    #[test]
    fn test_metric_with_breakdown_no_wildcard() {
        let spec = MetricSpec::parse("mcsim.radio.rx_packets/node").unwrap();
        assert!(!spec.breakdown_all);
        assert_eq!(spec.breakdowns, vec![BreakdownLabel::Node]);
    }

    #[test]
    fn test_breakdown_label_sort_order() {
        // Verify the sort order for wildcard breakdown nesting:
        // group, node_type, node, route_type, payload_type, payload_hash
        let mut labels = vec![
            BreakdownLabel::PayloadHash,
            BreakdownLabel::Node,
            BreakdownLabel::NodeType,
            BreakdownLabel::Group,
            BreakdownLabel::RouteType,
            BreakdownLabel::PayloadType,
        ];
        labels.sort_by_key(|l| l.sort_order());
        assert_eq!(
            labels,
            vec![
                BreakdownLabel::Group,
                BreakdownLabel::NodeType,
                BreakdownLabel::Node,
                BreakdownLabel::RouteType,
                BreakdownLabel::PayloadType,
                BreakdownLabel::PayloadHash,
            ]
        );
    }
}