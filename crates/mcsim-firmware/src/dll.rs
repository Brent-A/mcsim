//! Dynamic loading wrapper for MeshCore simulator DLLs.
//!
//! This module provides safe Rust wrappers around the C API exported by the
//! firmware DLLs (meshcore_repeater.dll, meshcore_room_server.dll, meshcore_companion.dll).
//!
//! # Example
//!
//! ```no_run
//! use mcsim_firmware::dll::{FirmwareDll, FirmwareNode, NodeConfig, FirmwareType};
//!
//! // Load the repeater DLL
//! let dll = FirmwareDll::load(FirmwareType::Repeater)?;
//!
//! // Create a node with default config
//! let config = NodeConfig::default();
//! let mut node = dll.create_node(&config)?;
//!
//! // Step the simulation
//! let result = node.step(1000, 1700000000);
//! println!("Node yielded: {:?}", result.reason);
//! # Ok::<(), mcsim_firmware::dll::DllError>(())
//! ```

use libloading::Library;
use std::ffi::{c_char, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

// ============================================================================
// Constants
// ============================================================================

/// Default initial RTC Unix timestamp (Nov 2023).
/// This matches the property default from mcsim_model::properties::FIRMWARE_INITIAL_RTC_SECS.
pub const DEFAULT_INITIAL_RTC_SECS: u64 = 1700000000;

/// Default spin detection threshold.
/// This matches the property default from mcsim_model::properties::FIRMWARE_SPIN_DETECTION_THRESHOLD.
pub const DEFAULT_SPIN_DETECTION_THRESHOLD: u32 = 3;

/// Default idle loops before yield.
/// This matches the property default from mcsim_model::properties::FIRMWARE_IDLE_LOOPS_BEFORE_YIELD.
pub const DEFAULT_IDLE_LOOPS_BEFORE_YIELD: u32 = 2;

/// Maximum length of node name.
pub const MAX_NODE_NAME: usize = 32;
/// Size of public key in bytes.
pub const PUB_KEY_SIZE: usize = 32;
/// Size of private key in bytes.
pub const PRV_KEY_SIZE: usize = 64;
/// Maximum radio packet size.
pub const MAX_RADIO_PACKET: usize = 256;
/// Maximum serial TX buffer size (must match SIM_MAX_SERIAL_TX in sim_api.h).
pub const MAX_SERIAL_TX: usize = 32768;
/// Maximum log output buffer size.
pub const MAX_LOG_OUTPUT: usize = 4096;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur when working with firmware DLLs.
#[derive(Debug, Error)]
pub enum DllError {
    /// Failed to load the DLL.
    #[error("Failed to load DLL: {0}")]
    LoadError(#[from] libloading::Error),

    /// Failed to find the DLL file.
    #[error("DLL not found: {0}")]
    NotFound(String),

    /// Failed to get a symbol from the DLL.
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    /// Node creation failed.
    #[error("Failed to create node")]
    CreateFailed,

    /// Invalid path string.
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// Filesystem operation failed.
    #[error("Filesystem error: {0}")]
    FilesystemError(i32),
}

// ============================================================================
// Firmware Simulation Parameters
// ============================================================================

/// Configuration parameters for firmware simulation behavior.
///
/// These parameters control timing and debugging aspects of the firmware simulation.
/// They are typically derived from simulation-scope properties.
#[derive(Debug, Clone)]
pub struct FirmwareSimulationParams {
    /// Poll count before detecting spin/yield.
    pub spin_detection_threshold: u32,
    /// Idle loop count before yield.
    pub idle_loops_before_yield: u32,
    /// Enable debug logging for spin detection.
    pub log_spin_detection: bool,
    /// Enable debug logging for loop iterations.
    pub log_loop_iterations: bool,
    /// Initial RTC Unix timestamp.
    pub initial_rtc_secs: u64,
    /// Startup time in microseconds. Events before this time are dropped.
    pub startup_time_us: u64,
}

impl Default for FirmwareSimulationParams {
    fn default() -> Self {
        Self {
            spin_detection_threshold: DEFAULT_SPIN_DETECTION_THRESHOLD,
            idle_loops_before_yield: DEFAULT_IDLE_LOOPS_BEFORE_YIELD,
            log_spin_detection: false,
            log_loop_iterations: false,
            initial_rtc_secs: DEFAULT_INITIAL_RTC_SECS,
            startup_time_us: 0,
        }
    }
}

// ============================================================================
// FFI Types
// ============================================================================

/// Yield reasons returned by the firmware.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YieldReason {
    /// Node is idle, waiting for next wake time.
    Idle = 0,
    /// Node started a radio transmission.
    RadioTxStart = 1,
    /// Radio transmission completed.
    RadioTxComplete = 2,
    /// Node requested a reboot.
    Reboot = 3,
    /// Node requested power off.
    PowerOff = 4,
    /// An error occurred.
    Error = 5,
}

/// Configuration for creating a firmware node.
#[repr(C)]
#[derive(Clone)]
pub struct NodeConfig {
    /// Ed25519 public key.
    pub public_key: [u8; PUB_KEY_SIZE],
    /// Ed25519 private key.
    pub private_key: [u8; PRV_KEY_SIZE],

    /// LoRa frequency in MHz (e.g., 915.0).
    pub lora_freq: f32,
    /// LoRa bandwidth in kHz (e.g., 250.0).
    pub lora_bw: f32,
    /// LoRa spreading factor (7-12).
    pub lora_sf: u8,
    /// LoRa coding rate (5-8).
    pub lora_cr: u8,
    /// TX power in dBm.
    pub lora_tx_power: u8,

    /// Initial millisecond clock value.
    pub initial_millis: u64,
    /// Initial RTC time (Unix timestamp).
    pub initial_rtc: u32,

    /// Seed for deterministic RNG.
    pub rng_seed: u32,

    /// Node name (null-terminated).
    pub node_name: [c_char; MAX_NODE_NAME],

    /// Spin detection threshold - poll count before detecting spin/yield.
    pub spin_detection_threshold: u32,
    /// Idle loops before yield.
    pub idle_loops_before_yield: u32,
    /// Enable debug logging for spin detection (bool as u8).
    pub log_spin_detection: u8,
    /// Enable debug logging for loop iterations (bool as u8).
    pub log_loop_iterations: u8,
    /// Alignment padding.
    _padding: [u8; 2],

    /// Reserved for future use.
    _reserved: [u8; 56],
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            public_key: [0; PUB_KEY_SIZE],
            private_key: [0; PRV_KEY_SIZE],
            lora_freq: 915.0,
            lora_bw: 250.0,
            lora_sf: 11,
            lora_cr: 5,
            lora_tx_power: 20,
            initial_millis: 0,
            initial_rtc: DEFAULT_INITIAL_RTC_SECS as u32,
            rng_seed: 12345,
            node_name: [0; MAX_NODE_NAME],
            spin_detection_threshold: DEFAULT_SPIN_DETECTION_THRESHOLD,
            idle_loops_before_yield: DEFAULT_IDLE_LOOPS_BEFORE_YIELD,
            log_spin_detection: 0,
            log_loop_iterations: 0,
            _padding: [0; 2],
            _reserved: [0; 56],
        }
    }
}

impl NodeConfig {
    /// Create a new config with the given name.
    pub fn with_name(mut self, name: &str) -> Self {
        let bytes = name.as_bytes();
        let len = bytes.len().min(MAX_NODE_NAME - 1);
        for (i, &b) in bytes[..len].iter().enumerate() {
            self.node_name[i] = b as c_char;
        }
        self.node_name[len] = 0;
        self
    }

    /// Set the keys from byte slices.
    pub fn with_keys(mut self, public_key: &[u8; 32], private_key: &[u8; 64]) -> Self {
        self.public_key.copy_from_slice(public_key);
        self.private_key.copy_from_slice(private_key);
        self
    }

    /// Set the RNG seed.
    pub fn with_rng_seed(mut self, seed: u32) -> Self {
        self.rng_seed = seed;
        self
    }

    /// Set the initial time values.
    pub fn with_initial_time(mut self, millis: u64, rtc: u32) -> Self {
        self.initial_millis = millis;
        self.initial_rtc = rtc;
        self
    }

    /// Set LoRa parameters.
    pub fn with_lora(mut self, freq: f32, bw: f32, sf: u8, cr: u8, tx_power: u8) -> Self {
        self.lora_freq = freq;
        self.lora_bw = bw;
        self.lora_sf = sf;
        self.lora_cr = cr;
        self.lora_tx_power = tx_power;
        self
    }

    /// Set spin detection configuration.
    pub fn with_spin_detection(mut self, threshold: u32, idle_loops: u32) -> Self {
        self.spin_detection_threshold = threshold;
        self.idle_loops_before_yield = idle_loops;
        self
    }

    /// Set spin detection logging options.
    pub fn with_spin_logging(mut self, log_spin: bool, log_loops: bool) -> Self {
        self.log_spin_detection = log_spin as u8;
        self.log_loop_iterations = log_loops as u8;
        self
    }
}

/// Result of a simulation step.
#[repr(C)]
#[derive(Clone)]
pub struct StepResult {
    /// Why the step yielded.
    pub reason: YieldReason,

    /// Current simulation time when step completed.
    pub current_millis: u64,
    /// Requested next wake time.
    pub wake_millis: u64,

    /// Radio TX data (valid when reason == RadioTxStart).
    pub radio_tx_data: [u8; MAX_RADIO_PACKET],
    /// Length of radio TX data.
    pub radio_tx_len: usize,
    /// Estimated TX airtime in milliseconds.
    pub radio_tx_airtime_ms: u32,

    /// Serial TX output (accumulated during step).
    pub serial_tx_data: [u8; MAX_SERIAL_TX],
    /// Length of serial TX data.
    pub serial_tx_len: usize,

    /// Log output from Serial.print() calls.
    pub log_output: [c_char; MAX_LOG_OUTPUT],
    /// Length of log output.
    pub log_output_len: usize,

    /// Error message (if reason == Error).
    pub error_msg: [c_char; 256],
}

impl StepResult {
    /// Get the radio TX data as a slice.
    pub fn radio_tx(&self) -> &[u8] {
        &self.radio_tx_data[..self.radio_tx_len]
    }

    /// Get the serial TX data as a slice.
    pub fn serial_tx(&self) -> &[u8] {
        &self.serial_tx_data[..self.serial_tx_len]
    }

    /// Get the log output as a string.
    pub fn log_output(&self) -> String {
        if self.log_output_len == 0 {
            return String::new();
        }
        let bytes: Vec<u8> = self.log_output[..self.log_output_len]
            .iter()
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Get the error message as a string.
    pub fn error_message(&self) -> Option<String> {
        if self.reason != YieldReason::Error {
            return None;
        }
        let cstr = unsafe { CStr::from_ptr(self.error_msg.as_ptr()) };
        Some(cstr.to_string_lossy().into_owned())
    }
}

impl std::fmt::Debug for StepResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepResult")
            .field("reason", &self.reason)
            .field("current_millis", &self.current_millis)
            .field("wake_millis", &self.wake_millis)
            .field("radio_tx_len", &self.radio_tx_len)
            .field("radio_tx_airtime_ms", &self.radio_tx_airtime_ms)
            .field("serial_tx_len", &self.serial_tx_len)
            .field("log_output_len", &self.log_output_len)
            .finish()
    }
}

/// Opaque node handle.
#[repr(C)]
struct SimNodeImpl {
    _private: [u8; 0],
}

type SimNodeHandle = *mut SimNodeImpl;

// ============================================================================
// Function Types
// ============================================================================

type FnSimCreate = unsafe extern "C" fn(*const NodeConfig) -> SimNodeHandle;
type FnSimDestroy = unsafe extern "C" fn(SimNodeHandle);
type FnSimReboot = unsafe extern "C" fn(SimNodeHandle, *const NodeConfig);
type FnSimStepBegin = unsafe extern "C" fn(SimNodeHandle, u64, u32);
type FnSimStepWait = unsafe extern "C" fn(SimNodeHandle) -> StepResult;
type FnSimStep = unsafe extern "C" fn(SimNodeHandle, u64, u32) -> StepResult;
type FnSimInjectRadioRx = unsafe extern "C" fn(SimNodeHandle, *const u8, usize, f32, f32);
type FnSimInjectSerialRx = unsafe extern "C" fn(SimNodeHandle, *const u8, usize);
type FnSimInjectSerialFrame = unsafe extern "C" fn(SimNodeHandle, *const u8, usize);
type FnSimCollectSerialFrame = unsafe extern "C" fn(SimNodeHandle, *mut u8, usize) -> usize;
type FnSimNotifyTxComplete = unsafe extern "C" fn(SimNodeHandle);
type FnSimNotifyStateChange = unsafe extern "C" fn(SimNodeHandle, u32);
type FnSimGetNodeType = unsafe extern "C" fn() -> *const c_char;
type FnSimGetPublicKey = unsafe extern "C" fn(SimNodeHandle, *mut u8);
type FnSimFsWrite = unsafe extern "C" fn(SimNodeHandle, *const c_char, *const u8, usize) -> i32;
type FnSimFsRead = unsafe extern "C" fn(SimNodeHandle, *const c_char, *mut u8, usize) -> i32;
type FnSimFsExists = unsafe extern "C" fn(SimNodeHandle, *const c_char) -> i32;
type FnSimFsRemove = unsafe extern "C" fn(SimNodeHandle, *const c_char) -> i32;

// ============================================================================
// Firmware Types
// ============================================================================

/// Available firmware types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirmwareType {
    /// Simple repeater node.
    Repeater,
    /// Room server node.
    RoomServer,
    /// Companion radio node.
    Companion,
}

impl FirmwareType {
    /// Get the DLL filename for this firmware type.
    pub fn dll_name(&self) -> &'static str {
        match self {
            FirmwareType::Repeater => "meshcore_repeater.dll",
            FirmwareType::RoomServer => "meshcore_room_server.dll",
            FirmwareType::Companion => "meshcore_companion.dll",
        }
    }
}

// ============================================================================
// FirmwareDll - Loaded DLL with function pointers
// ============================================================================

/// A loaded firmware DLL.
///
/// This struct holds the loaded library and provides methods to create nodes.
/// The library is kept alive as long as this struct (or any nodes created from it) exists.
pub struct FirmwareDll {
    _library: Arc<Library>,
    firmware_type: FirmwareType,

    // Function pointers
    sim_create: FnSimCreate,
    sim_destroy: FnSimDestroy,
    sim_reboot: FnSimReboot,
    sim_step_begin: FnSimStepBegin,
    sim_step_wait: FnSimStepWait,
    sim_step: FnSimStep,
    sim_inject_radio_rx: FnSimInjectRadioRx,
    sim_inject_serial_rx: FnSimInjectSerialRx,
    sim_inject_serial_frame: FnSimInjectSerialFrame,
    sim_collect_serial_frame: FnSimCollectSerialFrame,
    sim_notify_tx_complete: FnSimNotifyTxComplete,
    sim_notify_state_change: FnSimNotifyStateChange,
    sim_get_node_type: FnSimGetNodeType,
    sim_get_public_key: FnSimGetPublicKey,
    sim_fs_write: FnSimFsWrite,
    sim_fs_read: FnSimFsRead,
    sim_fs_exists: FnSimFsExists,
    sim_fs_remove: FnSimFsRemove,
}

impl FirmwareDll {
    /// Load a firmware DLL by type.
    ///
    /// This searches for the DLL in the build output directory.
    pub fn load(firmware_type: FirmwareType) -> Result<Self, DllError> {
        let dll_path = find_dll_path(firmware_type)?;
        Self::load_from_path(&dll_path, firmware_type)
    }

    /// Load a firmware DLL from a specific path.
    pub fn load_from_path(path: &Path, firmware_type: FirmwareType) -> Result<Self, DllError> {
        // SAFETY: We're loading a DLL that we built ourselves
        let library = unsafe { Library::new(path)? };
        let library = Arc::new(library);

        // Load all function pointers
        // SAFETY: We trust the DLL exports these symbols with the correct signatures
        unsafe {
            let sim_create: FnSimCreate = *library.get::<FnSimCreate>(b"sim_create")?;
            let sim_destroy: FnSimDestroy = *library.get::<FnSimDestroy>(b"sim_destroy")?;
            let sim_reboot: FnSimReboot = *library.get::<FnSimReboot>(b"sim_reboot")?;
            let sim_step_begin: FnSimStepBegin = *library.get::<FnSimStepBegin>(b"sim_step_begin")?;
            let sim_step_wait: FnSimStepWait = *library.get::<FnSimStepWait>(b"sim_step_wait")?;
            let sim_step: FnSimStep = *library.get::<FnSimStep>(b"sim_step")?;
            let sim_inject_radio_rx: FnSimInjectRadioRx =
                *library.get::<FnSimInjectRadioRx>(b"sim_inject_radio_rx")?;
            let sim_inject_serial_rx: FnSimInjectSerialRx =
                *library.get::<FnSimInjectSerialRx>(b"sim_inject_serial_rx")?;
            let sim_inject_serial_frame: FnSimInjectSerialFrame =
                *library.get::<FnSimInjectSerialFrame>(b"sim_inject_serial_frame")?;
            let sim_collect_serial_frame: FnSimCollectSerialFrame =
                *library.get::<FnSimCollectSerialFrame>(b"sim_collect_serial_frame")?;
            let sim_notify_tx_complete: FnSimNotifyTxComplete =
                *library.get::<FnSimNotifyTxComplete>(b"sim_notify_tx_complete")?;
            let sim_notify_state_change: FnSimNotifyStateChange =
                *library.get::<FnSimNotifyStateChange>(b"sim_notify_state_change")?;
            let sim_get_node_type: FnSimGetNodeType =
                *library.get::<FnSimGetNodeType>(b"sim_get_node_type")?;
            let sim_get_public_key: FnSimGetPublicKey =
                *library.get::<FnSimGetPublicKey>(b"sim_get_public_key")?;
            let sim_fs_write: FnSimFsWrite = *library.get::<FnSimFsWrite>(b"sim_fs_write")?;
            let sim_fs_read: FnSimFsRead = *library.get::<FnSimFsRead>(b"sim_fs_read")?;
            let sim_fs_exists: FnSimFsExists = *library.get::<FnSimFsExists>(b"sim_fs_exists")?;
            let sim_fs_remove: FnSimFsRemove = *library.get::<FnSimFsRemove>(b"sim_fs_remove")?;

            Ok(Self {
                _library: library,
                firmware_type,
                sim_create,
                sim_destroy,
                sim_reboot,
                sim_step_begin,
                sim_step_wait,
                sim_step,
                sim_inject_radio_rx,
                sim_inject_serial_rx,
                sim_inject_serial_frame,
                sim_collect_serial_frame,
                sim_notify_tx_complete,
                sim_notify_state_change,
                sim_get_node_type,
                sim_get_public_key,
                sim_fs_write,
                sim_fs_read,
                sim_fs_exists,
                sim_fs_remove,
            })
        }
    }

    /// Get the firmware type.
    pub fn firmware_type(&self) -> FirmwareType {
        self.firmware_type
    }

    /// Get the node type string from the DLL.
    pub fn node_type(&self) -> String {
        unsafe {
            let ptr = (self.sim_get_node_type)();
            if ptr.is_null() {
                return String::new();
            }
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    }

    /// Create a new firmware node.
    pub fn create_node(&self, config: &NodeConfig) -> Result<FirmwareNode<'_>, DllError> {
        let handle = unsafe { (self.sim_create)(config) };
        if handle.is_null() {
            return Err(DllError::CreateFailed);
        }
        Ok(FirmwareNode {
            dll: self,
            handle,
        })
    }
}

// ============================================================================
// FirmwareNode - A running firmware instance
// ============================================================================

/// A running firmware node instance.
///
/// This represents a single simulated node. The node is destroyed when this
/// struct is dropped.
pub struct FirmwareNode<'a> {
    dll: &'a FirmwareDll,
    handle: SimNodeHandle,
}

impl<'a> FirmwareNode<'a> {
    /// Get the public key of this node.
    pub fn public_key(&self) -> [u8; PUB_KEY_SIZE] {
        let mut key = [0u8; PUB_KEY_SIZE];
        unsafe {
            (self.dll.sim_get_public_key)(self.handle, key.as_mut_ptr());
        }
        key
    }

    /// Reboot the node with a new configuration.
    pub fn reboot(&mut self, config: &NodeConfig) {
        unsafe {
            (self.dll.sim_reboot)(self.handle, config);
        }
    }

    /// Begin an async simulation step.
    ///
    /// Call `step_wait()` to get the result.
    pub fn step_begin(&mut self, sim_millis: u64, sim_rtc_secs: u32) {
        unsafe {
            (self.dll.sim_step_begin)(self.handle, sim_millis, sim_rtc_secs);
        }
    }

    /// Wait for an async step to complete.
    pub fn step_wait(&mut self) -> StepResult {
        unsafe { (self.dll.sim_step_wait)(self.handle) }
    }

    /// Perform a synchronous simulation step.
    ///
    /// This combines `step_begin()` and `step_wait()`.
    pub fn step(&mut self, sim_millis: u64, sim_rtc_secs: u32) -> StepResult {
        unsafe { (self.dll.sim_step)(self.handle, sim_millis, sim_rtc_secs) }
    }

    /// Inject a received radio packet.
    pub fn inject_radio_rx(&mut self, data: &[u8], rssi: f32, snr: f32) {
        unsafe {
            (self.dll.sim_inject_radio_rx)(self.handle, data.as_ptr(), data.len(), rssi, snr);
        }
    }

    /// Inject received serial data.
    pub fn inject_serial_rx(&mut self, data: &[u8]) {
        unsafe {
            (self.dll.sim_inject_serial_rx)(self.handle, data.as_ptr(), data.len());
        }
    }

    /// Inject a serial frame (for frame-based interfaces like companion).
    pub fn inject_serial_frame(&mut self, data: &[u8]) {
        unsafe {
            (self.dll.sim_inject_serial_frame)(self.handle, data.as_ptr(), data.len());
        }
    }

    /// Collect a transmitted serial frame (for frame-based interfaces like companion).
    /// Returns Some(frame_data) if a frame was available, None otherwise.
    pub fn collect_serial_frame(&mut self) -> Option<Vec<u8>> {
        let mut buffer = vec![0u8; 256]; // MAX_FRAME_SIZE is 172, but use some margin
        let len = unsafe {
            (self.dll.sim_collect_serial_frame)(self.handle, buffer.as_mut_ptr(), buffer.len())
        };
        if len > 0 {
            buffer.truncate(len);
            Some(buffer)
        } else {
            None
        }
    }

    /// Notify that a radio transmission completed.
    pub fn notify_tx_complete(&mut self) {
        unsafe {
            (self.dll.sim_notify_tx_complete)(self.handle);
        }
    }

    /// Notify the radio of a state change (coordinator-driven state version update).
    /// This is used to synchronize radio state between the coordinator and the DLL.
    pub fn notify_state_change(&mut self, state_version: u32) {
        unsafe {
            (self.dll.sim_notify_state_change)(self.handle, state_version);
        }
    }

    /// Write a file to the node's filesystem.
    pub fn fs_write(&mut self, path: &str, data: &[u8]) -> Result<(), DllError> {
        let c_path = CString::new(path).map_err(|_| DllError::InvalidPath(path.to_string()))?;
        let result = unsafe {
            (self.dll.sim_fs_write)(self.handle, c_path.as_ptr(), data.as_ptr(), data.len())
        };
        if result < 0 {
            Err(DllError::FilesystemError(result))
        } else {
            Ok(())
        }
    }

    /// Read a file from the node's filesystem.
    pub fn fs_read(&mut self, path: &str, max_len: usize) -> Result<Vec<u8>, DllError> {
        let c_path = CString::new(path).map_err(|_| DllError::InvalidPath(path.to_string()))?;
        let mut buffer = vec![0u8; max_len];
        let result = unsafe {
            (self.dll.sim_fs_read)(self.handle, c_path.as_ptr(), buffer.as_mut_ptr(), max_len)
        };
        if result < 0 {
            Err(DllError::FilesystemError(result))
        } else {
            buffer.truncate(result as usize);
            Ok(buffer)
        }
    }

    /// Check if a file exists on the node's filesystem.
    pub fn fs_exists(&self, path: &str) -> Result<bool, DllError> {
        let c_path = CString::new(path).map_err(|_| DllError::InvalidPath(path.to_string()))?;
        let result = unsafe { (self.dll.sim_fs_exists)(self.handle, c_path.as_ptr()) };
        Ok(result != 0)
    }

    /// Remove a file from the node's filesystem.
    pub fn fs_remove(&mut self, path: &str) -> Result<(), DllError> {
        let c_path = CString::new(path).map_err(|_| DllError::InvalidPath(path.to_string()))?;
        let result = unsafe { (self.dll.sim_fs_remove)(self.handle, c_path.as_ptr()) };
        if result < 0 {
            Err(DllError::FilesystemError(result))
        } else {
            Ok(())
        }
    }
}

impl<'a> Drop for FirmwareNode<'a> {
    fn drop(&mut self) {
        unsafe {
            (self.dll.sim_destroy)(self.handle);
        }
    }
}

// SAFETY: FirmwareNode can be sent between threads as long as only one thread
// accesses it at a time (which is enforced by &mut self on all methods).
unsafe impl<'a> Send for FirmwareNode<'a> {}

// ============================================================================
// OwnedFirmwareNode - Self-contained firmware instance
// ============================================================================

/// An owned firmware node that can be stored persistently.
/// 
/// Unlike `FirmwareNode` which borrows the DLL, this type owns an Arc
/// to the DLL, allowing it to be stored in structs without lifetime issues.
pub struct OwnedFirmwareNode {
    dll: Arc<FirmwareDll>,
    handle: SimNodeHandle,
}

impl OwnedFirmwareNode {
    /// Create a new owned firmware node.
    pub fn new(dll: Arc<FirmwareDll>, config: &NodeConfig) -> Result<Self, DllError> {
        let handle = unsafe { (dll.sim_create)(config) };
        if handle.is_null() {
            return Err(DllError::CreateFailed);
        }
        Ok(OwnedFirmwareNode { dll, handle })
    }

    /// Get the public key of this node.
    pub fn public_key(&self) -> [u8; PUB_KEY_SIZE] {
        let mut key = [0u8; PUB_KEY_SIZE];
        unsafe {
            (self.dll.sim_get_public_key)(self.handle, key.as_mut_ptr());
        }
        key
    }

    /// Reboot the node with a new configuration.
    pub fn reboot(&mut self, config: &NodeConfig) {
        unsafe {
            (self.dll.sim_reboot)(self.handle, config);
        }
    }

    /// Begin an async simulation step.
    pub fn step_begin(&mut self, sim_millis: u64, sim_rtc_secs: u32) {
        unsafe {
            (self.dll.sim_step_begin)(self.handle, sim_millis, sim_rtc_secs);
        }
    }

    /// Wait for an async step to complete.
    pub fn step_wait(&mut self) -> StepResult {
        unsafe { (self.dll.sim_step_wait)(self.handle) }
    }

    /// Perform a synchronous simulation step.
    pub fn step(&mut self, sim_millis: u64, sim_rtc_secs: u32) -> StepResult {
        unsafe { (self.dll.sim_step)(self.handle, sim_millis, sim_rtc_secs) }
    }

    /// Inject a received radio packet.
    pub fn inject_radio_rx(&mut self, data: &[u8], rssi: f32, snr: f32) {
        unsafe {
            (self.dll.sim_inject_radio_rx)(self.handle, data.as_ptr(), data.len(), rssi, snr);
        }
    }

    /// Inject received serial data.
    pub fn inject_serial_rx(&mut self, data: &[u8]) {
        unsafe {
            (self.dll.sim_inject_serial_rx)(self.handle, data.as_ptr(), data.len());
        }
    }

    /// Notify the firmware that a radio TX completed.
    pub fn notify_tx_complete(&mut self) {
        unsafe {
            (self.dll.sim_notify_tx_complete)(self.handle);
        }
    }

    /// Notify the radio of a state change.
    pub fn notify_state_change(&mut self, state_version: u32) {
        unsafe {
            (self.dll.sim_notify_state_change)(self.handle, state_version);
        }
    }
}

impl Drop for OwnedFirmwareNode {
    fn drop(&mut self) {
        unsafe {
            (self.dll.sim_destroy)(self.handle);
        }
    }
}

// SAFETY: OwnedFirmwareNode can be sent between threads as long as only one thread
// accesses it at a time (which is enforced by &mut self on all methods).
unsafe impl Send for OwnedFirmwareNode {}

// ============================================================================
// Helper Functions
// ============================================================================

/// Find the path to a firmware DLL.
///
/// Searches in:
/// 1. Current directory
/// 2. OUT_DIR from build script (embedded at compile time)
/// 3. target/debug or target/release directories
fn find_dll_path(firmware_type: FirmwareType) -> Result<PathBuf, DllError> {
    let dll_name = firmware_type.dll_name();

    // Check current directory
    let current_dir = PathBuf::from(dll_name);
    if current_dir.exists() {
        return Ok(current_dir);
    }

    // Check OUT_DIR (set during build)
    if let Some(out_dir) = option_env!("OUT_DIR") {
        let out_path = PathBuf::from(out_dir).join(dll_name);
        if out_path.exists() {
            return Ok(out_path);
        }
    }

    // Check relative to executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_dll = exe_dir.join(dll_name);
            if exe_dll.exists() {
                return Ok(exe_dll);
            }
        }
    }

    // Check target directories
    for profile in &["debug", "release"] {
        // Check target/<profile>/build/mcsim-firmware-*/out/
        let target_dir = PathBuf::from("target")
            .join(profile)
            .join("build");
        
        if target_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&target_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(name) = path.file_name() {
                            if name.to_string_lossy().starts_with("mcsim-firmware-") {
                                let out_dll = path.join("out").join(dll_name);
                                if out_dll.exists() {
                                    return Ok(out_dll);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err(DllError::NotFound(dll_name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_config_default() {
        let config = NodeConfig::default();
        assert_eq!(config.lora_freq, 915.0);
        assert_eq!(config.lora_sf, 11);
        assert_eq!(config.rng_seed, 12345);
        assert_eq!(config.lora_bw, 250.0);
        assert_eq!(config.lora_cr, 5);
        assert_eq!(config.lora_tx_power, 20);
        assert_eq!(config.initial_millis, 0);
        assert_eq!(config.initial_rtc, DEFAULT_INITIAL_RTC_SECS as u32);
        assert_eq!(config.spin_detection_threshold, DEFAULT_SPIN_DETECTION_THRESHOLD);
        assert_eq!(config.idle_loops_before_yield, DEFAULT_IDLE_LOOPS_BEFORE_YIELD);
        assert_eq!(config.log_spin_detection, 0);
        assert_eq!(config.log_loop_iterations, 0);
    }

    #[test]
    fn test_firmware_simulation_params_default() {
        let params = FirmwareSimulationParams::default();
        assert_eq!(params.spin_detection_threshold, DEFAULT_SPIN_DETECTION_THRESHOLD);
        assert_eq!(params.idle_loops_before_yield, DEFAULT_IDLE_LOOPS_BEFORE_YIELD);
        assert_eq!(params.log_spin_detection, false);
        assert_eq!(params.log_loop_iterations, false);
        assert_eq!(params.initial_rtc_secs, DEFAULT_INITIAL_RTC_SECS);
    }

    #[test]
    fn test_node_config_builder() {
        let config = NodeConfig::default()
            .with_name("test_node")
            .with_rng_seed(42)
            .with_initial_time(1000, 1700000000);

        assert_eq!(config.rng_seed, 42);
        assert_eq!(config.initial_millis, 1000);
        assert_eq!(config.initial_rtc, 1700000000);
    }

    #[test]
    fn test_node_config_with_name() {
        let config = NodeConfig::default().with_name("my_node");
        // Check the name was set (first few bytes should match)
        assert_eq!(config.node_name[0], b'm' as c_char);
        assert_eq!(config.node_name[1], b'y' as c_char);
        assert_eq!(config.node_name[7], 0); // null terminator
    }

    #[test]
    fn test_node_config_with_name_truncation() {
        // Name longer than MAX_NODE_NAME should be truncated
        let long_name = "this_is_a_very_long_node_name_that_exceeds_the_maximum";
        let config = NodeConfig::default().with_name(long_name);
        // Should be truncated and null-terminated
        assert_eq!(config.node_name[MAX_NODE_NAME - 1], 0);
    }

    #[test]
    fn test_node_config_with_keys() {
        let pub_key = [1u8; 32];
        let prv_key = [2u8; 64];
        let config = NodeConfig::default().with_keys(&pub_key, &prv_key);

        assert_eq!(config.public_key, pub_key);
        assert_eq!(config.private_key, prv_key);
    }

    #[test]
    fn test_node_config_with_lora() {
        let config = NodeConfig::default().with_lora(868.0, 125.0, 12, 8, 14);

        assert_eq!(config.lora_freq, 868.0);
        assert_eq!(config.lora_bw, 125.0);
        assert_eq!(config.lora_sf, 12);
        assert_eq!(config.lora_cr, 8);
        assert_eq!(config.lora_tx_power, 14);
    }

    #[test]
    fn test_firmware_type_dll_names() {
        assert_eq!(FirmwareType::Repeater.dll_name(), "meshcore_repeater.dll");
        assert_eq!(
            FirmwareType::RoomServer.dll_name(),
            "meshcore_room_server.dll"
        );
        assert_eq!(FirmwareType::Companion.dll_name(), "meshcore_companion.dll");
    }

    #[test]
    fn test_yield_reason_values() {
        // Ensure enum values match C API
        assert_eq!(YieldReason::Idle as i32, 0);
        assert_eq!(YieldReason::RadioTxStart as i32, 1);
        assert_eq!(YieldReason::RadioTxComplete as i32, 2);
        assert_eq!(YieldReason::Reboot as i32, 3);
        assert_eq!(YieldReason::PowerOff as i32, 4);
        assert_eq!(YieldReason::Error as i32, 5);
    }

    #[test]
    fn test_find_dll_path_not_found() {
        // A non-existent firmware type would fail, but we can test error handling
        // by checking that find_dll_path returns NotFound for files that don't exist
        // in standard locations (assuming tests run from a clean directory)
    }

    // ========================================================================
    // Integration tests (require DLLs to be built)
    // ========================================================================

    #[test]
    fn test_load_repeater_dll() {
        match FirmwareDll::load(FirmwareType::Repeater) {
            Ok(dll) => {
                assert_eq!(dll.firmware_type(), FirmwareType::Repeater);
                let node_type = dll.node_type();
                assert!(!node_type.is_empty(), "Node type should not be empty");
            }
            Err(DllError::NotFound(_)) => {
                // DLL not built yet, skip test
                println!("Skipping test: DLL not found (run cargo build first)");
            }
            Err(e) => panic!("Unexpected error loading DLL: {}", e),
        }
    }

    #[test]
    fn test_load_room_server_dll() {
        match FirmwareDll::load(FirmwareType::RoomServer) {
            Ok(dll) => {
                assert_eq!(dll.firmware_type(), FirmwareType::RoomServer);
            }
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found (run cargo build first)");
            }
            Err(e) => panic!("Unexpected error loading DLL: {}", e),
        }
    }

    #[test]
    fn test_load_companion_dll() {
        match FirmwareDll::load(FirmwareType::Companion) {
            Ok(dll) => {
                assert_eq!(dll.firmware_type(), FirmwareType::Companion);
            }
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found (run cargo build first)");
            }
            Err(e) => panic!("Unexpected error loading DLL: {}", e),
        }
    }

    #[test]
    fn test_create_and_destroy_node() {
        let dll = match FirmwareDll::load(FirmwareType::Repeater) {
            Ok(dll) => dll,
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found");
                return;
            }
            Err(e) => panic!("Unexpected error: {}", e),
        };

        let config = NodeConfig::default()
            .with_name("test_node")
            .with_rng_seed(12345);

        let node = dll.create_node(&config);
        assert!(node.is_ok(), "Failed to create node: {:?}", node.err());

        // Node is automatically destroyed when dropped
    }

    #[test]
    fn test_node_public_key() {
        let dll = match FirmwareDll::load(FirmwareType::Repeater) {
            Ok(dll) => dll,
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found");
                return;
            }
            Err(e) => panic!("Unexpected error: {}", e),
        };

        let pub_key = [0x42u8; 32];
        let prv_key = [0x00u8; 64];
        let config = NodeConfig::default().with_keys(&pub_key, &prv_key);

        let node = dll.create_node(&config).expect("Failed to create node");
        let retrieved_key = node.public_key();

        // The node should have a public key (may be generated if config key was invalid)
        assert_eq!(retrieved_key.len(), PUB_KEY_SIZE);
    }

    #[test]
    fn test_node_step() {
        let dll = match FirmwareDll::load(FirmwareType::Repeater) {
            Ok(dll) => dll,
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found");
                return;
            }
            Err(e) => panic!("Unexpected error: {}", e),
        };

        let config = NodeConfig::default()
            .with_name("step_test")
            .with_rng_seed(999)
            .with_initial_time(0, 1700000000);

        let mut node = dll.create_node(&config).expect("Failed to create node");

        // Step the simulation forward
        let result = node.step(1000, 1700000001);

        // Should yield with some reason
        println!("Step result: {:?}", result);
        assert!(result.current_millis >= 1000);
    }

    #[test]
    fn test_node_step_sequence() {
        let dll = match FirmwareDll::load(FirmwareType::Repeater) {
            Ok(dll) => dll,
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found");
                return;
            }
            Err(e) => panic!("Unexpected error: {}", e),
        };

        let config = NodeConfig::default()
            .with_name("seq_test")
            .with_rng_seed(42);

        let mut node = dll.create_node(&config).expect("Failed to create node");

        // Run several steps
        let mut time_ms = 0u64;
        for i in 0..10 {
            let result = node.step(time_ms, 1700000000 + (time_ms / 1000) as u32);
            println!("Step {}: reason={:?}, wake_at={}", i, result.reason, result.wake_millis);

            // Advance time to wake time or by 1 second
            time_ms = result.wake_millis.max(time_ms + 1000);
        }
    }

    #[test]
    fn test_node_async_step() {
        let dll = match FirmwareDll::load(FirmwareType::Repeater) {
            Ok(dll) => dll,
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found");
                return;
            }
            Err(e) => panic!("Unexpected error: {}", e),
        };

        let config = NodeConfig::default().with_name("async_test");
        let mut node = dll.create_node(&config).expect("Failed to create node");

        // Use async stepping API
        node.step_begin(1000, 1700000000);
        let result = node.step_wait();

        assert!(result.current_millis >= 1000);
    }

    #[test]
    fn test_node_radio_injection() {
        let dll = match FirmwareDll::load(FirmwareType::Repeater) {
            Ok(dll) => dll,
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found");
                return;
            }
            Err(e) => panic!("Unexpected error: {}", e),
        };

        let config = NodeConfig::default().with_name("radio_test");
        let mut node = dll.create_node(&config).expect("Failed to create node");

        // Inject a fake radio packet
        let fake_packet = [0x01, 0x02, 0x03, 0x04, 0x05];
        node.inject_radio_rx(&fake_packet, -60.0, 10.0);

        // Step to process it
        let result = node.step(1000, 1700000000);
        println!("After injection: {:?}", result);
    }

    #[test]
    fn test_node_filesystem() {
        let dll = match FirmwareDll::load(FirmwareType::Repeater) {
            Ok(dll) => dll,
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found");
                return;
            }
            Err(e) => panic!("Unexpected error: {}", e),
        };

        let config = NodeConfig::default().with_name("fs_test");
        let mut node = dll.create_node(&config).expect("Failed to create node");

        // Write a test file
        let test_data = b"Hello, MeshCore!";
        let write_result = node.fs_write("/test.txt", test_data);
        
        if write_result.is_ok() {
            // Check it exists
            assert!(node.fs_exists("/test.txt").unwrap_or(false));

            // Read it back
            let read_data = node.fs_read("/test.txt", 256);
            if let Ok(data) = read_data {
                assert_eq!(&data, test_data);
            }

            // Remove it
            let _ = node.fs_remove("/test.txt");
            assert!(!node.fs_exists("/test.txt").unwrap_or(true));
        }
    }

    #[test]
    fn test_step_result_accessors() {
        // Test the StepResult accessor methods with a mock result
        let mut result = StepResult {
            reason: YieldReason::RadioTxStart,
            current_millis: 1000,
            wake_millis: 2000,
            radio_tx_data: [0; MAX_RADIO_PACKET],
            radio_tx_len: 5,
            radio_tx_airtime_ms: 100,
            serial_tx_data: [0; MAX_SERIAL_TX],
            serial_tx_len: 3,
            log_output: [0; MAX_LOG_OUTPUT],
            log_output_len: 0,
            error_msg: [0; 256],
        };

        // Set some test data
        result.radio_tx_data[0..5].copy_from_slice(&[1, 2, 3, 4, 5]);
        result.serial_tx_data[0..3].copy_from_slice(&[b'A', b'B', b'C']);

        assert_eq!(result.radio_tx(), &[1, 2, 3, 4, 5]);
        assert_eq!(result.serial_tx(), &[b'A', b'B', b'C']);
        assert_eq!(result.log_output(), "");
        assert!(result.error_message().is_none());
    }

    #[test]
    fn test_step_result_error_message() {
        let mut result = StepResult {
            reason: YieldReason::Error,
            current_millis: 0,
            wake_millis: 0,
            radio_tx_data: [0; MAX_RADIO_PACKET],
            radio_tx_len: 0,
            radio_tx_airtime_ms: 0,
            serial_tx_data: [0; MAX_SERIAL_TX],
            serial_tx_len: 0,
            log_output: [0; MAX_LOG_OUTPUT],
            log_output_len: 0,
            error_msg: [0; 256],
        };

        // Set an error message
        let msg = b"Test error\0";
        for (i, &b) in msg.iter().enumerate() {
            result.error_msg[i] = b as c_char;
        }

        let err = result.error_message();
        assert!(err.is_some());
        assert_eq!(err.unwrap(), "Test error");
    }

    #[test]
    fn test_multiple_nodes_same_dll() {
        let dll = match FirmwareDll::load(FirmwareType::Repeater) {
            Ok(dll) => dll,
            Err(DllError::NotFound(_)) => {
                println!("Skipping test: DLL not found");
                return;
            }
            Err(e) => panic!("Unexpected error: {}", e),
        };

        // Create multiple nodes from the same DLL
        let config1 = NodeConfig::default().with_name("node1").with_rng_seed(1);
        let config2 = NodeConfig::default().with_name("node2").with_rng_seed(2);

        let mut node1 = dll.create_node(&config1).expect("Failed to create node1");
        let mut node2 = dll.create_node(&config2).expect("Failed to create node2");

        // Step both nodes
        let result1 = node1.step(1000, 1700000000);
        let result2 = node2.step(1000, 1700000000);

        println!("Node1: {:?}", result1.reason);
        println!("Node2: {:?}", result2.reason);

        // Both should work independently
        assert!(result1.current_millis >= 1000);
        assert!(result2.current_millis >= 1000);
    }
}
