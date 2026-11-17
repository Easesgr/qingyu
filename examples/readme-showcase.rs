//! Run with: cargo run --locked --features record --example readme-showcase
// Give this development process its own macOS identity without creating an app bundle.
#[cfg(target_os = "macos")]
#[used]
#[link_section = "__TEXT,__info_plist"]
static INFO_PLIST: [u8; include_bytes!("readme-showcase-info.plist").len()] =
    *include_bytes!("readme-showcase-info.plist");

#[cfg(debug_assertions)]
fn main() -> eframe::Result<()> { qingyu::showcase::run() }

#[cfg(not(debug_assertions))]
fn main() { eprintln!("README capture is only available in debug builds."); }
