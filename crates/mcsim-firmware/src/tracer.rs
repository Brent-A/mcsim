//! Firmware tracing support using the common entity tracer.
//!
//! This module provides firmware-specific tracing helpers that work with
//! the generic EntityTracer from mcsim-common.

pub use mcsim_common::entity_tracer::{
    EntityTracer, EntityTracerConfig, FirmwareYieldReason, TraceCategory, TraceEvent,
};
