# Maccy clipboard integration

Upstream: https://github.com/p0deje/Maccy  
Pinned revision: `f0b571e32e4031d7b91ba811f50d0f33e8e5548b`  
License: MIT, Copyright (c) 2025 Alex Rodionov. Full terms are in [LICENSE](LICENSE).

`upstream/` preserves the source files used for this adaptation. `upstream.json` records their SHA-256 checksums. These files are reference source. Qingyu implements the clipboard policy in Rust, following the upstream design; the Swift application is not built as a dependency.

The Rust implementation follows Maccy's pasteboard type handling, sensitive-content filtering, item collection and file URL replay principles. Qingyu owns the Rust UI, legacy history migration, bounded local media copies, background import/cleanup, and active replay leases. The Objective-C bridge only reads metadata or requested bytes and writes payloads through macOS APIs; content policy remains in Rust. Maccy's SwiftUI application, SwiftData store and global shortcut manager are outside this integration.

The clipboard policy is implemented in `src/clipboard/maccy.rs`; history/cache handling is in `src/clipboard.rs`. The native bridge and private-pasteboard tests are in `src/mac_services.{m,rs}`. The application supports macOS 12 and later without a separately installed Maccy application.
