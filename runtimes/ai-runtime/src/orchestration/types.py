"""Pydantic models for execution plans."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any
from uuid import uuid4

from . import ActionType, EdgeCondition, NodeStatus, PlanStatus


class PlanNode:
    """A single step in an execution plan."""

    def __init__(
        self,
        label: str,
        action: ActionType,
        params: dict[str, Any] | None = None,
        *,
        max_retries: int = 2,
        requires_approval: bool | None = None,
        rollback_action: ActionType | None = None,
        rollback_params: dict[str, Any] | None = None,
    ) -> None:
        self.id = str(uuid4())
        self.label = label
        self.action = action
        self.params = params or {}
        self.status = NodeStatus.PENDING
        self.retry_count = 0
        self.max_retries = max_retries
        self.error: str | None = None
        self.result: str | None = None
        self.rollback_action = rollback_action
        self.rollback_params = rollback_params or {}
        self.requires_approval = (
            requires_approval
            if requires_approval is not None
            else self._default_approval()
        )

    def _default_approval(self) -> bool:
        """Determine if this node should require approval by default."""
        return self.action == ActionType.LAUNCH_COMMAND

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "label": self.label,
            "action_type": self.action.value,
            "action": {"type": self.action.value, "params": self.params},
            "status": self.status.value,
            "retry_count": self.retry_count,
            "max_retries": self.max_retries,
            "error": self.error,
            "result": self.result,
            "rollback_action": (
                {
                    "type": self.rollback_action.value,
                    "params": self.rollback_params,
                }
                if self.rollback_action
                else None
            ),
            "requires_approval": self.requires_approval,
        }

    def to_enad_action(self, action_type: ActionType) -> dict[str, Any]:
        """Convert this node to the enad IPC action format."""
        return {
            "type": action_type.value,
            "params": self.params,
        }


class PlanEdge:
    """A dependency edge between two plan nodes."""

    def __init__(
        self,
        from_id: str,
        to_id: str,
        condition: EdgeCondition = EdgeCondition.SUCCESS,
    ) -> None:
        self.from_id = from_id
        self.to_id = to_id
        self.condition = condition

    def to_dict(self) -> dict[str, Any]:
        return {
            "from": self.from_id,
            "to": self.to_id,
            "condition": self.condition.value,
        }


class ExecutionPlan:
    """A structured multi-step execution plan."""

    def __init__(
        self,
        title: str,
        description: str,
        nodes: list[PlanNode],
        edges: list[PlanEdge] | None = None,
        *,
        auto_approve: bool = False,
    ) -> None:
        self.id = str(uuid4())
        self.title = title
        self.description = description
        self.nodes = nodes
        self.edges = edges or []
        self.status = PlanStatus.PENDING_APPROVAL
        self.created_at = datetime.now(timezone.utc)
        self.approved_at: datetime | None = None
        self.completed_at: datetime | None = None
        self.auto_approve = auto_approve

    @property
    def requires_approval(self) -> bool:
        return any(n.requires_approval for n in self.nodes)

    def to_enad_plan(self) -> dict[str, Any]:
        """Serialize to enad's ExecutionPlan JSON format."""
        return {
            "id": self.id,
            "title": self.title,
            "description": self.description,
            "nodes": [
                {
                    "id": n.id,
                    "label": n.label,
                    "action": n.to_enad_action(n.action),
                    "status": NodeStatus.PENDING.value,
                    "retry_count": n.retry_count,
                    "max_retries": n.max_retries,
                    "error": n.error,
                    "result": n.result,
                    "rollback_action": (
                        n.to_enad_action(n.rollback_action)
                        if n.rollback_action
                        else None
                    ),
                    "requires_approval": n.requires_approval,
                    "started_at": None,
                    "completed_at": None,
                }
                for n in self.nodes
            ],
            "edges": [e.to_dict() for e in self.edges],
            "status": self.status.value,
            "created_at": self.created_at.isoformat(),
            "approved_at": None,
            "completed_at": None,
        }
