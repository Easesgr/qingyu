"""Focused storage/CLI contract tests; uses only temporary directories."""

import importlib.util
import errno
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).with_name("shiguang.py")
SPEC = importlib.util.spec_from_file_location("shiguang", SCRIPT)
sg = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sg)


class LibraryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="qingyu-shiguang-test-")
        self.base = Path(self.temp.name)
        self.root = self.base / "library"

    def tearDown(self):
        self.temp.cleanup()

    def payload(self, source_id="wechat:account:chat:42", **changes):
        value = {
            "source_id": source_id, "source": "微信 · Hermes", "kind": "link",
            "title": "用户收藏的设计文章", "summary": "评审前阅读；正文内容未读取。",
            "original": "收藏这个，下次评审前看看 https://example.com/design",
            "url": "https://example.com/design", "tags": ["设计", "待读"],
            "topic": "设计评审", "next_action": "阅读原文",
        }
        value.update(changes)
        return value

    def run_cli(self, *args, payload=None, root=None, env=None):
        command = [sys.executable, str(SCRIPT)]
        if root is not False:
            command += ["--root", str(root or self.root)]
        return subprocess.run(command + list(args), input=None if payload is None else json.dumps(payload, ensure_ascii=False),
                              capture_output=True, text=True, env=env, timeout=15)

    def test_retry_same_source_id_refines_one_record_and_keeps_creation(self):
        first = sg.ingest(self.root, self.payload(created_at="2026-11-15T01:00:00Z"))
        second = sg.ingest(self.root, self.payload(summary="读过原文后提炼的摘要"))
        self.assertTrue(first["created"])
        self.assertFalse(second["created"])
        self.assertEqual(first["id"], second["id"])
        self.assertEqual(second["record"]["created_at"], "2026-11-15T01:00:00Z")
        self.assertEqual(second["record"]["summary"], "读过原文后提炼的摘要")
        self.assertEqual(len(list((self.root / "records").glob("*.json"))), 1)

    def test_same_url_with_new_user_comment_is_a_distinct_capture(self):
        first = sg.ingest(self.root, self.payload())
        second = sg.ingest(self.root, self.payload("wechat:account:chat:43", original="我想把它做成周末原型 https://example.com/design"))
        self.assertNotEqual(first["id"], second["id"])
        self.assertEqual(len(sg.list_entries(self.root)["entries"]), 2)
        with self.assertRaises(sg.LibraryError):
            sg.ingest(self.root, self.payload(original="不同原文 https://example.com/design"))
        with self.assertRaises(sg.LibraryError):
            sg.ingest(self.root, self.payload(source="另一个来源"))
        self.assertEqual(sg.read_record(self.root / "records" / f"{first['id']}.json", first["id"])["original"], self.payload()["original"])

    def test_concurrent_retries_preserve_independent_ui_state_and_original(self):
        first = sg.ingest(self.root, self.payload())
        state_path = self.root / "states" / f"{first['id']}.json"
        state = {"version": 1, "id": first["id"], "state": {"pinned": True, "reviewed": True, "done": True, "archived": False}}
        sg.atomic_json(state_path, state)
        before = state_path.read_bytes()
        processes = []
        for index in range(6):
            process = subprocess.Popen([sys.executable, str(SCRIPT), "--root", str(self.root), "ingest"],
                                       stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            process.stdin.write(json.dumps(self.payload(summary=f"summary {index}"), ensure_ascii=False))
            process.stdin.close()
            processes.append(process)
        for process in processes:
            process.wait(timeout=15)
            self.assertEqual(process.returncode, 0, process.stderr.read())
            self.assertTrue(json.loads(process.stdout.read())["ok"])
            process.stdout.close()
            process.stderr.close()
        self.assertEqual(len(list((self.root / "records").glob("*.json"))), 1)
        self.assertEqual(state_path.read_bytes(), before)
        found = sg.list_entries(self.root)["entries"][0]
        self.assertEqual(found["state"], state["state"])
        self.assertEqual(found["original"], self.payload()["original"])
        self.assertEqual(found["created_at"], first["record"]["created_at"])

    def test_malformed_existing_record_is_not_overwritten(self):
        result = sg.ingest(self.root, self.payload())
        path = self.root / "records" / f"{result['id']}.json"
        path.write_bytes(b'{"broken":')
        with self.assertRaises(sg.LibraryError):
            sg.ingest(self.root, self.payload())
        self.assertEqual(path.read_bytes(), b'{"broken":')

    def test_partial_search_reports_corrupt_oversize_and_invalid_names(self):
        sg.ingest(self.root, self.payload())
        directory = self.root / "records"
        (directory / f"{'a' * 64}.json").write_bytes(b"{")
        (directory / f"{'b' * 64}.json").write_bytes(b" " * (sg.MAX_RECORD_BYTES + 1))
        (directory / "bad-name.json").write_text("{}")
        found = sg.list_entries(self.root)
        self.assertFalse(found["ok"])
        self.assertEqual(len(found["entries"]), 1)
        self.assertEqual(len(found["errors"]), 3)
        process = self.run_cli("search", "设计")
        self.assertEqual(process.returncode, 2)
        self.assertEqual(len(json.loads(process.stdout)["entries"]), 1)

    def test_validation_rejects_malformed_oversize_and_unknown_state_input(self):
        bad = [
            self.payload(source_id=""), self.payload(original=" "),
            self.payload(original="x" * (sg.MAX_ORIGINAL_BYTES + 1)),
            self.payload(tags=["x"] * 25), self.payload(url="javascript:alert(1)"),
            self.payload(url="https://example.com:99999/a"), self.payload(title="x" * 513),
            self.payload(kind="task"), self.payload(state={"done": True}),
            self.payload(kind=[]), self.payload(url=0), self.payload(updated_at="invented"),
            self.payload(created_at="2026-11-15"), self.payload(version=True),
            self.payload(summary="\ud800"),
        ]
        for payload in bad:
            with self.subTest(payload=list(payload)):
                with self.assertRaises(sg.LibraryError):
                    sg.ingest(self.root, payload)
        self.assertFalse(self.root.exists())

    def test_traversal_ids_are_rejected_and_opaque_source_ids_are_hashed(self):
        outside = self.base / "outside.json"
        outside.write_text("keep")
        with self.assertRaises(sg.LibraryError):
            sg.ingest(self.root, self.payload(id="../../outside.json"))
        result = sg.ingest(self.root, self.payload("../../outside.json"))
        self.assertRegex(result["id"], "^[0-9a-f]{64}$")
        self.assertTrue((self.root / "records" / f"{result['id']}.json").is_file())
        self.assertEqual(outside.read_text(), "keep")
        get = self.run_cli("get", "../../outside.json")
        self.assertEqual(get.returncode, 1)
        self.assertIn("id must", json.loads(get.stderr)["error"])

    def test_record_identity_and_filename_must_match(self):
        saved = sg.ingest(self.root, self.payload())
        wrong = self.root / "records" / f"{'a' * 64}.json"
        wrong.write_text(json.dumps(saved["record"], ensure_ascii=False))
        listing = sg.list_entries(self.root)
        self.assertFalse(listing["ok"])
        self.assertEqual(len(listing["entries"]), 1)
        self.assertEqual(sg.record_id("wechat:chat:42"), "e08dd90b146898e938a8bc19116068950563652bff5d38a2771193dd4de49042")

    def test_failed_replace_leaves_original_and_cleans_temporary_file(self):
        saved = sg.ingest(self.root, self.payload())
        path = self.root / "records" / f"{saved['id']}.json"
        before = path.read_bytes()
        with mock.patch.object(sg.os, "replace", side_effect=PermissionError("synthetic blocked replace")):
            with self.assertRaises(PermissionError):
                sg.ingest(self.root, self.payload(summary="must not appear"))
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse(list((self.root / "records").glob("*.tmp")))

    @unittest.skipUnless(os.name == "posix", "POSIX file lock errors")
    def test_unsupported_file_lock_is_reported_without_a_false_busy_retry(self):
        with mock.patch("fcntl.flock", side_effect=OSError(errno.EOPNOTSUPP, "unsupported lock")), mock.patch.object(sg.time, "sleep") as delay:
            with self.assertRaisesRegex(sg.LibraryError, "cannot lock record"):
                sg.ingest(self.root, self.payload())
            delay.assert_not_called()
        self.assertFalse(list((self.root / "records").glob("*.json")))

    def test_cli_failed_storage_has_nonzero_exit_and_no_success(self):
        self.root.write_text("blocked")
        failed = self.run_cli("ingest", payload=self.payload())
        self.assertEqual(failed.returncode, 1)
        self.assertEqual(failed.stdout, "")
        self.assertFalse(json.loads(failed.stderr)["ok"])

    def test_search_order_filters_and_archived_state(self):
        first = sg.ingest(self.root, self.payload("wechat:a", created_at="2026-11-15T10:00:00+09:00"))
        second = sg.ingest(self.root, self.payload("wechat:b", created_at="2026-11-15T01:01:00Z"))
        third = sg.ingest(self.root, self.payload("wechat:c", created_at="2026-11-15T01:01:00Z"))
        expected = sorted([second["id"], third["id"]]) + [first["id"]]
        self.assertEqual([entry["id"] for entry in sg.list_entries(self.root)["entries"]], expected)
        sg.atomic_json(self.root / "states" / f"{first['id']}.json", {
            "version": 1, "id": first["id"], "state": {"pinned": True, "archived": True}})
        listing = sg.list_entries(self.root, "评审", "link", True)
        self.assertEqual(listing["entries"][0]["id"], first["id"])
        self.assertEqual(len(sg.list_entries(self.root)["entries"]), 2)
        self.assertEqual(len(sg.list_entries(self.root, kind="action")["entries"]), 0)
        self.assertEqual(len(sg.list_entries(self.root, query="does not exist")["entries"]), 0)

    @unittest.skipUnless(os.name == "posix", "symlink test")
    def test_symlinks_cannot_redirect_record_or_state_access(self):
        outside = self.base / "outside"
        outside.mkdir()
        self.root.mkdir()
        (self.root / "records").symlink_to(outside, target_is_directory=True)
        with self.assertRaises(sg.LibraryError):
            sg.ingest(self.root, self.payload())
        self.assertEqual(list(outside.iterdir()), [])
        (self.root / "records").unlink()
        result = sg.ingest(self.root, self.payload())
        victim = outside / "victim.json"
        victim.write_text("keep")
        path = self.root / "records" / f"{result['id']}.json"
        path.unlink()
        path.symlink_to(victim)
        with self.assertRaises(sg.LibraryError):
            sg.ingest(self.root, self.payload())
        self.assertEqual(victim.read_text(), "keep")

    def test_default_and_dev_data_roots_and_installed_binding(self):
        dev = self.base / "dev data"
        with mock.patch.dict(os.environ, {"QINGYU_DATA_DIR": str(dev)}, clear=True):
            self.assertEqual(sg.resolve_root(), dev / "library")
        with mock.patch.dict(os.environ, {"DAOZAI_DATA_DIR": str(dev)}, clear=True):
            self.assertEqual(sg.resolve_root(), dev / "library")
        with mock.patch.dict(os.environ, {"HOME": str(self.base)}, clear=True), mock.patch.object(sg.sys, "platform", "darwin"):
            self.assertEqual(sg.resolve_root(), self.base / "Library/Application Support/Qingyu/library")
            legacy = self.base / "Library/Application Support/Daozai"
            legacy.mkdir(parents=True)
            self.assertEqual(sg.resolve_root(), legacy / "library")
        skill = self.base / "skill's $(no-shell)" / "scripts"
        skill.mkdir(parents=True)
        installed = skill / "shiguang.py"
        shutil.copyfile(SCRIPT, installed)
        bound_root = self.base / "bound library"
        (skill.parent / "config.json").write_text(json.dumps({"version": 1, "root": str(bound_root)}))
        process = subprocess.run([sys.executable, str(installed), "doctor"], capture_output=True, text=True,
                                 env={**os.environ, "QINGYU_DATA_DIR": str(dev)}, timeout=15)
        self.assertEqual(process.returncode, 0, process.stderr)
        self.assertEqual(json.loads(process.stdout)["root"], str(bound_root))
        explicit = subprocess.run([sys.executable, str(installed), "--root", str(self.root), "doctor"], capture_output=True, text=True, timeout=15)
        self.assertEqual(json.loads(explicit.stdout)["root"], str(self.root))

    def test_duplicate_json_keys_and_bounded_cli_input_fail(self):
        with self.assertRaises(sg.LibraryError):
            sg.decode_json(b'{"source_id":"a", "source_id":"b"}')
        process = subprocess.run([sys.executable, str(SCRIPT), "--root", str(self.root), "ingest"],
                                 input=" " * (sg.MAX_RECORD_BYTES + 1), capture_output=True, text=True, timeout=15)
        self.assertEqual(process.returncode, 1)
        self.assertIn("256 KiB", json.loads(process.stderr)["error"])
        self.assertFalse(self.root.exists())


if __name__ == "__main__":
    unittest.main()
