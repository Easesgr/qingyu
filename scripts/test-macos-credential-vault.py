#!/usr/bin/env python3
"""Verify persistent ownership, legacy migration and exact namespace priority."""
import hashlib
import importlib.util
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import uuid

ROOT = Path(__file__).resolve().parents[1]


def run(args, expected=0, **kwargs):
    result = subprocess.run([str(a) for a in args], capture_output=True, text=True, timeout=30, **kwargs)
    if result.returncode != expected:
        raise RuntimeError(f"{Path(args[0]).name} exited {result.returncode}: {result.stderr.strip()}")
    return result


OWNER = r'''
#include <Security/Security.h>
#include <stdio.h>
#include <string.h>
int main(int argc,char **argv) {
    if(argc!=4)return 2;SecKeychainSetUserInteractionAllowed(false);
    const char *service=argv[2],*account=argv[3],*secret="protected-newer-synthetic";
    SecKeychainItemRef item=NULL;OSStatus s=SecKeychainFindGenericPassword(NULL,(UInt32)strlen(service),service,(UInt32)strlen(account),account,NULL,NULL,&item);
    if(!strcmp(argv[1],"exists")){if(item)CFRelease(item);return s==errSecItemNotFound?3:s?4:0;}
    if(!strcmp(argv[1],"save")) {
        if(s==errSecItemNotFound)s=SecKeychainAddGenericPassword(NULL,(UInt32)strlen(service),service,(UInt32)strlen(account),account,(UInt32)strlen(secret),secret,NULL);
        else return 5;
    } else if(!strcmp(argv[1],"delete")) {if(!s)s=SecKeychainItemDelete(item);if(s==errSecItemNotFound)s=0;}
    if(item)CFRelease(item);if(s)fprintf(stderr,"OSStatus=%d\n",(int)s);return s?6:0;
}
'''


def main():
    helper = Path.home() / "Library/Application Support/Qingyu Development/qingyu-mail-credentials"
    directory = helper.parent
    vault = directory / "qingyu-mail-vault-v1"
    before = (vault.stat().st_ino, hashlib.sha256(vault.read_bytes()).hexdigest())
    signature = run(["codesign", "-d", "-r-", str(helper)])
    fingerprint = re.search(r'certificate leaf = H"([0-9a-f]+)"', signature.stdout + signature.stderr).group(1)
    spec = importlib.util.spec_from_file_location("client_template", ROOT / "scripts/test-macos-dev-keychain.py")
    template = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(template)
    source = template.SOURCE.replace('!strcmp(argv[1], "save") ? 2', '!strncmp(argv[1], "save", 4) ? 2')
    source = source.replace('action == 4 ? "synthetic-stale-secret" : "synthetic-test-secret"', 'strstr(argv[1], "newer") ? "synthetic-newer-secret" : action == 4 ? "synthetic-stale-secret" : "synthetic-test-secret"')
    accounts = ["vault-regression-" + uuid.uuid4().hex + "@example.invalid" for _ in range(4)]
    candidates = []
    with tempfile.TemporaryDirectory(prefix="qingyu-vault-regression-") as temporary:
        folder = Path(temporary)
        client_source = folder / "client.c"
        client_source.write_text(source)
        debug, installed, owner = (folder / name for name in ("development", "installed", "unrelated"))
        run(["xcrun", "clang", "-fobjc-arc", "-O2", "-Wno-deprecated-declarations", "-DREVISION=1", client_source,
             ROOT / "src/macos/mail_keychain.m", "-framework", "Foundation", "-framework", "Security", "-o", debug])
        shutil.copy2(debug, installed)
        owner_source = folder / "owner.c"
        owner_source.write_text(OWNER)
        run(["xcrun", "clang", "-Wno-deprecated-declarations", owner_source, "-framework", "Security", "-framework", "CoreFoundation", "-o", owner])
        for binary, identifier in [(debug, "com.qingyu.macos.debug"), (installed, "com.qingyu.macos"), (owner, "com.qingyu.macos.unrelated")]:
            run(["codesign", "--force", "--sign", fingerprint, "--identifier", identifier, "--timestamp=none", binary])

        def request(client, operation, account, bridge=helper, expected=0):
            return run([client, operation, account, bridge], expected=expected)

        legacy = sorted((directory / "legacy-readers").glob("reader-1-*"))[-1]
        debug_only = sorted((directory / "legacy-readers").glob("reader-0-*"))[0]
        try:
            # The exact legacy owner creates different Installed/Development
            # passwords. Migration must preserve the newer Installed value.
            request(debug, "save", accounts[0], legacy)
            request(installed, "save-newer", accounts[0], legacy)
            request(installed, "read-newer", accounts[0], debug_only, expected=77)
            request(debug, "read", accounts[0])
            request(installed, "read-newer", accounts[0])
            request(installed, "import-other", accounts[0])
            request(installed, "read-newer", accounts[0])

            # A denied existing Installed value cannot fall back to an already
            # readable, older Development vault value.
            request(debug, "save", accounts[1])
            run([owner, "save", "Qingyu.Mail.Installed", accounts[1]])
            request(installed, "read", accounts[1], expected=77)
            request(debug, "read", accounts[1])
            run([owner, "exists", "Qingyu.Mail.Vault.v1.Installed", accounts[1]], expected=3)

            # A denied existing Development value cannot copy an original
            # service over it or materialize a falsely successful vault entry.
            run([owner, "save", "Qingyu.Mail.Development", accounts[2]])
            run([owner, "save", "Qingyu.Mail", accounts[2]])
            request(debug, "read", accounts[2], expected=77)
            run([owner, "exists", "Qingyu.Mail.Vault.v1.Development", accounts[2]], expected=3)

            # Fresh one-shot processes read through two genuinely different
            # bridge code generations while vault code/inode remain fixed.
            for revision in (1, 2):
                candidate = directory / (".vault-regression-" + uuid.uuid4().hex)
                candidates.append(candidate)
                candidate_source = folder / f"bridge-{revision}.m"
                candidate_source.write_text(f'#define QINGYU_DEV_KEYCHAIN_HELPER 1\n#include "{ROOT}/src/macos/mail_keychain.m"\n__attribute__((used)) const char regression_generation[]="generation-{revision}";\n')
                run(["xcrun", "clang", "-fobjc-arc", "-O2", "-Wno-deprecated-declarations", candidate_source, "-framework", "Foundation", "-framework", "Security", "-o", candidate])
                run(["codesign", "--force", "--sign", fingerprint, "--identifier", "com.qingyu.macos.debug.credentials", "--timestamp=none", candidate])
                for _ in range(6):
                    request(debug, "read", accounts[0], candidate)
                    request(installed, "read-newer", accounts[0], candidate)
                denied = subprocess.run([str(candidate)], input=b"", capture_output=True, timeout=10)
                assert denied.returncode == 77 and not denied.stdout
            assert hashlib.sha256(candidates[0].read_bytes()).digest() != hashlib.sha256(candidates[1].read_bytes()).digest()
            assert before == (vault.stat().st_ino, hashlib.sha256(vault.read_bytes()).hexdigest())
        finally:
            # Only disposable accounts created in this test are removed.
            cleanup_errors = []
            for account in accounts:
                # Remove synthetic protected items through their actual owner
                # first; never ask a legacy reader to authenticate/delete them.
                for service in ("Qingyu.Mail.Installed", "Qingyu.Mail.Development", "Qingyu.Mail"):
                    if account != accounts[0]:
                        try: run([owner, "delete", service, account])
                        except Exception as error: cleanup_errors.append(str(error))
                for client in (debug, installed):
                    try: request(client, "delete", account)
                    except Exception as error: cleanup_errors.append(str(error))
                    if account == accounts[0]:
                        try: request(client, "delete", account, legacy)
                        except Exception as error: cleanup_errors.append(str(error))
            for candidate in candidates:
                candidate.unlink(missing_ok=True)
            if cleanup_errors:
                print("Disposable cleanup errors: " + "; ".join(cleanup_errors), file=__import__('sys').stderr)
    print("PASS: immutable vault across bridge updates/fresh processes; exact Installed/Development migration; denied newer values never fall back; add-only copies; unrelated callers rejected; disposable cleanup.")


if __name__ == "__main__":
    main()
