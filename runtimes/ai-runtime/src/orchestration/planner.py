"""Intent parser and execution plan builder.

The planner takes a natural language user intent and produces a
structured ExecutionPlan using LLM inference. This is the ONLY place
where the LLM touches the plan — after this, plans are pure data.
"""

from __future__ import annotations

import json
from collections.abc import AsyncIterator
from typing import Any

from src.context.state import DesktopState
from src.inference.ollama import OllamaProvider
from src.orchestration.types import (
    ActionType,
    EdgeCondition,
    ExecutionPlan,
    PlanEdge,
    PlanNode,
    classify_risk,
)

PLANNER_SYSTEM_PROMPT = """You are EnaOS Planner — a structured intent parser.

Your ONLY job is to convert user requests into a JSON execution plan.

RULES:
- Return ONLY valid JSON — no markdown, no explanation.
- Each action must be an object with: type (action type string) and params (object).
- Do NOT add actions that the system cannot do. Only use these action types:
  * open_app — params: {app: string}
  * open_url — params: {url: string}
  * focus_window — params: {app?: string, title?: string}
  * launch_command — params: {command: string, args: string[]}
  * switch_workspace — params: {workspace: string}
  * search_files — params: {query: string, path?: string}
  * media_control — params: {action: "play"|"pause"|"next"|"previous"}
  * clipboard_set — params: {text: string}
  * read_window_title — params: {}
  * notify — params: {title: string, body: string}

Plan format:
{
  "title": "Short plan title",
  "description": "What this plan does",
  "nodes": [
    {
      "label": "Human-readable step name",
      "action_type": "open_app",
      "params": {"app": "Firefox"},
      "requires_approval": false,
      "max_retries": 2,
      "rollback_action_type": null,
      "rollback_params": {}
    }
  ],
  "edges": [
    {"from": 0, "to": 1, "condition": "Success"}
  ]
}

Guidelines:
- Break multi-step requests into sequential nodes.
- Use "from" integer indices into the nodes array for edge dependencies.
- Mark risky actions (launch_command) as requires_approval: true.
- For simple single-action requests, return a single node with no edges.
- If the request is ambiguous or can't be translated to actions, return {"error": "explanation"}.
- Be conservative — only plan actions you are CERTAIN the user wants.
"""


class Planner:
    """Parses natural language intents into structured execution plans."""

    def __init__(self, provider: OllamaProvider | None = None) -> None:
        self._provider = provider or OllamaProvider()

    async def plan(
        self,
        intent: str,
        desktop_state: DesktopState | None = None,
        memory_context: str = "",
    ) -> ExecutionPlan | None:
        """Parse a user intent into an ExecutionPlan.

        Returns None if the intent cannot be parsed into actions.
        """
        context = ""
        if desktop_state:
            context = desktop_state.to_context_string()

        prompt = PLANNER_SYSTEM_PROMPT
        if context:
            prompt += f"\n\nCurrent desktop context:\n{context}"
        if memory_context:
            prompt += f"\n\nWorking memory:\n{memory_context}"
        prompt += f"\n\nUser request: {intent}"

        result = await self._provider.chat(
            messages=[{"role": "user", "content": intent}],
            system_prompt=prompt,
            stream=False,
        )

        if not result:
            return None

        return self._parse_plan_response(result)

    async def plan_stream(
        self,
        intent: str,
        desktop_state: DesktopState | None = None,
        memory_context: str = "",
    ) -> AsyncIterator[str]:
        """Stream the LLM response while building a plan.

        Yields the raw token stream; callers can display it
        while the final plan is being constructed.
        """
        context = ""
        if desktop_state:
            context = desktop_state.to_context_string()

        prompt = PLANNER_SYSTEM_PROMPT
        if context:
            prompt += f"\n\nCurrent desktop context:\n{context}"
        if memory_context:
            prompt += f"\n\nWorking memory:\n{memory_context}"
        prompt += f"\n\nUser request: {intent}"

        stream = await self._provider.chat(
            messages=[{"role": "user", "content": intent}],
            system_prompt=prompt,
            stream=True,
        )

        if isinstance(stream, AsyncIterator):
            async for token in stream:
                yield token

    def _parse_plan_response(self, text: str) -> ExecutionPlan | None:
        """Parse the LLM's JSON response into an ExecutionPlan."""
        # Strip any markdown fences.
        text = text.strip()
        if text.startswith("```"):
            text = text.split("\n", 1)[-1]
        if text.endswith("```"):
            text = text.rsplit("```", 1)[0]
        text = text.strip()

        try:
            data = json.loads(text)
        except json.JSONDecodeError:
            return None

        if "error" in data:
            return None

        if "title" not in data or "nodes" not in data:
            return None

        try:
            nodes: list[PlanNode] = []
            edges: list[PlanEdge] = []

            for i, n in enumerate(data["nodes"]):
                action_type = ActionType(n.get("action_type", ""))
                params = n.get("params", {})
                node = PlanNode(
                    label=n.get("label", f"Step {i + 1}"),
                    action=action_type,
                    params=params,
                    max_retries=n.get("max_retries", 2),
                    requires_approval=n.get("requires_approval"),
                    rollback_action=(
                        ActionType(n["rollback_action_type"])
                        if n.get("rollback_action_type")
                        else None
                    ),
                    rollback_params=n.get("rollback_params", {}),
                )
                nodes.append(node)

            for e in data.get("edges", []):
                from_idx = e.get("from", 0)
                to_idx = e.get("to", 1)
                condition_str = e.get("condition", "Success")
                condition = EdgeCondition(condition_str)
                edges.append(PlanEdge(
                    from_id=nodes[from_idx].id,
                    to_id=nodes[to_idx].id,
                    condition=condition,
                ))

            title = data.get("title", "Untitled plan")
            description = data.get("description", "")

            return ExecutionPlan(title, description, nodes, edges)

        except (KeyError, ValueError, IndexError, TypeError):
            return None

    def plan_requires_approval(self, plan: ExecutionPlan) -> tuple[bool, list[str]]:
        """Check if a plan requires user approval.

        Returns (requires_approval, reasons).
        """
        reasons: list[str] = []
        for node in plan.nodes:
            risk = classify_risk(node.action)
            if risk == "risky" or node.requires_approval:
                reasons.append(f"'{node.label}' ({risk} action: {node.action.value})")
        return len(reasons) > 0, reasons
