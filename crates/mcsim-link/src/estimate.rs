//! SNR estimation from observed (truncated) data.
//!
//! This module provides Maximum Likelihood Estimation for estimating the true
//! mean and standard deviation of SNR from observed values that are inherently
//! truncated (only packets above the sensitivity threshold are received).

use argmin::core::{CostFunction, Error, Executor, State};
use argmin::solver::neldermead::NelderMead;
use mcsim_model::properties::{
    ResolvedProperties, SimulationScope,
    RADIO_SNR_THRESHOLD_SF7_DB, RADIO_SNR_THRESHOLD_SF8_DB, RADIO_SNR_THRESHOLD_SF9_DB,
    RADIO_SNR_THRESHOLD_SF10_DB, RADIO_SNR_THRESHOLD_SF11_DB, RADIO_SNR_THRESHOLD_SF12_DB,
};
use statrs::distribution::{Continuous, ContinuousCDF, Normal};
use thiserror::Error;

/// Errors that can occur during SNR estimation.
#[derive(Debug, Error)]
pub enum SnrEstimationError {
    /// No observations provided.
    #[error("No observations provided")]
    NoObservations,

    /// Optimization failed.
    #[error("Optimization failed: {0}")]
    OptimizationFailed(String),

    /// Invalid radio parameters.
    #[error("Invalid radio parameters: {0}")]
    InvalidParameters(String),
}

// ============================================================================
// LoRa PHY Configuration
// ============================================================================

/// Configurable SNR thresholds for LoRa PHY.
///
/// This struct provides configurable SNR demodulation thresholds for each
/// spreading factor, allowing the thresholds to be loaded from simulation
/// properties.
///
/// # Example
///
/// ```ignore
/// use mcsim_link::LoraPhyConfig;
/// use mcsim_model::properties::{ResolvedProperties, SimulationScope};
///
/// // Use defaults
/// let config = LoraPhyConfig::default();
///
/// // Or load from properties
/// let sim_props: ResolvedProperties<SimulationScope> = ResolvedProperties::new();
/// let config = LoraPhyConfig::from_properties(&sim_props);
/// ```
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoraPhyConfig {
    /// SNR thresholds for SF7-SF12 (indexed 0-5).
    pub snr_thresholds: [f64; 6],
}

impl Default for LoraPhyConfig {
    fn default() -> Self {
        Self {
            snr_thresholds: [-7.5, -10.0, -12.5, -15.0, -17.5, -20.0],
        }
    }
}

impl LoraPhyConfig {
    /// Create a new `LoraPhyConfig` from simulation properties.
    ///
    /// This method extracts the SNR thresholds for each spreading factor
    /// from the resolved simulation properties.
    pub fn from_properties(props: &ResolvedProperties<SimulationScope>) -> Self {
        Self {
            snr_thresholds: [
                props.get(&RADIO_SNR_THRESHOLD_SF7_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF8_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF9_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF10_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF11_DB),
                props.get(&RADIO_SNR_THRESHOLD_SF12_DB),
            ],
        }
    }

    /// Get the SNR threshold for a given spreading factor.
    ///
    /// # Arguments
    ///
    /// * `spreading_factor` - LoRa spreading factor (7-12)
    ///
    /// # Returns
    ///
    /// The SNR threshold in dB for the given spreading factor.
    /// Returns the SF12 threshold (most sensitive) for unknown spreading factors.
    pub fn snr_threshold_for_sf(&self, spreading_factor: u8) -> f64 {
        match spreading_factor {
            7 => self.snr_thresholds[0],
            8 => self.snr_thresholds[1],
            9 => self.snr_thresholds[2],
            10 => self.snr_thresholds[3],
            11 => self.snr_thresholds[4],
            12 => self.snr_thresholds[5],
            _ => self.snr_thresholds[5], // Default to SF12 (most sensitive)
        }
    }
}

// ============================================================================
// LoRa Modulation Parameters
// ============================================================================

/// LoRa Modulation Parameters used to determine the reception threshold.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoraModulationParams {
    /// LoRa spreading factor (7 to 12).
    pub spreading_factor: u8,
    /// Bandwidth in Hz (e.g., 125000.0).
    pub bandwidth_hz: f64,
    /// Coding rate denominator (5 to 8, representing 4/5 to 4/8).
    pub coding_rate: u8,
}

impl Default for LoraModulationParams {
    fn default() -> Self {
        Self {
            spreading_factor: 7,
            bandwidth_hz: 125000.0,
            coding_rate: 5,
        }
    }
}

impl LoraModulationParams {
    /// Creates a new `LoraModulationParams` with the specified spreading factor.
    ///
    /// Uses default values for bandwidth (125 kHz) and coding rate (4/5).
    pub fn with_sf(spreading_factor: u8) -> Self {
        Self {
            spreading_factor,
            ..Default::default()
        }
    }

    /// Returns the theoretical SNR threshold (T) for the given LoRa configuration.
    ///
    /// Values are approximate based on Semtech documentation for LoRa.
    /// These thresholds represent the minimum SNR required for successful demodulation.
    ///
    /// For configurable thresholds, use [`sensitivity_threshold_snr_with_config`]
    /// or construct a [`LoraPhyConfig`] from properties.
    pub fn sensitivity_threshold_snr(&self) -> f64 {
        LoraPhyConfig::default().snr_threshold_for_sf(self.spreading_factor)
    }

    /// Returns the SNR threshold using a custom LoRa PHY configuration.
    ///
    /// This allows using thresholds loaded from simulation properties.
    ///
    /// # Arguments
    ///
    /// * `phy_config` - The PHY configuration containing SNR thresholds
    ///
    /// # Example
    ///
    /// ```ignore
    /// use mcsim_link::{LoraModulationParams, LoraPhyConfig};
    /// use mcsim_model::properties::{ResolvedProperties, SimulationScope};
    ///
    /// let sim_props: ResolvedProperties<SimulationScope> = ResolvedProperties::new();
    /// let phy_config = LoraPhyConfig::from_properties(&sim_props);
    /// let params = LoraModulationParams::with_sf(12);
    /// let threshold = params.sensitivity_threshold_snr_with_config(&phy_config);
    /// ```
    pub fn sensitivity_threshold_snr_with_config(&self, phy_config: &LoraPhyConfig) -> f64 {
        phy_config.snr_threshold_for_sf(self.spreading_factor)
    }
}

/// Internal cost function for Maximum Likelihood Estimation on truncated data.
///
/// This estimates the true Mean and Standard Deviation of SNR using
/// Maximum Likelihood Estimation, accounting for the fact that we only
/// observe samples above the reception threshold.
struct SnrCostFunction {
    observations: Vec<f64>,
    threshold: f64,
}

impl CostFunction for SnrCostFunction {
    type Param = Vec<f64>; // [mu, sigma]
    type Output = f64;

    fn cost(&self, p: &Self::Param) -> Result<Self::Output, Error> {
        let mu = p[0];
        let sigma = p[1].abs().max(0.1); // Ensure sigma is positive and non-zero

        let dist = match Normal::new(mu, sigma) {
            Ok(d) => d,
            Err(_) => return Ok(f64::INFINITY),
        };

        // For truncated data, we normalize the PDF by the area of the distribution
        // that exists above the threshold (the "survival function").
        let survival_prob = 1.0 - dist.cdf(self.threshold);

        // If survival probability is near zero, this parameter set is highly unlikely
        if survival_prob < 1e-10 {
            return Ok(f64::INFINITY);
        }

        let log_survival = survival_prob.ln();

        // Negative Log-Likelihood: Sum of (ln(PDF(y)) - ln(Survival))
        let nll: f64 = self
            .observations
            .iter()
            .map(|&y| dist.ln_pdf(y) - log_survival)
            .sum::<f64>();

        Ok(-nll)
    }
}

/// Result of SNR estimation from observed data.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SnrEstimationResult {
    /// Estimated true mean SNR (dB).
    ///
    /// This accounts for the truncation bias - the actual mean is typically
    /// lower than the sample mean of observed values.
    pub mean_snr: f64,

    /// Estimated standard deviation of SNR (dB).
    pub std_dev: f64,

    /// The sensitivity threshold used for estimation (dB).
    pub threshold: f64,

    /// Number of observations used in the estimation.
    pub observation_count: usize,

    /// Sample mean of the observed values (for comparison).
    pub sample_mean: f64,
}

impl SnrEstimationResult {
    /// Estimates the probability that a transmission will be successfully received.
    ///
    /// This is calculated as the probability that the SNR exceeds the threshold,
    /// based on the estimated distribution.
    pub fn reception_probability(&self) -> f64 {
        if let Ok(dist) = Normal::new(self.mean_snr, self.std_dev) {
            1.0 - dist.cdf(self.threshold)
        } else {
            0.0
        }
    }

    /// Returns the estimated link margin (mean SNR - threshold).
    pub fn link_margin(&self) -> f64 {
        self.mean_snr - self.threshold
    }
}

/// Estimates the true SNR distribution from observed (truncated) measurements.
///
/// When receiving LoRa packets, we only observe SNR values for packets that
/// were successfully demodulated (i.e., above the sensitivity threshold).
/// This creates a truncated distribution where we don't see the "failures"
/// below the threshold.
///
/// This function uses Maximum Likelihood Estimation with the Nelder-Mead
/// optimization algorithm to estimate the true underlying mean and standard
/// deviation of the SNR distribution.
///
/// # Arguments
///
/// * `observations` - Vector of observed SNR values (dB). These should all be
///   above the sensitivity threshold for the given modulation parameters.
/// * `params` - LoRa modulation parameters used to determine the sensitivity threshold.
///
/// # Returns
///
/// An `SnrEstimationResult` containing the estimated true mean and standard deviation.
///
/// # Example
///
/// ```
/// use mcsim_link::{estimate_snr, LoraModulationParams};
///
/// let params = LoraModulationParams {
///     spreading_factor: 12,
///     bandwidth_hz: 125000.0,
///     coding_rate: 5,
/// };
///
/// // SNR observations from received packets (all above -20 dB threshold for SF12)
/// let observations = vec![-15.2, -18.1, -14.5, -17.0, -16.2];
///
/// let result = estimate_snr(observations, params).unwrap();
/// println!("Estimated mean SNR: {:.1} dB", result.mean_snr);
/// println!("Estimated std dev: {:.1} dB", result.std_dev);
/// println!("Reception probability: {:.1}%", result.reception_probability() * 100.0);
/// ```
pub fn estimate_snr(
    observations: Vec<f64>,
    params: LoraModulationParams,
) -> Result<SnrEstimationResult, SnrEstimationError> {
    if observations.is_empty() {
        return Err(SnrEstimationError::NoObservations);
    }

    let threshold = params.sensitivity_threshold_snr();
    let observation_count = observations.len();

    // Calculate sample statistics for initial guess and reporting
    let sample_mean: f64 = observations.iter().sum::<f64>() / observation_count as f64;
    let sample_std = if observation_count > 1 {
        let variance: f64 = observations
            .iter()
            .map(|&x| (x - sample_mean).powi(2))
            .sum::<f64>()
            / (observation_count - 1) as f64;
        variance.sqrt().max(1.0)
    } else {
        2.0 // Default for single observation
    };

    let cost_fn = SnrCostFunction {
        observations,
        threshold,
    };

    // Initialize Nelder-Mead simplex with three points around our initial guess
    let solver = NelderMead::new(vec![
        vec![sample_mean, sample_std],
        vec![sample_mean - 2.0, sample_std + 1.0],
        vec![sample_mean + 2.0, sample_std + 0.5],
    ]);

    let res = Executor::new(cost_fn, solver)
        .configure(|state| state.max_iters(200))
        .run()
        .map_err(|e| SnrEstimationError::OptimizationFailed(e.to_string()))?;

    let best_param = res.state().get_best_param().ok_or_else(|| {
        SnrEstimationError::OptimizationFailed("No solution found".to_string())
    })?;

    Ok(SnrEstimationResult {
        mean_snr: best_param[0],
        std_dev: best_param[1].abs(),
        threshold,
        observation_count,
        sample_mean,
    })
}

/// Estimates the true SNR distribution using configurable PHY parameters.
///
/// This function is similar to [`estimate_snr`] but allows using custom
/// SNR thresholds loaded from simulation properties.
///
/// # Arguments
///
/// * `observations` - Vector of observed SNR values (dB).
/// * `params` - LoRa modulation parameters.
/// * `phy_config` - LoRa PHY configuration with SNR thresholds.
///
/// # Returns
///
/// An `SnrEstimationResult` containing the estimated true mean and standard deviation.
///
/// # Example
///
/// ```ignore
/// use mcsim_link::{estimate_snr_with_config, LoraModulationParams, LoraPhyConfig};
/// use mcsim_model::properties::{ResolvedProperties, SimulationScope};
///
/// let sim_props: ResolvedProperties<SimulationScope> = ResolvedProperties::new();
/// let phy_config = LoraPhyConfig::from_properties(&sim_props);
/// let params = LoraModulationParams::with_sf(12);
///
/// let observations = vec![-15.2, -18.1, -14.5, -17.0, -16.2];
/// let result = estimate_snr_with_config(observations, params, &phy_config).unwrap();
/// ```
pub fn estimate_snr_with_config(
    observations: Vec<f64>,
    params: LoraModulationParams,
    phy_config: &LoraPhyConfig,
) -> Result<SnrEstimationResult, SnrEstimationError> {
    let threshold = params.sensitivity_threshold_snr_with_config(phy_config);
    estimate_snr_with_threshold(observations, threshold)
}

/// Estimates SNR distribution with a custom threshold.
///
/// This is useful when you want to override the threshold derived from
/// modulation parameters, or when working with non-LoRa systems.
///
/// # Arguments
///
/// * `observations` - Vector of observed SNR values (dB).
/// * `threshold` - The SNR threshold below which signals cannot be received (dB).
///
/// # Returns
///
/// An `SnrEstimationResult` containing the estimated true mean and standard deviation.
pub fn estimate_snr_with_threshold(
    observations: Vec<f64>,
    threshold: f64,
) -> Result<SnrEstimationResult, SnrEstimationError> {
    if observations.is_empty() {
        return Err(SnrEstimationError::NoObservations);
    }

    let observation_count = observations.len();
    let sample_mean: f64 = observations.iter().sum::<f64>() / observation_count as f64;
    let sample_std = if observation_count > 1 {
        let variance: f64 = observations
            .iter()
            .map(|&x| (x - sample_mean).powi(2))
            .sum::<f64>()
            / (observation_count - 1) as f64;
        variance.sqrt().max(1.0)
    } else {
        2.0
    };

    let cost_fn = SnrCostFunction {
        observations,
        threshold,
    };

    let solver = NelderMead::new(vec![
        vec![sample_mean, sample_std],
        vec![sample_mean - 2.0, sample_std + 1.0],
        vec![sample_mean + 2.0, sample_std + 0.5],
    ]);

    let res = Executor::new(cost_fn, solver)
        .configure(|state| state.max_iters(200))
        .run()
        .map_err(|e| SnrEstimationError::OptimizationFailed(e.to_string()))?;

    let best_param = res.state().get_best_param().ok_or_else(|| {
        SnrEstimationError::OptimizationFailed("No solution found".to_string())
    })?;

    Ok(SnrEstimationResult {
        mean_snr: best_param[0],
        std_dev: best_param[1].abs(),
        threshold,
        observation_count,
        sample_mean,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimation_sf12() {
        let params = LoraModulationParams {
            spreading_factor: 12,
            bandwidth_hz: 125000.0,
            coding_rate: 5,
        };

        // Mock data: Observations are only those above -20.0
        let data = vec![-15.2, -18.1, -14.5, -17.0, -16.2];

        let result = estimate_snr(data, params).unwrap();
        println!("Estimated Result: {:?}", result);

        // The estimated mean should be lower than sample mean due to truncation
        assert!(result.mean_snr < result.sample_mean);
        // Standard deviation should be reasonable
        assert!(result.std_dev > 0.0 && result.std_dev < 20.0);
    }

    #[test]
    fn test_estimation_sf7() {
        let params = LoraModulationParams::with_sf(7);

        // Data closer to threshold (-7.5 for SF7)
        let data = vec![-2.0, -5.5, -1.0, -4.0, -3.2, 0.5, -6.0];

        let result = estimate_snr(data, params).unwrap();
        println!("SF7 Result: {:?}", result);

        assert!(result.std_dev > 0.0);
        assert!(result.threshold == -7.5);
    }

    #[test]
    fn test_empty_observations() {
        let params = LoraModulationParams::default();
        let result = estimate_snr(vec![], params);
        assert!(matches!(result, Err(SnrEstimationError::NoObservations)));
    }

    #[test]
    fn test_single_observation() {
        let params = LoraModulationParams::with_sf(10);
        let data = vec![-10.0];

        let result = estimate_snr(data, params).unwrap();
        assert!(result.observation_count == 1);
        assert!(result.mean_snr.is_finite());
    }

    #[test]
    fn test_reception_probability() {
        let params = LoraModulationParams::with_sf(12);

        // Strong signal well above threshold
        let strong_data = vec![0.0, 2.0, -1.0, 1.5, 3.0];
        let strong_result = estimate_snr(strong_data, params).unwrap();

        // Weak signal near threshold
        let weak_data = vec![-19.5, -18.5, -19.0, -17.5, -19.8];
        let weak_result = estimate_snr(weak_data, params).unwrap();

        // Strong signal should have higher reception probability
        assert!(strong_result.reception_probability() > weak_result.reception_probability());
    }

    #[test]
    fn test_with_custom_threshold() {
        let data = vec![5.0, 7.0, 6.5, 8.0, 4.5];
        let threshold = 3.0;

        let result = estimate_snr_with_threshold(data, threshold).unwrap();
        assert_eq!(result.threshold, threshold);
        assert!(result.mean_snr.is_finite());
    }

    #[test]
    fn test_sensitivity_thresholds() {
        assert_eq!(LoraModulationParams::with_sf(7).sensitivity_threshold_snr(), -7.5);
        assert_eq!(LoraModulationParams::with_sf(8).sensitivity_threshold_snr(), -10.0);
        assert_eq!(LoraModulationParams::with_sf(9).sensitivity_threshold_snr(), -12.5);
        assert_eq!(LoraModulationParams::with_sf(10).sensitivity_threshold_snr(), -15.0);
        assert_eq!(LoraModulationParams::with_sf(11).sensitivity_threshold_snr(), -17.5);
        assert_eq!(LoraModulationParams::with_sf(12).sensitivity_threshold_snr(), -20.0);
    }

    #[test]
    fn test_lora_phy_config_default() {
        let config = LoraPhyConfig::default();
        assert_eq!(config.snr_thresholds, [-7.5, -10.0, -12.5, -15.0, -17.5, -20.0]);
    }

    #[test]
    fn test_lora_phy_config_snr_threshold_for_sf() {
        let config = LoraPhyConfig::default();
        
        assert_eq!(config.snr_threshold_for_sf(7), -7.5);
        assert_eq!(config.snr_threshold_for_sf(8), -10.0);
        assert_eq!(config.snr_threshold_for_sf(9), -12.5);
        assert_eq!(config.snr_threshold_for_sf(10), -15.0);
        assert_eq!(config.snr_threshold_for_sf(11), -17.5);
        assert_eq!(config.snr_threshold_for_sf(12), -20.0);
        // Unknown SF defaults to SF12 (most sensitive)
        assert_eq!(config.snr_threshold_for_sf(6), -20.0);
        assert_eq!(config.snr_threshold_for_sf(13), -20.0);
    }

    #[test]
    fn test_lora_phy_config_from_properties() {
        let props: ResolvedProperties<SimulationScope> = ResolvedProperties::new();
        let config = LoraPhyConfig::from_properties(&props);

        // Verify values match property defaults
        assert_eq!(config.snr_thresholds[0], -7.5);  // SF7
        assert_eq!(config.snr_thresholds[1], -10.0); // SF8
        assert_eq!(config.snr_thresholds[2], -12.5); // SF9
        assert_eq!(config.snr_thresholds[3], -15.0); // SF10
        assert_eq!(config.snr_thresholds[4], -17.5); // SF11
        assert_eq!(config.snr_thresholds[5], -20.0); // SF12
    }

    #[test]
    fn test_sensitivity_threshold_with_config() {
        let config = LoraPhyConfig {
            snr_thresholds: [-8.0, -11.0, -13.0, -16.0, -18.0, -21.0], // Custom thresholds
        };

        let params = LoraModulationParams::with_sf(12);
        assert_eq!(params.sensitivity_threshold_snr_with_config(&config), -21.0);

        let params = LoraModulationParams::with_sf(7);
        assert_eq!(params.sensitivity_threshold_snr_with_config(&config), -8.0);
    }

    #[test]
    fn test_estimate_snr_with_config() {
        let phy_config = LoraPhyConfig::default();
        let params = LoraModulationParams::with_sf(12);
        
        // SNR observations from received packets (all above -20 dB threshold for SF12)
        let observations = vec![-15.2, -18.1, -14.5, -17.0, -16.2];
        
        let result = estimate_snr_with_config(observations, params, &phy_config).unwrap();
        
        // Verify threshold matches SF12
        assert_eq!(result.threshold, -20.0);
        // The estimated mean should be finite
        assert!(result.mean_snr.is_finite());
    }
}
