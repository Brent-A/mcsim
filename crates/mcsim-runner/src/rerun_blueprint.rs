//! Rerun Blueprint configuration for MCSim metrics visualization.
//!
//! This module provides functions to set up default Rerun visualization
//! by organizing metrics data into logical entity paths that Rerun will
//! auto-visualize appropriately.
//!
//! Since Rerun 0.28 has limited programmatic Blueprint API, this module
//! focuses on:
//! - Setting up annotation contexts for consistent colors
//! - Organizing entity paths for automatic time series visualization
//! - Logging descriptive metadata for better UI organization

// ============================================================================
// Real implementation (when rerun feature is enabled)
// ============================================================================

#[cfg(feature = "rerun")]
mod real_impl {

    /// Initialize metrics visualization setup for Rerun.
    ///
    /// Since Rerun 0.28 has limited programmatic Blueprint API, this function
    /// focuses on setting up proper entity paths and annotation contexts
    /// that Rerun will auto-visualize as time series.
    ///
    /// The entity path hierarchy follows this structure:
    /// - `/mcsim/nodes/{node_name}/metrics/radio/**` - Radio layer metrics
    /// - `/mcsim/nodes/{node_name}/metrics/packet/**` - Packet delivery metrics
    /// - `/mcsim/network/{network}/metrics/flood/**` - Flood propagation metrics
    /// - `/mcsim/network/{network}/metrics/direct/**` - Direct message metrics
    /// - `/stats/**` - Global simulation statistics
    ///
    /// # Arguments
    /// * `rec` - The Rerun RecordingStream to configure
    ///
    /// # Returns
    /// Result indicating success or failure of the setup
    pub fn initialize_metrics_visualization(
        rec: &rerun::RecordingStream,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Log descriptive text for the metrics hierarchy
        // This helps users understand what each entity path contains
        
        rec.log_static(
            "/mcsim",
            &rerun::TextLog::new("MCSim Metrics Dashboard - Use time panel to view metric time series"),
        )?;

        // Log documentation for main metric categories
        rec.log_static(
            "/mcsim/nodes",
            &rerun::TextLog::new("Per-node metrics: radio TX/RX, airtime, collisions, SNR/RSSI"),
        )?;

        rec.log_static(
            "/mcsim/network",
            &rerun::TextLog::new("Network-wide metrics: flood propagation, direct message delivery"),
        )?;

        rec.log_static(
            "/stats",
            &rerun::TextLog::new("Global simulation statistics: events, packets, collisions"),
        )?;

        // Set up annotation contexts with colors using Rgba32 (the correct type)
        setup_metric_annotations(rec)?;

        Ok(())
    }

    /// Set up annotation context for consistent metric visualization colors.
    ///
    /// This logs static annotation contexts that define colors for different
    /// metric types, ensuring consistent visualization across the dashboard.
    ///
    /// # Arguments
    /// * `rec` - The Rerun RecordingStream to log annotations to
    pub fn setup_metric_annotations(
        rec: &rerun::RecordingStream,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use rerun::Rgba32;
        
        // Helper to create Rgba32 from RGB values
        let rgb = |r: u8, g: u8, b: u8| -> Rgba32 {
            Rgba32::from_unmultiplied_rgba(r, g, b, 255)
        };

        // Define colors for radio metrics (blues)
        rec.log_static(
            "/mcsim/nodes",
            &rerun::AnnotationContext::new([
                (0u16, "tx_packets", rgb(65, 105, 225)),   // Royal Blue
                (1u16, "rx_packets", rgb(30, 144, 255)),   // Dodger Blue
                (2u16, "tx_airtime", rgb(0, 191, 255)),    // Deep Sky Blue
                (3u16, "rx_airtime", rgb(135, 206, 250)),  // Light Sky Blue
                (4u16, "rx_collided", rgb(255, 99, 71)),   // Tomato Red
                (5u16, "rx_weak", rgb(255, 165, 0)),       // Orange
            ]),
        )?;

        // Define colors for network metrics (greens)
        rec.log_static(
            "/mcsim/network",
            &rerun::AnnotationContext::new([
                (0u16, "flood_coverage", rgb(34, 139, 34)),     // Forest Green
                (1u16, "flood_latency", rgb(50, 205, 50)),      // Lime Green
                (2u16, "direct_success", rgb(0, 128, 0)),       // Green
                (3u16, "direct_failure", rgb(220, 20, 60)),     // Crimson
            ]),
        )?;

        // Define colors for global stats (purples)
        rec.log_static(
            "/stats",
            &rerun::AnnotationContext::new([
                (0u16, "total_events", rgb(148, 0, 211)),   // Dark Violet
                (1u16, "packets_tx", rgb(138, 43, 226)),    // Blue Violet
                (2u16, "packets_rx", rgb(186, 85, 211)),    // Medium Orchid
                (3u16, "collisions", rgb(255, 20, 147)),    // Deep Pink
            ]),
        )?;

        Ok(())
    }

    /// Create a metrics blueprint configuration (simplified version).
    ///
    /// This is a no-op in Rerun 0.28 since the full Blueprint API is not
    /// available. The actual visualization layout is handled automatically
    /// by Rerun based on the entity path hierarchy.
    ///
    /// Future versions of Rerun may support programmatic blueprint creation,
    /// at which point this function can be enhanced.
    pub fn create_metrics_blueprint(
        _rec: &rerun::RecordingStream,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Rerun 0.28 doesn't have a stable programmatic Blueprint API
        // The visualization is handled by the entity path hierarchy and
        // Rerun's automatic time series detection for Scalar data
        Ok(())
    }
}

// ============================================================================
// Stub implementation (when rerun feature is disabled)
// ============================================================================

#[cfg(not(feature = "rerun"))]
mod stub_impl {
    /// Initialize metrics visualization (no-op stub).
    pub fn initialize_metrics_visualization() -> Result<(), Box<dyn std::error::Error>> {
        // No-op: rerun feature is disabled
        Ok(())
    }
}

// Re-export the appropriate implementation
#[cfg(feature = "rerun")]
pub use real_impl::initialize_metrics_visualization;

#[cfg(not(feature = "rerun"))]
pub use stub_impl::initialize_metrics_visualization;

#[cfg(test)]
mod tests {
    #[test]
    fn test_stub_functions_compile() {
        // This test just ensures the stub functions compile correctly
        // when the rerun feature is disabled
        #[cfg(not(feature = "rerun"))]
        {
            use super::stub_impl::*;
            assert!(initialize_metrics_visualization().is_ok());
        }
    }
}
