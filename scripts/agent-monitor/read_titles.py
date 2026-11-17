# qingyu-monitor:v2 — bounded, read-only agent metadata lookup; standard library only.
import json
from pathlib import Path
import re
import sqlite3
import sys
import time

UUID = re.compile(r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\Z")


def parent_id(metadata, session):
    candidates = [metadata.get("attached_thread_parent_id"), metadata.get("parent_thread_id")]
    for field in ("source", "thread_source"):
        raw = metadata.get(field)
        if not isinstance(raw, str):
            continue
        try:
            value = json.loads(raw)
        except (ValueError, TypeError):
            continue
        if not isinstance(value, dict):
            continue
        candidates.extend([value.get("attached_thread_parent_id"), value.get("parent_thread_id")])
        subagent = value.get("subagent")
        spawn = subagent.get("thread_spawn") if isinstance(subagent, dict) else None
        if isinstance(spawn, dict):
            candidates.append(spawn.get("parent_thread_id"))
    return next((value for value in candidates if isinstance(value, str) and UUID.fullmatch(value) and value != session), None)


def child_identity(metadata, session):
    """Only documented thread identity fields; never search arbitrary nested data."""
    spawns = []
    for field in ("source", "thread_source"):
        raw = metadata.get(field)
        if not isinstance(raw, str):
            continue
        try:
            value = json.loads(raw)
        except (ValueError, TypeError):
            continue
        subagent = value.get("subagent") if isinstance(value, dict) else None
        spawn = subagent.get("thread_spawn") if isinstance(subagent, dict) else None
        if isinstance(spawn, dict):
            spawns.append(spawn)
    identity = {"parent": parent_id(metadata, session)}
    identity["is_subagent"] = bool(identity["parent"] or spawns)
    for key in ("agent_nickname", "agent_role"):
        candidates = [metadata.get(key)] + [spawn.get(key) for spawn in spawns]
        identity[key] = next((value.strip()[:160] for value in candidates if isinstance(value, str) and value.strip()), None)
    return identity


def main():
    requests = json.loads(sys.argv[1])
    output = []
    seen = set()
    deadline = time.monotonic() + 0.6
    for request in requests[:3]:
        if time.monotonic() >= deadline:
            break
        agent, filename, ids = request
        table = {"codex": "threads", "hermes": "sessions"}.get(agent)
        ids = [value for value in ids[:64] if isinstance(value, str) and len(value) <= 160]
        if not table or not ids:
            continue
        connection = None
        try:
            path = Path(filename)
            if path.is_symlink() or not path.is_file():
                continue
            connection = sqlite3.connect(path.absolute().as_uri() + "?mode=ro", uri=True, timeout=0.08)
            connection.execute("PRAGMA query_only=ON")
            connection.set_progress_handler(lambda: int(time.monotonic() >= deadline), 1000)
            allowed = {"id", "title", "name", "archived", "rollout_path", "source", "thread_source", "attached_thread_parent_id", "parent_thread_id", "agent_nickname", "agent_role"} if agent == "codex" else {"id", "title", "ended_at"}

            def authorize(operation, first, second, _database, _trigger):
                if operation == sqlite3.SQLITE_SELECT:
                    return sqlite3.SQLITE_OK
                if operation == sqlite3.SQLITE_READ and first == table and second in allowed:
                    return sqlite3.SQLITE_OK
                if operation == sqlite3.SQLITE_PRAGMA and first == "table_info" and second == table:
                    return sqlite3.SQLITE_OK
                if operation == sqlite3.SQLITE_FUNCTION and second == "substr":
                    return sqlite3.SQLITE_OK
                return sqlite3.SQLITE_DENY

            connection.set_authorizer(authorize)
            # Schema compatibility only: no message/auth tables or SELECT *.
            columns = {row[1] for row in connection.execute("PRAGMA table_info(" + table + ")").fetchmany(128)}
            if not {"id", "title"}.issubset(columns):
                continue
            selected = ["id", "substr(title,1,256)"]
            optional = [key for key in ("name", "archived", "rollout_path", "source", "thread_source", "attached_thread_parent_id", "parent_thread_id", "agent_nickname", "agent_role") if agent == "codex" and key in columns]
            hermes_ended = agent == "hermes" and "ended_at" in columns
            selected.extend("substr(" + key + ",1," + ("4096" if key in {"rollout_path", "source", "thread_source"} else "256") + ")" if key != "archived" else key for key in optional)
            if hermes_ended:
                selected.append("ended_at")

            def read_rows(requested):
                placeholders = ",".join("?" for _ in requested)
                statement = "SELECT " + ",".join(selected) + " FROM " + table + " WHERE id IN (" + placeholders + ") LIMIT 64"
                result = {}
                for values in connection.execute(statement, requested):
                    session, title = values[:2]
                    if isinstance(session, str) and (title is None or isinstance(title, str)):
                        metadata = dict(zip(optional, values[2:]))
                        if hermes_ended:
                            metadata["ended"] = values[2 + len(optional)] is not None
                        metadata["title"] = title or ""
                        if agent == "codex":
                            metadata.update(child_identity(metadata, session))
                        else:
                            metadata["parent"] = None
                        result[session] = metadata
                return result

            graph = read_rows(ids)
            # Follow only named parent-id fields for the requested open threads.
            # Parent rows supply names; they never become running sessions.
            for _ in range(8):
                if agent != "codex" or len(graph) >= 128 or time.monotonic() >= deadline:
                    break
                parents = sorted({row["parent"] for row in graph.values() if row["parent"] and row["parent"] not in graph})[:min(64, 128 - len(graph))]
                if not parents:
                    break
                loaded = read_rows(parents)
                if not loaded:
                    break
                graph.update(loaded)

            for session in ids:
                metadata = graph.get(session)
                if metadata is None:
                    continue
                title = metadata["title"]
                if isinstance(session, str) and (title is None or isinstance(title, str)) and (agent, session) not in seen:
                    seen.add((agent, session))
                    row = {"agent": agent, "id": session[:160], "title": (title or "")[:256]}
                    if "name" in optional:
                        row["has_name"] = True
                        row["name"] = metadata["name"] if isinstance(metadata["name"], str) else None
                    if "archived" in optional:
                        row["archived"] = bool(metadata["archived"])
                    if "rollout_path" in optional:
                        row["rollout_path"] = metadata["rollout_path"] if isinstance(metadata["rollout_path"], str) else None
                    if "ended" in metadata:
                        row["ended"] = metadata["ended"] is True
                    if metadata.get("is_subagent"):
                        row["is_subagent"] = True
                        for key in ("agent_nickname", "agent_role"):
                            if metadata.get(key):
                                row[key] = metadata[key]
                    parent = metadata["parent"]
                    if parent:
                        row["parent_id"] = parent
                        visited = {session}
                        for _ in range(8):
                            if parent in visited or parent not in graph:
                                break
                            visited.add(parent)
                            ancestor = graph[parent]
                            name = next((value.strip() for value in (ancestor.get("name"), ancestor.get("title")) if isinstance(value, str) and value.strip()), "")
                            if name:
                                row["parent_title"] = name[:160]
                                break
                            parent = ancestor["parent"]
                            if not parent:
                                break
                    output.append(row)
        except (sqlite3.Error, OSError, ValueError):
            pass  # Metadata is optional; leave the current title unchanged.
        finally:
            if connection:
                connection.close()
    for row in output:
        for key in ("name", "title", "parent_title", "agent_nickname", "agent_role"):
            if isinstance(row.get(key), str):
                row[key] = row[key][:160]
    body = json.dumps(output, ensure_ascii=False, separators=(",", ":"))
    if len(body.encode("utf-8")) > 120 * 1024:
        # Path metadata is optional; never lose the bounded name/archive result.
        for row in output:
            row.pop("rollout_path", None)
        body = json.dumps(output, ensure_ascii=False, separators=(",", ":"))
    for limit in (120, 80, 40):
        if len(body.encode("utf-8")) <= 120 * 1024:
            break
        for row in output:
            for key in ("name", "title", "parent_title", "agent_nickname", "agent_role"):
                if isinstance(row.get(key), str):
                    row[key] = row[key][:limit]
        body = json.dumps(output, ensure_ascii=False, separators=(",", ":"))
    while output and len(body.encode("utf-8")) > 120 * 1024:
        output.pop()
        body = json.dumps(output, ensure_ascii=False, separators=(",", ":"))
    print(body)


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("[]")
