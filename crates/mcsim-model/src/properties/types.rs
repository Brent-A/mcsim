//! Property type definitions and metadata.
//!
//! This module provides:
//! - [`PropertyScope`] - The scope to which a property applies (Node, Edge, Simulation)
//! - [`PropertyBaseType`] - The base type of a property value
//! - [`PropertyType`] - Full type specification including nullability and array modifiers
//! - [`PropertyDefault`] - Default values usable in const contexts
//! - [`PropertyDef`] - Property metadata for runtime operations
//! - [`Property<T, S>`] - Type-safe property definition with compile-time type info
//! - Scope marker types for compile-time scope checking

use super::value::PropertyValue;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

// ============================================================================
// Property Scope
// ============================================================================

/// The scope to which a property applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PropertyScope {
    /// Property applies to the entire simulation.
    Simulation,
    /// Property applies to individual nodes.
    Node,
    /// Property applies to edges (links between nodes).
    Edge,
}

impl std::fmt::Display for PropertyScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropertyScope::Simulation => write!(f, "simulation"),
            PropertyScope::Node => write!(f, "node"),
            PropertyScope::Edge => write!(f, "edge"),
        }
    }
}

// ============================================================================
// Property Base Type
// ============================================================================

/// The base type of a property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyBaseType {
    /// Integer value (i64).
    Integer,
    /// Floating point value (f64).
    Float,
    /// String value.
    String,
    /// Boolean value.
    Bool,
}

impl std::fmt::Display for PropertyBaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropertyBaseType::Integer => write!(f, "integer"),
            PropertyBaseType::Float => write!(f, "float"),
            PropertyBaseType::String => write!(f, "string"),
            PropertyBaseType::Bool => write!(f, "bool"),
        }
    }
}

// ============================================================================
// Property Type Specification
// ============================================================================

/// The type specification for a property, including nullability and array modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PropertyType {
    /// The base type of the property.
    pub base: PropertyBaseType,
    /// Whether the property can be null.
    pub nullable: bool,
    /// Whether the property is an array of the base type.
    pub is_array: bool,
}

impl PropertyType {
    /// Create a non-nullable, non-array property type.
    pub const fn new(base: PropertyBaseType) -> Self {
        Self {
            base,
            nullable: false,
            is_array: false,
        }
    }

    /// Make this type nullable.
    pub const fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    /// Make this type an array.
    pub const fn array(mut self) -> Self {
        self.is_array = true;
        self
    }

    /// Check if a PropertyValue matches this type specification.
    pub fn matches(&self, value: &PropertyValue) -> bool {
        // Null values are only valid for nullable types
        if value.is_null() {
            return self.nullable;
        }

        if self.is_array {
            // For arrays, check that it's a Vec and all elements match the base type
            if let PropertyValue::Vec(items) = value {
                items.iter().all(|item| self.base_type_matches(item))
            } else {
                false
            }
        } else {
            self.base_type_matches(value)
        }
    }

    /// Check if a value matches the base type.
    fn base_type_matches(&self, value: &PropertyValue) -> bool {
        match (self.base, value) {
            (PropertyBaseType::Integer, PropertyValue::Integer(_)) => true,
            // Also accept floats as integers if they're whole numbers
            (PropertyBaseType::Integer, PropertyValue::Float(f)) => f.fract() == 0.0,
            (PropertyBaseType::Float, PropertyValue::Float(_)) => true,
            // Integers can be promoted to floats
            (PropertyBaseType::Float, PropertyValue::Integer(_)) => true,
            (PropertyBaseType::String, PropertyValue::String(_)) => true,
            (PropertyBaseType::Bool, PropertyValue::Bool(_)) => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for PropertyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_array {
            write!(f, "array of {}", self.base)?;
        } else {
            write!(f, "{}", self.base)?;
        }
        if self.nullable {
            write!(f, " (nullable)")?;
        }
        Ok(())
    }
}

// ============================================================================
// Property Default (const-compatible)
// ============================================================================

/// The default value for a property, usable in const contexts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropertyDefault {
    /// Integer value (i64).
    Integer(i64),
    /// Floating point value (f64).
    Float(f64),
    /// Boolean value.
    Bool(bool),
    /// String value (static str for const compatibility).
    String(&'static str),
    /// Vector value (static slice of PropertyDefault for const compatibility).
    Vec(&'static [PropertyDefault]),
    /// Null value.
    Null,
}

impl PropertyDefault {
    /// Convert to a PropertyValue.
    pub fn to_value(self) -> PropertyValue {
        match self {
            PropertyDefault::Integer(v) => PropertyValue::Integer(v),
            PropertyDefault::Float(v) => PropertyValue::Float(v),
            PropertyDefault::Bool(v) => PropertyValue::Bool(v),
            PropertyDefault::String(v) => PropertyValue::String(v.to_string()),
            PropertyDefault::Vec(v) => {
                PropertyValue::Vec(v.iter().map(|pd| pd.to_value()).collect())
            }
            PropertyDefault::Null => PropertyValue::Null,
        }
    }
}

impl From<PropertyDefault> for PropertyValue {
    fn from(d: PropertyDefault) -> Self {
        d.to_value()
    }
}

impl std::fmt::Display for PropertyDefault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropertyDefault::Integer(v) => write!(f, "{}", v),
            PropertyDefault::Float(v) => write!(f, "{}", v),
            PropertyDefault::Bool(v) => write!(f, "{}", v),
            PropertyDefault::String(v) => write!(f, "{}", v),
            PropertyDefault::Vec(v) => {
                let strs: Vec<String> = v.iter().map(|pd| pd.to_string()).collect();
                write!(f, "[{}]", strs.join(", "))
            }
            PropertyDefault::Null => write!(f, "null"),
        }
    }
}

// ============================================================================
// Property Definition (Runtime Metadata)
// ============================================================================

/// Internal property metadata used for runtime operations.
///
/// This struct holds the property metadata without type information.
/// It's used internally for storage keys, YAML parsing, and the property registry.
#[derive(Debug, Clone, Copy)]
pub struct PropertyDef {
    /// Full property name including namespace (e.g., "radio/frequency_hz").
    pub name: &'static str,
    /// Human-readable description of the property.
    pub description: &'static str,
    /// The scope to which this property applies.
    pub scope: PropertyScope,
    /// The type specification for this property (base type, nullable, array).
    pub value_type: PropertyType,
    /// Default value for this property.
    pub default: PropertyDefault,
    /// Optional unit string (e.g., "Hz", "dB", "ms").
    pub unit: Option<&'static str>,
    /// Optional aliases for this property (e.g., "lat" for "latitude").
    pub aliases: &'static [&'static str],
}

impl PartialEq for PropertyDef {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for PropertyDef {}

impl std::hash::Hash for PropertyDef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PropertyDef {
    /// Check if this property matches a given name (including aliases).
    pub fn matches(&self, name: &str) -> bool {
        if self.name == name {
            return true;
        }
        self.aliases.iter().any(|alias| *alias == name)
    }

    /// Get the namespace of this property (everything before the last '/').
    pub fn namespace(&self) -> Option<&str> {
        self.name.rfind('/').map(|idx| &self.name[..idx])
    }

    /// Get the short name of this property (everything after the last '/').
    pub fn short_name(&self) -> &str {
        self.name
            .rfind('/')
            .map(|idx| &self.name[idx + 1..])
            .unwrap_or(self.name)
    }

    /// Get the default value as a PropertyValue.
    pub fn default_value(&self) -> PropertyValue {
        self.default.to_value()
    }
}

// ============================================================================
// Scope Marker Traits
// ============================================================================

/// Marker trait for property scope types.
pub trait ScopeMarker: 'static {
    /// The runtime scope value for this marker type.
    const SCOPE: PropertyScope;
}

/// Marker type for node-scoped properties.
#[derive(Debug, Clone, Copy, Default)]
pub struct NodeScope;
impl ScopeMarker for NodeScope {
    const SCOPE: PropertyScope = PropertyScope::Node;
}

/// Marker type for edge-scoped properties.
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeScope;
impl ScopeMarker for EdgeScope {
    const SCOPE: PropertyScope = PropertyScope::Edge;
}

/// Marker type for simulation-scoped properties.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimulationScope;
impl ScopeMarker for SimulationScope {
    const SCOPE: PropertyScope = PropertyScope::Simulation;
}

// ============================================================================
// Type-Safe Property Definition
// ============================================================================

/// A type-safe property definition.
///
/// This is the primary way to define and access properties. It combines the property
/// metadata with compile-time type information for the value type `T` and scope `S`.
///
/// # Example
///
/// ```ignore
/// use mcsim_model::properties::{RADIO_FREQUENCY_HZ, ResolvedProperties, NodeScope};
///
/// let props: ResolvedProperties<NodeScope> = ResolvedProperties::new();
/// // Returns u32 directly, no unwrapping needed
/// let freq: u32 = props.get(&RADIO_FREQUENCY_HZ);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Property<T, S: ScopeMarker> {
    /// The property metadata.
    pub(crate) def: PropertyDef,
    /// Phantom data to track the value type.
    _value_type: PhantomData<T>,
    /// Phantom data to track the scope type.
    _scope: PhantomData<S>,
}

impl<T, S: ScopeMarker> Property<T, S> {
    /// Create a new property definition with type inferred from default.
    pub const fn new(
        name: &'static str,
        description: &'static str,
        default: PropertyDefault,
    ) -> Self {
        let value_type = match default {
            PropertyDefault::Integer(_) => PropertyType::new(PropertyBaseType::Integer),
            PropertyDefault::Float(_) => PropertyType::new(PropertyBaseType::Float),
            PropertyDefault::Bool(_) => PropertyType::new(PropertyBaseType::Bool),
            PropertyDefault::String(_) => PropertyType::new(PropertyBaseType::String),
            PropertyDefault::Vec(_) => PropertyType::new(PropertyBaseType::String).array(),
            PropertyDefault::Null => PropertyType::new(PropertyBaseType::String).nullable(),
        };

        Self {
            def: PropertyDef {
                name,
                description,
                scope: S::SCOPE,
                value_type,
                default,
                unit: None,
                aliases: &[],
            },
            _value_type: PhantomData,
            _scope: PhantomData,
        }
    }

    /// Set an explicit type for this property (const-compatible).
    pub const fn with_type(mut self, value_type: PropertyType) -> Self {
        self.def.value_type = value_type;
        self
    }

    /// Set the unit for this property (const-compatible).
    pub const fn with_unit(mut self, unit: &'static str) -> Self {
        self.def.unit = Some(unit);
        self
    }

    /// Set aliases for this property (const-compatible).
    pub const fn with_aliases(mut self, aliases: &'static [&'static str]) -> Self {
        self.def.aliases = aliases;
        self
    }

    /// Get the property name.
    pub const fn name(&self) -> &'static str {
        self.def.name
    }

    /// Get the property description.
    pub const fn description(&self) -> &'static str {
        self.def.description
    }

    /// Get the property scope.
    pub const fn scope(&self) -> PropertyScope {
        self.def.scope
    }

    /// Get the property unit.
    pub const fn unit(&self) -> Option<&'static str> {
        self.def.unit
    }

    /// Get the internal property definition.
    pub const fn def(&self) -> &PropertyDef {
        &self.def
    }
}

impl<T, S: ScopeMarker> PartialEq for Property<T, S> {
    fn eq(&self, other: &Self) -> bool {
        self.def == other.def
    }
}

impl<T, S: ScopeMarker> Eq for Property<T, S> {}

impl<T, S: ScopeMarker> std::hash::Hash for Property<T, S> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.def.hash(state);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_type_matches() {
        let int_type = PropertyType::new(PropertyBaseType::Integer);
        assert!(int_type.matches(&PropertyValue::Integer(42)));
        assert!(!int_type.matches(&PropertyValue::String("42".to_string())));
        assert!(!int_type.matches(&PropertyValue::Null));

        let nullable_int = PropertyType::new(PropertyBaseType::Integer).nullable();
        assert!(nullable_int.matches(&PropertyValue::Integer(42)));
        assert!(nullable_int.matches(&PropertyValue::Null));

        let string_array = PropertyType::new(PropertyBaseType::String).array();
        assert!(string_array.matches(&PropertyValue::Vec(vec![
            PropertyValue::String("a".to_string()),
            PropertyValue::String("b".to_string()),
        ])));
    }
}
