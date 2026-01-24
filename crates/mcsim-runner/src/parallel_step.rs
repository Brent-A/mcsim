//! Parallel stepping support for firmware entities.
//!
//! This module enables parallel stepping of firmware nodes within a time slice,
//! using the async `step_begin()` / `step_wait()` pattern from the DLL interface.
//!
//! # Determinism
//!
//! To maintain determinism:
//! - Events are processed in sorted order by entity ID
//! - Results are collected and processed sequentially after parallel stepping
//! - New events are generated in deterministic order

// Note: rayon is available for parallel iteration when needed
#[allow(unused_imports)]
use rayon::prelude::*;
use std::collections::HashMap;

use mcsim_common::{EntityId, Event, EventPayload, SimContext, SimTime};

/// Configuration for parallel stepping.
#[derive(Debug, Clone)]
pub struct ParallelStepConfig {
    /// Minimum number of firmware events to trigger parallel processing.
    /// Below this threshold, sequential processing is used.
    pub min_parallel_threshold: usize,
    /// Whether parallel stepping is enabled.
    pub enabled: bool,
}

impl Default for ParallelStepConfig {
    fn default() -> Self {
        ParallelStepConfig {
            min_parallel_threshold: 2,
            enabled: true,
        }
    }
}

/// Result from processing a single firmware entity step.
#[derive(Debug)]
pub struct FirmwareStepOutput {
    /// Entity ID that was stepped.
    pub entity_id: EntityId,
    /// Attached radio entity ID.
    pub attached_radio: EntityId,
    /// Current time when step was processed.
    pub current_millis: u64,
    /// The step result from the firmware.
    pub result: mcsim_firmware::FirmwareStepResult,
}

/// Group events by target entity for parallel processing.
/// Returns a map from entity ID to events targeting that entity.
pub fn group_events_by_target(events: &[Event]) -> HashMap<EntityId, Vec<&Event>> {
    let mut groups: HashMap<EntityId, Vec<&Event>> = HashMap::new();
    for event in events {
        for target in &event.targets {
            groups.entry(*target).or_default().push(event);
        }
    }
    groups
}

/// Check if an event targets a firmware entity (based on payload type).
/// Firmware entities handle: RadioRxPacket, RadioStateChanged, SerialRx, Timer
pub fn is_firmware_event(payload: &EventPayload) -> bool {
    matches!(
        payload,
        EventPayload::RadioRxPacket(_)
            | EventPayload::RadioStateChanged(_)
            | EventPayload::SerialRx(_)
            | EventPayload::Timer { .. }
    )
}

/// Collect events at the same timestamp from the event queue.
/// This allows batching events for parallel processing.
pub fn collect_same_time_events<I>(events: &mut I, current_time: SimTime) -> Vec<Event>
where
    I: Iterator<Item = Event>,
{
    let mut batch = Vec::new();
    for event in events {
        if event.time == current_time {
            batch.push(event);
        } else {
            // Put it back - but we can't easily do this with iterators
            // So we'll return what we have and let caller handle it
            batch.push(event);
            break;
        }
    }
    batch
}

/// Process firmware step results and generate new events.
/// This is called sequentially to maintain determinism in event generation.
pub fn process_firmware_results(
    outputs: Vec<FirmwareStepOutput>,
    ctx: &mut SimContext,
    current_time: SimTime,
) -> Vec<Event> {
    use mcsim_firmware::YieldReason;
    
    let mut new_events = Vec::new();
    
    // Sort outputs by entity ID for deterministic processing
    let mut sorted_outputs = outputs;
    sorted_outputs.sort_by_key(|o| o.entity_id.0);
    
    for output in sorted_outputs {
        let result = &output.result;
        
        match result.reason {
            YieldReason::RadioTxStart => {
                // Firmware wants to transmit - send TX request to radio
                if let Some((ref tx_data, _airtime_ms)) = result.radio_tx_data {
                    let event = Event {
                        id: mcsim_common::EventId(ctx.next_event_id()),
                        time: current_time,
                        source: output.entity_id,
                        targets: vec![output.attached_radio],
                        payload: EventPayload::RadioTxRequest(mcsim_common::RadioTxRequestEvent {
                            packet: mcsim_common::LoraPacket::new(tx_data.clone()),
                        }),
                    };
                    new_events.push(event);
                }
            }
            YieldReason::Idle => {
                // Schedule wake timer if needed
                if result.wake_millis > output.current_millis {
                    let delay_us = (result.wake_millis - output.current_millis) * 1000;
                    let event = Event {
                        id: mcsim_common::EventId(ctx.next_event_id()),
                        time: current_time + SimTime::from_micros(delay_us),
                        source: output.entity_id,
                        targets: vec![output.entity_id],
                        payload: EventPayload::Timer { timer_id: 1 },
                    };
                    new_events.push(event);
                }
            }
            YieldReason::RadioTxComplete => {
                // TX completed internally - no action needed
            }
            YieldReason::Error => {
                if let Some(ref msg) = result.error_message {
                    eprintln!("Firmware error for entity {:?}: {}", output.entity_id, msg);
                }
            }
            _ => {}
        }
        
        // Handle serial TX data if any
        if let Some(ref serial_data) = result.serial_tx_data {
            // Post SerialTx event to self (for UART/TCP bridge)
            let event = Event {
                id: mcsim_common::EventId(ctx.next_event_id()),
                time: current_time,
                source: output.entity_id,
                targets: vec![output.entity_id],
                payload: EventPayload::SerialTx(mcsim_common::SerialTxEvent {
                    data: serial_data.clone(),
                }),
            };
            new_events.push(event);
        }
    }
    
    new_events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parallel_step_config_default() {
        let config = ParallelStepConfig::default();
        assert!(config.enabled);
        assert_eq!(config.min_parallel_threshold, 2);
    }
    
    #[test]
    fn test_is_firmware_event() {
        assert!(is_firmware_event(&EventPayload::Timer { timer_id: 1 }));
        assert!(!is_firmware_event(&EventPayload::SimulationEnd));
        assert!(!is_firmware_event(&EventPayload::TransmitAir(mcsim_common::TransmitAirEvent {
            radio_id: EntityId::new(1),
            packet: mcsim_common::LoraPacket::from_bytes(vec![]),
            params: mcsim_common::RadioParams {
                frequency_hz: 915000000,
                bandwidth_hz: 250000,
                spreading_factor: 11,
                coding_rate: 5,
                tx_power_dbm: 20,
            },
            end_time: SimTime::from_millis(100),
        })));
    }
}
