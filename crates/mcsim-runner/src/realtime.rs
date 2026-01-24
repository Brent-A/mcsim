//! Real-time simulation mode configuration and utilities.
//!
//! This module provides configuration options for running simulations
//! in real-time mode, where simulation time tracks wall clock time.
//!
//! ## Features
//!
//! - **Speed multiplier**: Run faster or slower than real-time
//! - **Catch-up logic**: Detect and handle when simulation falls behind
//! - **Drift tracking**: Monitor simulation vs wall clock drift

use std::time::{Duration, Instant};
use crate::SimTime;

/// Configuration for real-time simulation mode.
#[derive(Debug, Clone)]
pub struct RealTimeConfig {
    /// Speed multiplier (1.0 = real-time, 2.0 = 2x speed, 0.5 = half speed).
    /// Values must be positive.
    pub speed_multiplier: f64,
    
    /// Maximum catch-up time before warning (milliseconds).
    /// When the simulation lags behind wall clock by more than this amount,
    /// a warning is issued.
    pub max_catchup_ms: u64,
    
    /// Enable/disable real-time pacing.
    /// When disabled, the simulation runs as fast as possible.
    pub enabled: bool,
    
    /// Minimum sleep duration in microseconds to prevent busy-waiting.
    /// Default is 100 microseconds.
    pub min_sleep_us: u64,
    
    /// Warn about lag only once per interval (milliseconds).
    /// Prevents log spam when continuously lagging.
    pub lag_warn_interval_ms: u64,
    
    /// Interval for periodic stats output (in seconds).
    /// If None, periodic stats are disabled.
    pub periodic_stats_interval_secs: Option<u64>,
}

impl Default for RealTimeConfig {
    fn default() -> Self {
        RealTimeConfig {
            speed_multiplier: 1.0,
            max_catchup_ms: 100,
            enabled: true,
            min_sleep_us: 100,
            lag_warn_interval_ms: 5000,
            periodic_stats_interval_secs: Some(10),
        }
    }
}

impl RealTimeConfig {
    /// Create a new real-time config with the given speed multiplier.
    pub fn with_speed(speed: f64) -> Self {
        assert!(speed > 0.0, "Speed multiplier must be positive");
        RealTimeConfig {
            speed_multiplier: speed,
            ..Default::default()
        }
    }
    
    /// Create a disabled real-time config (run as fast as possible).
    pub fn disabled() -> Self {
        RealTimeConfig {
            enabled: false,
            ..Default::default()
        }
    }
    
    /// Set the maximum catch-up time before warning.
    pub fn with_max_catchup_ms(mut self, max_catchup_ms: u64) -> Self {
        self.max_catchup_ms = max_catchup_ms;
        self
    }
    
    /// Set the periodic stats interval. Pass None to disable periodic stats output.
    pub fn with_periodic_stats_interval(mut self, interval_secs: Option<u64>) -> Self {
        self.periodic_stats_interval_secs = interval_secs;
        self
    }
}

/// Tracks real-time vs simulation time drift.
#[derive(Debug)]
pub struct RealTimePacer {
    config: RealTimeConfig,
    
    /// Wall clock time when simulation started.
    start_wall: Instant,
    
    /// Simulation time when pacing started.
    start_sim: SimTime,
    
    /// Last time we warned about lag.
    last_lag_warn: Instant,
    
    /// Accumulated drift statistics.
    total_lag_warnings: u64,
    max_drift_seen_ms: i64,
    
    /// Last time we emitted periodic stats.
    last_periodic_stats: Instant,
    
    /// Event count at last periodic stats emission.
    last_periodic_event_count: u64,
}

impl RealTimePacer {
    /// Create a new real-time pacer.
    pub fn new(config: RealTimeConfig, start_sim: SimTime) -> Self {
        let now = Instant::now();
        RealTimePacer {
            config,
            start_wall: now,
            start_sim,
            last_lag_warn: now,
            total_lag_warnings: 0,
            max_drift_seen_ms: 0,
            last_periodic_stats: now,
            last_periodic_event_count: 0,
        }
    }
    
    /// Calculate the target simulation time based on elapsed wall clock time.
    /// Returns the simulation time that should have been reached by now.
    pub fn target_sim_time(&self) -> SimTime {
        if !self.config.enabled {
            // When disabled, return a very large time to process all events
            return SimTime::from_secs((u64::MAX / 1_000_000) as f64);
        }
        
        let elapsed_wall = self.start_wall.elapsed();
        let scaled_elapsed_us = (elapsed_wall.as_micros() as f64 * self.config.speed_multiplier) as u64;
        SimTime::from_micros(self.start_sim.as_micros() + scaled_elapsed_us)
    }
    
    /// Calculate the current drift between simulation and target time.
    /// Positive drift means simulation is behind (lagging).
    /// Negative drift means simulation is ahead (needs to wait).
    pub fn calculate_drift(&self, current_sim_time: SimTime) -> i64 {
        let target = self.target_sim_time();
        // Drift = target - current; positive means we're behind
        (target.as_micros() as i64) - (current_sim_time.as_micros() as i64)
    }
    
    /// Calculate drift in milliseconds.
    pub fn calculate_drift_ms(&self, current_sim_time: SimTime) -> i64 {
        self.calculate_drift(current_sim_time) / 1000
    }
    
    /// Check if simulation is lagging significantly and should warn.
    /// Returns the drift in milliseconds if a warning should be issued.
    pub fn check_lag_warning(&mut self, current_sim_time: SimTime) -> Option<i64> {
        if !self.config.enabled {
            return None;
        }
        
        let drift_ms = self.calculate_drift_ms(current_sim_time);
        
        // Track max drift
        if drift_ms > self.max_drift_seen_ms {
            self.max_drift_seen_ms = drift_ms;
        }
        
        // Check if we should warn
        if drift_ms > self.config.max_catchup_ms as i64 {
            let now = Instant::now();
            if now.duration_since(self.last_lag_warn).as_millis() >= self.config.lag_warn_interval_ms as u128 {
                self.last_lag_warn = now;
                self.total_lag_warnings += 1;
                return Some(drift_ms);
            }
        }
        
        None
    }
    
    /// Determine how long to sleep before the next event should be processed.
    /// Returns None if we should process events immediately (behind or at target).
    pub fn sleep_until_event(&self, next_event_time: SimTime) -> Option<Duration> {
        if !self.config.enabled {
            return None;
        }
        
        let target = self.target_sim_time();
        
        // If next event is in the past or at target, process immediately
        if next_event_time <= target {
            return None;
        }
        
        // Calculate how long until the event should be processed
        let event_offset_us = next_event_time.as_micros() - self.start_sim.as_micros();
        let event_wall_offset = (event_offset_us as f64 / self.config.speed_multiplier) as u64;
        let event_wall_time = self.start_wall + Duration::from_micros(event_wall_offset);
        
        let now = Instant::now();
        if event_wall_time > now {
            Some(event_wall_time - now)
        } else {
            None
        }
    }
    
    /// Get the minimum sleep duration for busy-wait prevention.
    pub fn min_sleep_duration(&self) -> Duration {
        Duration::from_micros(self.config.min_sleep_us)
    }
    
    /// Check if it's time to emit periodic stats and return the stats if so.
    /// Updates internal tracking to prevent duplicate emissions.
    ///
    /// Returns `Some(PeriodicStats)` if the interval has elapsed, `None` otherwise.
    /// Returns `None` immediately if periodic stats are disabled (interval is None).
    pub fn check_periodic_stats(
        &mut self,
        current_sim_time: SimTime,
        total_events: u64,
    ) -> Option<PeriodicStats> {
        // If periodic stats are disabled, return None immediately
        let interval_secs = self.config.periodic_stats_interval_secs?;
        
        let now = Instant::now();
        let elapsed_since_last = now.duration_since(self.last_periodic_stats);
        
        if elapsed_since_last.as_secs() >= interval_secs {
            let wall_elapsed = self.start_wall.elapsed();
            let sim_elapsed = current_sim_time.as_micros().saturating_sub(self.start_sim.as_micros());
            
            // Calculate simulation time : realtime ratio
            // If wall time is 10s and sim time is 20s, ratio is 2.0x
            let sim_to_realtime_ratio = if wall_elapsed.as_secs_f64() > 0.0 {
                (sim_elapsed as f64 / 1_000_000.0) / wall_elapsed.as_secs_f64()
            } else {
                0.0
            };
            
            // Calculate event rates
            let events_since_last = total_events.saturating_sub(self.last_periodic_event_count);
            // Events per second of real/wall time
            let event_rate_real = events_since_last as f64 / elapsed_since_last.as_secs_f64();
            // Events per second of simulation time (divide real rate by ratio)
            let event_rate_sim = if sim_to_realtime_ratio > 0.0 {
                event_rate_real / sim_to_realtime_ratio
            } else {
                0.0
            };
            
            // Get memory usage
            let memory_bytes = memory_stats::memory_stats()
                .map(|stats| stats.physical_mem)
                .unwrap_or(0);
            
            // Update tracking
            self.last_periodic_stats = now;
            self.last_periodic_event_count = total_events;
            
            Some(PeriodicStats {
                sim_time: current_sim_time,
                wall_elapsed,
                sim_to_realtime_ratio,
                total_events,
                event_rate_real,
                event_rate_sim,
                memory_bytes,
            })
        } else {
            None
        }
    }
    
    /// Get statistics about the pacing session.
    pub fn stats(&self) -> RealTimePacerStats {
        RealTimePacerStats {
            elapsed_wall: self.start_wall.elapsed(),
            total_lag_warnings: self.total_lag_warnings,
            max_drift_seen_ms: self.max_drift_seen_ms,
            speed_multiplier: self.config.speed_multiplier,
        }
    }
    
    /// Get a reference to the configuration.
    pub fn config(&self) -> &RealTimeConfig {
        &self.config
    }
}

/// Statistics from a real-time pacing session.
#[derive(Debug, Clone)]
pub struct RealTimePacerStats {
    /// Total wall clock time elapsed.
    pub elapsed_wall: Duration,
    /// Number of lag warnings issued.
    pub total_lag_warnings: u64,
    /// Maximum drift seen (positive = behind) in milliseconds.
    pub max_drift_seen_ms: i64,
    /// Speed multiplier used.
    pub speed_multiplier: f64,
}

/// Periodic statistics emitted during real-time simulation.
#[derive(Debug, Clone)]
pub struct PeriodicStats {
    /// Current simulation time.
    pub sim_time: SimTime,
    /// Total wall clock time elapsed since simulation start.
    pub wall_elapsed: Duration,
    /// Ratio of simulation time to real time (e.g., 1.0 = real-time, 2.0 = 2x faster).
    pub sim_to_realtime_ratio: f64,
    /// Cumulative event count.
    pub total_events: u64,
    /// Event rate per second of real/wall time (over the last interval).
    pub event_rate_real: f64,
    /// Event rate per second of simulation time (over the last interval).
    pub event_rate_sim: f64,
    /// Current memory usage in bytes.
    pub memory_bytes: usize,
}

impl PeriodicStats {
    /// Format memory size in human-readable format with fixed width.
    pub fn memory_human_readable(&self) -> String {
        let bytes = self.memory_bytes as f64;
        if bytes >= 1_073_741_824.0 {
            format!("{:>7.2} GB", bytes / 1_073_741_824.0)
        } else if bytes >= 1_048_576.0 {
            format!("{:>7.2} MB", bytes / 1_048_576.0)
        } else if bytes >= 1024.0 {
            format!("{:>7.2} KB", bytes / 1024.0)
        } else {
            format!("{:>7} B ", self.memory_bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = RealTimeConfig::default();
        assert_eq!(config.speed_multiplier, 1.0);
        assert_eq!(config.max_catchup_ms, 100);
        assert!(config.enabled);
    }
    
    #[test]
    fn test_speed_multiplier() {
        let config = RealTimeConfig::with_speed(2.0);
        assert_eq!(config.speed_multiplier, 2.0);
    }
    
    #[test]
    #[should_panic(expected = "Speed multiplier must be positive")]
    fn test_invalid_speed() {
        RealTimeConfig::with_speed(0.0);
    }
    
    #[test]
    fn test_disabled_config() {
        let config = RealTimeConfig::disabled();
        assert!(!config.enabled);
    }
    
    #[test]
    fn test_pacer_target_time() {
        let config = RealTimeConfig::with_speed(1.0);
        let start_sim = SimTime::from_micros(0);
        let pacer = RealTimePacer::new(config, start_sim);
        
        // Target time should be close to start initially
        let target = pacer.target_sim_time();
        // Allow some slack for test execution time
        assert!(target.as_micros() < 100_000); // Less than 100ms
    }
    
    #[test]
    fn test_pacer_drift_calculation() {
        let config = RealTimeConfig::with_speed(1.0);
        let start_sim = SimTime::from_micros(0);
        let pacer = RealTimePacer::new(config, start_sim);
        
        // If we're ahead of where we should be, drift is negative
        let far_ahead = SimTime::from_secs(1000.0);
        let drift = pacer.calculate_drift(far_ahead);
        assert!(drift < 0);
        
        // If we're behind, drift is positive
        let behind = SimTime::from_micros(0);
        std::thread::sleep(Duration::from_millis(10));
        let pacer2 = RealTimePacer::new(RealTimeConfig::default(), behind);
        std::thread::sleep(Duration::from_millis(5));
        let drift2 = pacer2.calculate_drift(behind);
        assert!(drift2 > 0);
    }
    
    #[test]
    fn test_periodic_stats_interval() {
        // Set a short interval for testing via config
        let config = RealTimeConfig::with_speed(1.0)
            .with_periodic_stats_interval(Some(1));
        let start_sim = SimTime::from_micros(0);
        let mut pacer = RealTimePacer::new(config, start_sim);
        
        // First check should return None (not enough time elapsed)
        let stats = pacer.check_periodic_stats(SimTime::from_secs(0.5), 100);
        assert!(stats.is_none());
        
        // Wait for interval to elapse
        std::thread::sleep(Duration::from_millis(1100));
        
        // Now should return stats
        let stats = pacer.check_periodic_stats(SimTime::from_secs(1.5), 200);
        assert!(stats.is_some());
        
        let stats = stats.unwrap();
        assert_eq!(stats.total_events, 200);
        assert!(stats.memory_bytes > 0); // Should have some memory usage
        assert!(stats.event_rate_real > 0.0); // Should have calculated event rate
    }
    
    #[test]
    fn test_periodic_stats_memory_format() {
        let stats = PeriodicStats {
            sim_time: SimTime::from_secs(10.0),
            wall_elapsed: Duration::from_secs(10),
            sim_to_realtime_ratio: 1.0,
            total_events: 1000,
            event_rate_real: 100.0,
            event_rate_sim: 100.0,
            memory_bytes: 1_500_000, // ~1.43 MB
        };
        
        let formatted = stats.memory_human_readable();
        assert!(formatted.contains("MB"));
        
        // Test KB formatting
        let stats_kb = PeriodicStats {
            memory_bytes: 5_000,
            ..stats.clone()
        };
        assert!(stats_kb.memory_human_readable().contains("KB"));
        
        // Test GB formatting
        let stats_gb = PeriodicStats {
            memory_bytes: 2_000_000_000,
            ..stats
        };
        assert!(stats_gb.memory_human_readable().contains("GB"));
    }
    
    #[test]
    fn test_periodic_stats_disabled() {
        // Disable periodic stats via config
        let config = RealTimeConfig::with_speed(1.0)
            .with_periodic_stats_interval(None);
        let start_sim = SimTime::from_micros(0);
        let mut pacer = RealTimePacer::new(config, start_sim);
        
        // Wait a bit and check - should always return None
        std::thread::sleep(Duration::from_millis(100));
        let stats = pacer.check_periodic_stats(SimTime::from_secs(10.0), 1000);
        assert!(stats.is_none(), "Should return None when periodic stats are disabled");
    }
}
