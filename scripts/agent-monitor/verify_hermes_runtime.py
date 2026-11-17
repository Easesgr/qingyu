#!/usr/bin/env python3
"""Offline contract check using the current Hermes launcher and an isolated home.

No installed configuration is changed, other plugins are never loaded, and socket
connections are refused inside the verification subprocess. No model is invoked.
"""
import json
import os
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent

DRIVER = r'''
import json, os, shutil, socket, sys, tempfile, time
from pathlib import Path

def no_connection(*args, **kwargs):
    raise AssertionError("Offline lifecycle verification attempted a connection")
socket.socket.connect = no_connection
socket.create_connection = no_connection
sys.dont_write_bytecode = True
here = Path(sys.argv[1])
with tempfile.TemporaryDirectory(prefix="qingyu-hermes-runtime-") as temporary:
    home = Path(temporary)
    home.chmod(0o700)
    os.environ["HERMES_HOME"] = str(home)
    (home / "config.yaml").write_text("plugins:\n  enabled: [qingyu-monitor]\n  isolation: in_process\n")
    root = home / "monitor"
    root.mkdir(mode=0o700)
    (root / "events").mkdir(mode=0o700)
    (root / "enabled").write_text("offline")
    (root / "enabled").chmod(0o600)
    directory = home / "plugins/qingyu-monitor"
    directory.mkdir(parents=True)
    (directory / "__init__.py").write_text((here / "hermes_plugin.py").read_text().replace("__QINGYU_MONITOR_ROOT__", json.dumps(str(root))))
    shutil.copyfile(here / "sink.py", directory / "sink.py")
    (directory / "plugin.yaml").write_text("name: qingyu-monitor\nversion: 3.0.0\n")

    from hermes_cli.plugins import PluginManager
    from hermes_cli.plugins_manifest import parse_manifest_file
    from gateway.session_context import scoped_current_session_id, scoped_session_key
    manager = PluginManager(scope_key=str(home))
    manifest = parse_manifest_file(directory / "plugin.yaml", directory, "user", "")
    assert manifest is not None
    manager._load_plugin(manifest)  # Only this plugin, through Hermes' actual loader.
    loaded = manager._plugins["qingyu-monitor"]
    assert loaded.enabled and not loaded.error, loaded.error
    required = {"pre_llm_call", "pre_tool_call", "post_tool_call", "on_session_end", "agent_loop_stopped", "pre_approval_request", "post_approval_response", "on_human_input_request", "on_human_input_resolved"}
    assert required.issubset(manager._hooks)

    def fire(hook, session=None, route=None, **values):
        if session:
            with scoped_current_session_id(session), scoped_session_key(route or "route-" + session):
                result = manager.invoke_hook(hook, session_id=session, **values)
        else:
            result = manager.invoke_hook(hook, **values)
        assert result == [], (hook, result)  # Observer returns never direct agent behavior.

    fire("on_session_start", "one")
    assert loaded.module._STATE["one"][0] == "SessionStart"
    fire("pre_llm_call", "one", turn_id="one-a", user_message="PRIVATE PROMPT", session_title="Offline One")
    fire("pre_llm_call", "two", turn_id="two-a", user_message="PRIVATE PROMPT", session_title="Offline Two")
    fire("on_human_input_request", "one", kind="clarify", prompt="PRIVATE QUESTION")
    fire("on_human_input_resolved", "one", kind="clarify", outcome="answered")
    fire("pre_approval_request", "two", turn_id="two-a", command="PRIVATE COMMAND")
    fire("on_human_input_request", "two", kind="approval", prompt="PRIVATE COMMAND")
    fire("on_human_input_resolved", "two", kind="approval", outcome="once")
    fire("post_approval_response", "two", turn_id="two-a", choice="once")
    fire("post_tool_call", "two", turn_id="two-a", tool_name="terminal", status="error", result="PRIVATE RESULT")
    fire("pre_tool_call", "two", turn_id="two-a", tool_name="terminal", args={"command":"PRIVATE COMMAND"})
    fire("on_session_end", "two", turn_id="two-a", completed=False, failed=True, interrupted=False)
    # The real gateway interrupt carries only session_key. The ContextVar route
    # captured at pre_llm_call must resolve it while two sessions coexist.
    fire("agent_loop_stopped", session_key="route-one", reason="operator stop")
    fire("on_session_end", "one", turn_id="one-a", completed=True)
    fire("pre_tool_call", "two", turn_id="two-a", tool_name="terminal")
    fire("pre_approval_request", session_key="unknown-route", command="PRIVATE COMMAND")
    fire("pre_llm_call", "one", turn_id="one-b")
    fire("on_session_end", "one", turn_id="one-b", completed=True, failed=False, interrupted=False)
    fire("on_session_end", "one", turn_id="one-b", completed=True)
    deadline = time.monotonic() + 3
    while len(list((root / "events").glob("*.json"))) < 15 and time.monotonic() < deadline:
        time.sleep(0.01)
    records = [json.loads(path.read_text()) for path in sorted((root / "events").glob("*.json"))]
    records.sort(key=lambda value:value["at_ms"])
    assert len(records) == 15, [(value["session_id"], value["event"]) for value in records]
    assert {record["session_id"] for record in records} == {"one", "two"}
    assert all(record["pid"] == os.getpid() for record in records)
    assert [value["event"] for value in records if value["session_id"] == "one"] == ["SessionStart", "UserPromptSubmit", "Waiting", "Resume", "Interrupt", "UserPromptSubmit", "Stop"]
    assert [value["event"] for value in records if value["session_id"] == "two"] == ["UserPromptSubmit", "PermissionRequest", "PermissionRequest", "Resume", "Resume", "PostToolUseFailure", "PreToolUse", "StopFailure"]
    assert sum(value["event"] == "Stop" for value in records) == 1
    assert "PRIVATE" not in json.dumps(records)
    for session, turn in (("one", "one-a"), ("two", "two-a")):
        own = [value for value in records if value["session_id"] == session and value["turn_id"] == turn]
        assert len({value["started_at_ms"] for value in own}) == 1
    manager.unload()
    print(json.dumps({"runtime": sys.version.split()[0], "registered_hooks": len(required), "events": len(records), "sessions": 2, "completions": 1, "network": "blocked", "config": "isolated"}))
'''


def main():
    home = Path(os.environ.get("HERMES_HOME", str(Path.home() / ".hermes")))
    source = home / "hermes-agent"
    launcher = source / ".hermes/bin/hermes"
    resolved = subprocess.run([str(launcher), "--print-runtime-command"], capture_output=True, timeout=5, check=True)
    runtime = json.loads(resolved.stdout)
    bootstrap = "import os,sys;os.environ.pop('PYTHONHOME',None);os.environ.pop('PYTHONPATH',None);os.environ.pop('VIRTUAL_ENV',None);sys.path.insert(0," + repr(str(source)) + ");import hermes_bootstrap;exec(sys.stdin.read())"
    result = subprocess.run([runtime[0], "-I", "-c", bootstrap, str(HERE)], input=DRIVER.encode(), capture_output=True, timeout=15)
    if result.returncode:
        sys.stderr.buffer.write(result.stderr[-8192:])
        raise SystemExit(result.returncode)
    sys.stdout.buffer.write(result.stdout)


if __name__ == "__main__":
    main()
