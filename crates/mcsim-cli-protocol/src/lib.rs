//! MeshCore CLI UART Protocol
//!
//! This crate provides types and utilities for communicating with MeshCore repeater
//! and room server firmware over their UART CLI interface. Unlike the companion
//! firmware which uses a binary framing protocol, the repeater and room server
//! use a simple line-based text CLI protocol.
//!
//! # Protocol Overview
//!
//! The CLI protocol is a simple line-based text interface:
//!
//! - **Commands** (host → firmware): Text commands terminated with `\r` (carriage return)
//! - **Responses** (firmware → host): Text prefixed with `  -> ` followed by the response
//! - **Echo**: Characters are echoed back as they are received
//!
//! # Command Types
//!
//! The firmware supports several categories of commands:
//!
//! - **Get commands**: `get <config_name>` - Returns configuration values prefixed with `> `
//! - **Set commands**: `set <config_name> <value>` - Sets configuration values, returns `OK`
//! - **Action commands**: `reboot`, `advert`, `clock sync`, etc.
//! - **Stats commands**: `neighbors`, `stats-core`, `stats-radio`, `stats-packets`
//!
//! # Example
//!
//! ```rust,ignore
//! use mcsim_cli_protocol::{Command, Response, LineCodec};
//!
//! // Build a command
//! let cmd = Command::GetConfig { name: "name".to_string() };
//! let line = cmd.encode();
//!
//! // Parse a response
//! let response = Response::parse("  -> OK")?;
//! ```

mod codec;
mod commands;
mod error;
mod responses;

pub use codec::*;
pub use commands::*;
pub use error::*;
pub use responses::*;
