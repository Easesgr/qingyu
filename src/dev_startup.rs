//! Direct debug execution must prepare the same identity as `cargo run` before
//! any GUI, singleton, stdin import, or mailbox operation is started.

use std::{ffi::CString, os::unix::{ffi::OsStrExt, process::CommandExt}, path::PathBuf, process::{Command, Stdio}};

extern "C" {
    fn qingyu_dev_startup_ready(helper_path: *const std::ffi::c_char) -> i32;
}

const ATTEMPTED: &str = "QINGYU_DEV_BOOTSTRAP_ATTEMPTED";

pub fn prepare() -> Result<(), String> {
    let helper = std::env::var_os("QINGYU_DEV_MAIL_HELPER").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support/Qingyu Development/qingyu-mail-credentials")))
        .ok_or("无法定位开发凭据助手，请确认 HOME 已设置。")?;
    let helper_path = CString::new(helper.as_os_str().as_bytes()).map_err(|_| "开发凭据助手路径无效。")?;
    let status = unsafe { qingyu_dev_startup_ready(helper_path.as_ptr()) };
    if status == 0 {
        // This marker can only stop a failed bootstrap; it never grants access.
        std::env::remove_var(ATTEMPTED);
        return Ok(());
    }
    if std::env::var_os(ATTEMPTED).is_some() {
        return Err(format!("开发版签名准备后仍未通过验证（OSStatus {status}，助手：{}）。请等待其他编译完成后重新启动；若仍失败，运行 bash scripts/macos-dev-runner.sh --prepare 检查本机签名身份。", helper.display()));
    }
    let executable = std::env::current_exe().map_err(|error| format!("无法定位当前开发程序：{error}"))?;
    let runner = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/macos-dev-runner.sh");
    if !runner.is_file() {
        return Err(format!("缺少开发签名脚本 {}，请从项目目录重新构建开发版。", runner.display()));
    }
    // The runner signs a copy and atomically replaces the path. Its stdin is
    // closed so it cannot consume a pending credential-import pipe.
    let prepared = Command::new("/bin/bash").arg(&runner).arg("--bootstrap").arg(&executable)
        .stdin(Stdio::null()).stdout(Stdio::null()).status()
        .map_err(|error| format!("无法准备开发版安全签名：{error}"))?;
    if !prepared.success() {
        return Err("开发版安全签名准备失败。请查看上方签名脚本的错误，确认登录钥匙串中保留 Qingyu Local Development 身份及其私钥后重试。".into());
    }
    let mut arguments = std::env::args_os();
    let original_argv0 = arguments.next();
    let mut replacement = Command::new(&executable);
    if let Some(argv0) = original_argv0 { replacement.arg0(argv0); }
    replacement.args(arguments).env(ATTEMPTED, "1");
    // exec preserves cwd, all existing environment variables, stdin/stdout,
    // arguments, PID, and the single-instance lifecycle. No workers exist yet.
    let error = replacement.exec();
    Err(format!("已准备签名，但无法重新执行开发程序：{error}"))
}
