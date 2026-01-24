//! MeshCore Companion UART Protocol
//!
//! This crate provides types and utilities for communicating with MeshCore companion
//! firmware over the UART protocol. The protocol uses framed messages where each
//! message starts with a command or response code byte.
//!
//! # Protocol Overview
//!
//! The companion firmware exposes a serial interface that allows a host application
//! to interact with the mesh network. Messages are either:
//!
//! - **Commands** (host → firmware): Start with a `CMD_*` byte
//! - **Responses** (firmware → host): Start with a `RESP_CODE_*` byte
//! - **Push notifications** (firmware → host): Start with a `PUSH_CODE_*` byte (0x80+)
//!
//! # Example
//!
//! ```rust,ignore
//! use mcsim_companion_protocol::{Command, Response, FrameCodec};
//!
//! // Build a command
//! let cmd = Command::DeviceQuery { app_version: 3 };
//! let frame = cmd.encode();
//!
//! // Parse a response
//! let response = Response::decode(&received_data)?;
//! ```

mod commands;
mod constants;
mod error;
mod frame;
mod responses;
mod types;

pub use commands::*;
pub use constants::*;
pub use error::*;
pub use frame::*;
pub use responses::*;
pub use types::*;
