# qingyu-monitor:v1 — original Hermes lifecycle observer; standard library only.
"""Never returns directives, mutates arguments, approves requests, or uses the network."""
import os
from pathlib import Path
import queue
import threading
import time

from .sink import emit

_ROOT = Path(__QINGYU_MONITOR_ROOT__)
_QUEUE = queue.Queue(maxsize=128)
_STATE = {}
_STARTS = {}
_LOCK = threading.Lock()
_STARTED = False
_ACTIVE = {"UserPromptSubmit", "Resume", "PreToolUse", "PostToolUse", "PermissionRequest"}


def _observe(event, values):
    try:
        session = values.get("session_id") or values.get("session_key") or ("process-" + str(os.getpid()))
        if not isinstance(session, str):
            return
        data = {
            "session_id": session[:160],
            "turn_id": values.get("turn_id") or values.get("task_id"),
            "cwd": os.getcwd(), "tool_name": values.get("tool_name"),
            "pid": os.getpid(), "at_ms": time.time_ns() // 1_000_000,
            "title": values.get("session_title") or values.get("title"),
        }
        with _LOCK:
            if len(_STATE) >= 64 and session not in _STATE:
                oldest = min(_STATE, key=lambda key: (_STATE[key][0] in _ACTIVE, _STATE[key][1]["at_ms"]))
                _STATE.pop(oldest)
                _STARTS.pop(oldest, None)
            previous = _STARTS.get(session)
            if previous and not data["turn_id"]:
                data["turn_id"] = previous[0]
            previous_data = _STATE.get(session, (None, {}))[1]
            if not data["title"]:
                data["title"] = previous_data.get("title")
            if event == "UserPromptSubmit" and (not previous or previous[0] != data["turn_id"] or _STATE.get(session, (None,))[0] not in _ACTIVE):
                _STARTS[session] = (data["turn_id"], data["at_ms"])
            if session in _STARTS and _STARTS[session][0] == data["turn_id"]:
                data["started_at_ms"] = _STARTS[session][1]
            _STATE[session] = (event, data)
        try:
            _QUEUE.put_nowait((event, data))
        except queue.Full:
            try:
                _QUEUE.get_nowait()
                _QUEUE.put_nowait((event, data))
            except (queue.Empty, queue.Full):
                pass
    except BaseException:
        pass
    return None


def _end(**values):
    event = "StopFailure" if values.get("failed") else "Interrupt" if values.get("interrupted") else "Stop"
    return _observe(event, values)


def _post_tool(**values):
    return _observe("PostToolUseFailure" if values.get("error") or values.get("is_error") else "PostToolUse", values)


def _worker():
    next_heartbeat = time.monotonic() + 15
    while True:
        try:
            event, data = _QUEUE.get(timeout=max(0.1, next_heartbeat - time.monotonic()))
            emit(_ROOT, "hermes", event, data)
        except queue.Empty:
            pass
        except BaseException:
            pass
        if time.monotonic() >= next_heartbeat:
            with _LOCK:
                active = [(event, dict(data)) for event, data in _STATE.values() if event in _ACTIVE]
            for event, data in active:
                data.update(at_ms=time.time_ns() // 1_000_000, heartbeat=True)
                emit(_ROOT, "hermes", event, data)
            next_heartbeat = time.monotonic() + 15


def register(ctx):
    global _STARTED
    mapping = {
        "on_session_start": "SessionStart", "pre_llm_call": "UserPromptSubmit",
        "pre_tool_call": "PreToolUse", "on_session_finalize": "SessionEnd",
        "pre_approval_request": "PermissionRequest", "post_approval_response": "Resume",
        "on_human_input_request": "PermissionRequest", "on_human_input_resolved": "Resume",
    }
    for hook, event in mapping.items():
        try:
            ctx.register_hook(hook, lambda _event=event, **values: _observe(_event, values))
        except Exception:
            pass  # Older Hermes releases may not provide every observer hook.
    for hook, callback in (("post_tool_call", _post_tool), ("on_session_end", _end)):
        try:
            ctx.register_hook(hook, callback)
        except Exception:
            pass
    if not _STARTED:
        try:
            threading.Thread(target=_worker, name="qingyu-status", daemon=True).start()
            _STARTED = True
        except Exception:
            pass
