#!/usr/bin/env python3
"""Check rebuild identity and ACL isolation with a disposable synthetic item."""
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]

SOURCE = r'''
#include <Security/Security.h>
#include <arpa/inet.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
extern int32_t qingyu_mail_verify_dev_helper(const char *);
extern int32_t qingyu_mail_verify_dev_helper_process(int32_t);
volatile int build_revision = REVISION;
int main(int argc, char **argv) {
    signal(SIGPIPE, SIG_IGN);
    if (argc != 4 || build_revision == 0) return 2;
    int32_t checked=qingyu_mail_verify_dev_helper(argv[3]);
    if (checked) { fprintf(stderr,"Helper file verification failed (%d)\n",checked); return 77; }
    unsigned char action = !strcmp(argv[1], "save") ? 2 : !strcmp(argv[1], "delete") ? 3 : !strcmp(argv[1], "import-other") ? 4 : 0;
    const char *account = argv[2], *secret = action == 4 ? "synthetic-stale-secret" : "synthetic-test-secret";
    uint32_t lengths[] = { htonl((uint32_t)strlen(account)), htonl(action == 2 || action == 4 ? (uint32_t)strlen(secret) : 0) };
    int input[2], output[2], started[2]; if (pipe(input) || pipe(output) || pipe(started)) return 3;
    fcntl(started[1], F_SETFD, FD_CLOEXEC);
    pid_t child = fork();
    if (!child) {
        close(started[0]);
        dup2(input[0], 0); dup2(output[1], 1);
        close(input[0]); close(input[1]); close(output[0]); close(output[1]);
        execl(argv[3], argv[3], NULL); _exit(127);
    }
    close(started[1]); char ready; read(started[0],&ready,1); close(started[0]);
    close(input[0]); close(output[1]);
    checked=child<0?-1:qingyu_mail_verify_dev_helper_process(child);
    if (checked) { fprintf(stderr,"Helper process verification failed (%d)\n",checked);if(child>0)kill(child,SIGKILL);return 77; }
    FILE *to = fdopen(input[1], "wb"), *from = fdopen(output[0], "rb");
    fwrite(&action,1,1,to);fwrite(lengths,1,sizeof(lengths),to);fwrite(account,1,strlen(account),to);
    if (action == 2 || action == 4) fwrite(secret,1,strlen(secret),to);
    fclose(to);
    uint32_t response[2]; size_t got=fread(response,1,sizeof(response),from);
    int result=0;
    if(got!=sizeof(response))result=77;
    else {
        int32_t status=(int32_t)ntohl(response[0]);uint32_t length=ntohl(response[1]);
        if(status) { fprintf(stderr,"Keychain OSStatus=%d\n",status);result=77; }
        else if(action==0) {
            char password[128];
            if(length!=strlen(secret)||length>sizeof(password)||fread(password,1,length,from)!=length||memcmp(password,secret,length))result=4;
        }
    }
    fclose(from);int status;waitpid(child,&status,0);
    return result;
}
'''


def run(*args, **kwargs):
    result = subprocess.run(args, capture_output=True, text=True, timeout=30, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{pathlib.Path(args[0]).name} exited {result.returncode}: {result.stderr.strip()}")
    return result


def signature(binary):
    result = run("/usr/bin/codesign", "-d", "-r-", "--verbose=4", str(binary))
    detail = result.stdout + result.stderr
    requirement = next(line for line in detail.splitlines() if line.startswith("designated =>"))
    codehash = re.search(r"^CDHash=(\w+)$", detail, re.MULTILINE).group(1)
    return requirement, codehash


def main():
    if sys.platform != "darwin":
        raise SystemExit("This test requires macOS and an unlocked development keychain.")
    account = "qingyu-signing-test-" + uuid.uuid4().hex + "@example.invalid"
    helper = pathlib.Path(run("bash", str(ROOT / "scripts/macos-dev-runner.sh"), "--helper-path").stdout.strip())
    helper_before = signature(helper)
    with tempfile.TemporaryDirectory(prefix="qingyu-keychain-regression-") as temporary:
        folder = pathlib.Path(temporary)
        source = folder / "credential.c"
        source.write_text(SOURCE)
        first, rebuilt, untrusted = (folder / name for name in ("first", "rebuilt", "untrusted"))
        for revision, binary in enumerate((first, rebuilt, untrusted), 1):
            run("xcrun", "clang", "-Wno-deprecated-declarations", "-fobjc-arc", "-O2", "-DREVISION=" + str(revision),
                str(source), str(ROOT / "src/macos/mail_keychain.m"), "-framework", "Security", "-framework", "Foundation", "-o", str(binary))
        for binary in (first, rebuilt):
            run("bash", str(ROOT / "scripts/macos-dev-runner.sh"), "--sign-only", str(binary))
        first_requirement, first_hash = signature(first)
        rebuilt_requirement, rebuilt_hash = signature(rebuilt)
        assert first_requirement == rebuilt_requirement, "Rebuild changed the trusted requirement"
        assert first_hash != rebuilt_hash, "Test binaries must contain different code"
        assert "certificate leaf" in first_requirement and "cdhash" not in first_requirement
        run("/usr/bin/codesign", "--force", "--sign", "-", "--identifier", "com.qingyu.macos.debug", str(untrusted))
        current, replacement = folder / "qingyu", folder / "replacement"
        shutil.copy2(first, current)
        run(str(current), "save", account, str(helper))
        try:
            run(str(current), "read", account, str(helper))
            run(str(current), "import-other", account, str(helper))
            run(str(current), "read", account, str(helper))
            shutil.copy2(rebuilt, replacement)
            replacement.replace(current)
            run(str(current), "read", account, str(helper))
            # Direct exec is the path used by in-app restart as well.
            run(str(current), "read", account, str(helper))
            denied = subprocess.run((str(untrusted), "read", account, str(helper)), capture_output=True, text=True, timeout=5)
            assert denied.returncode == 77, "An unsigned caller must not gain access: " + denied.stderr
            fingerprint = re.search(r'certificate leaf = H"([0-9a-f]+)"', first_requirement).group(1)
            run("/usr/bin/codesign", "--force", "--sign", fingerprint, "--identifier", "com.qingyu.macos.unrelated", str(untrusted))
            denied = subprocess.run((str(untrusted), "read", account, str(helper)), capture_output=True, text=True, timeout=5)
            assert denied.returncode == 77, "Another identifier must not gain access: " + denied.stderr
            # Invoking the helper directly from an unauthenticated process must
            # fail before it consumes the request or returns any credential.
            denied = subprocess.run((str(helper),), input=b"", capture_output=True, timeout=5)
            assert denied.returncode == 77 and not denied.stdout
            fake_helper = folder / "untrusted-helper"
            shutil.copy2(helper, fake_helper)
            run("/usr/bin/codesign", "--force", "--sign", "-", "--identifier", "com.qingyu.macos.debug.credentials", str(fake_helper))
            denied = subprocess.run((str(current), "read", account, str(fake_helper)), capture_output=True, text=True, timeout=5)
            assert denied.returncode == 77, "The client must reject an untrusted helper before transmitting data"
            run("bash", str(ROOT / "scripts/macos-dev-runner.sh"), "--sign-only", str(rebuilt))
            assert signature(rebuilt) == (rebuilt_requirement, rebuilt_hash)
            assert signature(helper) == helper_before, "An app rebuild must not change the Keychain helper"
        finally:
            run(str(current), "delete", account, str(helper))
    print("PASS: rebuild/restart access, add-only import, mutual signature verification, stable helper, and synthetic cleanup.")


if __name__ == "__main__":
    main()
