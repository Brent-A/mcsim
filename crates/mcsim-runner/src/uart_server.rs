//! UART TCP Server Module
//!
//! This module provides TCP connections that expose the UART interface of each
//! firmware entity in the simulation. Each node gets its own TCP port that can
//! be used to send/receive serial data to/from that node's UART.

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use std::sync::RwLock;

/// Shared state tracking which clients are connected.
type ConnectedClients = Arc<RwLock<HashSet<u64>>>;

// ============================================================================
// Types
// ============================================================================

/// Information about a node's UART connection.
#[derive(Debug, Clone)]
pub struct UartNodeInfo {
    /// Node name from the model.
    pub name: String,
    /// Node type (Repeater, Companion, RoomServer).
    pub node_type: String,
    /// TCP port number for UART connection.
    pub port: u16,
    /// Entity ID of the firmware.
    pub entity_id: u64,
    /// Public key (first 6 bytes as hex string).
    pub public_key_prefix: String,
}

/// Message types for UART communication.
#[derive(Debug)]
pub enum UartMessage {
    /// Data received from TCP client to be sent to firmware.
    RxData(Vec<u8>),
    /// Data from firmware to be sent to TCP client.
    TxData(Vec<u8>),
    /// Client connected.
    Connected,
    /// Client disconnected.
    Disconnected,
}

/// Handle for sending data to a UART connection.
#[derive(Clone)]
pub struct UartHandle {
    tx_sender: mpsc::Sender<Vec<u8>>,
    rx_receiver: Arc<Mutex<mpsc::Receiver<Vec<u8>>>>,
}

impl UartHandle {
    /// Send data to the connected TCP client (firmware TX -> TCP).
    pub async fn send(&self, data: &[u8]) -> Result<(), mpsc::error::SendError<Vec<u8>>> {
        self.tx_sender.send(data.to_vec()).await
    }

    /// Try to receive data from the TCP client (TCP -> firmware RX).
    /// Returns None if no data is available.
    pub fn try_recv(&self) -> Option<Vec<u8>> {
        // Use try_lock to avoid blocking
        if let Ok(mut receiver) = self.rx_receiver.try_lock() {
            receiver.try_recv().ok()
        } else {
            None
        }
    }

    /// Check if there's data available without consuming it.
    pub fn has_data(&self) -> bool {
        if let Ok(receiver) = self.rx_receiver.try_lock() {
            !receiver.is_empty()
        } else {
            false
        }
    }
}

/// A UART TCP server that manages connections for all nodes.
pub struct UartServer {
    /// Map from entity ID to UART handle.
    handles: HashMap<u64, UartHandle>,
    /// Node information for display.
    node_infos: Vec<UartNodeInfo>,
    /// Base port number for allocation.
    base_port: u16,
    /// Next available port for sequential allocation.
    next_port: u16,
    /// Set of ports that are reserved (either explicitly assigned or already allocated).
    reserved_ports: HashSet<u16>,
    /// Shared tracking of connected clients.
    connected_clients: ConnectedClients,
}

impl UartServer {
    /// Create a new UART server starting at the given base port.
    pub fn new(base_port: u16) -> Self {
        UartServer {
            handles: HashMap::new(),
            node_infos: Vec::new(),
            base_port,
            next_port: base_port,
            reserved_ports: HashSet::new(),
            connected_clients: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Get the base port.
    pub fn base_port(&self) -> u16 {
        self.base_port
    }

    /// Reserve a specific port to prevent sequential allocation from using it.
    /// Call this before registering nodes to reserve explicitly assigned ports.
    pub fn reserve_port(&mut self, port: u16) {
        self.reserved_ports.insert(port);
    }

    /// Get the next available port for sequential allocation, skipping reserved ports.
    fn allocate_next_port(&mut self) -> u16 {
        while self.reserved_ports.contains(&self.next_port) {
            self.next_port += 1;
        }
        let port = self.next_port;
        self.next_port += 1;
        self.reserved_ports.insert(port);
        port
    }

    /// Register a node for UART service.
    /// If `requested_port` is Some, that port will be used (it should have been reserved first).
    /// If `requested_port` is None, a port will be allocated sequentially.
    /// Returns the allocated port number.
    pub fn register_node(
        &mut self,
        entity_id: u64,
        name: String,
        node_type: String,
        public_key: &[u8; 32],
        requested_port: Option<u16>,
    ) -> u16 {
        let port = match requested_port {
            Some(p) => {
                // Explicit port assignment - should already be reserved
                self.reserved_ports.insert(p);
                p
            }
            None => {
                // Sequential allocation, skipping reserved ports
                self.allocate_next_port()
            }
        };

        self.node_infos.push(UartNodeInfo {
            name,
            node_type,
            port,
            entity_id,
            public_key_prefix: hex::encode(&public_key[..6]),
        });

        port
    }

    /// Start TCP listeners for all registered nodes.
    /// Returns handles for each entity.
    pub async fn start(&mut self) -> io::Result<()> {
        for info in &self.node_infos {
            let (tx_sender, tx_receiver) = mpsc::channel::<Vec<u8>>(256);
            let (rx_sender, rx_receiver) = mpsc::channel::<Vec<u8>>(256);

            let handle = UartHandle {
                tx_sender,
                rx_receiver: Arc::new(Mutex::new(rx_receiver)),
            };

            self.handles.insert(info.entity_id, handle);

            // Spawn the TCP listener task
            let port = info.port;
            let name = info.name.clone();
            let entity_id = info.entity_id;
            let connected_clients = self.connected_clients.clone();
            tokio::spawn(async move {
                if let Err(e) = run_uart_listener(port, &name, entity_id, tx_receiver, rx_sender, connected_clients).await {
                    eprintln!("UART listener error for {}: {}", name, e);
                }
            });
        }

        Ok(())
    }

    /// Get a handle for a specific entity.
    pub fn get_handle(&self, entity_id: u64) -> Option<&UartHandle> {
        self.handles.get(&entity_id)
    }

    /// Get all node infos for display.
    pub fn node_infos(&self) -> &[UartNodeInfo] {
        &self.node_infos
    }

    /// Check if a client is connected for the given entity.
    pub fn is_client_connected(&self, entity_id: u64) -> bool {
        self.connected_clients.read().map(|c| c.contains(&entity_id)).unwrap_or(false)
    }

    /// Get the connected clients tracker (for sharing with sync manager).
    pub fn connected_clients(&self) -> ConnectedClients {
        self.connected_clients.clone()
    }

    /// Print the node table to stderr.
    pub fn print_node_table(&self) {
        eprintln!();
        eprintln!("┌{}┬{}┬{}┬{}┐",
            "─".repeat(20),
            "─".repeat(14),
            "─".repeat(14),
            "─".repeat(8));
        eprintln!("│ {:^18} │ {:^12} │ {:^12} │ {:^6} │", "Node Name", "Type", "Public Key", "Port");
        eprintln!("├{}┼{}┼{}┼{}┤",
            "─".repeat(20),
            "─".repeat(14),
            "─".repeat(14),
            "─".repeat(8));
        
        for info in &self.node_infos {
            eprintln!("│ {:18} │ {:12} │ {:12} │ {:6} │", 
                info.name, 
                info.node_type,
                info.public_key_prefix,
                info.port);
        }
        
        eprintln!("└{}┴{}┴{}┴{}┘",
            "─".repeat(20),
            "─".repeat(14),
            "─".repeat(14),
            "─".repeat(8));
        eprintln!();
    }
}

/// Run a TCP listener for a single UART.
async fn run_uart_listener(
    port: u16,
    _name: &str,
    entity_id: u64,
    mut tx_receiver: mpsc::Receiver<Vec<u8>>,
    rx_sender: mpsc::Sender<Vec<u8>>,
    connected_clients: ConnectedClients,
) -> io::Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    
    loop {
        // Accept a connection
        let (stream, _peer_addr) = listener.accept().await?;
        
        // Mark as connected
        if let Ok(mut clients) = connected_clients.write() {
            clients.insert(entity_id);
        }

        // Handle the connection
        let result = handle_uart_connection(
            stream,
            &mut tx_receiver,
            &rx_sender,
        ).await;

        // Mark as disconnected
        if let Ok(mut clients) = connected_clients.write() {
            clients.remove(&entity_id);
        }

        if let Err(e) = result {
            eprintln!("[UART] Connection error on port {}: {}", port, e);
        }
    }
}

/// Handle a single UART TCP connection.
async fn handle_uart_connection(
    mut stream: TcpStream,
    tx_receiver: &mut mpsc::Receiver<Vec<u8>>,
    rx_sender: &mpsc::Sender<Vec<u8>>,
) -> io::Result<()> {
    let (mut reader, mut writer) = stream.split();
    let mut read_buf = [0u8; 1024];

    loop {
        tokio::select! {
            // Read from TCP client -> send to firmware RX
            result = reader.read(&mut read_buf) => {
                match result {
                    Ok(0) => {
                        // Connection closed
                        return Ok(());
                    }
                    Ok(n) => {
                        let data = read_buf[..n].to_vec();
                        if rx_sender.send(data).await.is_err() {
                            // Receiver dropped
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
            
            // Receive from firmware TX -> send to TCP client
            Some(data) = tx_receiver.recv() => {
                if let Err(e) = writer.write_all(&data).await {
                    return Err(e);
                }
                // Flush to ensure data is sent immediately
                if let Err(e) = writer.flush().await {
                    return Err(e);
                }
            }
        }
    }
}

// ============================================================================
// Synchronous API for use with non-async event loop
// ============================================================================

/// A synchronous wrapper around UartServer for use in the main event loop.
pub struct SyncUartManager {
    /// Runtime handle for spawning async tasks.
    runtime: tokio::runtime::Handle,
    /// The underlying async server.
    server: Arc<Mutex<UartServer>>,
    /// Cached handles for synchronous access.
    handles: HashMap<u64, UartHandle>,
    /// Shared connected clients tracker.
    connected_clients: ConnectedClients,
}

impl SyncUartManager {
    /// Create a new synchronous UART manager.
    pub fn new(base_port: u16, runtime: tokio::runtime::Handle) -> Self {
        let server = UartServer::new(base_port);
        let connected_clients = server.connected_clients();
        SyncUartManager {
            runtime,
            server: Arc::new(Mutex::new(server)),
            handles: HashMap::new(),
            connected_clients,
        }
    }

    /// Reserve a specific port to prevent sequential allocation from using it.
    /// Call this before registering nodes to reserve explicitly assigned ports.
    pub fn reserve_port(&mut self, port: u16) {
        let server = self.server.clone();
        self.runtime.block_on(async {
            let mut server = server.lock().await;
            server.reserve_port(port);
        });
    }

    /// Register a node for UART service (synchronous).
    /// If `requested_port` is Some, that port will be used.
    /// If `requested_port` is None, a port will be allocated sequentially.
    pub fn register_node(
        &mut self,
        entity_id: u64,
        name: String,
        node_type: String,
        public_key: &[u8; 32],
        requested_port: Option<u16>,
    ) -> u16 {
        let server = self.server.clone();
        let public_key = *public_key;
        self.runtime.block_on(async {
            let mut server = server.lock().await;
            server.register_node(entity_id, name, node_type, &public_key, requested_port)
        })
    }

    /// Start all TCP listeners (synchronous).
    pub fn start(&mut self) -> io::Result<()> {
        let server = self.server.clone();
        self.runtime.block_on(async {
            let mut server = server.lock().await;
            server.start().await?;
            // Cache the handles
            io::Result::Ok(())
        })?;
        
        // Copy handles for sync access
        let server = self.server.clone();
        self.handles = self.runtime.block_on(async {
            let server = server.lock().await;
            server.handles.clone()
        });
        
        Ok(())
    }

    /// Get a handle for a specific entity.
    pub fn get_handle(&self, entity_id: u64) -> Option<&UartHandle> {
        self.handles.get(&entity_id)
    }

    /// Print the node table (synchronous).
    pub fn print_node_table(&self) {
        let server = self.server.clone();
        self.runtime.block_on(async {
            let server = server.lock().await;
            server.print_node_table();
        });
    }

    /// Send data to a node's UART (firmware TX -> TCP).
    /// Only sends if a client is connected, otherwise silently drops data.
    pub fn send_to_client(&self, entity_id: u64, data: &[u8]) {
        // Only send if a client is actually connected
        if !self.is_client_connected(entity_id) {
            return;
        }
        
        if let Some(handle) = self.handles.get(&entity_id) {
            let tx_sender = handle.tx_sender.clone();
            let data = data.to_vec();
            
            // Use try_send to avoid blocking - drop data if buffer is full
            if let Err(e) = tx_sender.try_send(data) {
                match e {
                    mpsc::error::TrySendError::Full(_) => {
                        // Buffer full even with client connected - drop data
                        // This shouldn't happen often with a connected client
                        eprintln!("[UART] TX buffer full for entity {} (client connected but slow)", entity_id);
                    }
                    mpsc::error::TrySendError::Closed(_) => {
                        eprintln!("[UART] Channel closed for entity {}", entity_id);
                    }
                }
            }
        }
    }

    /// Try to receive data from a node's UART (TCP -> firmware RX).
    pub fn try_recv_from_client(&self, entity_id: u64) -> Option<Vec<u8>> {
        self.handles.get(&entity_id).and_then(|h| h.try_recv())
    }

    /// Get node infos for display.
    pub fn node_infos(&self) -> Vec<UartNodeInfo> {
        let server = self.server.clone();
        self.runtime.block_on(async {
            let server = server.lock().await;
            server.node_infos.clone()
        })
    }

    /// Check if a client is connected for the given entity.
    pub fn is_client_connected(&self, entity_id: u64) -> bool {
        self.connected_clients.read().map(|c| c.contains(&entity_id)).unwrap_or(false)
    }
}
