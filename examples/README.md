# MCSim Example Configurations

This directory contains example network configurations for the MeshCore simulator.

## Directory Structure

```
examples/
├── topologies/     # Base network topology configurations
├── behaviors/      # Behavior overlay configurations (agents, traffic patterns)
├── seattle/        # Seattle-specific network and overlays
└── README.md
```

## Running Examples

Run any example with:
```bash
cargo run -- run examples/topologies/<filename>.yaml --duration 300
```

Overlay examples can be combined with base configurations:
```bash
cargo run -- run examples/topologies/simple.yaml examples/behaviors/chatter.yaml --duration 600
```

Seattle examples:
```bash
cargo run -- run examples/seattle/sea.yaml examples/seattle/chatter.yaml --duration 600
```

## Topologies (Base Models)

All topologies include Alice and Bob companion nodes for consistent behavior overlay compatibility.

| File | Description |
|------|-------------|
| [topologies/simple.yaml](topologies/simple.yaml) | Simple 3-repeater mesh with 2 companions |
| [topologies/two_peers.yaml](topologies/two_peers.yaml) | Two companions connected through a single repeater |
| [topologies/diamond.yaml](topologies/diamond.yaml) | Diamond topology: 2 companions, 2 repeaters |
| [topologies/high_altitude.yaml](topologies/high_altitude.yaml) | 6 ground repeaters + 1 high-altitude repeater + 2 companions |
| [topologies/long_hops.yaml](topologies/long_hops.yaml) | Line of 64 repeaters, each with a companion |
| [topologies/multi_path.yaml](topologies/multi_path.yaml) | Four companions with a 3x3 grid of repeaters |
| [topologies/room_server.yaml](topologies/room_server.yaml) | Two companions adjacent to a room server |

## Behaviors (Overlay Configurations)

Behavior overlays configure agents and traffic patterns. They work with any topology that includes the expected node names (typically Alice and Bob).

| File | Compatible With | Description |
|------|-----------------|-------------|
| [behaviors/chatter.yaml](behaviors/chatter.yaml) | Any with Alice/Bob | Alice and Bob exchange periodic DMs |
| [behaviors/broadcast.yaml](behaviors/broadcast.yaml) | Any with Alice | Alice sends periodic messages to #test channel |
| [behaviors/marginal.yaml](behaviors/marginal.yaml) | two_peers.yaml | Configures marginal (unreliable) links |
| [behaviors/burst_traffic.yaml](behaviors/burst_traffic.yaml) | Any with Alice/Bob | Alice and Bob each send 5 channel messages (for collision testing) |
| [behaviors/single_broadcast.yaml](behaviors/single_broadcast.yaml) | Any with Alice | Alice sends exactly 1 channel message (deterministic testing) |
| [behaviors/single_dm.yaml](behaviors/single_dm.yaml) | Any with Alice/Bob | Alice sends exactly 1 DM to Bob (deterministic testing) |

## Seattle Network

The Seattle network is a real-world topology with its own overlay files.

| File | Description |
|------|-------------|
| [seattle/sea.yaml](seattle/sea.yaml) | Seattle area mesh network with real-world topology |
| [seattle/chatter.yaml](seattle/chatter.yaml) | Multiple nodes generate periodic DM/message chatter |
| [seattle/bot.yaml](seattle/bot.yaml) | Node with external port for MeshCoreBot |
| [seattle/client.yaml](seattle/client.yaml) | Node with external port for user client app |
| [seattle/flood.yaml](seattle/flood.yaml) | Single node sending periodic floods |

## Network Topology Descriptions

### simple.yaml

```
[Alice:Companion] <---> [Repeater1] <---> [Repeater2] <---> [Repeater3] <---> [Bob:Companion]
```

Basic linear topology for simple message routing tests.

### two_peers.yaml

```
[Alice:Companion] <---> [Repeater] <---> [Bob:Companion]
```

Minimal network for peer-to-peer communication testing.

### diamond.yaml

```
       [Repeater1]
      /           \
[Alice]           [Bob]
      \           /
       [Repeater2]
```

Each repeater sees both companions but not each other. Tests path diversity.

### high_altitude.yaml

```
                    [HAB: High Altitude]
                    /  /  |  |  \  \
                   /  /   |  |   \  \
[Alice] --- [R1]--[R2]--[R3]--[R4]--[R5]--[R6] --- [Bob]
```

A ground mesh of 6 repeaters with a high-altitude balloon/aircraft that can see all nodes.
Tests the impact of "super nodes" on routing.

### long_hops.yaml

```
[Alice]--[R1]--[R2]--[R3]-- ... --[R62]--[R63]--[R64]--[Bob]
          |     |     |            |      |      |
         [C1]  [C2]  [C3]  ...   [C62]  [C63]  [C64]
```

64 repeaters in a line, each with an adjacent companion. Tests maximum hop counts and
routing table scalability. Messages from Alice to Bob must traverse all 64 repeaters.

### multi_path.yaml

```
                 [Charlie]
                  |  |  |
           +--[R1]--[R2]--[R3]--+
           |   |     |     |   |
[Alice] --+--[R4]--[R5]--[R6]--+-- [Bob]
           |   |     |     |   |
           +--[R7]--[R8]--[R9]--+
                  |  |  |
                  [Dave]
```

Four companions on each edge of the grid. Alice connects to left column (R1, R4, R7), Bob to right (R3, R6, R9), Charlie to top row (R1, R2, R3), Dave to bottom (R7, R8, R9). Tests multi-path routing.

### room_server.yaml

```
[Alice:Companion] <---> [Room Server] <---> [Bob:Companion]
```

Tests room server functionality and channel messaging.

### seattle/sea.yaml

A subset of the real Seattle MeshCore network with multiple repeaters and room servers.
Good for testing realistic network behavior.

## Configuration Tips

### Using Overlays

Overlay files modify base configurations by:

- Adding new nodes
- Modifying existing nodes (by defining them again with updated properties)
- Adding or modifying edges
- Removing nodes or edges (using `remove: true`)

```yaml
# Modify an existing node to add an agent
nodes:
  - name: "Alice"  # Must match name from base config
    agent:
      type: Human
      dm_targets:
        - "Bob"

# Remove a node
nodes:
  - name: "UnwantedNode"
    remove: true
```

### Adding Traffic

To add message traffic, configure agents on companion nodes:

```yaml
nodes:
  - name: "Alice"
    agent:
      type: Human
      dm_targets:
        - "Bob"
      channels:
        - "#general"
      message_interval_min_s: 10.0
      message_interval_max_s: 30.0
```

### Exposing Nodes for External Connections

To connect MeshCoreBot or a client application to a simulated node:

```yaml
nodes:
  - name: "MyNode"
    uart_port: 9100  # External apps connect via TCP to this port
```

### Marginal Links

To create an unreliable link, use low SNR values:

```yaml
edges:
  - from: "Node1"
    to: "Node2"
    mean_snr_db_at20dbm: -5.0  # Barely above noise floor
    snr_std_dev: 3.0           # High variance
```
