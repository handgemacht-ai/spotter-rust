#!/usr/bin/env python3
import json
import uuid
from datetime import datetime, timedelta, timezone
from pathlib import Path


ROOT = Path("tests/fixtures/transcripts")
TOOL_SESSION = "d6e0bada-1959-4eec-a9d2-0bfade768d8f"
SHORT_SESSION = "55604662-cf2a-4331-851a-ec234028f8ca"
DISCOVERABLE_SESSION = "258c7280-ae70-4798-800f-63464d01a85d"
SUBAGENT_SESSION = "491e126e-1e71-469c-ade2-fcc8af567c74"
PROJECT_CWD = "/home/USER/projects/spotter"
WORKTREE_CWD = "/home/USER/projects/spotter-worktrees/spotter-public-fixture"
BASE = datetime(2026, 2, 10, 12, 0, tzinfo=timezone.utc)


def main() -> int:
    write_jsonl(ROOT / "tool_heavy.jsonl", tool_heavy_rows())
    write_jsonl(ROOT / "short.jsonl", short_rows())
    write_jsonl(ROOT / "subagent.jsonl", standalone_subagent_rows())
    write_jsonl(ROOT / f"{DISCOVERABLE_SESSION}.jsonl", discoverable_rows())
    write_jsonl(
        ROOT / DISCOVERABLE_SESSION / "subagents" / "agent-a881341.jsonl",
        child_subagent_rows(),
    )
    return 0


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in rows)
    path.write_text(text, encoding="utf-8")


def tool_heavy_rows() -> list[dict]:
    builder = RowBuilder(TOOL_SESSION, PROJECT_CWD, "master", "tool-heavy-fixture")
    builder.user("start phoenix in background")
    builder.tool_use(
        "toolu_018GZVh9ymkrdx1TnR8reg5Y",
        "Bash",
        {"command": "mix phx.server", "description": "Start demo service"},
        input_tokens=100,
    )
    builder.tool_result("toolu_018GZVh9ymkrdx1TnR8reg5Y", "phoenix demo server started")
    builder.tool_use(
        "toolu_01FZ9TWbiy1LPzHm1AAyUazo",
        "Bash",
        {
            "command": "tail -5 /tmp/spotter-demo/tasks/server.output",
            "description": "Check service output",
        },
        input_tokens=120,
    )
    builder.tool_result("toolu_01FZ9TWbiy1LPzHm1AAyUazo", "service output is ready")
    builder.tool_use(
        "toolu_read_assets",
        "Read",
        {"file_path": "assets/js/app.js"},
        input_tokens=130,
    )
    builder.tool_result("toolu_read_assets", "export const status = 'ready';")
    builder.tool_use(
        "toolu_edit_assets",
        "Edit",
        {
            "file_path": "assets/js/app.js",
            "old_string": "status = 'ready'",
            "new_string": "status = 'complete'",
        },
        input_tokens=140,
    )
    builder.tool_result("toolu_edit_assets", "updated assets/js/app.js")
    builder.tool_use(
        "toolu_grep_phoenix",
        "Grep",
        {"pattern": "phoenix", "path": "lib"},
        input_tokens=150,
    )
    builder.tool_result("toolu_grep_phoenix", "lib/demo.ex: phoenix handler")
    builder.tool_use(
        "toolu_list_routes",
        "Bash",
        {"command": "cargo test route_snapshot", "description": "Run route check"},
        input_tokens=160,
    )
    builder.tool_result("toolu_list_routes", "route snapshot passed")
    while len(builder.rows) < 39:
        builder.note(f"synthetic filler message {len(builder.rows)}")
    return builder.rows


def short_rows() -> list[dict]:
    builder = RowBuilder(SHORT_SESSION, PROJECT_CWD, "master", "short-fixture")
    builder.user("check rejected command behavior")
    builder.tool_use(
        "toolu_short_ok",
        "Bash",
        {"command": "mix compile", "description": "Compile demo app"},
        input_tokens=6_000,
        cache_read=0,
    )
    builder.tool_result("toolu_short_ok", "compiled")
    builder.tool_use(
        "toolu_short_rejected",
        "Bash",
        {"command": "mix run unsafe_task", "description": "Rejected demo command"},
        input_tokens=10_000,
        cache_read=220_000,
    )
    builder.tool_result("toolu_short_rejected", "User rejected tool use", is_error=True)
    builder.note("rejected command was left unchanged")
    return builder.rows


def discoverable_rows() -> list[dict]:
    builder = RowBuilder(
        DISCOVERABLE_SESSION,
        WORKTREE_CWD,
        "fixture-branch",
        "discoverable-fixture",
    )
    builder.user("parent session for a synthetic subagent")
    builder.tool_use(
        "toolu_parent_task",
        "Task",
        {"description": "Ask subagent for docs summary"},
        input_tokens=50,
    )
    builder.tool_result("toolu_parent_task", "subagent completed")
    return builder.rows


def child_subagent_rows() -> list[dict]:
    builder = RowBuilder(
        DISCOVERABLE_SESSION,
        WORKTREE_CWD,
        "fixture-branch",
        "discoverable-fixture",
        is_sidechain=True,
        agent_id="a881341",
    )
    builder.user("summarize public API docs")
    builder.tool_use(
        "toolu_subagent_fetch",
        "WebFetch",
        {"url": "https://example.com/docs", "prompt": "Summarize demo docs"},
        input_tokens=80,
    )
    builder.tool_result("toolu_subagent_fetch", "demo docs summary")
    return builder.rows


def standalone_subagent_rows() -> list[dict]:
    builder = RowBuilder(
        SUBAGENT_SESSION,
        PROJECT_CWD,
        "master",
        "standalone-subagent-fixture",
        agent_id="a111111",
    )
    builder.user("create a small fixture plan")
    builder.tool_use(
        "toolu_standalone_read",
        "Read",
        {"file_path": "docs/example.md"},
        input_tokens=70,
    )
    builder.tool_result("toolu_standalone_read", "example fixture document")
    return builder.rows


class RowBuilder:
    def __init__(
        self,
        session_id: str,
        cwd: str,
        git_branch: str,
        slug: str,
        *,
        is_sidechain: bool = False,
        agent_id: str | None = None,
    ) -> None:
        self.session_id = session_id
        self.cwd = cwd
        self.git_branch = git_branch
        self.slug = slug
        self.is_sidechain = is_sidechain
        self.agent_id = agent_id
        self.rows: list[dict] = []
        self._parent: str | None = None
        self._tick = 0
        self.progress("SessionStart", "startup", "/home/USER/.claude/hooks/demo-start.sh")

    def progress(self, event: str, name: str, command: str) -> None:
        self._append(
            {
                "type": "progress",
                "data": {
                    "type": "hook_progress",
                    "hookEvent": event,
                    "hookName": f"{event}:{name}",
                    "command": command,
                },
            }
        )

    def user(self, text: str) -> None:
        self._append({"type": "user", "message": {"role": "user", "content": text}})

    def note(self, text: str) -> None:
        self._append(
            {
                "type": "assistant",
                "message": {
                    "id": message_id(self.session_id, len(self.rows)),
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "text", "text": text}],
                    "stop_reason": None,
                    "stop_sequence": None,
                    "usage": usage(input_tokens=1, cache_creation=0, cache_read=0, output=1),
                },
            }
        )

    def tool_use(
        self,
        tool_use_id: str,
        name: str,
        tool_input: dict,
        *,
        input_tokens: int,
        cache_read: int = 1_000,
    ) -> None:
        self._append(
            {
                "type": "assistant",
                "message": {
                    "id": message_id(tool_use_id, 0),
                    "type": "message",
                    "role": "assistant",
                    "content": [
                        {
                            "type": "tool_use",
                            "id": tool_use_id,
                            "name": name,
                            "input": tool_input,
                        }
                    ],
                    "stop_reason": None,
                    "stop_sequence": None,
                    "usage": usage(
                        input_tokens=input_tokens,
                        cache_creation=10,
                        cache_read=cache_read,
                        output=5,
                    ),
                },
            }
        )

    def tool_result(self, tool_use_id: str, content: str, *, is_error: bool = False) -> None:
        self._append(
            {
                "type": "user",
                "message": {
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": content,
                            "is_error": is_error,
                        }
                    ],
                },
                "toolUseResult": content,
            }
        )

    def _append(self, row: dict) -> None:
        current_uuid = deterministic_uuid(self.session_id, len(self.rows))
        common = {
            "uuid": current_uuid,
            "parentUuid": self._parent,
            "isSidechain": self.is_sidechain,
            "userType": "external",
            "cwd": self.cwd,
            "sessionId": self.session_id,
            "version": "2.1.99",
            "gitBranch": self.git_branch,
            "slug": self.slug,
            "timestamp": timestamp(self._tick),
        }
        if self.agent_id is not None:
            common["agentId"] = self.agent_id
        common.update(row)
        self.rows.append(common)
        self._parent = current_uuid
        self._tick += 1


def usage(
    *, input_tokens: int, cache_creation: int, cache_read: int, output: int
) -> dict:
    return {
        "input_tokens": input_tokens,
        "cache_creation_input_tokens": cache_creation,
        "cache_read_input_tokens": cache_read,
        "cache_creation": {
            "ephemeral_5m_input_tokens": 0,
            "ephemeral_1h_input_tokens": cache_creation,
        },
        "output_tokens": output,
        "service_tier": "standard",
        "inference_geo": "not_available",
    }


def deterministic_uuid(seed: str, index: int) -> str:
    return str(uuid.uuid5(uuid.NAMESPACE_URL, f"spotter-fixture:{seed}:{index}"))


def message_id(seed: str, index: int) -> str:
    return f"msg_{uuid.uuid5(uuid.NAMESPACE_URL, f'spotter-message:{seed}:{index}').hex[:24]}"


def timestamp(index: int) -> str:
    value = BASE + timedelta(milliseconds=index * 1_000)
    return value.isoformat(timespec="milliseconds").replace("+00:00", "Z")


if __name__ == "__main__":
    raise SystemExit(main())
