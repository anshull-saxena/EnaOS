# 5. Security, Infrastructure, and APIs

## 5.1 Security Model
EnaOS fundamentally changes how code executes. Because AI can run arbitrary workflows, the security model must be pessimistic.

- **Zero Trust IPC:** Every gRPC call between services requires an authorization token.
- **Root Isolation:** Only `enad` runs as root. The Compositor runs as the user. The AI Runtime runs as an unprivileged user (`ena-ai-user`).
- **Network Banning:** Agent containers are spun up without network access by default. If an agent needs to scrape the web, it requests the `NETWORK_EGRESS` capability, which prompts the user for approval.

## 5.2 Permission System
Similar to Android/iOS, but for the desktop.
Capabilities include:
- `filesystem:read:<path>`
- `filesystem:write:<path>`
- `system:execute_command`
- `browser:control`
- `window:manipulate`

The **Agent Orchestrator** reads these required capabilities from the agent manifest. If unapproved, the Ena Bar prompts the user:
_"Agent 'Research Assistant' wants to read ~/Documents and access the network. Allow?"_

## 5.3 Initial API Contracts (gRPC / protobuf)

### Example: `ena.v1.AgentService`
```protobuf
syntax = "proto3";
package ena.v1;

service AgentService {
  rpc SpawnAgent (SpawnRequest) returns (SpawnResponse);
  rpc StreamAgentLogs (LogRequest) returns (stream LogChunk);
  rpc TerminateAgent (TerminateRequest) returns (TerminateResponse);
}

message SpawnRequest {
  string task_description = 1;
  repeated string required_capabilities = 2;
}

message SpawnResponse {
  string agent_id = 1;
  string status = 2;
}
```

### Example: `ena.v1.ContextService`
```protobuf
service ContextService {
  rpc GetCurrentContext (Empty) returns (ContextSnapshot);
  rpc AddMemory (MemoryItem) returns (Empty);
}
```

## 5.4 Build System & Container Strategy
- **Base OS:** We recommend providing a NixOS flake for native installation, and a Fedora Atomic/Silverblue OSTree image for broader adoption.
- **Build Matrix:**
  - Rust binaries: Compiled for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.
  - Python Runtime: Packaged using `pexpect` or `shiv` into a single executable zip to avoid dependency hell, or deployed as a Podman container alongside the host OS.
- **Containers:** We use Podman (daemonless) instead of Docker for better security and rootless execution of sandboxed agents.