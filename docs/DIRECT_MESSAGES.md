# Direct Message Behavior in MeshCore

This document describes how direct messages (DMs) work in MeshCore and how we replicate this behavior in the simulation.

## Overview

Direct messages in MeshCore involve coordination between three layers:
1. **Mobile App** - Initiates DMs, handles retries and path management
2. **Firmware** - Routes messages, manages contacts and paths, sends acknowledgments
3. **Radio** - Physical transmission

The simulation replicates this with:
1. **Agent** - Corresponds to the mobile app behavior
2. **Simulated Firmware** - Implements the MeshCore firmware logic
3. **Radio Model** - Simulates RF propagation

## Message Routing Types

MeshCore supports two routing types for direct messages:

| Type | Header Value | Description |
|------|-------------|-------------|
| **Flood** | `ROUTE_TYPE_FLOOD (0x01)` | Message propagates to all nodes, path built during propagation |
| **Direct** | `ROUTE_TYPE_DIRECT (0x02)` | Message follows a pre-established path |

The firmware chooses routing based on `ContactInfo.out_path_len`:
- `-1` (unknown) → **Flood routing**
- `>= 0` → **Direct routing** using stored path

## Path Management

### Path Data Structure

Each contact in the firmware has path information:

```cpp
struct ContactInfo {
  mesh::Identity id;                    // 32-byte public key
  char name[32];                        // Contact name
  uint8_t type;                         // ADV_TYPE_* (Chat, Repeater, RoomServer)
  int8_t out_path_len;                  // Path length: -1 = unknown/flood, >= 0 = direct
  uint8_t out_path[MAX_PATH_SIZE];      // Route to contact (up to 64 bytes)
  uint8_t shared_secret[PUB_KEY_SIZE];  // Pre-calculated ECDH shared secret
  // ... timestamps, GPS, etc.
};
```

### Path Discovery

Paths are learned through several mechanisms:

1. **Advertisement Reception**: When receiving an advertisement, the inbound path (built during flood propagation) is stored as the reverse path to that contact.

2. **PATH_RETURN Packets**: When a node receives a flood message, it responds with a `PATH_RETURN` packet that contains:
   - The path TO the sender (so sender can use direct routing for responses)
   - Optionally an embedded ACK

3. **Path Updates**: Whenever a packet arrives via flood from a known contact, the path is updated.

### Path Building During Flood

When a flood packet is retransmitted, each intermediate node appends its own 1-byte hash to the path:

```cpp
// In routeRecvPacket()
if (packet->isRouteFlood() && packet->path_len + PATH_HASH_SIZE <= MAX_PATH_SIZE) {
    packet->path_len += self_id.copyHashTo(&packet->path[packet->path_len]);
    // retransmit...
}
```

### Path Clearing

Paths can be reset via:
- `CMD_RESET_PATH` companion command - sets `out_path_len = -1`
- App can explicitly reset when direct routing fails repeatedly

## DM Sending Flow (Normal Behavior)

### Step 1: Send Initial Message

When the app sends a DM:

```
App: CMD_SEND_MSG(recipient_prefix, text)
  ↓
Firmware: lookupContactByPubKey(prefix)
  ↓
If contact.out_path_len < 0:
  → sendFlood(packet)
  → Return MSG_SEND_SENT_FLOOD
Else:
  → sendDirect(packet, contact.out_path)
  → Return MSG_SEND_SENT_DIRECT
```

### Step 2: Wait for ACK

The app sets a timeout based on routing type:
- **Flood**: Longer timeout (depends on estimated network propagation)
- **Direct**: Shorter timeout (based on path length)

### Step 3: Retry Logic (App Layer)

If no ACK is received:

1. **Retries 1-2**: Resend with same routing method
2. **Retry 3**: If using direct routing, reset path and switch to flood
3. **Final Attempts**: Continue with flood routing
4. **Give Up**: After max retries, report failure to user

```
Retry 1: Same routing (flood or direct)
Retry 2: Same routing
Retry 3: If direct → Reset path → Flood
Retry 4+: Flood
```

> **Note**: The 3-retry fallback logic is implemented in the **mobile app**, not the firmware.

## Acknowledgment Requirements

### Critical: Contacts Must Exist

**The firmware will only acknowledge a DM if the sender is in the receiver's contacts list.**

The ACK flow:
```cpp
// When TXT_MSG received:
int num = searchPeersByHash(&src_hash);  // Search contacts
if (num == 0) {
    // NO ACK - sender not in contacts!
    return;
}

// Try to decrypt with each matching contact's shared secret
for (int j = 0; j < num; j++) {
    uint8_t secret[PUB_KEY_SIZE];
    getPeerSharedSecret(secret, j);
    if (decrypt_succeeds(secret, ...)) {
        // Send ACK via PATH_RETURN (flood) or direct ACK
        onPeerDataRecv(pkt, ...);  // Triggers ACK
        break;
    }
}
```

### Why Contacts Must Exist

1. **Identity Verification**: The sender's hash (1 byte) must match a known contact's public key
2. **Decryption**: The pre-calculated ECDH `shared_secret` is needed to decrypt the message
3. **No Shared Secret = Cannot Decrypt = No ACK**

### Contact Pre-population Methods

Contacts can be established through:

1. **Advertisement Exchange**: Both nodes broadcast advertisements; auto-add adds them to contacts
2. **Explicit Add**: `CMD_ADD_UPDATE_CONTACT` companion command with full public key
3. **Import**: `CMD_IMPORT_CONTACT` with a contact advertisement blob
4. **Persistent Storage**: Loaded from `/contacts3` file on startup

## Simulation Requirements

### For DM Tests to Work

1. **Pre-populate Contacts**: Before sending DMs, ensure both nodes have each other in their contacts list

2. **Provide Public Keys**: The simulation must make public keys available so agents can populate contact lists via `CMD_ADD_UPDATE_CONTACT`

3. **Shared Secret Calculation**: The firmware must calculate ECDH shared secrets when contacts are added (the C++ firmware does this in `addContact()`)

### Configuration Changes Needed

Add to node/simulation configuration:

```yaml
nodes:
  - name: Alice
    identity: <alice_pubkey_hex>
    contacts:
      - name: Bob
        pubkey: <bob_pubkey_hex>
      
  - name: Bob
    identity: <bob_pubkey_hex>
    contacts:
      - name: Alice
        pubkey: <alice_pubkey_hex>
```

### Implementation Checklist

- [ ] Add `contacts` field to node configuration
- [ ] Pass contact public keys to agents during simulation setup
- [ ] Agents send `CMD_ADD_UPDATE_CONTACT` at startup to populate firmware contacts
- [ ] Firmware calculates ECDH shared secrets when contacts are added
- [ ] Implement retry logic in agent with configurable attempts
- [ ] Track routing type in metrics (flood vs direct)

## New Properties

### Companion Properties

| Property | Type | Default | Description |
| -------- | ---- | ------- | ----------- |
| `companion/contacts` | `string[]` | `null` | Node names to add as contacts at startup. Resolved to public keys during build. |

### Messaging Properties

These properties control the retry and timeout behavior for direct messages.

#### Timeout Properties

| Property | Type | Default | Description |
| -------- | ---- | ------- | ----------- |
| `messaging/flood_ack_timeout_s` | `float` | `30.0` | Timeout waiting for ACK when message sent via flood routing |
| `messaging/direct_ack_timeout_per_hop_s` | `float` | `5.0` | Timeout per hop when message sent via direct routing (total = hops × this value) |

#### Retry Properties (Starting with Direct - Path Known)

When the sender has a known path to the recipient:

| Property | Type | Default | Description |
| -------- | ---- | ------- | ----------- |
| `messaging/direct_attempts` | `int` | `3` | Number of attempts using direct routing before clearing path |
| `messaging/flood_attempts_after_direct` | `int` | `1` | Number of flood attempts after direct attempts exhausted |

#### Retry Properties (Starting with Flood - No Path Known)

When the sender does NOT have a known path to the recipient:

| Property | Type | Default | Description |
| -------- | ---- | ------- | ----------- |
| `messaging/flood_attempts_no_path` | `int` | `3` | Number of flood attempts when no path is known |

### Example Configuration

```yaml
defaults:
  companion:
    contacts: null  # Must be specified per-node
  messaging:
    # Timeouts
    flood_ack_timeout_s: 30.0
    direct_ack_timeout_per_hop_s: 5.0
    # When path is known: try direct 3 times, then flood 1 time
    direct_attempts: 3
    flood_attempts_after_direct: 1
    # When no path: try flood 3 times
    flood_attempts_no_path: 3

nodes:
  - name: Alice
    firmware:
      type: companion
    companion:
      contacts:
        - Bob
        - Carol

  - name: Bob
    firmware:
      type: companion
    companion:
      contacts:
        - Alice
```

### Retry Behavior

#### When Path is Known (Direct Routing)

With defaults (`direct_attempts: 3`, `flood_attempts_after_direct: 1`):

```
Attempt 1: Direct routing
Attempt 2: Direct routing (retry)
Attempt 3: Direct routing (retry)
   ↓ Clear path, switch to flood
Attempt 4: Flood routing
   ↓ Give up
```

Total attempts: `direct_attempts + flood_attempts_after_direct` = 4

#### When No Path is Known (Flood Routing)

With defaults (`flood_attempts_no_path: 3`):

```
Attempt 1: Flood routing
Attempt 2: Flood routing (retry)
Attempt 3: Flood routing (retry)
   ↓ Give up
```

Total attempts: `flood_attempts_no_path` = 3

### State Machine

```
                    ┌─────────────────────────┐
                    │     Send Message        │
                    └───────────┬─────────────┘
                                │
                    ┌───────────▼───────────┐
                    │   Path Known?         │
                    └───────────┬───────────┘
                         │           │
                        YES          NO
                         │           │
              ┌──────────▼──────┐   │
              │  Direct Mode    │   │
              │  attempts = 0   │   │
              └────────┬────────┘   │
                       │            │
         ┌─────────────▼────────────▼─────────────┐
         │           Send Packet                  │
         │  (direct if path, flood otherwise)     │
         └─────────────────┬──────────────────────┘
                           │
              ┌────────────▼────────────┐
              │     Wait for ACK        │
              └────────────┬────────────┘
                    │            │
                   ACK        TIMEOUT
                    │            │
         ┌──────────▼──┐   ┌────▼─────────────────────┐
         │   Success   │   │  Increment attempts      │
         └─────────────┘   └────┬─────────────────────┘
                                │
                    ┌───────────▼───────────┐
                    │  In Direct Mode?      │
                    └───────────┬───────────┘
                         │           │
                        YES          NO
                         │           │
              ┌──────────▼──────────────────┐
              │ attempts < direct_attempts? │
              └──────────┬──────────────────┘
                    │         │
                   YES        NO
                    │         │
                    │    ┌────▼─────────────┐
                    │    │  Clear Path      │
                    │    │  Switch to Flood │
                    │    │  attempts = 0    │
                    │    └────┬─────────────┘
                    │         │
                    │   ┌─────▼───────────────────────┐
                    │   │ attempts < flood_attempts_* │
                    │   └─────┬───────────────────────┘
                    │         │         │
                    │        YES        NO
                    │         │         │
                    └────►RETRY    ┌────▼────┐
                              │    │  Fail   │
                              │    └─────────┘
                              │
                    ┌─────────▼─────────┐
                    │   Send Packet     │
                    └───────────────────┘
```

## Implementation Plan

### Phase 1: Contact Configuration

Add support for configuring contacts in the node YAML:

```yaml
nodes:
  - name: Alice
    type: companion
    keys:
      public_key: "0102..."  # Optional: specify or auto-generate
    companion:
      contacts:
        - Bob      # Reference by name (resolved to public key)
        - Carol
      
  - name: Bob
    type: companion
    companion:
      contacts:
        - Alice
```

The simulation builder will:

1. Resolve contact names to public keys during the build phase
2. Pass the list of contact public keys to the agent configuration
3. Agents will send `CMD_ADD_UPDATE_CONTACT` for each contact during protocol setup

### Phase 2: Agent Contact Setup

Modify the agent's protocol initialization sequence:

```
Current sequence:
1. DeviceQuery → DeviceInfo
2. AppStart → SelfInfo
3. SetChannel (for each channel)
4. Ready

New sequence:
1. DeviceQuery → DeviceInfo
2. AppStart → SelfInfo
3. AddUpdateContact (for each configured contact)
4. SetChannel (for each channel)
5. Ready
```

The agent will need:
- A list of `ContactInfo` structs in its configuration
- A new state `SettingUpContacts` in the protocol state machine
- Logic to send `CMD_ADD_UPDATE_CONTACT` for each contact

### Phase 3: Retry Logic (Future)

Implement the 3-retry pattern in the agent:

```rust
enum DirectMessageState {
    WaitingStartup,
    Idle,
    WaitingInterval,
    WaitingAck { expected_ack: u32, attempt: u8, last_route_type: RouteType },
    WaitingSession,
    Shutdown,
    Disabled,
}
```

On ACK timeout:
1. If `attempt < 2`: Resend same message, increment attempt
2. If `attempt == 2`: Send `CMD_RESET_PATH`, then resend
3. If `attempt >= 3`: Give up, record failure metric

### Phase 4: Metrics Enhancement

Track routing type in metrics:

- `mcsim.dm.sent{route_type=flood}` - DMs sent via flood
- `mcsim.dm.sent{route_type=direct}` - DMs sent via direct routing
- `mcsim.dm.ack_timeout{route_type=*}` - ACK timeouts by routing type
- `mcsim.dm.path_reset` - Path reset events

## Current Simulation Architecture

### Agent Configuration Flow

The simulation builder (`mcsim-model/src/lib.rs:build_simulation`) creates agents:

1. **First pass**: Allocate entity IDs, create firmware entities
2. **Second pass**: Create agent entities with configuration:
   - `direct_target_ids` resolved from `agent/direct/targets` (node names → `NodeId`)
   - `AgentConfig` passed to `Agent::new()`

### Key Data Structures

**NodeId** (`mcsim-common`): 32-byte public key identifying a node

**Agent targets**: `Vec<NodeId>` - list of nodes to send DMs to

**ContactInfo** (`mcsim-companion-protocol`): Full contact structure for firmware

### Required Changes Summary

| Component | File | Change |
| --------- | ---- | ------ |
| Properties | `mcsim-model/src/properties/definitions.rs` | Add `AGENT_CONTACTS` property |
| Model builder | `mcsim-model/src/lib.rs` | Resolve contact names, pass to agent config |
| Agent config | `mcsim-agents/src/lib.rs` | Add `contacts: Vec<ContactConfig>` field |
| Agent setup | `mcsim-agents/src/lib.rs` | Send `AddUpdateContact` during protocol init |
| Protocol state | `mcsim-agents/src/lib.rs` | Add `SettingUpContacts` state |

## Companion Protocol Commands

### CMD_SEND_MSG (0x17)

Send a direct message:
```
[0x17] [flags:1] [pubkey_prefix:6] [text:N]

flags: TXT_TYPE_PLAIN=0, etc.
pubkey_prefix: First 6 bytes of recipient's public key
```

Response: `RESP_CODE_SENT_FLOOD` or `RESP_CODE_SENT_DIRECT`

### CMD_ADD_UPDATE_CONTACT (0x09)

Add or update a contact:
```
[0x09] [last_mod:4] [pubkey:32] [type:1] [flags:1] [name:N]

last_mod: Timestamp
pubkey: Full 32-byte public key
type: ADV_TYPE_CHAT=0, ADV_TYPE_REPEATER=1, etc.
```

### CMD_RESET_PATH (0x0A)

Reset path for a contact (force flood routing):
```
[0x0A] [pubkey:32]
```

### RESP_CODE_SENT_FLOOD (0xF1)

Response indicating message was sent via flood routing.

### RESP_CODE_SENT_DIRECT (0xF2)

Response indicating message was sent via direct routing.

## Metrics

The simulation tracks these DM-related metrics:

| Metric | Description |
|--------|-------------|
| `mcsim.dm.sent` | DMs sent by agent |
| `mcsim.dm.received` | DMs received by agent |
| `mcsim.dm.ack_received` | ACKs received for sent DMs |
| `mcsim.radio.tx_packets{route_type=flood}` | Packets transmitted via flood |
| `mcsim.radio.tx_packets{route_type=direct}` | Packets transmitted via direct routing |

## References

- [MeshCore/src/helpers/BaseChatMesh.cpp](../MeshCore/src/helpers/BaseChatMesh.cpp) - Contact management, message sending
- [MeshCore/src/Mesh.cpp](../MeshCore/src/Mesh.cpp) - Packet routing
- [MeshCore/src/helpers/Contacts.h](../MeshCore/src/helpers/Contacts.h) - ContactInfo structure
- [MeshCore/docs/FAQ.md](../MeshCore/docs/FAQ.md) - Retry behavior explanation
