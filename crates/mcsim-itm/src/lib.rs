//! Rust wrapper for the NTIA Irregular Terrain Model (ITM)
//!
//! This crate provides safe Rust bindings to the ITM library, which predicts
//! terrestrial radiowave propagation for frequencies between 20 MHz and 20 GHz.
//!
//! # Example
//!
//! ```no_run
//! use mcsim_itm::{Itm, Climate, Polarization, SitingCriteria};
//!
//! let itm = Itm::new().expect("Failed to load ITM library");
//!
//! // Point-to-point prediction with terrain profile
//! let pfl = vec![10.0, 100.0, 50.0, 55.0, 60.0, 58.0, 52.0, 48.0, 45.0, 42.0, 40.0, 38.0];
//! let result = itm.p2p_tls(
//!     10.0,           // tx height in meters
//!     2.0,            // rx height in meters
//!     &pfl,           // terrain profile
//!     Climate::ContinentalTemperate,
//!     301.0,          // surface refractivity N_0
//!     915.0,          // frequency in MHz
//!     Polarization::Vertical,
//!     15.0,           // relative permittivity (epsilon)
//!     0.005,          // conductivity (sigma) in S/m
//!     0,              // mode of variability
//!     50.0,           // time variability %
//!     50.0,           // location variability %
//!     50.0,           // situation variability %
//! ).expect("ITM calculation failed");
//!
//! println!("Path loss: {} dB", result.loss_db);
//! ```

mod error;
mod ffi;
mod types;

pub use error::{ItmError, ItmResult};
pub use types::*;

use std::path::Path;

/// Handle to the loaded ITM library
pub struct Itm {
    lib: libloading::Library,
}

impl Itm {
    /// Load the ITM library from the default location
    ///
    /// This searches for the itm library in the following locations:
    /// 1. The `itm` directory relative to the executable
    /// 2. The current working directory
    /// 3. System PATH
    pub fn new() -> ItmResult<Self> {
        // Try different paths to find the DLL
        let possible_paths = [
            "itm/itm.dll",
            "itm.dll",
            "../itm/itm.dll",
            "../../itm/itm.dll",

            "itm/libitm.so",
            "libitm.so",
            "../itm/libitm.so",
            "../../itm/libitm.so",

            "itm/libitm.dylib",
            "libitm.dylib",
            "../itm/libitm.dylib",
            "../../itm/libitm.dylib",
        ];

        for path in &possible_paths {
            if let Ok(itm) = Self::from_path(path) {
                return Ok(itm);
            }
        }

        Err(ItmError::LibraryNotFound)
    }

    /// Load the ITM library from a specific path
    pub fn from_path<P: AsRef<Path>>(path: P) -> ItmResult<Self> {
        let lib = unsafe {
            libloading::Library::new(path.as_ref()).map_err(|e| ItmError::LoadError(e.to_string()))?
        };
        Ok(Self { lib })
    }

    /// Point-to-Point prediction mode using Time/Location/Situation variability
    ///
    /// # Arguments
    ///
    /// * `h_tx_meter` - Structural height of the transmitter (0.5 to 3000 meters)
    /// * `h_rx_meter` - Structural height of the receiver (0.5 to 3000 meters)
    /// * `pfl` - Terrain profile in PFL format:
    ///   - `pfl[0]`: Number of elevation points - 1
    ///   - `pfl[1]`: Resolution in meters
    ///   - `pfl[2..]`: Elevation above sea level in meters
    /// * `climate` - Radio climate classification
    /// * `n_0` - Minimum monthly mean surface refractivity reduced to sea level (250 to 400 N-Units)
    /// * `f_mhz` - Frequency in MHz (20 to 20000)
    /// * `pol` - Polarization
    /// * `epsilon` - Relative permittivity (> 1)
    /// * `sigma` - Conductivity in S/m (> 0)
    /// * `mdvar` - Mode of variability (0-3, optionally +10 or +20)
    /// * `time` - Time variability percentage (0 < time < 100)
    /// * `location` - Location variability percentage (0 < location < 100)
    /// * `situation` - Situation variability percentage (0 < situation < 100)
    #[allow(clippy::too_many_arguments)]
    pub fn p2p_tls(
        &self,
        h_tx_meter: f64,
        h_rx_meter: f64,
        pfl: &[f64],
        climate: Climate,
        n_0: f64,
        f_mhz: f64,
        pol: Polarization,
        epsilon: f64,
        sigma: f64,
        mdvar: i32,
        time: f64,
        location: f64,
        situation: f64,
    ) -> ItmResult<ItmResult_> {
        let mut a_db: f64 = 0.0;
        let mut warnings: i32 = 0;

        let func: libloading::Symbol<ffi::FnItmP2pTls> = unsafe {
            self.lib
                .get(b"ITM_P2P_TLS")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        let error_code = unsafe {
            func(
                h_tx_meter,
                h_rx_meter,
                pfl.as_ptr(),
                climate as i32,
                n_0,
                f_mhz,
                pol as i32,
                epsilon,
                sigma,
                mdvar,
                time,
                location,
                situation,
                &mut a_db,
                &mut warnings,
            )
        };

        // 0 = SUCCESS, 1 = SUCCESS_WITH_WARNINGS, >= 1000 are errors
        if error_code >= 1000 {
            return Err(ItmError::from_code(error_code));
        }

        Ok(ItmResult_ {
            loss_db: a_db,
            warnings: ItmWarnings::from_bits(warnings),
        })
    }

    /// Point-to-Point prediction mode using Time/Location/Situation variability
    /// with extended output (intermediate values)
    #[allow(clippy::too_many_arguments)]
    pub fn p2p_tls_ex(
        &self,
        h_tx_meter: f64,
        h_rx_meter: f64,
        pfl: &[f64],
        climate: Climate,
        n_0: f64,
        f_mhz: f64,
        pol: Polarization,
        epsilon: f64,
        sigma: f64,
        mdvar: i32,
        time: f64,
        location: f64,
        situation: f64,
    ) -> ItmResult<ItmResultEx> {
        let mut a_db: f64 = 0.0;
        let mut warnings: i32 = 0;
        let mut inter_values = ffi::IntermediateValues::default();

        let func: libloading::Symbol<ffi::FnItmP2pTlsEx> = unsafe {
            self.lib
                .get(b"ITM_P2P_TLS_Ex")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        let error_code = unsafe {
            func(
                h_tx_meter,
                h_rx_meter,
                pfl.as_ptr(),
                climate as i32,
                n_0,
                f_mhz,
                pol as i32,
                epsilon,
                sigma,
                mdvar,
                time,
                location,
                situation,
                &mut a_db,
                &mut warnings,
                &mut inter_values,
            )
        };

        if error_code >= 1000 {
            return Err(ItmError::from_code(error_code));
        }

        Ok(ItmResultEx {
            loss_db: a_db,
            warnings: ItmWarnings::from_bits(warnings),
            intermediate: IntermediateValues::from(inter_values),
        })
    }

    /// Point-to-Point prediction mode using Confidence/Reliability variability
    #[allow(clippy::too_many_arguments)]
    pub fn p2p_cr(
        &self,
        h_tx_meter: f64,
        h_rx_meter: f64,
        pfl: &[f64],
        climate: Climate,
        n_0: f64,
        f_mhz: f64,
        pol: Polarization,
        epsilon: f64,
        sigma: f64,
        mdvar: i32,
        confidence: f64,
        reliability: f64,
    ) -> ItmResult<ItmResult_> {
        let mut a_db: f64 = 0.0;
        let mut warnings: i32 = 0;

        let func: libloading::Symbol<ffi::FnItmP2pCr> = unsafe {
            self.lib
                .get(b"ITM_P2P_CR")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        let error_code = unsafe {
            func(
                h_tx_meter,
                h_rx_meter,
                pfl.as_ptr(),
                climate as i32,
                n_0,
                f_mhz,
                pol as i32,
                epsilon,
                sigma,
                mdvar,
                confidence,
                reliability,
                &mut a_db,
                &mut warnings,
            )
        };

        if error_code >= 1000 {
            return Err(ItmError::from_code(error_code));
        }

        Ok(ItmResult_ {
            loss_db: a_db,
            warnings: ItmWarnings::from_bits(warnings),
        })
    }

    /// Point-to-Point prediction mode using Confidence/Reliability variability
    /// with extended output (intermediate values)
    #[allow(clippy::too_many_arguments)]
    pub fn p2p_cr_ex(
        &self,
        h_tx_meter: f64,
        h_rx_meter: f64,
        pfl: &[f64],
        climate: Climate,
        n_0: f64,
        f_mhz: f64,
        pol: Polarization,
        epsilon: f64,
        sigma: f64,
        mdvar: i32,
        confidence: f64,
        reliability: f64,
    ) -> ItmResult<ItmResultEx> {
        let mut a_db: f64 = 0.0;
        let mut warnings: i32 = 0;
        let mut inter_values = ffi::IntermediateValues::default();

        let func: libloading::Symbol<ffi::FnItmP2pCrEx> = unsafe {
            self.lib
                .get(b"ITM_P2P_CR_Ex")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        let error_code = unsafe {
            func(
                h_tx_meter,
                h_rx_meter,
                pfl.as_ptr(),
                climate as i32,
                n_0,
                f_mhz,
                pol as i32,
                epsilon,
                sigma,
                mdvar,
                confidence,
                reliability,
                &mut a_db,
                &mut warnings,
                &mut inter_values,
            )
        };

        if error_code >= 1000 {
            return Err(ItmError::from_code(error_code));
        }

        Ok(ItmResultEx {
            loss_db: a_db,
            warnings: ItmWarnings::from_bits(warnings),
            intermediate: IntermediateValues::from(inter_values),
        })
    }

    /// Area prediction mode using Time/Location/Situation variability
    #[allow(clippy::too_many_arguments)]
    pub fn area_tls(
        &self,
        h_tx_meter: f64,
        h_rx_meter: f64,
        tx_siting_criteria: SitingCriteria,
        rx_siting_criteria: SitingCriteria,
        d_km: f64,
        delta_h_meter: f64,
        climate: Climate,
        n_0: f64,
        f_mhz: f64,
        pol: Polarization,
        epsilon: f64,
        sigma: f64,
        mdvar: i32,
        time: f64,
        location: f64,
        situation: f64,
    ) -> ItmResult<ItmResult_> {
        let mut a_db: f64 = 0.0;
        let mut warnings: i32 = 0;

        let func: libloading::Symbol<ffi::FnItmAreaTls> = unsafe {
            self.lib
                .get(b"ITM_AREA_TLS")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        let error_code = unsafe {
            func(
                h_tx_meter,
                h_rx_meter,
                tx_siting_criteria as i32,
                rx_siting_criteria as i32,
                d_km,
                delta_h_meter,
                climate as i32,
                n_0,
                f_mhz,
                pol as i32,
                epsilon,
                sigma,
                mdvar,
                time,
                location,
                situation,
                &mut a_db,
                &mut warnings,
            )
        };

        if error_code >= 1000 {
            return Err(ItmError::from_code(error_code));
        }

        Ok(ItmResult_ {
            loss_db: a_db,
            warnings: ItmWarnings::from_bits(warnings),
        })
    }

    /// Area prediction mode using Time/Location/Situation variability
    /// with extended output (intermediate values)
    #[allow(clippy::too_many_arguments)]
    pub fn area_tls_ex(
        &self,
        h_tx_meter: f64,
        h_rx_meter: f64,
        tx_siting_criteria: SitingCriteria,
        rx_siting_criteria: SitingCriteria,
        d_km: f64,
        delta_h_meter: f64,
        climate: Climate,
        n_0: f64,
        f_mhz: f64,
        pol: Polarization,
        epsilon: f64,
        sigma: f64,
        mdvar: i32,
        time: f64,
        location: f64,
        situation: f64,
    ) -> ItmResult<ItmResultEx> {
        let mut a_db: f64 = 0.0;
        let mut warnings: i32 = 0;
        let mut inter_values = ffi::IntermediateValues::default();

        let func: libloading::Symbol<ffi::FnItmAreaTlsEx> = unsafe {
            self.lib
                .get(b"ITM_AREA_TLS_Ex")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        let error_code = unsafe {
            func(
                h_tx_meter,
                h_rx_meter,
                tx_siting_criteria as i32,
                rx_siting_criteria as i32,
                d_km,
                delta_h_meter,
                climate as i32,
                n_0,
                f_mhz,
                pol as i32,
                epsilon,
                sigma,
                mdvar,
                time,
                location,
                situation,
                &mut a_db,
                &mut warnings,
                &mut inter_values,
            )
        };

        if error_code >= 1000 {
            return Err(ItmError::from_code(error_code));
        }

        Ok(ItmResultEx {
            loss_db: a_db,
            warnings: ItmWarnings::from_bits(warnings),
            intermediate: IntermediateValues::from(inter_values),
        })
    }

    /// Area prediction mode using Confidence/Reliability variability
    #[allow(clippy::too_many_arguments)]
    pub fn area_cr(
        &self,
        h_tx_meter: f64,
        h_rx_meter: f64,
        tx_siting_criteria: SitingCriteria,
        rx_siting_criteria: SitingCriteria,
        d_km: f64,
        delta_h_meter: f64,
        climate: Climate,
        n_0: f64,
        f_mhz: f64,
        pol: Polarization,
        epsilon: f64,
        sigma: f64,
        mdvar: i32,
        confidence: f64,
        reliability: f64,
    ) -> ItmResult<ItmResult_> {
        let mut a_db: f64 = 0.0;
        let mut warnings: i32 = 0;

        let func: libloading::Symbol<ffi::FnItmAreaCr> = unsafe {
            self.lib
                .get(b"ITM_AREA_CR")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        let error_code = unsafe {
            func(
                h_tx_meter,
                h_rx_meter,
                tx_siting_criteria as i32,
                rx_siting_criteria as i32,
                d_km,
                delta_h_meter,
                climate as i32,
                n_0,
                f_mhz,
                pol as i32,
                epsilon,
                sigma,
                mdvar,
                confidence,
                reliability,
                &mut a_db,
                &mut warnings,
            )
        };

        if error_code >= 1000 {
            return Err(ItmError::from_code(error_code));
        }

        Ok(ItmResult_ {
            loss_db: a_db,
            warnings: ItmWarnings::from_bits(warnings),
        })
    }

    /// Area prediction mode using Confidence/Reliability variability
    /// with extended output (intermediate values)
    #[allow(clippy::too_many_arguments)]
    pub fn area_cr_ex(
        &self,
        h_tx_meter: f64,
        h_rx_meter: f64,
        tx_siting_criteria: SitingCriteria,
        rx_siting_criteria: SitingCriteria,
        d_km: f64,
        delta_h_meter: f64,
        climate: Climate,
        n_0: f64,
        f_mhz: f64,
        pol: Polarization,
        epsilon: f64,
        sigma: f64,
        mdvar: i32,
        confidence: f64,
        reliability: f64,
    ) -> ItmResult<ItmResultEx> {
        let mut a_db: f64 = 0.0;
        let mut warnings: i32 = 0;
        let mut inter_values = ffi::IntermediateValues::default();

        let func: libloading::Symbol<ffi::FnItmAreaCrEx> = unsafe {
            self.lib
                .get(b"ITM_AREA_CR_Ex")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        let error_code = unsafe {
            func(
                h_tx_meter,
                h_rx_meter,
                tx_siting_criteria as i32,
                rx_siting_criteria as i32,
                d_km,
                delta_h_meter,
                climate as i32,
                n_0,
                f_mhz,
                pol as i32,
                epsilon,
                sigma,
                mdvar,
                confidence,
                reliability,
                &mut a_db,
                &mut warnings,
                &mut inter_values,
            )
        };

        if error_code >= 1000 {
            return Err(ItmError::from_code(error_code));
        }

        Ok(ItmResultEx {
            loss_db: a_db,
            warnings: ItmWarnings::from_bits(warnings),
            intermediate: IntermediateValues::from(inter_values),
        })
    }

    /// Compute the terrain irregularity parameter (delta_h) from a terrain profile
    pub fn compute_delta_h(
        &self,
        pfl: &[f64],
        d_start_meter: f64,
        d_end_meter: f64,
    ) -> ItmResult<f64> {
        let func: libloading::Symbol<ffi::FnComputeDeltaH> = unsafe {
            self.lib
                .get(b"ComputeDeltaH")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        Ok(unsafe { func(pfl.as_ptr(), d_start_meter, d_end_meter) })
    }

    /// Calculate free space path loss
    pub fn free_space_loss(&self, d_meter: f64, f_mhz: f64) -> ItmResult<f64> {
        let func: libloading::Symbol<ffi::FnFreeSpaceLoss> = unsafe {
            self.lib
                .get(b"FreeSpaceLoss")
                .map_err(|e| ItmError::SymbolNotFound(e.to_string()))?
        };

        Ok(unsafe { func(d_meter, f_mhz) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_library() {
        let itm = Itm::new();
        assert!(itm.is_ok(), "Failed to load ITM library. Run scripts/setup_dependencies.ps1 to install it.");
    }

    #[test]
    fn test_area_prediction() {
        let itm = Itm::new().expect("ITM library not found. Run scripts/setup_dependencies.ps1 to install it.");

        let result = itm
            .area_tls(
                10.0,  // tx height
                2.0,   // rx height
                SitingCriteria::Random,
                SitingCriteria::Random,
                10.0,  // distance in km
                90.0,  // terrain irregularity
                Climate::ContinentalTemperate,
                301.0, // surface refractivity
                915.0, // frequency MHz
                Polarization::Vertical,
                15.0,  // epsilon
                0.005, // sigma
                0,     // mdvar
                50.0,  // time %
                50.0,  // location %
                50.0,  // situation %
            )
            .unwrap();

        println!("Area prediction loss: {} dB", result.loss_db);
        assert!(result.loss_db > 0.0, "Expected positive path loss");
    }
}
