#!/usr/bin/env python3
import argparse
import json
from datetime import datetime, timedelta, timezone
from pathlib import Path


def uuidish(number):
    raw = f"{number:032x}"
    return f"{raw[:8]}-{raw[8:12]}-{raw[12:16]}-{raw[16:20]}-{raw[20:]}"


def timestamp(base, ordinal):
    return (base + timedelta(milliseconds=ordinal * 10)).isoformat().replace("+00:00", "Z")


def assistant_tool_use(session_id, ordinal, tool_id, parent_uuid, command, base):
    uuid = uuidish(ordinal + 1)
    return {
        "parentUuid": parent_uuid,
        "isSidechain": False,
        "userType": "external",
        "cwd": "/tmp/spotter-perf",
        "sessionId": session_id,
        "version": "2.1.38",
        "gitBranch": "main",
        "type": "assistant",
        "message": {
            "model": "claude-opus-4-6",
            "id": f"msg_perf_{ordinal}",
            "type": "message",
            "role": "assistant",
            "content": [
                {
                    "type": "tool_use",
                    "id": tool_id,
                    "name": "Bash",
                    "input": {
                        "command": command,
                        "description": "performance fixture command",
                    },
                    "caller": {"type": "direct"},
                }
            ],
            "stop_reason": None,
            "stop_sequence": None,
            "usage": {
                "input_tokens": 10,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "cache_creation": {
                    "ephemeral_5m_input_tokens": 0,
                    "ephemeral_1h_input_tokens": 0,
                },
                "output_tokens": 5,
                "service_tier": "standard",
                "inference_geo": "not_available",
            },
        },
        "requestId": f"req_perf_{ordinal}",
        "uuid": uuid,
        "timestamp": timestamp(base, ordinal),
    }


def user_tool_result(session_id, ordinal, tool_id, parent_uuid, content, base):
    return {
        "parentUuid": parent_uuid,
        "isSidechain": False,
        "userType": "external",
        "cwd": "/tmp/spotter-perf",
        "sessionId": session_id,
        "version": "2.1.38",
        "gitBranch": "main",
        "type": "user",
        "message": {
            "role": "user",
            "content": [
                {
                    "tool_use_id": tool_id,
                    "type": "tool_result",
                    "content": content,
                    "is_error": False,
                }
            ],
        },
        "uuid": uuidish(ordinal + 1),
        "timestamp": timestamp(base, ordinal),
        "toolUseResult": {
            "stdout": content,
            "stderr": "",
            "interrupted": False,
            "isImage": False,
            "noOutputExpected": False,
        },
        "sourceToolAssistantUUID": parent_uuid,
    }


def text_message(session_id, ordinal, parent_uuid, text, base):
    uuid = uuidish(ordinal + 1)
    return {
        "parentUuid": parent_uuid,
        "isSidechain": False,
        "userType": "external",
        "cwd": "/tmp/spotter-perf",
        "sessionId": session_id,
        "version": "2.1.38",
        "gitBranch": "main",
        "type": "user",
        "message": {"role": "user", "content": text},
        "uuid": uuid,
        "timestamp": timestamp(base, ordinal),
    }


def write_jsonl(path, rows):
    with path.open("w", encoding="utf-8") as handle:
        for row in rows:
            handle.write(json.dumps(row, separators=(",", ":")))
            handle.write("\n")


def tool_call_rows(session_id, count, output_size):
    base = datetime(2026, 1, 1, tzinfo=timezone.utc)
    parent_uuid = None
    for index in range(count):
        tool_id = f"toolu_perf_{index:06d}"
        command = f"printf perf-{index:06d}"
        assistant = assistant_tool_use(session_id, index * 2, tool_id, parent_uuid, command, base)
        parent_uuid = assistant["uuid"]
        yield assistant
        result = user_tool_result(session_id, index * 2 + 1, tool_id, parent_uuid, "x" * output_size, base)
        parent_uuid = result["uuid"]
        yield result


def write_tool_call_file(path, session_id, count, output_size=32):
    write_jsonl(path, tool_call_rows(session_id, count, output_size))


def write_target_size_file(path, session_id, target_bytes):
    output_size = 1024
    sample_path = path.with_suffix(".sample")
    write_tool_call_file(sample_path, session_id, 10, output_size)
    bytes_per_call = max(1, sample_path.stat().st_size // 10)
    sample_path.unlink()
    count = max(1, target_bytes // bytes_per_call)
    while True:
        write_tool_call_file(path, session_id, count, output_size)
        size = path.stat().st_size
        if size >= target_bytes:
            return
        count = int(count * target_bytes / max(size, 1)) + 1


def write_message_file(path, session_id, count):
    base = datetime(2026, 1, 1, tzinfo=timezone.utc)
    parent_uuid = None
    rows = []
    for index in range(count):
        row = text_message(session_id, index, parent_uuid, f"message {index:05d} perf payload", base)
        parent_uuid = row["uuid"]
        rows.append(row)
    write_jsonl(path, rows)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("output_dir")
    parser.add_argument("--tool-calls", type=int, default=10_000)
    parser.add_argument("--inspect-messages", type=int, default=5_000)
    parser.add_argument("--sync-mib", type=int, default=1)
    parser.add_argument("--rss-mib", type=int, default=100)
    args = parser.parse_args()

    out = Path(args.output_dir)
    out.mkdir(parents=True, exist_ok=True)

    write_tool_call_file(out / "tool-calls-10k.jsonl", "perf-tool-calls-10k", args.tool_calls)
    write_tool_call_file(
        out / "inspect-5k-messages.jsonl",
        "perf-inspect-5k",
        args.inspect_messages // 2,
    )
    write_target_size_file(out / "sync-1mib.jsonl", "perf-sync-1mib", args.sync_mib * 1024 * 1024)
    write_target_size_file(out / "sync-100mib.jsonl", "perf-sync-100mib", args.rss_mib * 1024 * 1024)
    write_message_file(out / "messages-5k.jsonl", "perf-messages-5k", args.inspect_messages)

    for path in sorted(out.glob("*.jsonl")):
        print(f"{path}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    main()
