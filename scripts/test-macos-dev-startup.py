#!/usr/bin/env python3
"""Exercise the actual debug bootstrap with rebuilt, directly executed callers."""
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
RUSTC = Path.home() / ".cargo/bin/rustc"

HARNESS = r'''
#[path = "BOOTSTRAP_SOURCE"] mod dev_startup;
fn main() {
    if let Err(error) = dev_startup::prepare() { eprintln!("{error}"); std::process::exit(1); }
    let arguments = std::env::args_os().collect::<Vec<_>>();
    assert_eq!(arguments, ["./qingyu-direct", "--settings", "--mail-credential-check", "参数 包含空格"].map(std::ffi::OsString::from));
    assert_eq!(std::env::current_dir().unwrap().as_os_str(), std::env::var_os("EXPECTED_CWD").unwrap());
    assert_eq!(std::env::var_os("QINGYU_DATA_DIR").unwrap(), std::env::var_os("EXPECTED_DATA_DIR").unwrap());
    assert!(std::env::var_os("QINGYU_DEV_BOOTSTRAP_ATTEMPTED").is_none());
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).unwrap();
    assert_eq!(input, "synthetic stdin preserved across signing\n");
    println!("startup-ok-REVISION:{}", std::process::id());
}
'''


def run(args, **kwargs):
    result = subprocess.run(args, capture_output=True, text=True, timeout=30, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{Path(args[0]).name} exited {result.returncode}: {result.stderr.strip()}")
    return result


def signature(path):
    result = run(["/usr/bin/codesign", "-d", "-r-", "--verbose=4", str(path)])
    return result.stdout + result.stderr


def main():
    if sys.platform != "darwin":
        raise SystemExit("This test requires macOS and the prepared local development identity.")
    helper = Path.home() / "Library/Application Support/Qingyu Development/qingyu-mail-credentials"
    helper_before = hashlib.sha256(helper.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix="qingyu-direct-startup-") as temporary:
        folder = Path(temporary)
        objects = []
        for name in ("dev_startup", "mail_keychain"):
            obj = folder / f"{name}.o"
            run(["xcrun", "clang", "-c", "-fobjc-arc", "-O2", "-Wno-deprecated-declarations",
                 str(ROOT / f"src/macos/{name}.m"), "-o", str(obj)])
            objects.append(obj)
        executable, source = folder / "qingyu-direct", folder / "main.rs"
        environment = dict(os.environ, CARGO_MANIFEST_DIR=str(ROOT), EXPECTED_CWD=str(folder.resolve()),
                           QINGYU_DATA_DIR=str(folder / "data with spaces"), EXPECTED_DATA_DIR=str(folder / "data with spaces"))
        environment.pop("QINGYU_DEV_MAIL_HELPER", None)
        environment.pop("QINGYU_DEV_BOOTSTRAP_ATTEMPTED", None)
        arguments = ["./qingyu-direct", "--settings", "--mail-credential-check", "参数 包含空格"]

        def launch(extra=None, expected_revision=None):
            current_env = dict(environment, **(extra or {}))
            child = subprocess.Popen(arguments, cwd=folder, env=current_env, stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            stdout, stderr = child.communicate("synthetic stdin preserved across signing\n", timeout=20)
            if expected_revision is not None:
                assert child.returncode == 0, stderr
                assert stdout.strip() == f"startup-ok-{expected_revision}:{child.pid}", stdout
            return child.returncode, stdout, stderr

        ad_hoc_hashes, signed_hashes, signed_requirements = [], [], []
        for revision in (1, 2):
            # Actual code differs between builds; neither uses the Cargo runner.
            source.write_text(HARNESS.replace("BOOTSTRAP_SOURCE", str(ROOT / "src/dev_startup.rs")).replace("REVISION", str(revision)))
            command = [str(RUSTC), "--edition", "2021", str(source), "-o", str(executable)]
            for obj in objects: command += ["-C", f"link-arg={obj}"]
            command += ["-C", "link-arg=-framework", "-C", "link-arg=Foundation", "-C", "link-arg=-framework", "-C", "link-arg=Security"]
            run(command, env=environment)
            before = signature(executable)
            assert "# designated => cdhash" in before, before
            ad_hoc_hashes.append(hashlib.sha256(executable.read_bytes()).hexdigest())
            if revision == 1:
                code, stdout, stderr = launch({"QINGYU_DEV_BOOTSTRAP_ATTEMPTED": "1"})
                assert code == 1 and not stdout and "OSStatus" in stderr
                code, stdout, stderr = launch({"QINGYU_DEV_SIGNING_IDENTITY": "invalid"})
                assert code == 1 and not stdout and "签名准备失败" in stderr
            launch(expected_revision=revision)
            signed = signature(executable)
            requirement = next(line.removeprefix("designated => ") for line in signed.splitlines() if line.startswith("designated => "))
            assert 'identifier "com.qingyu.macos.debug" and certificate leaf' in requirement
            assert "cdhash" not in requirement
            signed_requirements.append(requirement)
            signed_hashes.append(hashlib.sha256(executable.read_bytes()).hexdigest())
            run(["/usr/bin/codesign", "--verify", "--strict", "-R", "=" + signed_requirements[0], str(executable)])
            signed_before = executable.stat().st_ino, hashlib.sha256(executable.read_bytes()).hexdigest()
            for _ in range(10):
                # This would fail if an already authenticated launch spawned
                # the signing runner again. The marker cannot bypass checks.
                launch({"QINGYU_DEV_SIGNING_IDENTITY": "invalid", "QINGYU_DEV_BOOTSTRAP_ATTEMPTED": "1"}, revision)
            assert signed_before == (executable.stat().st_ino, hashlib.sha256(executable.read_bytes()).hexdigest())
        assert ad_hoc_hashes[0] != ad_hoc_hashes[1]
        assert signed_hashes[0] != signed_hashes[1]
        assert signed_requirements[0] == signed_requirements[1], "changed-code builds must preserve the same TCC code requirement"
    assert helper_before == hashlib.sha256(helper.read_bytes()).hexdigest()
    print("PASS: changed-code direct bootstrap preserves the exact certificate/identifier requirement, argv/cwd/env/stdin/PID, stable fast path, and bounded fail-closed errors; helper unchanged.")


if __name__ == "__main__":
    main()
