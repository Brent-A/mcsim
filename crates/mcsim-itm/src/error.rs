//! Error types for ITM operations

use thiserror::Error;

/// Result type for ITM operations
pub type ItmResult<T> = Result<T, ItmError>;

/// Errors that can occur when using the ITM library
#[derive(Debug, Error)]
pub enum ItmError {
    /// ITM library DLL was not found
    #[error("ITM library not found. Ensure itm.dll is in the expected location.")]
    LibraryNotFound,

    /// Failed to load the ITM library
    #[error("Failed to load ITM library: {0}")]
    LoadError(String),

    /// Failed to find a symbol in the ITM library
    #[error("Symbol not found in ITM library: {0}")]
    SymbolNotFound(String),

    // ITM-specific error codes based on ERRORS_AND_WARNINGS.md

    /// TX terminal height is out of range
    #[error("TX terminal height out of range (must be 0.5 to 3000 meters)")]
    TxHeightOutOfRange,

    /// RX terminal height is out of range
    #[error("RX terminal height out of range (must be 0.5 to 3000 meters)")]
    RxHeightOutOfRange,

    /// Invalid climate value
    #[error("Invalid climate value (must be 1-7)")]
    InvalidClimate,

    /// Time percentage out of range
    #[error("Time percentage out of range (must be 0 < time < 100)")]
    TimeOutOfRange,

    /// Location percentage out of range
    #[error("Location percentage out of range (must be 0 < location < 100)")]
    LocationOutOfRange,

    /// Situation percentage out of range
    #[error("Situation percentage out of range (must be 0 < situation < 100)")]
    SituationOutOfRange,

    /// Confidence percentage out of range
    #[error("Confidence percentage out of range (must be 0 < confidence < 100)")]
    ConfidenceOutOfRange,

    /// Reliability percentage out of range
    #[error("Reliability percentage out of range (must be 0 < reliability < 100)")]
    ReliabilityOutOfRange,

    /// Surface refractivity out of range
    #[error("Surface refractivity out of range (must be 250 to 400 N-Units)")]
    RefractivityOutOfRange,

    /// Frequency out of range
    #[error("Frequency out of range (must be 20 to 20000 MHz)")]
    FrequencyOutOfRange,

    /// Invalid polarization value
    #[error("Invalid polarization value (must be 0 or 1)")]
    InvalidPolarization,

    /// Invalid epsilon (relative permittivity)
    #[error("Invalid epsilon value (must be > 1)")]
    InvalidEpsilon,

    /// Invalid sigma (conductivity)
    #[error("Invalid sigma value (must be > 0)")]
    InvalidSigma,

    /// Invalid mode of variability
    #[error("Invalid mode of variability")]
    InvalidMdvar,

    /// Path distance is out of range
    #[error("Path distance out of range")]
    DistanceOutOfRange,

    /// Delta H (terrain irregularity) is out of range
    #[error("Terrain irregularity parameter out of range")]
    DeltaHOutOfRange,

    /// Invalid siting criteria
    #[error("Invalid siting criteria")]
    InvalidSitingCriteria,

    /// Terrain profile is invalid
    #[error("Invalid terrain profile")]
    InvalidTerrainProfile,

    /// Unknown error code from ITM
    #[error("ITM error code: {0}")]
    Unknown(i32),
}

impl ItmError {
    /// Convert an ITM error code to an ItmError
    pub fn from_code(code: i32) -> Self {
        match code {
            1000 => ItmError::TxHeightOutOfRange,
            1001 => ItmError::RxHeightOutOfRange,
            1002 => ItmError::InvalidClimate,
            1003 => ItmError::TimeOutOfRange,
            1004 => ItmError::LocationOutOfRange,
            1005 => ItmError::SituationOutOfRange,
            1006 => ItmError::ConfidenceOutOfRange,
            1007 => ItmError::ReliabilityOutOfRange,
            1008 => ItmError::RefractivityOutOfRange,
            1009 => ItmError::FrequencyOutOfRange,
            1010 => ItmError::InvalidPolarization,
            1011 => ItmError::InvalidEpsilon,
            1012 => ItmError::InvalidSigma,
            1013 => ItmError::InvalidMdvar,
            1014 => ItmError::DistanceOutOfRange,
            1016 => ItmError::DeltaHOutOfRange,
            1017 => ItmError::InvalidSitingCriteria,
            1018 => ItmError::InvalidTerrainProfile,
            _ => ItmError::Unknown(code),
        }
    }
}
