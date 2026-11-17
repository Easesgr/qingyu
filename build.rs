//! 把轻屿图标作为 RT_GROUP_ICON #1 编进 exe，供 LoadImageW(hInst, 1, ...) 取用。
//! 不引入额外依赖：直接调用 Windows SDK 的 rc.exe，把 .res 交给链接器。

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn find_rc() -> Option<PathBuf> {
    let roots = [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ];
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else { continue };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let rc = dir.join("x64").join("rc.exe");
            if rc.exists() {
                found.push(rc);
            }
        }
    }
    found.sort();
    found.pop()
}

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/qingyu/app-icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_macos();
        return;
    }
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("qingyu.res");
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe 未找到，跳过图标资源");
        return;
    };
    let status = Command::new(&rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&out)
        .arg("app.rc")
        .status();
    if !status.map(|s| s.success()).unwrap_or(false) {
        println!("cargo:warning=rc.exe 编译 app.rc 失败");
        return;
    }
    println!("cargo:rustc-link-arg={}", out.display());
}

fn build_macos() {
    for source in ["src/macos/mail_credential_identity.h", "src/macos/mail_credential_protocol.h", "src/macos/mail_credential_bridge.m"] {
        println!("cargo:rerun-if-changed={source}");
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        other => panic!("Unsupported macOS architecture: {other:?}"),
    };
    let mut objects = Vec::new();
    for (source, name) in [("src/macos/native.m", "macos-native"), ("src/macos/notifications.m", "macos-notifications"), ("src/macos/agent_open.m", "macos-agent-open"), ("src/mac_services.m", "macos-services"), ("src/macos/qq_music.m", "macos-qq-music"), ("src/macos/stats.m", "macos-stats"), ("src/macos/mail_keychain.m", "macos-mail-keychain"), ("src/macos/dev_startup.m", "macos-dev-startup")] {
        println!("cargo:rerun-if-changed={source}");
        let object = out.join(format!("{name}.o"));
        let status = Command::new("xcrun").args(["--sdk", "macosx", "clang", "-c", "-fobjc-arc", "-fblocks", "-O2",
            "-mmacosx-version-min=12.0", "-arch", arch, source, "-o"]).arg(&object).status().expect("Xcode Command Line Tools are required");
        assert!(status.success(), "Could not compile {source}");
        objects.push(object);
    }
    let library = out.join("libqingyu_macos.a");
    let status = Command::new("xcrun").args(["libtool", "-static", "-o"]).arg(&library).args(&objects).status().unwrap();
    assert!(status.success(), "Could not archive macOS bridge");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=qingyu_macos");
    // Objective-C availability checks use Clang's platform-version helper.
    // Rust links with -nodefaultlibs, so release builds targeting older macOS
    // need the compiler runtime that clang would normally add itself.
    let runtime = Command::new("xcrun").args(["--sdk", "macosx", "clang", "--print-file-name=libclang_rt.osx.a"])
        .output().expect("Could not locate the macOS Clang runtime");
    assert!(runtime.status.success(), "Could not locate the macOS Clang runtime");
    let runtime = PathBuf::from(String::from_utf8(runtime.stdout).expect("Invalid Clang runtime path").trim());
    assert!(runtime.is_file(), "Missing macOS Clang runtime: {}", runtime.display());
    println!("cargo:rustc-link-search=native={}", runtime.parent().unwrap().display());
    println!("cargo:rustc-link-lib=static=clang_rt.osx");
    for framework in ["AppKit", "ApplicationServices", "Foundation", "CoreAudio", "CoreGraphics", "Security", "SystemConfiguration", "QuartzCore", "UniformTypeIdentifiers", "UserNotifications", "WebKit", "IOKit"] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }
}
