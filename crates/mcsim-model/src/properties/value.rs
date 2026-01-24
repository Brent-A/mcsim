//! Property value types and conversion traits.
//!
//! This module provides:
//! - [`PropertyValue`] - The dynamic value type for properties
//! - [`FromPropertyValue`] - Trait for extracting typed values from PropertyValue
//! - [`ToPropertyValue`] - Trait for converting typed values to PropertyValue

use serde::{Deserialize, Serialize};
use std::time::Duration;

// ============================================================================
// Property Value Enum
// ============================================================================

/// The type of value a property can hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    /// Integer value (i64).
    Integer(i64),
    /// Floating point value (f64).
    Float(f64),
    /// String value.
    String(String),
    /// Boolean value.
    Bool(bool),
    /// Vector value.
    Vec(Vec<PropertyValue>),
    /// Null value.
    Null,
}

impl PropertyValue {
    /// Convert to i64 if possible.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            PropertyValue::Integer(v) => Some(*v),
            PropertyValue::Float(v) => Some(*v as i64),
            _ => None,
        }
    }

    /// Convert to u64 if possible.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            PropertyValue::Integer(v) if *v >= 0 => Some(*v as u64),
            PropertyValue::Float(v) if *v >= 0.0 => Some(*v as u64),
            _ => None,
        }
    }

    /// Convert to u32 if possible.
    pub fn as_u32(&self) -> Option<u32> {
        self.as_u64().and_then(|v| u32::try_from(v).ok())
    }

    /// Convert to u16 if possible.
    pub fn as_u16(&self) -> Option<u16> {
        self.as_u64().and_then(|v| u16::try_from(v).ok())
    }

    /// Convert to u8 if possible.
    pub fn as_u8(&self) -> Option<u8> {
        self.as_u64().and_then(|v| u8::try_from(v).ok())
    }

    /// Convert to i8 if possible.
    pub fn as_i8(&self) -> Option<i8> {
        self.as_i64().and_then(|v| i8::try_from(v).ok())
    }

    /// Convert to f64 if possible.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            PropertyValue::Float(v) => Some(*v),
            PropertyValue::Integer(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Convert to string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            PropertyValue::String(v) => Some(v),
            _ => None,
        }
    }

    /// Convert to bool if possible.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            PropertyValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Convert to vector if possible.
    pub fn as_vec(&self) -> Option<&Vec<PropertyValue>> {
        match self {
            PropertyValue::Vec(v) => Some(v),
            _ => None,
        }
    }

    /// Check if the value is null.
    pub fn is_null(&self) -> bool {
        matches!(self, PropertyValue::Null)
    }
}

impl std::fmt::Display for PropertyValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropertyValue::Integer(v) => write!(f, "{}", v),
            PropertyValue::Float(v) => write!(f, "{}", v),
            PropertyValue::String(v) => write!(f, "{}", v),
            PropertyValue::Bool(v) => write!(f, "{}", v),
            PropertyValue::Vec(v) => {
                let strs: Vec<String> = v.iter().map(|pv| pv.to_string()).collect();
                write!(f, "[{}]", strs.join(", "))
            }
            PropertyValue::Null => write!(f, "null"),
        }
    }
}

// ============================================================================
// From implementations for PropertyValue
// ============================================================================

impl From<i64> for PropertyValue {
    fn from(v: i64) -> Self {
        PropertyValue::Integer(v)
    }
}

impl From<Option<i64>> for PropertyValue {
    fn from(v: Option<i64>) -> Self {
        match v {
            Some(val) => PropertyValue::from(val),
            None => PropertyValue::Null,
        }
    }
}

impl From<i32> for PropertyValue {
    fn from(v: i32) -> Self {
        PropertyValue::Integer(v as i64)
    }
}

impl From<Option<i32>> for PropertyValue {
    fn from(v: Option<i32>) -> Self {
        match v {
            Some(val) => PropertyValue::from(val),
            None => PropertyValue::Null,
        }
    }
}

impl From<u64> for PropertyValue {
    fn from(v: u64) -> Self {
        PropertyValue::Integer(v as i64)
    }
}

impl From<Option<u64>> for PropertyValue {
    fn from(v: Option<u64>) -> Self {
        match v {
            Some(val) => PropertyValue::from(val),
            None => PropertyValue::Null,
        }
    }
}

impl From<u32> for PropertyValue {
    fn from(v: u32) -> Self {
        PropertyValue::Integer(v as i64)
    }
}

impl From<Option<u32>> for PropertyValue {
    fn from(v: Option<u32>) -> Self {
        match v {
            Some(val) => PropertyValue::from(val),
            None => PropertyValue::Null,
        }
    }
}

impl From<f64> for PropertyValue {
    fn from(v: f64) -> Self {
        PropertyValue::Float(v)
    }
}

impl From<Option<f64>> for PropertyValue {
    fn from(v: Option<f64>) -> Self {
        match v {
            Some(val) => PropertyValue::from(val),
            None => PropertyValue::Null,
        }
    }
}

impl From<f32> for PropertyValue {
    fn from(v: f32) -> Self {
        PropertyValue::Float(v as f64)
    }
}

impl From<Option<f32>> for PropertyValue {
    fn from(v: Option<f32>) -> Self {
        match v {
            Some(val) => PropertyValue::from(val),
            None => PropertyValue::Null,
        }
    }
}

impl From<String> for PropertyValue {
    fn from(v: String) -> Self {
        PropertyValue::String(v)
    }
}

impl From<Option<String>> for PropertyValue {
    fn from(v: Option<String>) -> Self {
        match v {
            Some(val) => PropertyValue::from(val),
            None => PropertyValue::Null,
        }
    }
}

impl From<&str> for PropertyValue {
    fn from(v: &str) -> Self {
        PropertyValue::String(v.to_string())
    }
}

impl From<Option<&str>> for PropertyValue {
    fn from(v: Option<&str>) -> Self {
        match v {
            Some(val) => PropertyValue::from(val),
            None => PropertyValue::Null,
        }
    }
}

impl From<bool> for PropertyValue {
    fn from(v: bool) -> Self {
        PropertyValue::Bool(v)
    }
}

impl From<Option<bool>> for PropertyValue {
    fn from(v: Option<bool>) -> Self {
        match v {
            Some(val) => PropertyValue::from(val),
            None => PropertyValue::Null,
        }
    }
}

// ============================================================================
// Type-Safe Property Value Extraction
// ============================================================================

/// Trait for types that can be extracted from a PropertyValue.
///
/// This enables type-safe property access where the Rust type is known
/// at compile time based on the property definition.
pub trait FromPropertyValue: Sized {
    /// Extract a value from a PropertyValue, returning a default if conversion fails.
    fn from_property_value(value: &PropertyValue) -> Option<Self>;
}

impl FromPropertyValue for i64 {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_i64()
    }
}

impl FromPropertyValue for Option<i64> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_i64().map(Some)
        }
    }
}

impl FromPropertyValue for u64 {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_u64()
    }
}

impl FromPropertyValue for Option<u64> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_u64().map(Some)
        }
    }
}

impl FromPropertyValue for u32 {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_u32()
    }
}

impl FromPropertyValue for Option<u32> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_u32().map(Some)
        }
    }
}

impl FromPropertyValue for u16 {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_u16()
    }
}

impl FromPropertyValue for Option<u16> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_u16().map(Some)
        }
    }
}

impl FromPropertyValue for u8 {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_u8()
    }
}

impl FromPropertyValue for Option<u8> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_u8().map(Some)
        }
    }
}

impl FromPropertyValue for i8 {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_i8()
    }
}

impl FromPropertyValue for Option<i8> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_i8().map(Some)
        }
    }
}

impl FromPropertyValue for f64 {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_f64()
    }
}

impl FromPropertyValue for Option<f64> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_f64().map(Some)
        }
    }
}

impl FromPropertyValue for f32 {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_f64().map(|v| v as f32)
    }
}

impl FromPropertyValue for Option<f32> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_f64().map(|v| Some(v as f32))
        }
    }
}

impl FromPropertyValue for String {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_str().map(|s| s.to_string())
    }
}

impl FromPropertyValue for Option<String> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_str().map(|s| Some(s.to_string()))
        }
    }
}

impl FromPropertyValue for bool {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_bool()
    }
}

impl FromPropertyValue for Option<bool> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_bool().map(Some)
        }
    }
}

impl FromPropertyValue for Duration {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value.as_f64().map(Duration::from_secs_f64)
    }
}

impl FromPropertyValue for Option<Duration> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value.as_f64().map(|v| Some(Duration::from_secs_f64(v)))
        }
    }
}

impl<T: FromPropertyValue> FromPropertyValue for Vec<T> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        value
            .as_vec()
            .map(|v| v.iter().filter_map(T::from_property_value).collect())
    }
}

impl<T: FromPropertyValue> FromPropertyValue for Option<Vec<T>> {
    fn from_property_value(value: &PropertyValue) -> Option<Self> {
        if value.is_null() {
            Some(None)
        } else {
            value
                .as_vec()
                .map(|v| Some(v.iter().filter_map(T::from_property_value).collect()))
        }
    }
}

// ============================================================================
// Type-Safe Property Value Insertion
// ============================================================================

/// Trait for types that can be converted into a PropertyValue.
///
/// This enables type-safe property setting where the Rust type is constrained
/// at compile time based on the property definition.
pub trait ToPropertyValue {
    /// Convert this value into a PropertyValue.
    fn to_property_value(self) -> PropertyValue;
}

impl ToPropertyValue for i64 {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Integer(self)
    }
}

impl ToPropertyValue for Option<i64> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Integer(v),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for u64 {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Integer(self as i64)
    }
}

impl ToPropertyValue for Option<u64> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Integer(v as i64),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for u32 {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Integer(self as i64)
    }
}

impl ToPropertyValue for Option<u32> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Integer(v as i64),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for u16 {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Integer(self as i64)
    }
}

impl ToPropertyValue for Option<u16> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Integer(v as i64),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for u8 {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Integer(self as i64)
    }
}

impl ToPropertyValue for Option<u8> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Integer(v as i64),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for i8 {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Integer(self as i64)
    }
}

impl ToPropertyValue for Option<i8> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Integer(v as i64),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for f64 {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Float(self)
    }
}

impl ToPropertyValue for Option<f64> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Float(v),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for f32 {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Float(self as f64)
    }
}

impl ToPropertyValue for Option<f32> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Float(v as f64),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for String {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::String(self)
    }
}

impl ToPropertyValue for Option<String> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::String(v),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for &str {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::String(self.to_string())
    }
}

impl ToPropertyValue for Option<&str> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::String(v.to_string()),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for bool {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Bool(self)
    }
}

impl ToPropertyValue for Option<bool> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Bool(v),
            None => PropertyValue::Null,
        }
    }
}

impl ToPropertyValue for Duration {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Float(self.as_secs_f64())
    }
}

impl ToPropertyValue for Option<Duration> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Float(v.as_secs_f64()),
            None => PropertyValue::Null,
        }
    }
}

impl<T: ToPropertyValue> ToPropertyValue for Vec<T> {
    fn to_property_value(self) -> PropertyValue {
        PropertyValue::Vec(self.into_iter().map(|v| v.to_property_value()).collect())
    }
}

impl<T: ToPropertyValue> ToPropertyValue for Option<Vec<T>> {
    fn to_property_value(self) -> PropertyValue {
        match self {
            Some(v) => PropertyValue::Vec(v.into_iter().map(|v| v.to_property_value()).collect()),
            None => PropertyValue::Null,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_property_value_conversions() {
        let v = PropertyValue::Integer(42);
        assert_eq!(v.as_i64(), Some(42));
        assert_eq!(v.as_u64(), Some(42));
        assert_eq!(v.as_f64(), Some(42.0));

        let v = PropertyValue::Float(3.14);
        assert_eq!(v.as_f64(), Some(3.14));
        assert_eq!(v.as_i64(), Some(3));

        let v = PropertyValue::String("hello".to_string());
        assert_eq!(v.as_str(), Some("hello"));
        assert_eq!(v.as_i64(), None);

        let v = PropertyValue::Bool(true);
        assert_eq!(v.as_bool(), Some(true));
    }
}
