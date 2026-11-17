#!/usr/bin/env python3
# qingyu-monitor:v1 — original local, observe-only status transport.
"""A bounded local event sink. No prompts, responses, tool arguments or network I/O."""
import json
import os
from pathlib import Path
import re
import secrets
import stat
import subprocess
import sys
import time

MAX_EVENTS = 256
MAX_INPUT = 1024 * 1024


def _text(value, limit=160):
    return "".join(c for c in value if c >= " " and c != "\x7f")[:limit] if isinstance(value, str) else ""


def _private(path, directory=False):
    info = path.lstat()
    expected = stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode)
    return expected and info.st_uid == os.getuid() and not info.st_mode & 0o077


def emit(root, agent, event, metadata):
    """Best effort only. Every failure is swallowed; the caller's behavior is unchanged."""
    try:
        root = Path(root)
        spool = root / "events"
        marker = root / "enabled"
        if not (_private(root, True) and _private(spool, True) and _private(marker)):
            return
        if not 0 <= time.time() - marker.stat().st_mtime < 45:
            return
        with os.scandir(spool) as entries:
            if sum(1 for _, _entry in zip(range(MAX_EVENTS), entries)) >= MAX_EVENTS:
                return
        tool = _text(metadata.get("tool_name"), 96)
        if not re.fullmatch(r"[A-Za-z0-9_.:/-]{0,96}", tool):
            tool = ""
        pid = metadata.get("pid")
        pid = pid if isinstance(pid, int) and 0 < pid < 2 ** 32 else None
        value = {
            "version": 1, "agent": agent, "event": event,
            "at_ms": metadata.get("at_ms", time.time_ns() // 1_000_000),
            "session_id": _text(metadata.get("session_id")),
            "turn_id": _text(metadata.get("turn_id")),
            "project": _text(metadata.get("cwd"), 1024),
            "tool": tool, "pid": pid,
            "heartbeat": metadata.get("heartbeat") is True,
            "title": _text(metadata.get("title"), 256),
            "started_at_ms": metadata.get("started_at_ms") if isinstance(metadata.get("started_at_ms"), int) else None,
        }
        body = json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        if len(body) > 4096:
            return
        path = spool / (str(time.time_ns()) + "-" + str(os.getpid()) + "-" + secrets.token_hex(4) + ".json")
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
        fd = os.open(path, flags, 0o600)
        try:
            os.write(fd, body)
        finally:
            os.close(fd)
    except BaseException:
        pass


def _agent_pid():
    """Resolve the short-lived hook shell while it is alive; read no command arguments."""
    original = pid = os.getppid()
    try:
        for _ in range(4):
            output = subprocess.run(["/bin/ps", "-p", str(pid), "-o", "ppid=,comm="],
                                    capture_output=True, timeout=0.2, check=False).stdout[:2048]
            parent, executable = output.decode("utf-8", "replace").strip().split(None, 1)
            if Path(executable).name not in {"sh", "bash", "zsh", "dash", "fish"}:
                return pid
            pid = int(parent)
            if pid <= 1:
                break
    except BaseException:
        pass
    return original


def main():
    try:
        if len(sys.argv) != 4:
            return
        root, agent, event = sys.argv[1:]
        raw = sys.stdin.buffer.read(MAX_INPUT + 1)
        if len(raw) > MAX_INPUT:
            return
        value = json.loads(raw) if raw else {}
        if not isinstance(value, dict):
            return
        if event == "Notification":
            if value.get("notification_type") not in {"permission_prompt", "elicitation_dialog"}:
                return
            event = "PermissionRequest"
        # Select metadata before it reaches disk; never persist the hook payload.
        emit(root, agent, event, {
            "session_id": value.get("session_id") or value.get("thread_id"),
            "turn_id": value.get("turn_id") or value.get("task_id"),
            "cwd": value.get("cwd"),
            "tool_name": value.get("tool_name"),
            "pid": value.get(agent + "_pid") or value.get("pid") or _agent_pid(),
            "title": value.get("session_title") or value.get("thread_name") or value.get("title"),
        })
    except BaseException:
        pass


if __name__ == "__main__":
    main()
