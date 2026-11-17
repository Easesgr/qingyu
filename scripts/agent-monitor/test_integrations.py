#!/usr/bin/env python3
"""Isolated transport/plugin/config tests. No real home directories are modified."""
import importlib.util
import ast
import json
import os
from pathlib import Path
import queue
import shutil
import shlex
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import unittest

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent


def load(path, name, package=False):
    spec = importlib.util.spec_from_file_location(name, path, submodule_search_locations=[str(path.parent)] if package else None)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


class SinkTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="qingyu-sink-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "status"
        self.root.mkdir(mode=0o700)
        (self.root / "events").mkdir(mode=0o700)
        marker = self.root / "enabled"
        marker.write_text("test")
        marker.chmod(0o600)
        self.sink = load(HERE / "sink.py", "qingyu_sink_test")

    def files(self):
        return list((self.root / "events").glob("*.json"))

    def test_transport_selects_metadata_and_never_saves_content(self):
        self.sink.emit(self.root, "codex", "PreToolUse", {
            "session_id": "session", "turn_id": "turn", "cwd": "/project", "pid": os.getpid(),
            "tool_name": "exec_command", "prompt": "SECRET PROMPT", "tool_input": "SECRET COMMAND",
            "response": "SECRET RESPONSE",
        })
        value = json.loads(self.files()[0].read_text())
        self.assertEqual(value["tool"], "exec_command")
        self.assertNotIn("SECRET", json.dumps(value))
        self.assertEqual(self.files()[0].stat().st_mode & 0o777, 0o600)

    def test_disabled_stale_and_symlink_markers_prevent_events(self):
        marker = self.root / "enabled"
        os.utime(marker, (time.time() - 60, time.time() - 60))
        self.sink.emit(self.root, "codex", "Stop", {})
        self.assertEqual(self.files(), [])
        marker.unlink()
        target = Path(self.temporary.name) / "other"
        target.write_text("other")
        target.chmod(0o600)
        marker.symlink_to(target)
        self.sink.emit(self.root, "codex", "Stop", {})
        self.assertEqual(self.files(), [])
        marker.unlink()
        self.sink.emit(self.root, "codex", "Stop", {})
        self.assertEqual(self.files(), [])

    def test_spool_remains_bounded_when_monitor_is_not_draining(self):
        for _ in range(300):
            self.sink.emit(self.root, "codex", "Stop", {"session_id": "session"})
        self.assertEqual(len(self.files()), self.sink.MAX_EVENTS)

    def test_hook_exits_successfully_for_malformed_or_oversized_stdin(self):
        command = [sys.executable, str(HERE / "sink.py"), str(self.root), "claude", "UserPromptSubmit"]
        for payload in (b"{invalid", b"x" * (self.sink.MAX_INPUT + 1)):
            result = subprocess.run(command, input=payload, capture_output=True, timeout=3)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b"")
            self.assertEqual(result.stderr, b"")
        self.assertEqual(self.files(), [])

    def test_notification_waiting_is_limited_to_permission_notifications(self):
        command = [sys.executable, str(HERE / "sink.py"), str(self.root), "claude", "Notification"]
        for kind in ("idle_prompt", "permission_prompt"):
            result = subprocess.run(command, input=json.dumps({"session_id": "session", "notification_type": kind, "pid": 10}).encode(), capture_output=True, timeout=3)
            self.assertEqual(result.returncode, 0)
        self.assertEqual(len(self.files()), 1)
        self.assertEqual(json.loads(self.files()[0].read_text())["event"], "PermissionRequest")

    def test_hook_resolves_a_short_lived_shell_to_its_live_parent(self):
        command = " ".join(shlex.quote(value) for value in [sys.executable, str(HERE / "sink.py"), str(self.root), "claude", "UserPromptSubmit"]) + " || true"
        result = subprocess.run(["/bin/sh", "-c", command], input=b'{"session_id":"shell-session"}', capture_output=True, timeout=3)
        self.assertEqual(result.returncode, 0)
        value = json.loads(self.files()[0].read_text())
        self.assertEqual(value["pid"], os.getpid())

    def test_claude_hook_subprocesses_share_turn_identity_and_timing_without_content(self):
        records = []
        for event in ("UserPromptSubmit", "PreToolUse", "PermissionRequest", "PostToolUse", "Stop", "UserPromptSubmit"):
            before = set(self.files())
            payload = {"session_id": "claude-session", "pid": os.getpid(), "cwd": "/project", "prompt": "PRIVATE PROMPT", "tool_input": {"command": "PRIVATE"}, "transcript_path": "/metadata/claude-session.jsonl"}
            result = subprocess.run([sys.executable, str(HERE / "sink.py"), str(self.root), "claude", event], input=json.dumps(payload).encode(), capture_output=True, timeout=3)
            self.assertEqual(result.returncode, 0)
            record = json.loads(next(iter(set(self.files()) - before)).read_text())
            records.append(record)
        first = records[0]
        self.assertTrue(first["turn_id"].startswith("hook-"))
        self.assertGreater(first["started_at_ms"], 0)
        self.assertTrue(all(row["turn_id"] == first["turn_id"] and row["started_at_ms"] == first["started_at_ms"] for row in records[:5]))
        self.assertNotEqual(records[5]["turn_id"], first["turn_id"])
        state = list((self.root / "hook-state").glob("*.json"))
        self.assertEqual(len(state), 1)
        self.assertNotIn("PRIVATE", state[0].read_text())
        self.assertNotIn("PRIVATE", json.dumps(records))
        self.assertEqual(first["transcript"], "/metadata/claude-session.jsonl")

    def test_claude_question_tool_emits_wait_without_recording_questions(self):
        payload = {"session_id": "session", "pid": os.getpid(), "tool_name": "AskUserQuestion", "tool_input": {"questions": "PRIVATE QUESTION"}}
        result = subprocess.run([sys.executable, str(HERE / "sink.py"), str(self.root), "claude", "PreToolUse"], input=json.dumps(payload).encode(), capture_output=True, timeout=3)
        self.assertEqual(result.returncode, 0)
        record = json.loads(self.files()[0].read_text())
        self.assertEqual((record["event"], record["tool"]), ("Waiting", "AskUserQuestion"))
        self.assertNotIn("PRIVATE", json.dumps(record))

    def test_hook_turn_state_does_not_mutate_when_monitoring_is_disabled(self):
        (self.root / "enabled").unlink()
        data = {"session_id": "session", "pid": os.getpid()}
        self.sink._hook_turn(self.root, "claude", "UserPromptSubmit", data)
        self.assertFalse((self.root / "hook-state").exists())

    def test_hook_turn_state_preserves_foreign_and_symlink_files(self):
        import hashlib
        state = self.root / "hook-state"
        state.mkdir(mode=0o700)
        original = self.root / "foreign"
        original.write_text("unrelated")
        original.chmod(0o600)
        digest = hashlib.sha256(b"claude:session").hexdigest()
        (state / (digest + ".json")).symlink_to(original)
        self.sink._hook_turn(self.root, "claude", "UserPromptSubmit", {"session_id": "session", "pid": os.getpid()})
        self.assertEqual(original.read_text(), "unrelated")


class PluginTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="qingyu-plugin-test-")
        self.addCleanup(self.temporary.cleanup)
        directory = Path(self.temporary.name)
        shutil.copyfile(HERE / "sink.py", directory / "sink.py")
        source = (HERE / "hermes_plugin.py").read_text().replace("__QINGYU_MONITOR_ROOT__", json.dumps(str(directory / "data")))
        (directory / "__init__.py").write_text(source)
        self.plugin = load(directory / "__init__.py", "qingyu_test_plugin", package=True)
        self.plugin._STARTED = True  # Test callbacks deterministically without a daemon.

        class Context:
            def __init__(self):
                self.hooks = {}
                self.unload_callbacks = []

            def register_hook(self, name, callback):
                self.hooks[name] = callback

            def on_unload(self, callback):
                self.unload_callbacks.append(callback)

        self.ctx = Context()
        self.plugin.register(self.ctx)

    def test_official_unload_stops_the_observer_worker_and_discards_old_generation_state(self):
        import threading
        self.plugin._STARTED = False
        self.plugin._STOP = None
        self.plugin._WORKER = None
        emitted = threading.Event()
        self.plugin.emit = lambda *args: emitted.set()
        self.plugin.register(self.ctx)
        worker = self.plugin._WORKER
        self.assertTrue(worker.is_alive())
        self.plugin._observe("UserPromptSubmit", {"session_id": "stale", "turn_id": "old"})
        self.assertTrue(emitted.wait(1))
        self.assertEqual(len(self.plugin._STATE), 1)
        self.plugin.register(self.ctx)
        self.assertIs(self.plugin._WORKER, worker)
        self.assertEqual(len(self.ctx.unload_callbacks), 2)
        self.ctx.unload_callbacks[0]()
        self.assertFalse(worker.is_alive())
        self.assertFalse(self.plugin._STARTED)
        self.assertEqual(self.plugin._STATE, {})
        self.assertEqual(self.plugin._STARTS, {})
        self.assertEqual(self.plugin._ALIASES, {})
        self.assertEqual(self.plugin._RETIRED, {})
        self.assertTrue(self.plugin._QUEUE.empty())
        self.assertIsNone(self.plugin._observe("PreToolUse", {"session_id": "stale", "turn_id": "old"}))
        self.assertTrue(self.plugin._QUEUE.empty())

    def test_hermes_lifecycle_maps_turn_end_separately_from_session_finalize(self):
        mapping = [
            ("on_session_start", {}, "SessionStart"),
            ("pre_llm_call", {}, "UserPromptSubmit"),
            ("pre_tool_call", {"tool_name": "terminal"}, "PreToolUse"),
            ("post_tool_call", {}, "PostToolUse"),
            ("post_tool_call", {"is_error": True}, "PostToolUseFailure"),
            ("pre_approval_request", {}, "PermissionRequest"),
            ("post_approval_response", {}, "Resume"),
            ("on_session_end", {"completed": True}, "Stop"),
            ("on_session_end", {"failed": True}, "StopFailure"),
            ("on_session_end", {"interrupted": True}, "Interrupt"),
            ("on_session_finalize", {}, "SessionEnd"),
        ]
        for hook, values, expected in mapping:
            result = self.ctx.hooks[hook](session_id=hook + expected, turn_id="turn", user_message="SECRET", args={"command": "SECRET"}, **values)
            self.assertIsNone(result, hook)
            event, data = self.plugin._QUEUE.get_nowait()
            self.assertEqual(event, expected)
            self.assertNotIn("SECRET", json.dumps(data))
            self.assertEqual(data["pid"], os.getpid())

    def test_callback_queue_and_session_state_are_bounded(self):
        for i in range(300):
            self.ctx.hooks["pre_llm_call"](session_id=str(i), turn_id=str(i))
        self.assertLessEqual(self.plugin._QUEUE.qsize(), 128)

        self.assertLessEqual(len(self.plugin._STATE), 64)

    def test_tools_and_resumes_retain_the_original_turn_start_and_explicit_title(self):
        self.ctx.hooks["pre_llm_call"](session_id="session", turn_id="turn", session_title="Real session name", user_message="PRIVATE PROMPT")
        _, first = self.plugin._QUEUE.get_nowait()
        self.ctx.hooks["pre_tool_call"](session_id="session", tool_name="terminal", args={"command": "PRIVATE"})
        _, tool = self.plugin._QUEUE.get_nowait()
        self.assertEqual(tool["started_at_ms"], first["at_ms"])
        self.assertEqual(tool["turn_id"], "turn")
        self.assertEqual(tool["title"], "Real session name")
        self.ctx.hooks["post_approval_response"](session_id="session")
        event, resumed = self.plugin._QUEUE.get_nowait()
        self.assertEqual(event, "Resume")
        self.assertEqual(resumed["started_at_ms"], first["at_ms"])
        self.assertNotIn("PRIVATE", json.dumps(resumed))
        self.ctx.hooks["on_session_end"](session_id="session", turn_id="turn", completed=True)
        self.plugin._QUEUE.get_nowait()
        self.ctx.hooks["pre_llm_call"](session_id="session", turn_id="next")
        _, next_turn = self.plugin._QUEUE.get_nowait()
        self.assertEqual(next_turn["started_at_ms"], next_turn["at_ms"])
        self.assertEqual(next_turn["turn_id"], "next")

    def test_approval_routing_never_creates_a_fake_session_or_crosses_concurrent_sessions(self):
        self.ctx.hooks["pre_llm_call"](session_id="one", turn_id="turn-one", session_key="route-one")
        self.plugin._QUEUE.get_nowait()
        self.ctx.hooks["pre_llm_call"](session_id="two", turn_id="turn-two", session_key="route-two")
        self.plugin._QUEUE.get_nowait()
        self.ctx.hooks["pre_approval_request"](session_key="route-two")
        event, data = self.plugin._QUEUE.get_nowait()
        self.assertEqual((event, data["session_id"], data["turn_id"]), ("PermissionRequest", "two", "turn-two"))
        self.ctx.hooks["pre_approval_request"](turn_id="turn-one")
        _, data = self.plugin._QUEUE.get_nowait()
        self.assertEqual(data["session_id"], "one")
        self.ctx.hooks["pre_approval_request"](session_key="unknown-route")
        self.assertTrue(self.plugin._QUEUE.empty())
        self.assertEqual(set(self.plugin._STATE), {"one", "two"})

    def test_hermes_unknown_end_is_not_success_and_end_then_new_turn_has_a_new_identity(self):
        self.ctx.hooks["pre_llm_call"](session_id="session")
        _, start = self.plugin._QUEUE.get_nowait()
        self.ctx.hooks["on_session_end"](session_id="session", completed=False, failed=False, interrupted=False)
        event, data = self.plugin._QUEUE.get_nowait()
        self.assertEqual(event, "Interrupt")
        self.assertEqual(data["turn_id"], start["turn_id"])
        time.sleep(0.002)
        self.ctx.hooks["pre_llm_call"](session_id="session")
        _, next_turn = self.plugin._QUEUE.get_nowait()
        self.assertNotEqual(next_turn["turn_id"], start["turn_id"])
        self.ctx.hooks["on_session_end"](session_id="session", completed=True)
        event, _ = self.plugin._QUEUE.get_nowait()
        self.assertEqual(event, "Stop")

    def test_plugin_is_an_observer_under_missing_identity_and_full_queues(self):
        self.assertIsNone(self.ctx.hooks["pre_tool_call"](tool_name="terminal", args={"command": "PRIVATE"}))
        self.assertTrue(self.plugin._QUEUE.empty())
        for i in range(200):
            self.assertIsNone(self.ctx.hooks["pre_tool_call"](session_id="session", turn_id="turn", tool_name="terminal"))
        self.assertLessEqual(self.plugin._QUEUE.qsize(), 128)

    def test_unscoped_requests_never_borrow_the_only_active_session(self):
        self.ctx.hooks["pre_llm_call"](session_id="one", turn_id="turn-one")
        self.plugin._QUEUE.get_nowait()
        for hook in ("pre_approval_request", "post_approval_response", "on_human_input_request", "agent_loop_stopped"):
            self.assertIsNone(self.ctx.hooks[hook](session_key="unknown", prompt="PRIVATE"))
        self.assertTrue(self.plugin._QUEUE.empty())
        self.assertEqual(set(self.plugin._STATE), {"one"})

    def test_human_waits_and_tool_failure_follow_current_hermes_metadata(self):
        self.ctx.hooks["pre_llm_call"](session_id="one", turn_id="turn")
        _, first = self.plugin._QUEUE.get_nowait()
        for hook, values, expected in [
            ("on_human_input_request", {"kind": "clarify", "prompt": "PRIVATE QUESTION"}, "Waiting"),
            ("on_human_input_resolved", {"kind": "clarify", "outcome": "answered"}, "Resume"),
            ("on_human_input_request", {"kind": "approval", "prompt": "PRIVATE COMMAND"}, "PermissionRequest"),
            ("pre_approval_request", {}, "PermissionRequest"),
            ("post_approval_response", {"choice": "once"}, "Resume"),
            ("post_tool_call", {"status": "error", "result": "PRIVATE RESULT"}, "PostToolUseFailure"),
            ("pre_tool_call", {}, "PreToolUse"),
        ]:
            self.ctx.hooks[hook](session_id="one", **values)
            event, data = self.plugin._QUEUE.get_nowait()
            self.assertEqual(event, expected)
            self.assertEqual(data["turn_id"], "turn")
            self.assertEqual(data["started_at_ms"], first["at_ms"])
            self.assertNotIn("PRIVATE", json.dumps(data))

    def test_terminal_and_retired_turns_cannot_restart_heartbeat_or_finish_twice(self):
        for terminal, extra in [("on_session_end", {"completed": True}), ("on_session_end", {"failed": True}), ("agent_loop_stopped", {})]:
            with self.subTest(terminal=extra):
                self.plugin._STATE.clear()
                self.plugin._STARTS.clear()
                self.plugin._RETIRED.clear()
                self.ctx.hooks["pre_llm_call"](session_id="session", turn_id="old")
                self.plugin._QUEUE.get_nowait()
                self.ctx.hooks[terminal](session_id="session", turn_id="old", **extra)
                self.plugin._QUEUE.get_nowait()
                for hook in ("pre_tool_call", "post_approval_response", "pre_llm_call"):
                    self.ctx.hooks[hook](session_id="session", turn_id="old")
                self.ctx.hooks["on_session_end"](session_id="session", turn_id="old", completed=True)
                self.assertTrue(self.plugin._QUEUE.empty())
                self.assertNotIn(self.plugin._STATE["session"][0], self.plugin._ACTIVE)
                self.ctx.hooks["pre_llm_call"](session_id="session", turn_id="next")
                self.plugin._QUEUE.get_nowait()
                self.ctx.hooks["pre_llm_call"](session_id="session", turn_id="old")
                self.ctx.hooks["on_session_end"](session_id="session", turn_id="old", completed=True)
                self.assertTrue(self.plugin._QUEUE.empty())
                self.assertEqual(self.plugin._STATE["session"][1]["turn_id"], "next")

    def test_concurrent_callbacks_keep_each_session_turn_and_queue_order(self):
        barrier = threading.Barrier(8)

        def run(index):
            barrier.wait(timeout=3)
            for hook, extra in [("pre_llm_call", {}), ("pre_tool_call", {}), ("pre_approval_request", {}), ("post_approval_response", {}), ("on_session_end", {"completed": True})]:
                self.ctx.hooks[hook](session_id=str(index), turn_id="turn-" + str(index), **extra)

        threads = [threading.Thread(target=run, args=(i,)) for i in range(8)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(timeout=5)
            self.assertFalse(thread.is_alive())
        records = []
        while not self.plugin._QUEUE.empty():
            records.append(self.plugin._QUEUE.get_nowait())
        self.assertEqual(len(records), 40)
        for i in range(8):
            own = [(event, data) for event, data in records if data["session_id"] == str(i)]
            self.assertEqual([event for event, _ in own], ["UserPromptSubmit", "PreToolUse", "PermissionRequest", "Resume", "Stop"])
            self.assertTrue(all(data["turn_id"] == "turn-" + str(i) and data["started_at_ms"] == own[0][1]["at_ms"] for _, data in own))


class InstalledHermesContractTests(unittest.TestCase):
    def test_installed_hermes_register_hook_and_observability_fields_match_the_adapter(self):
        home = Path(os.environ.get("HERMES_HOME", str(Path.home() / ".hermes")))
        source = home / "hermes-agent"
        if not (source / "hermes_cli/plugins.py").is_file():
            self.skipTest("No installed Hermes source to inspect; adapter fixtures still run")
        tree = ast.parse((source / "hermes_cli/plugins.py").read_text())
        declared = next(node for node in ast.walk(tree) if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and node.target.id == "VALID_HOOKS")
        hooks = ast.literal_eval(declared.value)
        self.assertTrue({"pre_llm_call", "pre_tool_call", "post_tool_call", "on_session_end", "on_session_finalize", "pre_approval_request", "post_approval_response"}.issubset(hooks))
        registration = next(node for node in ast.walk(tree) if isinstance(node, ast.FunctionDef) and node.name == "register_hook")
        self.assertEqual([value.arg for value in registration.args.args[:3]], ["self", "hook_name", "callback"])

        def call_fields(filename, event):
            tree = ast.parse((source / filename).read_text())
            return next({value.arg for value in node.keywords} for node in ast.walk(tree) if isinstance(node, ast.Call) and node.args and isinstance(node.args[0], ast.Constant) and node.args[0].value == event)

        self.assertTrue({"session_id", "task_id", "turn_id"}.issubset(call_fields("agent/turn_context.py", "pre_llm_call")))
        self.assertTrue({"session_id", "turn_id", "completed", "failed", "interrupted"}.issubset(call_fields("agent/turn_finalizer.py", "on_session_end")))
        self.assertTrue({"session_id", "turn_id", "tool_name"}.issubset(call_fields("hermes_cli/plugins.py", "pre_tool_call")))
        self.assertTrue(any(isinstance(node, ast.FunctionDef) and node.name == "on_unload" for node in ast.walk(tree)))
        plugin = ast.parse((HERE / "hermes_plugin.py").read_text())
        self.assertTrue(any(isinstance(node, ast.Call) and isinstance(node.func, ast.Name) and node.func.id == "getattr"
                            and any(isinstance(arg, ast.Constant) and arg.value == "on_unload" for arg in node.args)
                            for node in ast.walk(plugin)))


class TitleDatabaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="qingyu-title-db-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def read(self, requests):
        result = subprocess.run([sys.executable, str(HERE / "read_titles.py"), json.dumps(requests)], capture_output=True, timeout=3)
        self.assertEqual(result.returncode, 0)
        return json.loads(result.stdout)

    def test_only_requested_title_metadata_is_read_and_database_is_unchanged(self):
        path = self.root / "state_5.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT, instructions TEXT)")
        connection.execute("CREATE TABLE messages (content TEXT)")
        connection.executemany("INSERT INTO threads VALUES (?,?,?)", [("session", "Real title", "PRIVATE INSTRUCTIONS"), ("other", "Other title", "PRIVATE")])
        connection.execute("INSERT INTO messages VALUES ('PRIVATE MESSAGE')")
        connection.commit()
        connection.close()
        before = path.read_bytes()
        self.assertEqual(self.read([["codex", str(path), ["session"]]]), [{"agent": "codex", "id": "session", "title": "Real title"}])
        self.assertEqual(path.read_bytes(), before)

    def test_current_wal_renames_refresh_without_transcript_changes(self):
        path = self.root / "state.db"
        connection = sqlite3.connect(path)
        self.addCleanup(connection.close)
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT)")
        connection.execute("INSERT INTO sessions VALUES ('session','First title')")
        connection.commit()
        request = [["hermes", str(path), ["session"]]]
        self.assertEqual(self.read(request)[0]["title"], "First title")
        connection.execute("UPDATE sessions SET title='Renamed title' WHERE id='session'")
        connection.commit()
        before = path.read_bytes()
        wal_before = Path(str(path) + "-wal").read_bytes()
        self.assertEqual(self.read(request)[0]["title"], "Renamed title")
        self.assertEqual(path.read_bytes(), before)
        self.assertEqual(Path(str(path) + "-wal").read_bytes(), wal_before)

    def test_hermes_end_state_is_read_without_session_content_and_database_stays_unchanged(self):
        path = self.root / "state.db"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT, ended_at REAL, end_reason TEXT, system_prompt TEXT)")
        connection.executemany("INSERT INTO sessions VALUES (?,?,?,?,?)", [
            ("ended-cron", "Finished cron", 1794780460.0, "cron_complete", "PRIVATE SYSTEM PROMPT"),
            ("live-session", "Live session", None, None, "PRIVATE SYSTEM PROMPT"),
        ])
        connection.commit()
        connection.close()
        before = path.read_bytes()
        rows = self.read([["hermes", str(path), ["ended-cron", "live-session"]]])
        self.assertEqual({row["id"]: row["ended"] for row in rows}, {"ended-cron": True, "live-session": False})
        self.assertEqual({row["id"]: row["title"] for row in rows}, {"ended-cron": "Finished cron", "live-session": "Live session"})
        self.assertNotIn("PRIVATE", json.dumps(rows))
        self.assertEqual(path.read_bytes(), before)

    def test_hermes_end_state_is_read_without_session_content_and_database_stays_unchanged(self):
        path = self.root / "state.db"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT, ended_at REAL, end_reason TEXT, system_prompt TEXT)")
        connection.executemany("INSERT INTO sessions VALUES (?,?,?,?,?)", [
            ("ended-cron", "Finished cron", 1794780460.0, "cron_complete", "PRIVATE SYSTEM PROMPT"),
            ("live-session", "Live session", None, None, "PRIVATE SYSTEM PROMPT"),
        ])
        connection.commit()
        connection.close()
        before = path.read_bytes()
        rows = self.read([["hermes", str(path), ["ended-cron", "live-session"]]])
        self.assertEqual({row["id"]: row["ended"] for row in rows}, {"ended-cron": True, "live-session": False})
        self.assertEqual({row["id"]: row["title"] for row in rows}, {"ended-cron": "Finished cron", "live-session": "Live session"})
        self.assertNotIn("PRIVATE", json.dumps(rows))
        self.assertEqual(path.read_bytes(), before)

    def test_unexpected_views_cannot_read_message_content_and_symlinks_are_ignored(self):
        path = self.root / "unexpected.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE messages (id TEXT, content TEXT)")
        connection.execute("INSERT INTO messages VALUES ('session','PRIVATE MESSAGE')")
        connection.execute("CREATE VIEW threads AS SELECT id,content AS title FROM messages")
        connection.commit()
        connection.close()
        self.assertEqual(self.read([["codex", str(path), ["session"]]]), [])
        link = self.root / "link.sqlite"
        link.symlink_to(path)
        self.assertEqual(self.read([["codex", str(link), ["session"]]]), [])

    def test_manual_name_archive_and_rollout_metadata_are_read_without_other_columns(self):
        path = self.root / "state_5.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY, name TEXT, title TEXT, archived INTEGER, rollout_path TEXT, first_user_message TEXT)")
        connection.execute("INSERT INTO threads VALUES (?,?,?,?,?,?)", ("session", "Manual name", "Old automatic title", 0, "/metadata/rollout-session.jsonl", "PRIVATE PROMPT"))
        connection.commit()
        connection.close()
        before = path.read_bytes()
        result = self.read([["codex", str(path), ["session"]]])
        self.assertEqual(result, [{"agent": "codex", "id": "session", "title": "Old automatic title", "has_name": True, "name": "Manual name", "archived": False, "rollout_path": "/metadata/rollout-session.jsonl"}])
        self.assertNotIn("PRIVATE", json.dumps(result))
        self.assertEqual(path.read_bytes(), before)

    def test_manual_rename_clear_and_archive_are_visible_in_wal(self):
        path = self.root / "state_5.sqlite"
        connection = sqlite3.connect(path)
        self.addCleanup(connection.close)
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY, name TEXT, title TEXT, archived INTEGER)")
        connection.execute("INSERT INTO threads VALUES ('session','Manual one','Automatic',0)")
        connection.commit()
        requests = [["codex", str(path), ["session"]]]
        self.assertEqual(self.read(requests)[0]["name"], "Manual one")
        connection.execute("UPDATE threads SET name='Manual two' WHERE id='session'")
        connection.commit()
        self.assertEqual(self.read(requests)[0]["name"], "Manual two")
        connection.execute("UPDATE threads SET name=NULL,archived=1 WHERE id='session'")
        connection.commit()
        before = path.read_bytes()
        wal_before = Path(str(path) + "-wal").read_bytes()
        result = self.read(requests)[0]
        self.assertIsNone(result["name"])
        self.assertEqual(result["title"], "Automatic")
        self.assertTrue(result["archived"])
        self.assertEqual(path.read_bytes(), before)
        self.assertEqual(Path(str(path) + "-wal").read_bytes(), wal_before)

    def test_old_schema_without_name_remains_compatible(self):
        path = self.root / "old.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY,title TEXT,archived INTEGER)")
        connection.execute("INSERT INTO threads VALUES ('session','Legacy title',0)")
        connection.commit()
        connection.close()
        result = self.read([["codex", str(path), ["session"]]])[0]
        self.assertEqual(result["title"], "Legacy title")
        self.assertNotIn("has_name", result)
        self.assertFalse(result["archived"])

    def test_modern_view_cannot_smuggle_message_content_through_name(self):
        path = self.root / "view.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE messages (id TEXT, content TEXT)")
        connection.execute("INSERT INTO messages VALUES ('session','PRIVATE CONTENT')")
        connection.execute("CREATE VIEW threads AS SELECT id,'automatic' AS title,content AS name,0 AS archived FROM messages")
        connection.commit()
        connection.close()
        self.assertEqual(self.read([["codex", str(path), ["session"]]]), [])

    def test_response_bytes_stay_bounded_for_many_long_names_and_paths(self):
        path = self.root / "bounded.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY,name TEXT,title TEXT,rollout_path TEXT)")
        ids = [str(i) for i in range(64)]
        connection.executemany("INSERT INTO threads VALUES (?,?,?,?)", [(value, "名" * 5000, "题" * 5000, "/" + "路" * 9000) for value in ids])
        connection.commit()
        connection.close()
        result = self.read([["codex", str(path), ids]])
        self.assertEqual(len(result), 64)
        self.assertLess(len(json.dumps(result, ensure_ascii=False).encode()), 128 * 1024)
        self.assertTrue(all(row["has_name"] and len(row["name"]) == 160 for row in result))

    def test_internal_thread_returns_parent_context_without_emitting_parent_as_a_session(self):
        child = "11111111-1111-4111-8111-111111111111"
        parent = "22222222-2222-4222-8222-222222222222"
        path = self.root / "parents.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY,name TEXT,title TEXT,source TEXT,first_user_message TEXT)")
        source = json.dumps({"subagent": {"thread_spawn": {"parent_thread_id": parent, "depth": 1}}})
        connection.executemany("INSERT INTO threads VALUES (?,?,?,?,?)", [(child, None, "", source, "PRIVATE CHILD PROMPT"), (parent, "Real parent conversation", "Old automatic title", "cli", "PRIVATE PARENT PROMPT")])
        connection.commit()
        connection.close()
        result = self.read([["codex", str(path), [child]]])
        self.assertEqual(len(result), 1)
        self.assertEqual(result[0]["id"], child)
        self.assertEqual(result[0]["parent_id"], parent)
        self.assertEqual(result[0]["parent_title"], "Real parent conversation")
        self.assertTrue(result[0]["is_subagent"])
        self.assertNotIn("PRIVATE", json.dumps(result))
        self.assertNotIn("source", result[0])

    def test_parent_name_chain_supports_attached_parent_and_live_renames(self):
        child = "11111111-1111-4111-8111-111111111111"
        intermediate = "22222222-2222-4222-8222-222222222222"
        parent = "33333333-3333-4333-8333-333333333333"
        path = self.root / "parents-wal.sqlite"
        connection = sqlite3.connect(path)
        self.addCleanup(connection.close)
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY,name TEXT,title TEXT,thread_source TEXT,attached_thread_parent_id TEXT)")
        connection.executemany("INSERT INTO threads VALUES (?,?,?,?,?)", [(child, None, "", "", intermediate), (intermediate, None, "", json.dumps({"parent_thread_id": parent}), None), (parent, "Parent name one", "Old title", "cli", None)])
        connection.commit()
        request = [["codex", str(path), [child]]]
        self.assertEqual(self.read(request)[0]["parent_title"], "Parent name one")
        connection.execute("UPDATE threads SET name='Parent name two' WHERE id=?", [parent])
        connection.commit()
        before = path.read_bytes()
        wal_before = Path(str(path) + "-wal").read_bytes()
        self.assertEqual(self.read(request)[0]["parent_title"], "Parent name two")
        self.assertEqual(path.read_bytes(), before)
        self.assertEqual(Path(str(path) + "-wal").read_bytes(), wal_before)

    def test_parent_resolution_rejects_cycles_malformed_ids_and_untrusted_nested_fields(self):
        child = "11111111-1111-4111-8111-111111111111"
        parent = "22222222-2222-4222-8222-222222222222"
        path = self.root / "cycle.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY,name TEXT,title TEXT,source TEXT)")
        connection.executemany("INSERT INTO threads VALUES (?,?,?,?)", [(child, None, "", json.dumps({"parent_thread_id": parent})), (parent, None, "", json.dumps({"parent_thread_id": child}))])
        connection.commit()
        self.assertNotIn("parent_title", self.read([["codex", str(path), [child]]])[0])
        for source in ('{"parent_thread_id":"not-an-id"}', '{"tool_input":{"parent_thread_id":"' + parent + '"}}', '{broken'):
            connection.execute("UPDATE threads SET source=? WHERE id=?", [source, child])
            connection.commit()
            result = self.read([["codex", str(path), [child]]])[0]
            self.assertNotIn("parent_id", result)
            self.assertNotIn("parent_title", result)
        connection.close()

    def test_parent_chain_depth_is_bounded_and_legacy_source_schema_still_resolves_names(self):
        ids = [f"00000000-0000-4000-8000-{i:012d}" for i in range(12)]
        path = self.root / "depth.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY,title TEXT,source TEXT)")
        connection.executemany("INSERT INTO threads VALUES (?,?,?)", [(value, "Deep parent" if i == 11 else "", json.dumps({"parent_thread_id": ids[i + 1]}) if i + 1 < len(ids) else "cli") for i, value in enumerate(ids)])
        connection.commit()
        self.assertNotIn("parent_title", self.read([["codex", str(path), [ids[0]]]])[0])
        connection.execute("UPDATE threads SET title='Near parent' WHERE id=?", [ids[2]])
        connection.commit()
        self.assertEqual(self.read([["codex", str(path), [ids[0]]]])[0]["parent_title"], "Near parent")
        connection.close()

    def test_child_columns_and_trusted_spawn_fields_supply_their_own_identity_only(self):
        first = "11111111-1111-4111-8111-111111111111"
        second = "22222222-2222-4222-8222-222222222222"
        parent = "33333333-3333-4333-8333-333333333333"
        path = self.root / "child-identities.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY,name TEXT,title TEXT,agent_nickname TEXT,agent_role TEXT,source TEXT,first_user_message TEXT)")
        spawn = lambda nickname, role: json.dumps({"subagent": {"thread_spawn": {"parent_thread_id": parent, "agent_nickname": nickname, "agent_role": role}}, "tool_input": {"agent_nickname": "PRIVATE INPUT"}})
        connection.executemany("INSERT INTO threads VALUES (?,?,?,?,?,?,?)", [
            (first, None, "", "Column nickname", "Column role", spawn("Older source nickname", "Older source role"), "PRIVATE PROMPT"),
            (second, "Own manual name", "Own automatic title", None, None, spawn("Source nickname", "Source role"), "PRIVATE RESPONSE"),
            (parent, "Parent name", "Parent title", None, None, "cli", "PRIVATE PARENT"),
        ])
        connection.commit()
        connection.close()
        before = path.read_bytes()
        result = {row["id"]: row for row in self.read([["codex", str(path), [first, second]]])}
        self.assertEqual(set(result), {first, second})
        self.assertTrue(all(row["is_subagent"] and row["parent_id"] == parent for row in result.values()))
        self.assertEqual(result[first]["agent_nickname"], "Column nickname")
        self.assertEqual(result[first]["agent_role"], "Column role")
        self.assertEqual(result[second]["agent_nickname"], "Source nickname")
        self.assertEqual(result[second]["agent_role"], "Source role")
        self.assertEqual(result[second]["name"], "Own manual name")
        self.assertNotIn("PRIVATE", json.dumps(result))
        self.assertNotIn("source", result[first])
        self.assertEqual(path.read_bytes(), before)

    def test_nested_and_cyclic_children_retain_their_own_nickname_metadata(self):
        first = "11111111-1111-4111-8111-111111111111"
        second = "22222222-2222-4222-8222-222222222222"
        path = self.root / "child-cycle.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY,title TEXT,thread_source TEXT)")
        connection.executemany("INSERT INTO threads VALUES (?,?,?)", [
            (first, "", json.dumps({"subagent": {"thread_spawn": {"parent_thread_id": second, "agent_nickname": "First child", "agent_role": "Worker"}}})),
            (second, "", json.dumps({"subagent": {"thread_spawn": {"parent_thread_id": first, "agent_nickname": "Second child", "agent_role": "Worker"}}})),
        ])
        connection.commit()
        connection.close()
        result = self.read([["codex", str(path), [first, second]]])
        self.assertEqual(len(result), 2)
        self.assertEqual({row["agent_nickname"] for row in result}, {"First child", "Second child"})
        self.assertTrue(all(row["is_subagent"] and "parent_title" not in row for row in result))

    def test_child_metadata_refreshes_with_nickname_role_and_parent_name_edits_in_wal(self):
        child = "11111111-1111-4111-8111-111111111111"
        parent = "22222222-2222-4222-8222-222222222222"
        path = self.root / "child-renames.sqlite"
        connection = sqlite3.connect(path)
        self.addCleanup(connection.close)
        connection.execute("PRAGMA journal_mode=WAL")
        connection.execute("CREATE TABLE threads (id TEXT PRIMARY KEY,name TEXT,title TEXT,agent_nickname TEXT,agent_role TEXT,attached_thread_parent_id TEXT)")
        connection.executemany("INSERT INTO threads VALUES (?,?,?,?,?,?)", [(child, None, "", "Nickname one", "Role one", parent), (parent, "Parent one", "", None, None, None)])
        connection.commit()
        first = self.read([["codex", str(path), [child]]])[0]
        self.assertEqual(first["agent_nickname"], "Nickname one")
        connection.execute("UPDATE threads SET agent_nickname='Nickname two',agent_role='Role two' WHERE id=?", [child])
        connection.execute("UPDATE threads SET name='Parent two' WHERE id=?", [parent])
        connection.commit()
        before = path.read_bytes()
        wal_before = Path(str(path) + "-wal").read_bytes()
        second = self.read([["codex", str(path), [child]]])[0]
        self.assertEqual(second["agent_nickname"], "Nickname two")
        self.assertEqual(second["agent_role"], "Role two")
        self.assertEqual(second["parent_title"], "Parent two")
        self.assertEqual(path.read_bytes(), before)
        self.assertEqual(Path(str(path) + "-wal").read_bytes(), wal_before)

    def test_nickname_view_cannot_read_message_content_or_untrusted_nested_arguments(self):
        child = "11111111-1111-4111-8111-111111111111"
        path = self.root / "child-view.sqlite"
        connection = sqlite3.connect(path)
        connection.execute("CREATE TABLE messages (id TEXT,content TEXT)")
        connection.execute("INSERT INTO messages VALUES (?,?)", [child, "PRIVATE MESSAGE"])
        connection.execute("CREATE VIEW threads AS SELECT id,'' AS title,content AS agent_nickname,'{}' AS source FROM messages")
        connection.commit()
        connection.close()
        self.assertEqual(self.read([["codex", str(path), [child]]]), [])
        helper = load(HERE / "read_titles.py", "qingyu_child_metadata_test")
        identity = helper.child_identity({"source": json.dumps({"tool_input": {"subagent": {"thread_spawn": {"agent_nickname": "PRIVATE", "agent_role": "PRIVATE"}}}})}, child)
        self.assertFalse(identity["is_subagent"])
        self.assertIsNone(identity["agent_nickname"])
        self.assertIsNone(identity["agent_role"])


@unittest.skipUnless(importlib.util.find_spec("yaml") or importlib.util.find_spec("hermes_yaml"), "Run with the installed Hermes Python to test YAML configuration")
class HermesConfigurationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="qingyu-yaml-test-")
        self.addCleanup(self.temporary.cleanup)
        self.path = Path(self.temporary.name) / "config.yaml"

    def run_helper(self, operation):
        command = [sys.executable]
        source = Path(os.environ.get("HERMES_HOME", str(Path.home() / ".hermes"))) / "hermes-agent"
        if "hermes_yaml" in sys.modules and (source / "hermes_bootstrap.py").is_file():
            command.extend(["-I", "-c", "import sys,runpy;sys.path.insert(0," + repr(str(source)) + ");import hermes_bootstrap;script=sys.argv.pop(1);runpy.run_path(script,run_name='__main__')"])
        return subprocess.run([*command, str(HERE / "configure_hermes.py"), str(self.path), operation], capture_output=True, timeout=3)

    def test_install_uninstall_are_idempotent_and_preserve_other_settings(self):
        try:
            import hermes_yaml as yaml
        except ImportError:
            import yaml
        original = {"model": {"provider": "existing"}, "approval": {"mode": "ask"}, "plugins": {"enabled": ["existing-plugin"], "disabled": ["another-plugin"], "entries": {"existing-plugin": {"allow_network": False}}}}
        self.path.write_text(yaml.safe_dump(original))
        self.assertEqual(self.run_helper("install").returncode, 0)
        once = self.path.read_bytes()
        self.assertEqual(self.run_helper("install").returncode, 0)
        self.assertEqual(self.path.read_bytes(), once)
        result = yaml.safe_load(once)
        self.assertEqual(result["plugins"]["enabled"], ["existing-plugin", "qingyu-monitor"])
        self.assertEqual(result["plugins"]["entries"], original["plugins"]["entries"])
        self.assertEqual(result["approval"], original["approval"])
        self.assertEqual(self.run_helper("uninstall").returncode, 0)
        self.assertEqual(yaml.safe_load(self.path.read_text()), original)
        once = self.path.read_bytes()
        self.assertEqual(self.run_helper("uninstall").returncode, 0)
        self.assertEqual(self.path.read_bytes(), once)

    def test_configuration_backup_is_private_and_written_once_for_each_change(self):
        self.path.write_text("model: existing\nplugins:\n  enabled: [existing]\n")
        original = self.path.read_bytes()
        self.assertEqual(self.run_helper("install").returncode, 0)
        directory = self.path.parent / ".qingyu-backups"
        backups = list(directory.glob("*.bak"))
        self.assertEqual(len(backups), 1)
        self.assertEqual(backups[0].read_bytes(), original)
        self.assertEqual(directory.stat().st_mode & 0o777, 0o700)
        self.assertEqual(backups[0].stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.run_helper("install").returncode, 0)
        self.assertEqual(len(list(directory.glob("*.bak"))), 1)

    def test_yaml_aliases_do_not_change_unrelated_plugin_settings(self):
        try:
            import hermes_yaml as yaml
        except ImportError:
            import yaml
        self.path.write_text("existing: &entries [other]\nplugins: &settings\n  enabled: *entries\nunrelated: *settings\n")
        self.assertEqual(self.run_helper("install").returncode, 0)
        result = yaml.safe_load(self.path.read_text())
        self.assertEqual(result["plugins"]["enabled"], ["other", "qingyu-monitor"])
        self.assertEqual(result["existing"], ["other"])
        self.assertEqual(result["unrelated"]["enabled"], ["other"])

    def test_current_hermes_preserves_comments_quotes_and_yaml_11_values(self):
        try:
            import hermes_yaml
        except ImportError:
            self.skipTest("Current Hermes runtime has the comment-preserving YAML editor")
        self.path.write_text('# KEEP COMMENT\nmodel: "quoted-value"\nboolean: yes\nplugins:\n  enabled: [other]  # KEEP PLUGINS\n')
        self.assertEqual(self.run_helper("install").returncode, 0)
        text = self.path.read_text()
        self.assertIn("# KEEP COMMENT", text)
        self.assertIn('# KEEP PLUGINS', text)
        self.assertIn('"quoted-value"', text)
        self.assertIs(hermes_yaml.safe_load(text)["boolean"], True)

    def test_malformed_duplicate_and_unsupported_yaml_are_never_overwritten(self):
        for source in ("broken: [\n", "plugins:\n  enabled: wrong\n", "plugins:\n  enabled: []\n  enabled: [other]\n", "- unexpected\n", "private: !unsupported value\n"):
            self.path.write_text(source)
            result = self.run_helper("install")
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(self.path.read_text(), source)
            self.assertNotIn(source.encode(), result.stderr)

    def test_explicit_plugin_disable_and_symlink_config_are_preserved(self):
        source = "plugins:\n  disabled: [qingyu-monitor]\n  enabled: [existing-plugin]\n"
        self.path.write_text(source)
        self.assertNotEqual(self.run_helper("install").returncode, 0)
        self.assertEqual(self.path.read_text(), source)
        self.path.unlink()
        other = self.path.with_name("other.yaml")
        other.write_text("model: existing\n")
        self.path.symlink_to(other)
        self.assertNotEqual(self.run_helper("install").returncode, 0)
        self.assertEqual(other.read_text(), "model: existing\n")


if __name__ == "__main__":
    unittest.main()
