"""Execution plan types for the orchestration layer."""

from __future__ import annotations

from enum import Enum
from typing import Any


class PlanStatus(str, Enum):
    PENDING_APPROVAL = "PendingApproval"
    APPROVED = "Approved"
    RUNNING = "Running"
    COMPLETED = "Completed"
    FAILED = "Failed"
    CANCELLED = "Cancelled"
    ROLLING_BACK = "RollingBack"
    ROLLED_BACK = "RolledBack"


class NodeStatus(str, Enum):
    PENDING = "Pending"
    RUNNING = "Running"
    COMPLETED = "Completed"
    FAILED = "Failed"
    SKIPPED = "Skipped"
    CANCELLED = "Cancelled"


class EdgeCondition(str, Enum):
    SUCCESS = "Success"
    ALWAYS = "Always"
    ON_FAILURE = "OnFailure"


class ActionType(str, Enum):
    OPEN_APP = "open_app"
    OPEN_URL = "open_url"
    FOCUS_WINDOW = "focus_window"
    LAUNCH_COMMAND = "launch_command"
    SWITCH_WORKSPACE = "switch_workspace"
    SEARCH_FILES = "search_files"
    MEDIA_CONTROL = "media_control"
    CLIPBOARD_SET = "clipboard_set"
    READ_WINDOW_TITLE = "read_window_title"
    NOTIFY = "notify"


def get_action_params(action: ActionType, **kwargs: Any) -> dict[str, Any]:
    """Build params dict for an action type from keyword arguments."""
    match action:
        case ActionType.OPEN_APP:
            return {"app": kwargs.get("app", "")}
        case ActionType.OPEN_URL:
            return {"url": kwargs.get("url", "")}
        case ActionType.FOCUS_WINDOW:
            return {
                "app": kwargs.get("app"),
                "title": kwargs.get("title"),
            }
        case ActionType.LAUNCH_COMMAND:
            return {
                "command": kwargs.get("command", ""),
                "args": kwargs.get("args", []),
            }
        case ActionType.SWITCH_WORKSPACE:
            return {"workspace": kwargs.get("workspace", "")}
        case ActionType.SEARCH_FILES:
            return {
                "query": kwargs.get("query", ""),
                "path": kwargs.get("path"),
            }
        case ActionType.MEDIA_CONTROL:
            return {"action": kwargs.get("action", "")}
        case ActionType.CLIPBOARD_SET:
            return {"text": kwargs.get("text", "")}
        case ActionType.READ_WINDOW_TITLE:
            return {}
        case ActionType.NOTIFY:
            return {
                "title": kwargs.get("title", ""),
                "body": kwargs.get("body", ""),
            }


def classify_risk(action: ActionType) -> str:
    """Classify an action's risk level."""
    match action:
        case ActionType.OPEN_APP | ActionType.OPEN_URL | ActionType.MEDIA_CONTROL | ActionType.NOTIFY | ActionType.READ_WINDOW_TITLE:
            return "safe"
        case ActionType.FOCUS_WINDOW | ActionType.SWITCH_WORKSPACE | ActionType.CLIPBOARD_SET | ActionType.SEARCH_FILES:
            return "privileged"
        case ActionType.LAUNCH_COMMAND:
            return "risky"
