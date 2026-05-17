"""Desktop state snapshot manager.

Maintains a live view of the system state by consuming events from enad.
The snapshot is injected into LLM prompts for contextual awareness.
"""

import time
from dataclasses import dataclass, field


@dataclass
class DesktopState:
    """Current snapshot of the desktop environment."""

    # Window context
    focused_app: str = ""
    focused_title: str = ""
    workspace: str = ""

    # Power
    battery_pct: float = 0.0
    battery_state: str = ""

    # Network
    network_connected: bool = True
    network_ssid: str = ""

    # Audio
    audio_volume: float = 0.0
    audio_muted: bool = False
    media_player: str = ""
    media_state: str = ""
    media_title: str = ""

    # Clipboard
    clipboard_preview: str = ""
    clipboard_type: str = ""

    # System
    hostname: str = ""
    os_name: str = ""

    # History — recent events for context
    recent_events: list[dict] = field(default_factory=list)
    _max_events: int = 50

    def update(self, event: dict) -> None:
        """Update state from an enad system event."""
        kind = event.get("kind", "")
        payload = event.get("payload", {})
        event_type = payload.get("type", "")
        data = payload.get("data", {})

        # Track event in history.
        self.recent_events.append({
            "kind": kind,
            "type": event_type,
            "data": data,
            "ts": time.time(),
        })
        if len(self.recent_events) > self._max_events:
            self.recent_events = self.recent_events[-self._max_events:]

        # Update state based on event type.
        match event_type:
            case "WindowFocused":
                self.focused_app = data.get("app", self.focused_app)
                self.focused_title = data.get("title", self.focused_title)

            case "WorkspaceChanged":
                self.workspace = data.get("workspace", self.workspace)

            case "BatteryStatus":
                self.battery_pct = data.get("percentage", self.battery_pct)
                self.battery_state = data.get("state", self.battery_state)

            case "NetworkStatus":
                self.network_connected = data.get("connected", self.network_connected)
                self.network_ssid = data.get("ssid", self.network_ssid)

            case "AudioVolumeChanged":
                self.audio_volume = data.get("volume", self.audio_volume)
                self.audio_muted = data.get("muted", self.audio_muted)

            case "MediaPlayback":
                self.media_player = data.get("player", self.media_player)
                self.media_state = data.get("state", self.media_state)
                self.media_title = data.get("title", self.media_title)

            case "ClipboardUpdated":
                self.clipboard_preview = data.get("preview", self.clipboard_preview)
                self.clipboard_type = data.get("content_type", self.clipboard_type)

    def to_context_string(self) -> str:
        """Format the current state as a context block for LLM injection."""
        parts = []

        if self.focused_app:
            parts.append(f"Focused application: {self.focused_app}")
            if self.focused_title:
                parts.append(f"Window title: {self.focused_title}")

        if self.workspace:
            parts.append(f"Current workspace: {self.workspace}")

        if self.battery_state and self.battery_state not in ("fully-charged", "unknown"):
            parts.append(f"Battery: {self.battery_pct:.0f}% ({self.battery_state})")

        if not self.network_connected:
            parts.append("Network: disconnected")
        elif self.network_ssid:
            parts.append(f"Network: connected to {self.network_ssid}")

        if self.media_player and self.media_state == "Playing":
            media_info = f"{self.media_player}: {self.media_title}" if self.media_title else self.media_player
            parts.append(f"Media playing: {media_info}")

        if self.clipboard_preview:
            parts.append(f"Clipboard ({self.clipboard_type}): {self.clipboard_preview}")

        if self.hostname:
            parts.append(f"Hostname: {self.hostname}")

        if not parts:
            return "No desktop context available."

        return "Desktop context:\n" + "\n".join(f"- {p}" for p in parts)

    def to_dict(self) -> dict:
        """Serialize state for API responses."""
        return {
            "focused_app": self.focused_app,
            "focused_title": self.focused_title,
            "workspace": self.workspace,
            "battery_pct": self.battery_pct,
            "battery_state": self.battery_state,
            "network_connected": self.network_connected,
            "network_ssid": self.network_ssid,
            "audio_volume": self.audio_volume,
            "audio_muted": self.audio_muted,
            "media_player": self.media_player,
            "media_state": self.media_state,
            "media_title": self.media_title,
            "clipboard_preview": self.clipboard_preview,
            "clipboard_type": self.clipboard_type,
            "hostname": self.hostname,
        }
