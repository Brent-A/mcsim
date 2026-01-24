//! Property registry, lookup functions, and property set types.
//!
//! This module provides:
//! - [`ALL_PROPERTIES`] - Array of all registered property definitions
//! - Lookup functions for finding properties by name
//! - [`ResolvedProperties`] - A complete set of property values with defaults
//! - [`UnresolvedProperties`] - A partial set of properties from YAML parsing

use super::definitions::*;
use super::types::{PropertyDef, PropertyScope, Property, ScopeMarker};
use super::value::{FromPropertyValue, PropertyValue, ToPropertyValue};
use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

// ============================================================================
// All Properties Array (for runtime lookup)
// ============================================================================

/// All registered property definitions (for runtime lookup).
pub const ALL_PROPERTIES: &[&PropertyDef] = &[
    // Radio
    &RADIO_FREQUENCY_HZ.def,
    &RADIO_BANDWIDTH_HZ.def,
    &RADIO_SPREADING_FACTOR.def,
    &RADIO_CODING_RATE.def,
    &RADIO_TX_POWER_DBM.def,
    // Companion
    &COMPANION_CHANNELS.def,
    &COMPANION_CONTACTS.def,
    // Messaging
    &MESSAGING_FLOOD_ACK_TIMEOUT_S.def,
    &MESSAGING_DIRECT_ACK_TIMEOUT_PER_HOP_S.def,
    &MESSAGING_DIRECT_ATTEMPTS.def,
    &MESSAGING_FLOOD_ATTEMPTS_AFTER_DIRECT.def,
    &MESSAGING_FLOOD_ATTEMPTS_NO_PATH.def,
    // Room Server
    &ROOM_SERVER_ROOM_ID.def,
    // Agent Direct Message
    &AGENT_DIRECT_ENABLED.def,
    &AGENT_DIRECT_STARTUP_S.def,
    &AGENT_DIRECT_STARTUP_JITTER_S.def,
    &AGENT_DIRECT_TARGETS.def,
    &AGENT_DIRECT_INTERVAL_S.def,
    &AGENT_DIRECT_INTERVAL_JITTER_S.def,
    &AGENT_DIRECT_ACK_TIMEOUT_S.def,
    &AGENT_DIRECT_SESSION_MESSAGE_COUNT.def,
    &AGENT_DIRECT_SESSION_INTERVAL_S.def,
    &AGENT_DIRECT_SESSION_INTERVAL_JITTER_S.def,
    &AGENT_DIRECT_MESSAGE_COUNT.def,
    &AGENT_DIRECT_SHUTDOWN_S.def,
    // Agent Channel Message
    &AGENT_CHANNEL_ENABLED.def,
    &AGENT_CHANNEL_STARTUP_S.def,
    &AGENT_CHANNEL_STARTUP_JITTER_S.def,
    &AGENT_CHANNEL_TARGETS.def,
    &AGENT_CHANNEL_INTERVAL_S.def,
    &AGENT_CHANNEL_INTERVAL_JITTER_S.def,
    &AGENT_CHANNEL_SESSION_MESSAGE_COUNT.def,
    &AGENT_CHANNEL_SESSION_INTERVAL_S.def,
    &AGENT_CHANNEL_SESSION_INTERVAL_JITTER_S.def,
    &AGENT_CHANNEL_MESSAGE_COUNT.def,
    &AGENT_CHANNEL_SHUTDOWN_S.def,
    // Link
    &LINK_MEAN_SNR_DB_AT20DBM.def,
    &LINK_SNR_STD_DEV.def,
    &LINK_RSSI_DBM.def,
    // Simulation
    &SIMULATION_DURATION_S.def,
    &SIMULATION_SEED.def,
    &SIMULATION_UART_BASE_PORT.def,
    // Keys
    &KEYS_PRIVATE_KEY.def,
    &KEYS_PUBLIC_KEY.def,
    // Location
    &LOCATION_LATITUDE.def,
    &LOCATION_LONGITUDE.def,
    &LOCATION_ALTITUDE_M.def,
    // Firmware (Node scope)
    &FIRMWARE_TYPE.def,
    &FIRMWARE_UART_PORT.def,
    &FIRMWARE_STARTUP_TIME_S.def,
    &FIRMWARE_STARTUP_JITTER_S.def,
    // Metrics (Node scope)
    &METRICS_GROUPS.def,
    // Metrics (Simulation scope)
    &METRICS_WARMUP_S.def,
    // CLI
    &CLI_PASSWORD.def,
    &CLI_COMMANDS.def,
    // Radio Thresholds (Simulation scope)
    &RADIO_CAPTURE_EFFECT_THRESHOLD_DB.def,
    &RADIO_NOISE_FLOOR_DBM.def,
    &RADIO_RX_TO_TX_TURNAROUND_US.def,
    &RADIO_TX_TO_RX_TURNAROUND_US.def,
    // SNR Thresholds per Spreading Factor (Simulation scope)
    &RADIO_SNR_THRESHOLD_SF7_DB.def,
    &RADIO_SNR_THRESHOLD_SF8_DB.def,
    &RADIO_SNR_THRESHOLD_SF9_DB.def,
    &RADIO_SNR_THRESHOLD_SF10_DB.def,
    &RADIO_SNR_THRESHOLD_SF11_DB.def,
    &RADIO_SNR_THRESHOLD_SF12_DB.def,
    // Link Quality Classification (Simulation scope)
    &LINK_MARGIN_EXCELLENT_DB.def,
    &LINK_MARGIN_GOOD_DB.def,
    &LINK_MARGIN_MARGINAL_DB.def,
    // Predict-Link Parameters (Simulation scope)
    &PREDICT_FREQUENCY_MHZ.def,
    &PREDICT_TX_POWER_DBM.def,
    &PREDICT_SPREADING_FACTOR.def,
    &PREDICT_DEM_DIR.def,
    &PREDICT_ELEVATION_CACHE_DIR.def,
    &PREDICT_ELEVATION_SOURCE.def,
    &PREDICT_ELEVATION_ZOOM_LEVEL.def,
    &PREDICT_TERRAIN_SAMPLES.def,
    // ITM Prediction Parameters (Simulation scope)
    &ITM_MIN_DISTANCE_M.def,
    &ITM_TERRAIN_SAMPLES.def,
    &ITM_CLIMATE.def,
    &ITM_POLARIZATION.def,
    &ITM_GROUND_PERMITTIVITY.def,
    &ITM_GROUND_CONDUCTIVITY.def,
    &ITM_SURFACE_REFRACTIVITY.def,
    // FSPL Prediction (Simulation scope)
    &FSPL_MIN_DISTANCE_M.def,
    // Colocated Prediction (Simulation scope)
    &COLOCATED_PATH_LOSS_DB.def,
    // LoRa PHY (Simulation scope)
    &LORA_PREAMBLE_SYMBOLS.def,
    // Firmware Simulation (Simulation scope)
    &FIRMWARE_SPIN_DETECTION_THRESHOLD.def,
    &FIRMWARE_IDLE_LOOPS_BEFORE_YIELD.def,
    &FIRMWARE_LOG_SPIN_DETECTION.def,
    &FIRMWARE_LOG_LOOP_ITERATIONS.def,
    &FIRMWARE_INITIAL_RTC_SECS.def,
    // Runner (Simulation scope)
    &RUNNER_WATCHDOG_TIMEOUT_S.def,
    &RUNNER_PERIODIC_STATS_INTERVAL_S.def,
];

// ============================================================================
// Lookup Functions
// ============================================================================

/// Check if a property name is registered (including aliases).
pub fn is_known_property(name: &str) -> bool {
    ALL_PROPERTIES.iter().any(|p| p.matches(name))
}

/// Get a property definition by name from the const array (including aliases).
pub fn get_property_def(name: &str) -> Option<&'static PropertyDef> {
    ALL_PROPERTIES.iter().find(|p| p.matches(name)).copied()
}

/// Get all known namespaces from the const array.
pub fn known_namespaces() -> Vec<&'static str> {
    let mut namespaces: Vec<&'static str> = ALL_PROPERTIES
        .iter()
        .filter_map(|p| p.namespace())
        .collect();
    namespaces.sort();
    namespaces.dedup();
    namespaces
}

/// Get all properties for a given scope.
pub fn properties_by_scope(scope: PropertyScope) -> impl Iterator<Item = &'static PropertyDef> {
    ALL_PROPERTIES
        .iter()
        .filter(move |p| p.scope == scope)
        .copied()
}

/// Get all properties in a given namespace.
pub fn properties_by_namespace(namespace: &str) -> impl Iterator<Item = &'static PropertyDef> + '_ {
    ALL_PROPERTIES
        .iter()
        .filter(move |p| p.namespace() == Some(namespace))
        .copied()
}

/// Get the default value for a property by name.
pub fn default_value(name: &str) -> Option<PropertyValue> {
    get_property_def(name).map(|p| p.default_value())
}

// ============================================================================
// Property Set Errors
// ============================================================================

/// Errors that can occur when manipulating a property set.
#[derive(Debug, Clone)]
pub enum PropertySetError {
    /// Unknown property name.
    UnknownProperty(String),
    /// Property used in wrong scope.
    InvalidPropertyScope(String, PropertyScope),
    /// Unsupported value type.
    UnsupportedValueType(String, String),
    /// Type mismatch between expected and actual value.
    TypeMismatch {
        /// Property name.
        property: String,
        /// Expected type.
        expected: String,
        /// Actual type.
        actual: String,
    },
}

impl std::fmt::Display for PropertySetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropertySetError::UnknownProperty(name) => {
                write!(f, "Unknown property: {}", name)
            }
            PropertySetError::InvalidPropertyScope(name, scope) => {
                write!(
                    f,
                    "Invalid scope for property {}: expected {:?}",
                    name, scope
                )
            }
            PropertySetError::UnsupportedValueType(name, vtype) => {
                write!(f, "Unsupported value type for property {}: {}", name, vtype)
            }
            PropertySetError::TypeMismatch {
                property,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "Type mismatch for property '{}': expected {}, got {}",
                    property, expected, actual
                )
            }
        }
    }
}

impl std::error::Error for PropertySetError {}

// ============================================================================
// Resolved Property Set
// ============================================================================

/// A set of resolved property values for a specific entity (node, edge, or simulation).
///
/// The generic parameter `S` specifies the scope of properties that can
/// be stored (Node, Edge, or Simulation).
#[derive(Debug, Clone)]
pub struct ResolvedProperties<S: ScopeMarker> {
    /// Map from property definition to resolved value.
    values: HashMap<&'static PropertyDef, PropertyValue>,
    /// Phantom data to track the scope type.
    _scope: PhantomData<S>,
}

impl<S: ScopeMarker> ResolvedProperties<S> {
    /// Create a new property set with all defaults.
    pub fn new() -> Self {
        let values = ALL_PROPERTIES
            .iter()
            .filter(|p| p.scope == S::SCOPE)
            .map(|p| (*p, p.default_value()))
            .collect();

        Self {
            values,
            _scope: PhantomData,
        }
    }

    /// Get the scope of this property set.
    pub fn scope(&self) -> PropertyScope {
        S::SCOPE
    }

    /// Set a property value using a static property reference.
    ///
    /// This method is type-safe - the value type must match the property's declared type.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use mcsim_model::properties::{RADIO_FREQUENCY_HZ, ResolvedProperties, NodeScope};
    ///
    /// let mut props: ResolvedProperties<NodeScope> = ResolvedProperties::new();
    /// // Only accepts u32 values since RADIO_FREQUENCY_HZ is Property<u32, NodeScope>
    /// props.set(&RADIO_FREQUENCY_HZ, 915000000u32).unwrap();
    /// ```
    pub fn set<T: ToPropertyValue>(
        &mut self,
        prop: &'static Property<T, S>,
        value: T,
    ) -> Result<(), PropertySetError> {
        self.values.insert(&prop.def, value.to_property_value());
        Ok(())
    }

    /// Get a property value with compile-time type safety.
    ///
    /// Returns the value converted to the property's declared type, or the default
    /// if conversion fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use mcsim_model::properties::{RADIO_FREQUENCY_HZ, ResolvedProperties, NodeScope};
    ///
    /// let props: ResolvedProperties<NodeScope> = ResolvedProperties::new();
    /// let freq: u32 = props.get(&RADIO_FREQUENCY_HZ);
    /// ```
    pub fn get<T: FromPropertyValue>(&self, prop: &Property<T, S>) -> T {
        let value = self
            .values
            .get(&prop.def)
            .cloned()
            .unwrap_or_else(|| prop.def.default_value());
        T::from_property_value(&value).expect("Types are validated on insertion")
    }

    /// Get the raw PropertyValue for a property (for serialization or debugging).
    pub fn get_raw<T>(&self, prop: &Property<T, S>) -> PropertyValue {
        self.values
            .get(&prop.def)
            .cloned()
            .unwrap_or_else(|| prop.def.default_value())
    }

    /// Merge another property set into this one (other takes precedence).
    pub fn merge(&mut self, other: &ResolvedProperties<S>) {
        for (k, v) in &other.values {
            self.values.insert(*k, v.clone());
        }
    }

    /// Apply unresolved properties to this resolved set.
    pub fn apply_unresolved(&mut self, unresolved: &UnresolvedProperties<S>) {
        for (prop, value) in &unresolved.values {
            self.values.insert(*prop, value.clone());
        }
    }
}

impl<S: ScopeMarker> Default for ResolvedProperties<S> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Unresolved Property Set
// ============================================================================

/// A set of unresolved (partial) property values parsed from YAML.
///
/// Unlike `ResolvedProperties`, this only contains properties that were
/// explicitly specified - it does not include defaults.
#[derive(Debug, Clone, Default)]
pub struct UnresolvedProperties<S: ScopeMarker> {
    /// Map from property definition to the value specified in YAML.
    pub(crate) values: HashMap<&'static PropertyDef, PropertyValue>,
    /// Phantom data to track the scope type.
    _scope: PhantomData<S>,
}

impl<S: ScopeMarker> UnresolvedProperties<S> {
    /// Create a new empty unresolved property set.
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            _scope: PhantomData,
        }
    }

    /// Get the scope of this property set.
    pub fn scope(&self) -> PropertyScope {
        S::SCOPE
    }

    /// Get a reference to the underlying values map.
    pub fn values(&self) -> &HashMap<&'static PropertyDef, PropertyValue> {
        &self.values
    }

    /// Get a property value if it was specified.
    pub fn get_raw(&self, prop: &'static PropertyDef) -> Option<&PropertyValue> {
        self.values.get(prop)
    }

    /// Check if a property was specified.
    pub fn contains<T>(&self, prop: &Property<T, S>) -> bool {
        self.values.contains_key(&prop.def)
    }

    /// Insert a property value using a static property reference.
    pub fn insert<T>(
        &mut self,
        prop: &'static Property<T, S>,
        value: PropertyValue,
    ) -> Result<(), PropertySetError> {
        if !prop.def.value_type.matches(&value) {
            return Err(PropertySetError::TypeMismatch {
                property: prop.def.name.to_string(),
                expected: prop.def.value_type.to_string(),
                actual: describe_value_type(&value),
            });
        }

        self.values.insert(&prop.def, value);
        Ok(())
    }

    /// Merge another unresolved property set into this one (other takes precedence).
    pub fn merge(&mut self, other: &UnresolvedProperties<S>) {
        for (k, v) in &other.values {
            self.values.insert(*k, v.clone());
        }
    }

    /// Apply these unresolved properties to a ResolvedProperties set.
    pub fn apply_to(&self, resolved: &mut ResolvedProperties<S>) {
        for (prop, value) in &self.values {
            resolved.values.insert(*prop, value.clone());
        }
    }
}

/// Describe the type of a PropertyValue for error messages.
fn describe_value_type(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Integer(_) => "integer".to_string(),
        PropertyValue::Float(_) => "float".to_string(),
        PropertyValue::String(_) => "string".to_string(),
        PropertyValue::Bool(_) => "bool".to_string(),
        PropertyValue::Vec(items) => {
            if items.is_empty() {
                "empty array".to_string()
            } else {
                format!("array of {}", describe_value_type(&items[0]))
            }
        }
        PropertyValue::Null => "null".to_string(),
    }
}

// ============================================================================
// Custom Deserializer for UnresolvedProperties
// ============================================================================

impl<'de, S: ScopeMarker> Deserialize<'de> for UnresolvedProperties<S> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(UnresolvedPropertiesVisitor::<S>(PhantomData))
    }
}

struct UnresolvedPropertiesVisitor<S: ScopeMarker>(PhantomData<S>);

impl<'de, S: ScopeMarker> Visitor<'de> for UnresolvedPropertiesVisitor<S> {
    type Value = UnresolvedProperties<S>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a map of {} properties", S::SCOPE)
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut result = UnresolvedProperties::<S>::new();
        let mut pending: HashMap<String, serde_yaml::Value> = HashMap::new();

        while let Some((key, value)) = map.next_entry::<String, serde_yaml::Value>()? {
            pending.insert(key, value);
        }

        for (key, value) in pending {
            process_yaml_property::<S, M>(&mut result, &key, &value)?;
        }

        Ok(result)
    }
}

/// Process a single YAML property, recursively handling nested maps.
fn process_yaml_property<'de, S: ScopeMarker, M: MapAccess<'de>>(
    result: &mut UnresolvedProperties<S>,
    prefix: &str,
    value: &serde_yaml::Value,
) -> Result<(), M::Error> {
    match value {
        serde_yaml::Value::Mapping(map) => {
            for (k, v) in map {
                if let serde_yaml::Value::String(key) = k {
                    let full_name = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}/{}", prefix, key)
                    };
                    process_yaml_property::<S, M>(result, &full_name, v)?;
                }
            }
            Ok(())
        }
        _ => {
            let prop_def = get_property_def(prefix).ok_or_else(|| {
                de::Error::custom(format!(
                    "unknown {} property: '{}'. Run \"mcsim properties\" for more details",
                    S::SCOPE,
                    prefix,
                ))
            })?;

            if prop_def.scope != S::SCOPE {
                return Err(de::Error::custom(format!(
                    "property '{}' is a {} property, but was used in {} context",
                    prefix, prop_def.scope, S::SCOPE
                )));
            }

            let prop_value = yaml_value_to_property(value).map_err(|e| {
                de::Error::custom(format!("invalid value for property '{}': {}", prefix, e))
            })?;

            if !prop_def.value_type.matches(&prop_value) {
                return Err(de::Error::custom(format!(
                    "type mismatch for property '{}': expected {}, got {}",
                    prefix,
                    prop_def.value_type,
                    describe_value_type(&prop_value)
                )));
            }

            result.values.insert(prop_def, prop_value);
            Ok(())
        }
    }
}

impl<S: ScopeMarker> Serialize for UnresolvedProperties<S> {
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.values.len()))?;
        for (prop, value) in &self.values {
            map.serialize_entry(prop.name, value)?;
        }
        map.end()
    }
}

/// Convert a serde_yaml::Value to a PropertyValue.
fn yaml_value_to_property(value: &serde_yaml::Value) -> Result<PropertyValue, PropertySetError> {
    match value {
        serde_yaml::Value::Bool(b) => Ok(PropertyValue::Bool(*b)),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(PropertyValue::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(PropertyValue::Float(f))
            } else {
                Err(PropertySetError::UnsupportedValueType(
                    "number".to_string(),
                    format!("{:?}", n),
                ))
            }
        }
        serde_yaml::Value::String(s) => Ok(PropertyValue::String(s.clone())),
        serde_yaml::Value::Sequence(seq) => {
            let values: Vec<PropertyValue> = seq
                .iter()
                .filter_map(|v| yaml_value_to_property(v).ok())
                .collect();
            Ok(PropertyValue::Vec(values))
        }
        serde_yaml::Value::Null => Ok(PropertyValue::Null),
        serde_yaml::Value::Mapping(_) => Err(PropertySetError::UnsupportedValueType(
            "mapping".to_string(),
            "nested mapping".to_string(),
        )),
        serde_yaml::Value::Tagged(_) => Err(PropertySetError::UnsupportedValueType(
            "tagged".to_string(),
            "tagged value".to_string(),
        )),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::super::types::{EdgeScope, NodeScope, SimulationScope};
    use super::*;

    #[test]
    fn test_typed_get() {
        let props: ResolvedProperties<NodeScope> = ResolvedProperties::new();

        // Test default values with correct types
        let freq: u32 = props.get(&RADIO_FREQUENCY_HZ);
        assert_eq!(freq, 910_525_000);

        let bandwidth: u32 = props.get(&RADIO_BANDWIDTH_HZ);
        assert_eq!(bandwidth, 62_500);

        let sf: u8 = props.get(&RADIO_SPREADING_FACTOR);
        assert_eq!(sf, 7);

        let tx_power: i8 = props.get(&RADIO_TX_POWER_DBM);
        assert_eq!(tx_power, 20);
    }

    #[test]
    fn test_typed_get_float() {
        let props: ResolvedProperties<NodeScope> = ResolvedProperties::new();

        let lat: f64 = props.get(&LOCATION_LATITUDE);
        assert_eq!(lat, 0.0);
    }

    #[test]
    fn test_typed_get_string() {
        let props: ResolvedProperties<NodeScope> = ResolvedProperties::new();

        let fw_type: String = props.get(&FIRMWARE_TYPE);
        assert_eq!(fw_type, "Repeater");

        let private_key: String = props.get(&KEYS_PRIVATE_KEY);
        assert_eq!(private_key, "*");
    }

    #[test]
    fn test_typed_set_and_get() {
        let mut props: ResolvedProperties<NodeScope> = ResolvedProperties::new();
        props.set(&RADIO_FREQUENCY_HZ, 920_000_000_u32).unwrap();

        let freq: u32 = props.get(&RADIO_FREQUENCY_HZ);
        assert_eq!(freq, 920_000_000);
    }

    #[test]
    fn test_typed_get_opt_for_nullable() {
        let mut props: ResolvedProperties<NodeScope> = ResolvedProperties::new();

        // Default is null, so get_opt should return None
        let altitude: Option<f64> = props.get(&LOCATION_ALTITUDE_M);
        assert!(altitude.is_none());

        // Set a value
        props.set(&LOCATION_ALTITUDE_M, Some(1500.5_f64)).unwrap();
        let altitude: Option<f64> = props.get(&LOCATION_ALTITUDE_M);
        assert_eq!(altitude, Some(1500.5));
    }

    #[test]
    fn test_typed_get_vec() {
        let mut props: ResolvedProperties<NodeScope> = ResolvedProperties::new();

        // Default is empty vec
        let groups: Vec<String> = props.get(&METRICS_GROUPS);
        assert!(groups.is_empty());

        // Set a value
        props
            .set(
                &METRICS_GROUPS,
                vec!["group1".to_string(), "group2".to_string()],
            )
            .unwrap();

        let groups: Vec<String> = props.get(&METRICS_GROUPS);
        assert_eq!(groups, vec!["group1".to_string(), "group2".to_string()]);
    }

    #[test]
    fn test_edge_properties() {
        let props: ResolvedProperties<EdgeScope> = ResolvedProperties::new();

        let snr: f64 = props.get(&LINK_MEAN_SNR_DB_AT20DBM);
        assert_eq!(snr, 0.0);

        let std_dev: f64 = props.get(&LINK_SNR_STD_DEV);
        assert_eq!(std_dev, 1.8);

        let rssi: f64 = props.get(&LINK_RSSI_DBM);
        assert_eq!(rssi, -100.0);
    }

    #[test]
    fn test_simulation_properties() {
        let props: ResolvedProperties<SimulationScope> = ResolvedProperties::new();

        let duration: f64 = props.get(&SIMULATION_DURATION_S);
        assert_eq!(duration, 3600.0);

        let seed: i64 = props.get(&SIMULATION_SEED);
        assert_eq!(seed, 0);

        let port: u16 = props.get(&SIMULATION_UART_BASE_PORT);
        assert_eq!(port, 9000);
    }

    #[test]
    fn test_property_name() {
        assert_eq!(RADIO_FREQUENCY_HZ.name(), "radio/frequency_hz");
        assert_eq!(LOCATION_LATITUDE.name(), "location/latitude");
        assert_eq!(LINK_SNR_STD_DEV.name(), "link/snr_std_dev");
    }

    #[test]
    fn test_property_lookup() {
        assert!(is_known_property("radio/frequency_hz"));
        assert!(is_known_property("link/mean_snr_db_at20dbm"));
        assert!(!is_known_property("unknown/property"));
    }

    #[test]
    fn test_unresolved_properties_deserialize() {
        let yaml = r#"
            radio:
                frequency_hz: 915000000
                bandwidth_hz: 125000
        "#;

        let props: UnresolvedProperties<NodeScope> = serde_yaml::from_str(yaml).unwrap();

        assert!(props.contains(&RADIO_FREQUENCY_HZ));
        assert!(props.contains(&RADIO_BANDWIDTH_HZ));
    }
}
