# qingyu-monitor:v4 — original Hermes lifecycle observer; standard library only.
"""Never returns directives, mutates arguments, approves requests, or uses the network."""
import os
from pathlib import Path
import queue
import secrets
import threading
import time

from .sink import emit

_ROOT = Path(__QINGYU_MONITOR_ROOT__)
_QUEUE = queue.Queue(maxsize=128)
_STATE = {}
_STARTS = {}
_ALIASES = {}
_RETIRED = {}
_LOCK = threading.Lock()
_STARTED = False
_STOP = None
_WORKER = None
_ACTIVE = {"UserPromptSubmit", "Resume", "PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest", "Waiting"}
_TERMINAL = {"Stop", "StopFailure", "Interrupt", "SessionEnd"}


def _routing_key(values, session):
    routing = values.get("session_key")
    if isinstance(routing, str) and routing:
        return routing
    # Hermes copies ContextVars into plugin callback workers. Never associate an
    # ambient gateway route unless its durable session id agrees with this event.
    try:
        from gateway.session_context import get_session_env
        if session and get_session_env("HERMES_SESSION_ID") == session:
            return get_session_env("HERMES_SESSION_KEY")
    except Exception:
        pass
    return None


def _observe(event, values):
    try:
        if not _STARTED or (_STOP is not None and _STOP.is_set()):
            return None
        session = values.get("session_id")
        turn = values.get("turn_id") or values.get("task_id")
        routing = _routing_key(values, session)
        with _LOCK:
            if not _STARTED or (_STOP is not None and _STOP.is_set()):
                return None
            if not session and isinstance(routing, str):
                session = _ALIASES.get(routing)
            if not session and turn:
                matches = [key for key, state in _STATE.items() if state[1].get("turn_id") == turn and state[0] in _ACTIVE]
                session = matches[0] if len(matches) == 1 else None
            if not isinstance(session, str) or not session:
                return None
            data = {
                "session_id": session[:160],
                "turn_id": turn,
                "cwd": os.getcwd(), "tool_name": values.get("tool_name"),
                "pid": os.getpid(), "at_ms": time.time_ns() // 1_000_000,
                "title": values.get("session_title") or values.get("title"),
            }
            if len(_STATE) >= 64 and session not in _STATE:
                oldest = min(_STATE, key=lambda key: (_STATE[key][0] in _ACTIVE, _STATE[key][1]["at_ms"]))
                _STATE.pop(oldest)
                _STARTS.pop(oldest, None)
                _RETIRED.pop(oldest, None)
                for alias in [key for key, value in _ALIASES.items() if value == oldest]:
                    _ALIASES.pop(alias, None)
            previous = _STARTS.get(session)
            if event == "UserPromptSubmit" and not data["turn_id"]:
                data["turn_id"] = "hermes-" + str(data["at_ms"]) + "-" + secrets.token_hex(4)
            if previous and not data["turn_id"]:
                data["turn_id"] = previous[0]
            previous_event, previous_data = _STATE.get(session, (None, {}))
            if data["turn_id"] and data["turn_id"] in _RETIRED.get(session, ()):
                return None
            changed_turn = previous and data["turn_id"] != previous[0]
            if changed_turn and event != "UserPromptSubmit":
                return None
            if previous_event in _TERMINAL and event != "SessionEnd" and not (event == "UserPromptSubmit" and changed_turn):
                return None
            if not data["title"]:
                data["title"] = previous_data.get("title")
            if event == "SessionStart" and previous:
                return None
            if event == "UserPromptSubmit" and (not previous or changed_turn):
                if previous:
                    _RETIRED[session] = (_RETIRED.get(session, []) + [previous[0]])[-8:]
                _STARTS[session] = (data["turn_id"], data["at_ms"])
            if isinstance(routing, str) and routing:
                if len(_ALIASES) >= 128 and routing not in _ALIASES:
                    _ALIASES.pop(next(iter(_ALIASES)))
                _ALIASES[routing] = session
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
    event = "StopFailure" if values.get("failed") else "Interrupt" if values.get("interrupted") or values.get("completed") is not True else "Stop"
    return _observe(event, values)


def _post_tool(**values):
    return _observe("PostToolUseFailure" if values.get("error") or values.get("is_error") or values.get("status") in {"error", "failed", "timeout", "cancelled"} else "PostToolUse", values)


def _human_request(**values):
    return _observe("PermissionRequest" if values.get("kind") == "approval" else "Waiting", values)


def _worker(stop):
    next_heartbeat = time.monotonic() + 15
    while not stop.is_set():
        try:
            event, data = _QUEUE.get(timeout=min(1.0, max(0.1, next_heartbeat - time.monotonic())))
            if stop.is_set():
                return
            emit(_ROOT, "hermes", event, data)
        except queue.Empty:
            pass
        except BaseException:
            pass
        if time.monotonic() >= next_heartbeat:
            with _LOCK:
                active = [(event, dict(data)) for event, data in _STATE.values() if event in _ACTIVE]
            for event, data in active:
                if stop.is_set():
                    return
                data.update(at_ms=time.time_ns() // 1_000_000, heartbeat=True)
                emit(_ROOT, "hermes", event, data)
            next_heartbeat = time.monotonic() + 15


def _stop_observer(stop, worker):
    global _STARTED, _STOP, _WORKER
    stop.set()
    with _LOCK:
        _STATE.clear()
        _STARTS.clear()
        _ALIASES.clear()
        _RETIRED.clear()
        while True:
            try:
                _QUEUE.get_nowait()
            except queue.Empty:
                break
    if worker is not threading.current_thread():
        worker.join(timeout=2.0)
    if worker.is_alive():
        # The loop polls at most once per second; an alive observer after the
        # join window must not be hidden by a later registration.
        return
    _STARTED = False
    if _STOP is stop:
        _STOP = None
        _WORKER = None


def register(ctx):
    global _STARTED, _STOP, _WORKER
    mapping = {
        "on_session_start": "SessionStart", "pre_llm_call": "UserPromptSubmit",
        "pre_tool_call": "PreToolUse", "on_session_finalize": "SessionEnd",
        "on_session_reset": "SessionEnd", "agent_loop_stopped": "Interrupt",
        "pre_approval_request": "PermissionRequest", "post_approval_response": "Resume",
        "on_human_input_resolved": "Resume",
    }
    for hook, event in mapping.items():
        try:
            ctx.register_hook(hook, lambda _event=event, **values: _observe(_event, values))
        except Exception:
            pass  # Older Hermes releases may not provide every observer hook.
    for hook, callback in (("post_tool_call", _post_tool), ("on_session_end", _end), ("on_human_input_request", _human_request)):
        try:
            ctx.register_hook(hook, callback)
        except Exception:
            pass
    if not _STARTED or (_WORKER is not None and not _WORKER.is_alive()):
        try:
            _STOP = threading.Event()
            _WORKER = threading.Thread(target=_worker, args=(_STOP,), name="qingyu-status", daemon=True)
            _WORKER.start()
            _STARTED = True
        except Exception:
            pass
    register_unload = getattr(ctx, "on_unload", None)
    if callable(register_unload) and _STOP is not None and _WORKER is not None:
        try:
            register_unload(lambda stop=_STOP, worker=_WORKER: _stop_observer(stop, worker))
        except Exception:
            pass
