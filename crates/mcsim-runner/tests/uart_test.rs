//! Integration tests for Phase 4: Direct TCP Handling in node threads.
//!
//! These tests verify that TCP/UART connections work correctly with the
//! per-node threading architecture. TCP data is handled directly in node
//! threads instead of routing through the coordinator.

#![cfg(feature = "per_node_threading")]

use crossbeam_channel;
use mcsim_common::{EntityId, SimTime};
use mcsim_runner::node_thread::{
    NodeCommand, NodeReport, NodeThread, NodeThreadConfig, 
    UartChannels, LocalEventPayload, spawn_node_thread_with_uart,
};
use std::time::Duration;

/// Helper to create a test node configuration.
fn test_config(name: &str, node_index: usize) -> NodeThreadConfig {
    NodeThreadConfig {
        name: name.to_string(),
        node_index,
        firmware_entity_id: EntityId::new(node_index as u64 * 2 + 1),
        radio_entity_id: EntityId::new(node_index as u64 * 2 + 2),
        uart_port: Some(5000 + node_index as u16),
        tracing_enabled: true,
    }
}

// ============================================================================
// UartChannels Tests
// ============================================================================

#[test]
fn test_uart_channels_bidirectional_communication() {
    // Test full bidirectional communication through UART channels
    let (node_channels, tcp_channels) = UartChannels::new_pair();
    
    // Send data from TCP to node
    for i in 0..10 {
        tcp_channels.send(vec![i]).expect("Send should succeed");
    }
    
    // Verify all data arrives at node in order
    for i in 0..10 {
        let data = node_channels.try_recv().expect("Should receive data");
        assert_eq!(data, vec![i]);
    }
    
    // Send data from node to TCP
    for i in 10..20 {
        node_channels.send(vec![i]).expect("Send should succeed");
    }
    
    // Verify all data arrives at TCP in order
    for i in 10..20 {
        let data = tcp_channels.try_recv().expect("Should receive data");
        assert_eq!(data, vec![i]);
    }
}

#[test]
fn test_uart_channels_large_message() {
    // Test sending larger messages through UART channels
    let (node_channels, tcp_channels) = UartChannels::new_pair();
    
    // Create a large message (simulate a firmware image chunk)
    let large_message: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    
    tcp_channels.send(large_message.clone()).expect("Send should succeed");
    
    let received = node_channels.try_recv().expect("Should receive data");
    assert_eq!(received.len(), 10000);
    assert_eq!(received, large_message);
}

#[test]
fn test_uart_channels_clone() {
    // Test that UartChannels can be cloned (for passing to multiple handlers)
    let (node_channels, tcp_channels) = UartChannels::new_pair();
    
    // Clone both sides
    let node_clone = node_channels.clone();
    let _tcp_clone = tcp_channels.clone();
    
    // Send from original, receive from clone
    tcp_channels.send(vec![1, 2, 3]).expect("Send should succeed");
    let received = node_clone.try_recv().expect("Should receive data");
    assert_eq!(received, vec![1, 2, 3]);
    
    // Send from clone, receive from original
    node_clone.send(vec![4, 5, 6]).expect("Send should succeed");
    let received = tcp_channels.try_recv().expect("Should receive data");
    assert_eq!(received, vec![4, 5, 6]);
}

// ============================================================================
// Node Thread with UART Tests
// ============================================================================

#[test]
fn test_node_thread_with_uart_basic_lifecycle() {
    // Test basic lifecycle of a node thread with UART channels
    let (report_tx, report_rx) = crossbeam_channel::unbounded();
    let (node_channels, _tcp_channels) = UartChannels::new_pair();
    
    let handle = spawn_node_thread_with_uart(
        test_config("lifecycle_test", 0),
        report_tx,
        node_channels,
    );
    
    // Advance time a few times
    for i in 1..=5 {
        let target = SimTime::from_millis(i * 100);
        handle.send(NodeCommand::AdvanceTime { until: target }).unwrap();
        
        let (idx, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(idx, 0);
        match report {
            NodeReport::TimeReached { time, .. } => {
                assert_eq!(time, target);
            }
            _ => panic!("Expected TimeReached report at iteration {}", i),
        }
    }
    
    // Clean shutdown
    handle.send(NodeCommand::Shutdown).unwrap();
    let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(report, NodeReport::Shutdown));
    
    handle.join().expect("Thread should join cleanly");
}

#[test]
fn test_node_thread_with_uart_tcp_data_processing() {
    // Test that TCP data is processed by the node thread
    let (report_tx, report_rx) = crossbeam_channel::unbounded();
    let (node_channels, tcp_channels) = UartChannels::new_pair();
    
    let handle = spawn_node_thread_with_uart(
        test_config("tcp_process_test", 0),
        report_tx,
        node_channels,
    );
    
    // Send TCP data multiple times
    for i in 0..5 {
        tcp_channels.send(vec![i]).expect("Send should succeed");
    }
    
    // Give the thread time to process
    std::thread::sleep(Duration::from_millis(100));
    
    // Advance time to verify thread is still responsive after TCP processing
    handle.send(NodeCommand::AdvanceTime { 
        until: SimTime::from_millis(500) 
    }).unwrap();
    
    let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    match report {
        NodeReport::TimeReached { time, .. } => {
            assert_eq!(time, SimTime::from_millis(500));
        }
        _ => panic!("Expected TimeReached report"),
    }
    
    // Clean shutdown
    handle.send(NodeCommand::Shutdown).unwrap();
    let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(report, NodeReport::Shutdown));
    
    handle.join().expect("Thread should join cleanly");
}

#[test]
fn test_node_thread_with_uart_interleaved_commands_and_tcp() {
    // Test interleaving coordinator commands with TCP data
    let (report_tx, report_rx) = crossbeam_channel::unbounded();
    let (node_channels, tcp_channels) = UartChannels::new_pair();
    
    let handle = spawn_node_thread_with_uart(
        test_config("interleave_test", 0),
        report_tx,
        node_channels,
    );
    
    // Interleave commands and TCP data
    for i in 0..10 {
        // Send some TCP data
        tcp_channels.send(vec![i as u8]).expect("Send should succeed");
        
        // Advance time
        let target = SimTime::from_millis((i + 1) * 100);
        handle.send(NodeCommand::AdvanceTime { until: target }).unwrap();
        
        let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match report {
            NodeReport::TimeReached { time, .. } => {
                assert_eq!(time, target);
            }
            _ => panic!("Expected TimeReached report at iteration {}", i),
        }
    }
    
    // Clean shutdown
    handle.send(NodeCommand::Shutdown).unwrap();
    let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(report, NodeReport::Shutdown));
    
    handle.join().expect("Thread should join cleanly");
}

#[test]
fn test_tcp_channel_closure_doesnt_crash_thread() {
    // Test that closing the TCP side of channels doesn't crash the node thread
    let (report_tx, report_rx) = crossbeam_channel::unbounded();
    let (node_channels, tcp_channels) = UartChannels::new_pair();
    
    let handle = spawn_node_thread_with_uart(
        test_config("tcp_close_test", 0),
        report_tx,
        node_channels,
    );
    
    // Send some TCP data
    tcp_channels.send(vec![1, 2, 3]).expect("Send should succeed");
    
    // Drop the TCP channels (simulate TCP disconnection)
    drop(tcp_channels);
    
    // Give the thread time to notice
    std::thread::sleep(Duration::from_millis(50));
    
    // Thread should still respond to coordinator commands
    handle.send(NodeCommand::AdvanceTime { 
        until: SimTime::from_millis(100) 
    }).unwrap();
    
    let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    match report {
        NodeReport::TimeReached { time, .. } => {
            assert_eq!(time, SimTime::from_millis(100));
        }
        _ => panic!("Expected TimeReached report"),
    }
    
    // Clean shutdown
    handle.send(NodeCommand::Shutdown).unwrap();
    let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(report, NodeReport::Shutdown));
    
    handle.join().expect("Thread should join cleanly");
}

// ============================================================================
// NodeThread Direct Tests (without spawning thread)
// ============================================================================

#[test]
fn test_handle_tcp_data_without_crash() {
    // Test that handle_tcp_data doesn't crash and completes successfully
    let (report_tx, _report_rx) = crossbeam_channel::unbounded();
    let (uart_tx, _uart_rx) = crossbeam_channel::unbounded();
    
    let config = NodeThreadConfig {
        tracing_enabled: true,
        ..test_config("trace_test", 0)
    };
    let mut node = NodeThread::new(config);
    
    // Handle TCP data - should complete without panicking
    node.handle_tcp_data(vec![0xAA, 0xBB, 0xCC], &uart_tx, &report_tx);
    
    // Verify node is still in valid state
    assert_eq!(node.pending_event_count(), 0);
}

#[test]
fn test_tcp_data_event_processed_correctly() {
    // Test that TcpData events in the local queue are processed
    let (report_tx, _report_rx) = crossbeam_channel::unbounded();
    
    let config = NodeThreadConfig {
        tracing_enabled: true,
        ..test_config("tcp_event_test", 0)
    };
    let mut node = NodeThread::new(config);
    
    // Push a TcpData event
    node.push_local_event(
        SimTime::from_millis(100),
        LocalEventPayload::TcpData {
            data: vec![0x01, 0x02, 0x03],
        },
    );
    
    // Process events
    let processed = node.process_local_events(SimTime::from_millis(100), &report_tx);
    assert_eq!(processed, 1);
    
    // Verify no events remain
    assert_eq!(node.pending_event_count(), 0);
}

// ============================================================================
// Stress Tests
// ============================================================================

#[test]
fn test_high_volume_tcp_data() {
    // Test handling high volume of TCP data
    let (report_tx, report_rx) = crossbeam_channel::unbounded();
    let (node_channels, tcp_channels) = UartChannels::new_pair();
    
    let handle = spawn_node_thread_with_uart(
        test_config("high_volume_test", 0),
        report_tx,
        node_channels,
    );
    
    // Send a lot of TCP data rapidly
    for i in 0..1000 {
        tcp_channels.send(vec![(i % 256) as u8]).expect("Send should succeed");
    }
    
    // Give time to process
    std::thread::sleep(Duration::from_millis(500));
    
    // Verify thread is still responsive
    handle.send(NodeCommand::AdvanceTime { 
        until: SimTime::from_millis(1000) 
    }).unwrap();
    
    let (_, report) = report_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(report, NodeReport::TimeReached { .. }));
    
    // Clean shutdown
    handle.send(NodeCommand::Shutdown).unwrap();
    let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(report, NodeReport::Shutdown));
    
    handle.join().expect("Thread should join cleanly");
}

#[test]
fn test_rapid_advance_time_with_tcp() {
    // Test rapid AdvanceTime commands while TCP data is being sent
    let (report_tx, report_rx) = crossbeam_channel::unbounded();
    let (node_channels, tcp_channels) = UartChannels::new_pair();
    
    let handle = spawn_node_thread_with_uart(
        test_config("rapid_advance_test", 0),
        report_tx,
        node_channels,
    );
    
    // Start a thread to continuously send TCP data
    let tcp_sender = std::thread::spawn(move || {
        for i in 0..100 {
            if tcp_channels.send(vec![i as u8]).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_micros(100));
        }
    });
    
    // Rapidly advance time
    for i in 1..=50 {
        let target = SimTime::from_millis(i * 10);
        handle.send(NodeCommand::AdvanceTime { until: target }).unwrap();
        
        let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        match report {
            NodeReport::TimeReached { time, .. } => {
                assert_eq!(time, target);
            }
            _ => panic!("Expected TimeReached at iteration {}", i),
        }
    }
    
    // Clean shutdown
    handle.send(NodeCommand::Shutdown).unwrap();
    let (_, report) = report_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(report, NodeReport::Shutdown));
    
    handle.join().expect("Thread should join cleanly");
    tcp_sender.join().expect("TCP sender should finish");
}
