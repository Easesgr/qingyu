#!/bin/bash
# Cargo's macOS runner: authenticate debug callers and keep the credential
# helper unchanged across app rebuilds. No application bundle is generated.
set -euo pipefail

if [ "$(uname -s)" != Darwin ]; then
    exec "$@"
fi

identity_name="Qingyu Local Development ($(id -u))"
identifier=com.qingyu.macos.debug
bootstrap=0
if [ "${1:-}" = --bootstrap ]; then bootstrap=1; fi
keychain="$(/usr/bin/security default-keychain -d user | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//')"

find_identity() {
    /usr/bin/security find-identity -p codesigning "$keychain" |
        awk -v name="$identity_name" 'index($0, "\"" name "\"") && $1 ~ /^[0-9]+\)$/ { print $2; exit }'
}

identity="${QINGYU_DEV_SIGNING_IDENTITY:-$(find_identity)}"
if [ -z "$identity" ]; then
    if [ "$bootstrap" -eq 1 ]; then
        echo "No installed Qingyu development signing identity. Run bash scripts/macos-dev-runner.sh --prepare to prepare it before starting the app." >&2
        exit 1
    fi
    if /usr/bin/security find-certificate -c "$identity_name" "$keychain" >/dev/null 2>&1; then
        echo "The Qingyu signing certificate exists, but its private key is unavailable. Restore the key before continuing." >&2
        exit 1
    fi
    # Only codesign can use this non-extractable key without another approval.
    # The temporary private key is owner-only and removed immediately on import.
    umask 077
    signing_tmp="$(mktemp -d "${TMPDIR:-/tmp}/qingyu-signing.XXXXXX")"
    trap 'rm -rf "$signing_tmp"' EXIT
    cat > "$signing_tmp/certificate.conf" <<CONFIG
[req]
distinguished_name = subject
x509_extensions = signing
prompt = no
[subject]
CN = $identity_name
[signing]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CONFIG
    /usr/bin/openssl req -new -x509 -newkey rsa:2048 -nodes -days 3650 \
        -config "$signing_tmp/certificate.conf" \
        -keyout "$signing_tmp/private.key" -out "$signing_tmp/certificate.pem" 2>/dev/null
    /usr/bin/security import "$signing_tmp/private.key" -k "$keychain" -t priv \
        -x -T /usr/bin/codesign >/dev/null
    /usr/bin/security import "$signing_tmp/certificate.pem" -k "$keychain" -t cert >/dev/null
    identity="$(find_identity)"
    rm -rf "$signing_tmp"
    trap - EXIT
    if [ -z "$identity" ]; then
        echo "Could not prepare the Qingyu development signing identity." >&2
        exit 1
    fi
fi

# A fingerprint unambiguously selects both the certificate and its private key.
# Never turn a display name or an ad hoc identity into an unchecked requirement.
if ! [[ "$identity" =~ ^[0-9a-fA-F]{40}$ ]]; then
    echo "QINGYU_DEV_SIGNING_IDENTITY must be a code-signing certificate's 40-character SHA-1 fingerprint." >&2
    exit 2
fi
identity="$(printf '%s' "$identity" | tr '[:lower:]' '[:upper:]')"
if ! /usr/bin/security find-identity -p codesigning "$keychain" |
    awk -v hash="$identity" '$2 == hash { found=1 } END { exit !found }'; then
    echo "The selected Qingyu signing certificate has no matching code-signing private key in the default keychain." >&2
    exit 1
fi

sign_executable() {
    local executable="$1" signing_identifier="$2" signed_tmp requirement original_digest
    requirement="identifier \"$signing_identifier\" and certificate leaf = H\"$identity\""
    if /usr/bin/codesign --verify --strict -R "=$requirement" "$executable" 2>/dev/null; then return; fi
    # Atomic replacement leaves any currently running image untouched.
    signed_tmp="$(mktemp "${executable}.dev-sign.XXXXXX")"
    trap 'rm -f "$signed_tmp"' EXIT
    cp -p "$executable" "$signed_tmp"
    original_digest="$(shasum -a 256 "$signed_tmp" | awk '{ print $1 }')"
    /usr/bin/codesign --force --sign "$identity" --keychain "$keychain" \
        --identifier "$signing_identifier" --timestamp=none "$signed_tmp"
    /usr/bin/codesign --verify --strict -R "=$requirement" "$signed_tmp"
    if [ "$(shasum -a 256 "$executable" | awk '{ print $1 }')" != "$original_digest" ]; then
        echo "The executable changed during signing. Wait for the other build to finish, then launch it again." >&2
        exit 1
    fi
    mv -f "$signed_tmp" "$executable"
    trap - EXIT
}

# Keep this outside target/ and outside the default Qingyu data directory:
# cargo clean must retain it, and preparing it must not suppress data migration.
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
helper_dir="${HOME}/Library/Application Support/Qingyu Development"
helper="$helper_dir/qingyu-mail-credentials"
helper_source="$repo_root/src/macos/mail_keychain.m"
umask 077
mkdir -p "$helper_dir"
vault="$helper_dir/qingyu-mail-vault-v1"
vault_requirement="identifier \"com.qingyu.macos.credentials.vault-v1\" and certificate leaf = H\"$identity\""
if [ ! -f "$vault" ]; then
    vault_tmp="$(mktemp -d "$helper_dir/.vault-install.XXXXXX")"
    xcrun clang -fobjc-arc -O2 -Wno-deprecated-declarations -mmacosx-version-min=12.0 \
        "$repo_root/src/macos/mail_vault_v1.m" -framework Foundation -framework Security \
        -o "$vault_tmp/vault"
    sign_executable "$vault_tmp/vault" com.qingyu.macos.credentials.vault-v1
    # Never update an installed vault for source/compiler/SDK changes. Its
    # exact code is the persistent Keychain owner; upgrades need a new version.
    # link(2) installs without replacement even if two launches prepare at once.
    if ! ln "$vault_tmp/vault" "$vault" 2>/dev/null && [ ! -f "$vault" ]; then
        echo "Could not install the persistent credential vault." >&2
        exit 1
    fi
    rm -rf "$vault_tmp"
fi
/usr/bin/codesign --verify --strict -R "=$vault_requirement" "$vault"
source_digest="$(cat "$helper_source" "$repo_root/src/macos/mail_credential_identity.h" "$repo_root/src/macos/mail_credential_protocol.h" "$repo_root/src/macos/mail_credential_bridge.m" | shasum -a 256 | awk '{ print $1 }')"
source_stamp="vault1:$source_digest:$identity:$(xcrun clang --version | head -1)"
if [ ! -f "$helper" ] || [ "$(cat "$helper_dir/source-stamp" 2>/dev/null || true)" != "$source_stamp" ]; then
    # Retain the exact legacy owners before replacing the bridge. Migration
    # can read their existing items without editing ACLs or exporting secrets.
    if ! [[ "$(cat "$helper_dir/source-stamp" 2>/dev/null || true)" = vault1:* ]]; then
        mkdir -p "$helper_dir/legacy-readers"
        for reader in "$helper.previous" "$helper"; do
            if [ -f "$reader" ]; then
                # Never mistake a prior bridge for a raw Keychain owner, even
                # if another interrupted installer left an older source stamp.
                if /usr/bin/grep -a -q -E 'mail-client-development|QINGYU_CREDENTIAL_BRIDGE_VAULT1' "$reader"; then continue; fi
                reader_hash="$(shasum -a 256 "$reader" | awk '{ print $1 }')"
                case "$reader" in *.previous) reader_order=0 ;; *) reader_order=1 ;; esac
                reader_copy="$helper_dir/legacy-readers/reader-$reader_order-$reader_hash"
                if [ ! -f "$reader_copy" ]; then cp -p "$reader" "$reader_copy"; fi
            fi
        done
    fi
    helper_tmp="$(mktemp -d "$helper_dir/.build.XXXXXX")"
    trap 'rm -rf "$helper_tmp"' EXIT
    xcrun clang -fobjc-arc -fblocks -O2 -Wno-deprecated-declarations \
        -mmacosx-version-min=12.0 -DQINGYU_DEV_KEYCHAIN_HELPER=1 \
        "$helper_source" -framework Foundation -framework Security \
        -o "$helper_tmp/qingyu-mail-credentials"
    sign_executable "$helper_tmp/qingyu-mail-credentials" com.qingyu.macos.debug.credentials
    xcrun clang -fobjc-arc -fblocks -O2 -Wno-deprecated-declarations \
        -mmacosx-version-min=12.0 -DQINGYU_CREDENTIAL_BROKER=1 \
        "$helper_source" -framework Foundation -framework Security \
        -o "$helper_tmp/mail-client-development"
    cp -p "$helper_tmp/mail-client-development" "$helper_tmp/mail-client-installed"
    sign_executable "$helper_tmp/mail-client-development" com.qingyu.macos.debug
    sign_executable "$helper_tmp/mail-client-installed" com.qingyu.macos
    mv -f "$helper_tmp/mail-client-development" "$helper_dir/mail-client-development"
    mv -f "$helper_tmp/mail-client-installed" "$helper_dir/mail-client-installed"
    if [ -f "$helper" ]; then cp -p "$helper" "$helper_dir/qingyu-mail-credentials.previous"; fi
    mv -f "$helper_tmp/qingyu-mail-credentials" "$helper"
    printf '%s' "$source_stamp" > "$helper_dir/source-stamp"
    rm -rf "$helper_tmp"
    trap - EXIT
    if ! QINGYU_DEV_ACCOUNT_FILE="$repo_root/.dev-data/mail-accounts.json" "$helper" --migrate-existing >&2; then
        echo "Some existing credentials remain protected. They were preserved; use the mailbox Test action once to copy the authorized current value into the persistent vault." >&2
    fi
fi
sign_executable "$helper" com.qingyu.macos.debug.credentials
export QINGYU_DEV_MAIL_HELPER="$helper"

if [ "${1:-}" = --prepare ]; then
    echo "Qingyu development signing identity and credential helper are ready."
    exit 0
fi
if [ "${1:-}" = --helper-path ]; then printf '%s\n' "$helper"; exit 0; fi
if [ "${1:-}" = --signing-identity ]; then printf '%s\n' "$identity"; exit 0; fi

sign_only=0
if [ "${1:-}" = --sign-only ] || [ "${1:-}" = --bootstrap ]; then sign_only=1; shift; fi
if [ "$#" -eq 0 ]; then
    echo "Usage: $0 [--prepare | --helper-path | --signing-identity | --sign-only EXECUTABLE | --bootstrap EXECUTABLE | EXECUTABLE [ARG...]]" >&2
    exit 2
fi

executable="$1"
shift
if [ ! -f "$executable" ]; then
    echo "Missing executable: $executable" >&2
    exit 1
fi

sign_executable "$executable" "$identifier"

if [ "$sign_only" -eq 0 ]; then exec "$executable" "$@"; fi
