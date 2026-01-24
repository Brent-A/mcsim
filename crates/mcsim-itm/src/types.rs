//! Type definitions for ITM parameters and results

use crate::ffi;

/// Radio climate classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Climate {
    /// Equatorial climate
    Equatorial = 1,
    /// Continental Subtropical climate
    ContinentalSubtropical = 2,
    /// Maritime Subtropical climate
    MaritimeSubtropical = 3,
    /// Desert climate
    Desert = 4,
    /// Continental Temperate climate
    ContinentalTemperate = 5,
    /// Maritime Temperate Over Land climate
    MaritimeTemperateOverLand = 6,
    /// Maritime Temperate Over Sea climate
    MaritimeTemperateOverSea = 7,
}

/// Polarization of the radio signal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Polarization {
    /// Horizontal polarization
    Horizontal = 0,
    /// Vertical polarization
    Vertical = 1,
}

/// Siting criteria for area mode predictions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SitingCriteria {
    /// Random siting - no special care taken
    Random = 0,
    /// Careful siting - some care taken in antenna placement
    Careful = 1,
    /// Very careful siting - significant care taken in antenna placement
    VeryCareful = 2,
}

/// Mode of variability for ITM calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeOfVariability(pub i32);

impl ModeOfVariability {
    /// Single Message Mode
    pub const SINGLE_MESSAGE: Self = Self(0);
    /// Accidental Mode
    pub const ACCIDENTAL: Self = Self(1);
    /// Mobile Mode
    pub const MOBILE: Self = Self(2);
    /// Broadcast Mode
    pub const BROADCAST: Self = Self(3);

    /// Create a mode with location variability eliminated
    pub fn eliminate_location(self) -> Self {
        Self(self.0 + 10)
    }

    /// Create a mode with situation variability eliminated
    pub fn eliminate_situation(self) -> Self {
        Self(self.0 + 20)
    }
}

impl From<ModeOfVariability> for i32 {
    fn from(mode: ModeOfVariability) -> i32 {
        mode.0
    }
}

/// Mode of propagation as determined by ITM
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropagationMode {
    /// Line of Sight propagation
    LineOfSight,
    /// Diffraction propagation
    Diffraction,
    /// Troposcatter propagation
    Troposcatter,
    /// Unknown mode
    Unknown(i32),
}

impl Default for PropagationMode {
    fn default() -> Self {
        PropagationMode::Unknown(0)
    }
}

impl From<i32> for PropagationMode {
    fn from(value: i32) -> Self {
        match value {
            1 => PropagationMode::LineOfSight,
            2 => PropagationMode::Diffraction,
            3 => PropagationMode::Troposcatter,
            _ => PropagationMode::Unknown(value),
        }
    }
}

/// Warning flags returned by ITM calculations
#[derive(Debug, Clone, Copy, Default)]
pub struct ItmWarnings {
    bits: i32,
}

impl ItmWarnings {
    /// Create warnings from raw bits
    pub fn from_bits(bits: i32) -> Self {
        Self { bits }
    }

    /// Get the raw warning bits
    pub fn bits(&self) -> i32 {
        self.bits
    }

    /// Check if there are any warnings
    pub fn has_warnings(&self) -> bool {
        self.bits != 0
    }

    /// Warning: TX terminal height limited internally
    pub fn tx_height_limited(&self) -> bool {
        self.bits & 0x0001 != 0
    }

    /// Warning: RX terminal height limited internally
    pub fn rx_height_limited(&self) -> bool {
        self.bits & 0x0002 != 0
    }

    /// Warning: Frequency extrapolated
    pub fn frequency_extrapolated(&self) -> bool {
        self.bits & 0x0004 != 0
    }

    /// Warning: Path distance too short for model
    pub fn path_distance_warning(&self) -> bool {
        self.bits & 0x0008 != 0
    }

    /// Warning: Delta H extrapolated
    pub fn delta_h_extrapolated(&self) -> bool {
        self.bits & 0x0010 != 0
    }
}

/// Basic result from ITM calculations
#[derive(Debug, Clone)]
pub struct ItmResult_ {
    /// Basic transmission loss in dB
    pub loss_db: f64,
    /// Warning flags
    pub warnings: ItmWarnings,
}

/// Extended result from ITM calculations with intermediate values
#[derive(Debug, Clone)]
pub struct ItmResultEx {
    /// Basic transmission loss in dB
    pub loss_db: f64,
    /// Warning flags
    pub warnings: ItmWarnings,
    /// Intermediate values from the calculation
    pub intermediate: IntermediateValues,
}

/// Intermediate values from ITM calculations
#[derive(Debug, Clone, Default)]
pub struct IntermediateValues {
    /// Terminal horizon angles in radians [TX, RX]
    pub theta_hzn: [f64; 2],
    /// Terminal horizon distances in meters [TX, RX]
    pub d_hzn_meter: [f64; 2],
    /// Terminal effective heights in meters [TX, RX]
    pub h_e_meter: [f64; 2],
    /// Surface refractivity in N-Units
    pub n_s: f64,
    /// Terrain irregularity parameter in meters
    pub delta_h_meter: f64,
    /// Reference attenuation in dB
    pub a_ref_db: f64,
    /// Free space basic transmission loss in dB
    pub a_fs_db: f64,
    /// Path distance in km
    pub d_km: f64,
    /// Mode of propagation
    pub mode: PropagationMode,
}

impl From<ffi::IntermediateValues> for IntermediateValues {
    fn from(v: ffi::IntermediateValues) -> Self {
        Self {
            theta_hzn: v.theta_hzn,
            d_hzn_meter: v.d_hzn__meter,
            h_e_meter: v.h_e__meter,
            n_s: v.N_s,
            delta_h_meter: v.delta_h__meter,
            a_ref_db: v.A_ref__db,
            a_fs_db: v.A_fs__db,
            d_km: v.d__km,
            mode: PropagationMode::from(v.mode),
        }
    }
}

/// Common ground constants for different terrain types
pub mod ground_constants {
    /// Average ground (epsilon=15, sigma=0.005)
    pub const AVERAGE_GROUND: (f64, f64) = (15.0, 0.005);
    /// Poor ground (epsilon=4, sigma=0.001)
    pub const POOR_GROUND: (f64, f64) = (4.0, 0.001);
    /// Good ground (epsilon=25, sigma=0.02)
    pub const GOOD_GROUND: (f64, f64) = (25.0, 0.02);
    /// Fresh water (epsilon=81, sigma=0.01)
    pub const FRESH_WATER: (f64, f64) = (81.0, 0.01);
    /// Sea water (epsilon=81, sigma=5.0)
    pub const SEA_WATER: (f64, f64) = (81.0, 5.0);
}

/// Typical surface refractivity values for different regions
pub mod refractivity {
    /// Equatorial regions (typical value)
    pub const EQUATORIAL: f64 = 360.0;
    /// Continental Subtropical (typical value)
    pub const CONTINENTAL_SUBTROPICAL: f64 = 320.0;
    /// Maritime Subtropical (typical value)
    pub const MARITIME_SUBTROPICAL: f64 = 370.0;
    /// Desert (typical value)
    pub const DESERT: f64 = 280.0;
    /// Continental Temperate (typical value)
    pub const CONTINENTAL_TEMPERATE: f64 = 301.0;
    /// Maritime Temperate Over Land (typical value)
    pub const MARITIME_TEMPERATE_LAND: f64 = 320.0;
    /// Maritime Temperate Over Sea (typical value)
    pub const MARITIME_TEMPERATE_SEA: f64 = 350.0;
}

/// Helper to build a terrain profile in PFL format
#[derive(Debug, Clone)]
pub struct TerrainProfile {
    elevations: Vec<f64>,
    resolution_meter: f64,
}

impl TerrainProfile {
    /// Create a new terrain profile with given resolution
    pub fn new(resolution_meter: f64) -> Self {
        Self {
            elevations: Vec::new(),
            resolution_meter,
        }
    }

    /// Add an elevation point
    pub fn add_elevation(&mut self, elevation_meter: f64) {
        self.elevations.push(elevation_meter);
    }

    /// Create from a slice of elevations
    pub fn from_elevations(resolution_meter: f64, elevations: &[f64]) -> Self {
        Self {
            elevations: elevations.to_vec(),
            resolution_meter,
        }
    }

    /// Convert to PFL format array for ITM
    ///
    /// PFL format:
    /// - pfl[0]: Number of elevation points - 1
    /// - pfl[1]: Resolution in meters
    /// - pfl[2..]: Elevation values
    pub fn to_pfl(&self) -> Vec<f64> {
        let mut pfl = Vec::with_capacity(self.elevations.len() + 2);
        pfl.push((self.elevations.len() - 1) as f64);
        pfl.push(self.resolution_meter);
        pfl.extend_from_slice(&self.elevations);
        pfl
    }

    /// Get the total path distance in meters
    pub fn path_distance_meter(&self) -> f64 {
        if self.elevations.is_empty() {
            return 0.0;
        }
        (self.elevations.len() - 1) as f64 * self.resolution_meter
    }
}
