//! FFI bindings to the ITM DLL
//!
//! The naming conventions here intentionally match the upstream ITM C++ API,
//! which uses double underscores to denote units (e.g., `h_tx__meter`).

#![allow(non_snake_case)]

/// Intermediate values structure matching the C++ struct
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct IntermediateValues {
    /// Terminal horizon angles in radians [TX, RX]
    pub theta_hzn: [f64; 2],
    /// Terminal horizon distances in meters [TX, RX]
    pub d_hzn__meter: [f64; 2],
    /// Terminal effective heights in meters [TX, RX]
    pub h_e__meter: [f64; 2],
    /// Surface refractivity in N-Units
    pub N_s: f64,
    /// Terrain irregularity parameter in meters
    pub delta_h__meter: f64,
    /// Reference attenuation in dB
    pub A_ref__db: f64,
    /// Free space basic transmission loss in dB
    pub A_fs__db: f64,
    /// Path distance in km
    pub d__km: f64,
    /// Mode of propagation (1=LOS, 2=Diffraction, 3=Troposcatter)
    pub mode: i32,
}

// Function pointer types for ITM DLL functions

/// ITM_P2P_TLS - Point-to-Point with Time/Location/Situation variability
pub type FnItmP2pTls = unsafe extern "C" fn(
    h_tx__meter: f64,
    h_rx__meter: f64,
    pfl: *const f64,
    climate: i32,
    N_0: f64,
    f__mhz: f64,
    pol: i32,
    epsilon: f64,
    sigma: f64,
    mdvar: i32,
    time: f64,
    location: f64,
    situation: f64,
    A__db: *mut f64,
    warnings: *mut i32,
) -> i32;

/// ITM_P2P_TLS_Ex - Point-to-Point TLS with extended output
pub type FnItmP2pTlsEx = unsafe extern "C" fn(
    h_tx__meter: f64,
    h_rx__meter: f64,
    pfl: *const f64,
    climate: i32,
    N_0: f64,
    f__mhz: f64,
    pol: i32,
    epsilon: f64,
    sigma: f64,
    mdvar: i32,
    time: f64,
    location: f64,
    situation: f64,
    A__db: *mut f64,
    warnings: *mut i32,
    interValues: *mut IntermediateValues,
) -> i32;

/// ITM_P2P_CR - Point-to-Point with Confidence/Reliability variability
pub type FnItmP2pCr = unsafe extern "C" fn(
    h_tx__meter: f64,
    h_rx__meter: f64,
    pfl: *const f64,
    climate: i32,
    N_0: f64,
    f__mhz: f64,
    pol: i32,
    epsilon: f64,
    sigma: f64,
    mdvar: i32,
    confidence: f64,
    reliability: f64,
    A__db: *mut f64,
    warnings: *mut i32,
) -> i32;

/// ITM_P2P_CR_Ex - Point-to-Point CR with extended output
pub type FnItmP2pCrEx = unsafe extern "C" fn(
    h_tx__meter: f64,
    h_rx__meter: f64,
    pfl: *const f64,
    climate: i32,
    N_0: f64,
    f__mhz: f64,
    pol: i32,
    epsilon: f64,
    sigma: f64,
    mdvar: i32,
    confidence: f64,
    reliability: f64,
    A__db: *mut f64,
    warnings: *mut i32,
    interValues: *mut IntermediateValues,
) -> i32;

/// ITM_AREA_TLS - Area mode with Time/Location/Situation variability
pub type FnItmAreaTls = unsafe extern "C" fn(
    h_tx__meter: f64,
    h_rx__meter: f64,
    tx_site_criteria: i32,
    rx_site_criteria: i32,
    d__km: f64,
    delta_h__meter: f64,
    climate: i32,
    N_0: f64,
    f__mhz: f64,
    pol: i32,
    epsilon: f64,
    sigma: f64,
    mdvar: i32,
    time: f64,
    location: f64,
    situation: f64,
    A__db: *mut f64,
    warnings: *mut i32,
) -> i32;

/// ITM_AREA_TLS_Ex - Area mode TLS with extended output
pub type FnItmAreaTlsEx = unsafe extern "C" fn(
    h_tx__meter: f64,
    h_rx__meter: f64,
    tx_site_criteria: i32,
    rx_site_criteria: i32,
    d__km: f64,
    delta_h__meter: f64,
    climate: i32,
    N_0: f64,
    f__mhz: f64,
    pol: i32,
    epsilon: f64,
    sigma: f64,
    mdvar: i32,
    time: f64,
    location: f64,
    situation: f64,
    A__db: *mut f64,
    warnings: *mut i32,
    interValues: *mut IntermediateValues,
) -> i32;

/// ITM_AREA_CR - Area mode with Confidence/Reliability variability
pub type FnItmAreaCr = unsafe extern "C" fn(
    h_tx__meter: f64,
    h_rx__meter: f64,
    tx_site_criteria: i32,
    rx_site_criteria: i32,
    d__km: f64,
    delta_h__meter: f64,
    climate: i32,
    N_0: f64,
    f__mhz: f64,
    pol: i32,
    epsilon: f64,
    sigma: f64,
    mdvar: i32,
    confidence: f64,
    reliability: f64,
    A__db: *mut f64,
    warnings: *mut i32,
) -> i32;

/// ITM_AREA_CR_Ex - Area mode CR with extended output
pub type FnItmAreaCrEx = unsafe extern "C" fn(
    h_tx__meter: f64,
    h_rx__meter: f64,
    tx_site_criteria: i32,
    rx_site_criteria: i32,
    d__km: f64,
    delta_h__meter: f64,
    climate: i32,
    N_0: f64,
    f__mhz: f64,
    pol: i32,
    epsilon: f64,
    sigma: f64,
    mdvar: i32,
    confidence: f64,
    reliability: f64,
    A__db: *mut f64,
    warnings: *mut i32,
    interValues: *mut IntermediateValues,
) -> i32;

/// ComputeDeltaH - Compute terrain irregularity parameter
pub type FnComputeDeltaH = unsafe extern "C" fn(
    pfl: *const f64,
    d_start__meter: f64,
    d_end__meter: f64,
) -> f64;

/// FreeSpaceLoss - Calculate free space path loss
pub type FnFreeSpaceLoss = unsafe extern "C" fn(d__meter: f64, f__mhz: f64) -> f64;
