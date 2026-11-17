#!/usr/bin/env python3
"""Qingyu 拾光 local capture/search CLI. Python standard library; no network I/O.

Content belongs in records/. The application alone owns states/. A source_id is
an opaque, stable identity, never a filesystem path. Re-ingestion may refine the
derived fields but cannot replace the original message or its source label.
"""

from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import errno
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import time
from typing import Any, Iterator
from urllib.parse import urlsplit

VERSION = 1
MAX_RECORD_BYTES = 256 * 1024
MAX_STATE_BYTES = 4096
MAX_ORIGINAL_BYTES = 128 * 1024
MAX_FILES = 10_000
MAX_SCAN_FILES = 20_000
MAX_READ_BYTES = 32 * 1024 * 1024
ID_RE = re.compile(r"[0-9a-f]{64}\Z")
TIME_RE = re.compile(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)\Z")
KINDS = {"idea", "link", "action"}
FIELDS = {
    "version", "id", "source_id", "kind", "title", "summary", "original", "url",
    "tags", "topic", "next_action", "source", "created_at", "updated_at",
}
STATE_FIELDS = {"pinned", "reviewed", "done", "archived"}


class LibraryError(Exception):
    """An expected validation or storage failure, suitable for the CLI caller."""


def record_id(source_id: str) -> str:
    return hashlib.sha256(b"qingyu-library-v1\0" + source_id.encode("utf-8")).hexdigest()


def text(value: Any, label: str, limit: int, required: bool = False) -> str:
    if not isinstance(value, str):
        raise LibraryError(f"{label} must be a string")
    if required and not value.strip():
        raise LibraryError(f"{label} must not be empty")
    if "\0" in value:
        raise LibraryError(f"{label} must not contain NUL")
    try:
        length = len(value.encode("utf-8"))
    except UnicodeEncodeError as exc:
        raise LibraryError(f"{label} must be valid UTF-8") from exc
    if length > limit:
        raise LibraryError(f"{label} exceeds {limit} UTF-8 bytes")
    return value


def timestamp(value: Any) -> dt.datetime:
    value = text(value, "timestamp", 64, True)
    if not TIME_RE.fullmatch(value):
        raise LibraryError("timestamp must be ISO 8601 with seconds and a timezone")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise LibraryError("invalid timestamp") from exc
    if parsed.tzinfo is None:
        raise LibraryError("timestamp requires a timezone")
    return parsed


def iso(value: dt.datetime) -> str:
    return value.astimezone(dt.timezone.utc).isoformat(timespec="microseconds").replace("+00:00", "Z")


def validate_url(value: Any) -> str:
    value = text(value, "url", 8192, True)
    if any(c.isspace() or ord(c) < 32 or ord(c) == 127 for c in value):
        raise LibraryError("url must not contain whitespace or control characters")
    try:
        parsed = urlsplit(value)
        if parsed.scheme not in {"http", "https"} or not parsed.hostname:
            raise ValueError("missing http(s) scheme or hostname")
        _ = parsed.port  # validates malformed/out-of-range ports
        if any(c in parsed.hostname for c in "\\<>%{}|^`"):
            raise ValueError("invalid hostname")
    except ValueError as exc:
        raise LibraryError("url must be a valid http(s) URL") from exc
    return value


def extract_url(value: str) -> str | None:
    found = re.search(r'https?://[^\s<>"“”「」『』]+', value, re.IGNORECASE)
    if not found:
        return None
    return validate_url(found.group(0).rstrip(".,;!?)]}'。，；！？）】》"))


def validate_record(value: Any, expected_id: str | None = None) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise LibraryError("record must be a JSON object")
    unknown = set(value) - FIELDS
    if unknown:
        raise LibraryError("unsupported record fields: " + ", ".join(sorted(unknown)))
    if type(value.get("version")) is not int or value["version"] != VERSION:
        raise LibraryError("unsupported record version")
    identity = text(value.get("source_id"), "source_id", 1024, True)
    identifier = value.get("id")
    if not isinstance(identifier, str) or not ID_RE.fullmatch(identifier) or identifier != record_id(identity):
        raise LibraryError("id must be the SHA-256 identity of source_id")
    if expected_id is not None and identifier != expected_id:
        raise LibraryError("record id does not match its filename")
    if not isinstance(value.get("kind"), str) or value["kind"] not in KINDS:
        raise LibraryError("kind must be idea, link, or action")
    text(value.get("title"), "title", 512, True)
    text(value.get("original"), "original", MAX_ORIGINAL_BYTES, True)
    text(value.get("summary", ""), "summary", 16 * 1024)
    text(value.get("source"), "source", 256, True)
    tags = value.get("tags", [])
    if not isinstance(tags, list) or len(tags) > 24:
        raise LibraryError("tags must be a list of at most 24 strings")
    for tag in tags:
        text(tag, "tag", 128, True)
    for key, limit in (("topic", 512), ("next_action", 2048)):
        if value.get(key) is not None:
            text(value[key], key, limit, True)
    if value.get("url") is not None:
        validate_url(value["url"])
    if value["kind"] == "link" and not value.get("url"):
        raise LibraryError("link records require an http(s) url")
    if timestamp(value.get("updated_at")) < timestamp(value.get("created_at")):
        raise LibraryError("updated_at precedes created_at")
    return value


def validate_state(value: Any, identifier: str) -> dict[str, bool]:
    if not isinstance(value, dict) or set(value) != {"version", "id", "state"}:
        raise LibraryError("invalid state envelope")
    if type(value["version"]) is not int or value["version"] != VERSION or value["id"] != identifier:
        raise LibraryError("invalid state version or id")
    state_value = value["state"]
    if not isinstance(state_value, dict) or set(state_value) - STATE_FIELDS:
        raise LibraryError("invalid UI state fields")
    if any(type(v) is not bool for v in state_value.values()):
        raise LibraryError("UI state values must be boolean")
    return {key: state_value.get(key, False) for key in sorted(STATE_FIELDS)}


def _unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise LibraryError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def decode_json(raw: bytes) -> Any:
    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=_unique_object)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as exc:
        raise LibraryError("invalid UTF-8 JSON") from exc


def check_directory(path: Path, create: bool = False) -> None:
    try:
        info = path.lstat()
    except FileNotFoundError:
        if not create:
            return
        path.mkdir(mode=0o700, parents=True, exist_ok=True)
        info = path.lstat()
    if not stat.S_ISDIR(info.st_mode):
        raise LibraryError(f"not a real directory (symlinks are not supported): {path}")


def check_file(path: Path, missing_ok: bool = False) -> None:
    try:
        info = path.lstat()
    except FileNotFoundError:
        if missing_ok:
            return
        raise
    if not stat.S_ISREG(info.st_mode):
        raise LibraryError(f"not a regular file: {path}")


def read_regular(path: Path, limit: int) -> bytes:
    check_file(path)
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    with os.fdopen(descriptor, "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode):
            raise LibraryError(f"not a regular file: {path}")
        if info.st_size > limit:
            raise LibraryError(f"file exceeds {limit} bytes: {path.name}")
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise LibraryError(f"file exceeds {limit} bytes: {path.name}")
    return raw


def sync_directory(path: Path) -> None:
    if os.name == "posix":
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def atomic_json(path: Path, value: Any) -> None:
    check_directory(path.parent, create=True)
    check_file(path, missing_ok=True)
    raw = (json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode("utf-8")
    if len(raw) > MAX_RECORD_BYTES:
        raise LibraryError("encoded record exceeds 256 KiB")
    descriptor, temp_name = tempfile.mkstemp(prefix=".qingyu-", suffix=".tmp", dir=path.parent)
    try:
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temp_name, path)
        sync_directory(path.parent)
    finally:
        try:
            os.unlink(temp_name)
        except FileNotFoundError:
            pass


@contextlib.contextmanager
def record_lock(root: Path, identifier: str, timeout: float = 10.0) -> Iterator[None]:
    # File locks are released by the OS even when the Hermes process is stopped.
    lock_dir = root / "records" / ".locks"
    check_directory(lock_dir, create=True)
    path = lock_dir / f"{identifier}.lock"
    check_file(path, missing_ok=True)
    descriptor = os.open(path, os.O_RDWR | os.O_CREAT | getattr(os, "O_NOFOLLOW", 0), 0o600)
    if not stat.S_ISREG(os.fstat(descriptor).st_mode):
        os.close(descriptor)
        raise LibraryError("record lock is not a regular file")
    deadline = time.monotonic() + timeout
    locked = False
    try:
        if os.name == "nt":
            import msvcrt
            if os.fstat(descriptor).st_size == 0:
                os.write(descriptor, b"\0")
            def acquire() -> None:
                os.lseek(descriptor, 0, os.SEEK_SET)
                msvcrt.locking(descriptor, msvcrt.LK_NBLCK, 1)
            def release() -> None:
                os.lseek(descriptor, 0, os.SEEK_SET)
                msvcrt.locking(descriptor, msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            def acquire() -> None:
                fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            def release() -> None:
                fcntl.flock(descriptor, fcntl.LOCK_UN)
        while not locked:
            try:
                acquire()
                locked = True
            except (BlockingIOError, PermissionError, OSError) as exc:
                if exc.errno not in {errno.EACCES, errno.EAGAIN}:
                    raise LibraryError(f"cannot lock record: {exc}") from exc
                if time.monotonic() >= deadline:
                    raise LibraryError("record is busy; retry with the same source_id") from exc
                time.sleep(0.05)
        yield
    finally:
        if locked:
            release()
        os.close(descriptor)


def read_record(path: Path, identifier: str) -> dict[str, Any]:
    return validate_record(decode_json(read_regular(path, MAX_RECORD_BYTES)), identifier)


def as_entry(root: Path, record: dict[str, Any]) -> dict[str, Any]:
    entry = {key: value for key, value in record.items() if key not in {"version", "source_id"}}
    entry["source_id"] = record["source_id"]  # CLI exposes provenance; the UI can omit it.
    entry["state"] = {key: False for key in sorted(STATE_FIELDS)}
    state_dir = root / "states"
    check_directory(state_dir)
    try:
        state_value = decode_json(read_regular(state_dir / f"{record['id']}.json", MAX_STATE_BYTES))
    except FileNotFoundError:
        return entry
    entry["state"] = validate_state(state_value, record["id"])
    return entry


def ingest(root: Path, payload: Any) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise LibraryError("input must be one JSON object")
    if set(payload) - FIELDS:
        raise LibraryError("unsupported input fields: " + ", ".join(sorted(set(payload) - FIELDS)))
    if "updated_at" in payload:
        raise LibraryError("updated_at is managed by storage; omit it from capture input")
    identity = text(payload.get("source_id"), "source_id", 1024, True)
    identifier = record_id(identity)
    if "id" in payload and payload["id"] != identifier:
        raise LibraryError("supplied id does not match source_id; do not use a path as an id")
    if "version" in payload and (type(payload["version"]) is not int or payload["version"] != VERSION):
        raise LibraryError("unsupported record version")
    original = text(payload.get("original"), "original", MAX_ORIGINAL_BYTES, True)
    source = text(payload.get("source", "微信 · Hermes"), "source", 256, True)
    now = dt.datetime.now(dt.timezone.utc)
    created = payload.get("created_at", iso(now))
    timestamp(created)
    record = {
        "version": VERSION, "id": identifier, "source_id": identity,
        "kind": payload.get("kind"), "title": payload.get("title"),
        "summary": payload.get("summary", ""), "original": original,
        "url": extract_url(original) if payload.get("url") is None else validate_url(payload["url"]), "tags": payload.get("tags", []),
        "topic": payload.get("topic"), "next_action": payload.get("next_action"), "source": source,
        "created_at": created, "updated_at": iso(max(now, timestamp(created))),
    }
    # Validate all fields before creating any directories or opening the lock.
    validate_record(record)
    check_directory(root, create=True)
    check_directory(root / "records", create=True)
    path = root / "records" / f"{identifier}.json"
    with record_lock(root, identifier):
        try:
            previous = read_record(path, identifier)
        except FileNotFoundError:
            previous = None
        if previous is not None:
            if previous["original"] != original or previous["source"] != source:
                raise LibraryError("source_id already exists with a different original/source; use a new source_id for a different message")
            record["created_at"] = previous["created_at"]
            record["updated_at"] = iso(max(now, timestamp(previous["updated_at"]), timestamp(previous["created_at"])))
        validate_record(record)
        atomic_json(path, record)
        saved = read_record(path, identifier)
        if saved != record:
            raise LibraryError("record read-back did not match; retry with the same source_id")
    # A broken UI state must not make an already durable capture look unsaved.
    return {"ok": True, "id": identifier, "created": previous is None, "record": saved}


def list_entries(root: Path, query: str = "", kind: str | None = None,
                 include_archived: bool = False, limit: int = 50) -> dict[str, Any]:
    check_directory(root)
    directory = root / "records"
    check_directory(directory)
    paths: list[tuple[str, Path]] = []
    errors: list[str] = []
    try:
        with os.scandir(directory) as scan:
            for count, item in enumerate(scan):
                if count >= MAX_SCAN_FILES:
                    raise LibraryError("too many files; scan limit is 20000")
                if not item.name.endswith(".json"):
                    continue
                identifier = item.name[:-5]
                if not ID_RE.fullmatch(identifier):
                    errors.append(f"invalid record filename: {item.name[:100]!r}")
                    continue
                if len(paths) >= MAX_FILES:
                    raise LibraryError("too many records; limit is 10000")
                paths.append((identifier, Path(item.path)))
    except FileNotFoundError:
        pass
    paths.sort()
    budget = MAX_READ_BYTES
    entries = []
    query = query.casefold()
    for identifier, path in paths:
        try:
            size = path.lstat().st_size
            if size > MAX_RECORD_BYTES:
                raise LibraryError("record exceeds 256 KiB")
            if size > budget:
                errors.append("read limit is 32 MiB; results are partial")
                break
            budget -= size
            record = read_record(path, identifier)
            entry = as_entry(root, record)
            if not include_archived and entry["state"]["archived"]:
                continue
            if kind and record["kind"] != kind:
                continue
            haystack = "\n".join(str(record.get(key) or "") for key in (
                "title", "summary", "original", "url", "tags", "topic", "next_action", "source"
            )).casefold()
            if query and query not in haystack:
                continue
            entries.append(entry)
        except (LibraryError, OSError) as exc:
            errors.append(f"{identifier[:8]}: {exc}")
    entries.sort(key=lambda item: (not item["state"]["pinned"], -timestamp(item["created_at"]).timestamp(), item["id"]))
    result = {"ok": not errors, "entries": entries[:limit], "matched": len(entries), "errors": errors[:8]}
    if len(errors) > 8:
        result["errors"].append(f"{len(errors) - 8} additional errors")
    return result


def resolve_root(explicit: str | None = None) -> Path:
    if explicit is not None:
        text(explicit, "root", 16 * 1024, True)
        return Path(os.path.abspath(os.path.expanduser(explicit)))
    config = Path(__file__).resolve().parent.parent / "config.json"
    try:
        value = decode_json(read_regular(config, 32 * 1024))
    except FileNotFoundError:
        value = None
    if value is not None:
        if not isinstance(value, dict) or set(value) != {"version", "root"} or type(value["version"]) is not int or value["version"] != VERSION:
            raise LibraryError("invalid installed config.json; reinstall the Qingyu skill")
        root = text(value["root"], "configured root", 16 * 1024, True)
        if not Path(root).is_absolute():
            raise LibraryError("installed root must be absolute; reinstall the Qingyu skill")
        return Path(root)
    data_override = os.environ.get("QINGYU_DATA_DIR") or os.environ.get("DAOZAI_DATA_DIR")
    if data_override:
        return Path(os.path.abspath(os.path.expanduser(data_override))) / "library"
    home = Path.home()
    if sys.platform == "darwin":
        data = home / "Library" / "Application Support" / "Qingyu"
    elif os.name == "nt":
        data = Path(os.environ.get("APPDATA", tempfile.gettempdir())) / "Qingyu"
    else:
        data = Path(os.environ.get("XDG_DATA_HOME", str(home / ".local" / "share"))) / "Qingyu"
    legacy = data.with_name("Daozai")
    if not data.exists() and legacy.is_dir():
        data = legacy  # The app owns migration; the CLI never moves other data.
    return data / "library"


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description="拾光 local capture/search; no network calls")
    value.add_argument("--root", help="absolute/relative library directory; overrides installed binding")
    commands = value.add_subparsers(dest="command", required=True)
    capture = commands.add_parser("ingest", help="save/refine one original message by stable source_id")
    capture.add_argument("--input", default="-", help="UTF-8 JSON file, or - for stdin (default)")
    for name in ("list", "search"):
        listing = commands.add_parser(name, help="find saved ideas, links and actions")
        if name == "search":
            listing.add_argument("query")
        else:
            listing.add_argument("--query", default="")
        listing.add_argument("--kind", choices=sorted(KINDS))
        listing.add_argument("--include-archived", action="store_true")
        listing.add_argument("--limit", type=int, default=50)
    get = commands.add_parser("get", help="read one record and its independent UI state")
    get.add_argument("id")
    commands.add_parser("doctor", help="show the resolved local path and storage errors")
    return value


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        root = resolve_root(args.root)
        if args.command == "ingest":
            if args.input == "-":
                raw = sys.stdin.buffer.read(MAX_RECORD_BYTES + 1)
                if len(raw) > MAX_RECORD_BYTES:
                    raise LibraryError("input exceeds 256 KiB")
            else:
                raw = read_regular(Path(args.input), MAX_RECORD_BYTES)
            result = ingest(root, decode_json(raw))
        elif args.command in {"list", "search"}:
            if not 1 <= args.limit <= 500:
                raise LibraryError("limit must be between 1 and 500")
            result = list_entries(root, args.query, args.kind, args.include_archived, args.limit)
        elif args.command == "get":
            if not ID_RE.fullmatch(args.id):
                raise LibraryError("id must be 64 lowercase hex characters")
            check_directory(root)
            check_directory(root / "records")
            record = read_record(root / "records" / f"{args.id}.json", args.id)
            result = {"ok": True, "entry": as_entry(root, record)}
        else:
            listing = list_entries(root, include_archived=True, limit=1)
            result = {"ok": listing["ok"], "root": str(root), "records": listing["matched"],
                      "errors": listing["errors"], "wechat_gateway": "not tested by this CLI"}
        print(json.dumps(result, ensure_ascii=False, allow_nan=False))
        return 0 if result["ok"] else 2
    except (LibraryError, OSError, ValueError, TypeError) as exc:
        print(json.dumps({"ok": False, "error": str(exc)}, ensure_ascii=False), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
