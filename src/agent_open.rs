//! Open the existing application which owns an observed agent conversation.
//!
//! Call `open_observed` from a background command worker: native process
//! inspection and application activation have bounded waits. No command line,
//! environment, transcript, prompt, or reply is read, and no CLI is launched.
use crate::agent_monitor::AgentKind;
use serde::{Deserialize, Serialize};
use std::path::Path;

const CODEX: &str = "com.openai.codex";
const CLAUDE: &str = "com.anthropic.claudefordesktop";

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
struct Process {
    pid: u32,
    parent: u32,
    /// Microsecond process birth stamp, rechecked immediately before activation.
    start_stamp: u64,
    executable: String,
    name: String,
    bundle_id: String,
    app_path: String,
    regular: bool,
    tty: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
struct Application {
    pid: u32,
    start_stamp: u64,
    bundle_id: String,
    app_path: String,
}

#[derive(Default, Deserialize)]
struct Inspection {
    #[serde(default)] chain: Vec<Process>,
    #[serde(default)] running: Vec<Application>,
    #[serde(default)] installed: Vec<Application>,
}

#[derive(Debug, Serialize)]
struct Action {
    kind: &'static str,
    application: Application,
    source: Option<Process>,
    session_id: String,
    url: Option<String>,
    tty: String,
}

/// Compatibility entry point when no observation timestamp is available.
pub fn open(agent: AgentKind, session_id: &str, pid: Option<u32>) -> Result<(), String> {
    open_observed(agent, session_id, pid, None)
}

/// Activate the exact verified owner, or a known agent GUI fallback. A process
/// born after the supplied last session event cannot be that event's owner.
pub fn open_observed(agent: AgentKind, session_id: &str, pid: Option<u32>, observed_at_ms: Option<u64>) -> Result<(), String> {
    let session_id = session_identity(agent, session_id)?;
    if pid == Some(0) || pid.is_some_and(|pid| pid > i32::MAX as u32) { return Err("无效的会话进程编号".into()); }
    #[cfg(target_os = "macos")]
    {
        let inspection = native::inspect(pid.unwrap_or(0))?;
        let action = select(agent, &session_id, pid, observed_at_ms, &inspection)?;
        native::perform(&action)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (session_id, observed_at_ms);
        Err("当前平台尚不支持定位 Agent 应用窗口".into())
    }
}

fn session_identity(agent: AgentKind, id: &str) -> Result<String, String> {
    if id.len() > 256 || id.chars().any(char::is_control) { return Err("无效的会话编号".into()); }
    let expected = match agent {
        AgentKind::Codex => "codex", AgentKind::Claude => "claude", AgentKind::Hermes => "hermes",
        AgentKind::Gemini => "gemini", AgentKind::OpenCode => "opencode", AgentKind::Cursor => "cursor-agent", AgentKind::Aider => "aider",
    };
    if let Some((prefix, raw)) = id.split_once(':') {
        let prefix = match prefix { "claude-code" => "claude", "gemini-cli" => "gemini", prefix => prefix };
        if prefix != expected { return Err("会话编号与 Agent 应用不匹配".into()); }
        Ok(raw.into())
    } else { Ok(id.into()) }
}

fn codex_url(id: &str) -> Option<String> {
    // Verified against the installed app's existing-conversation parser in
    // .vite/build/early-bootstrap.js: host "threads", one UUID path segment.
    // Never use /new, prompt, a query, or the claude-cli task-creation handler.
    (id.len() == 36 && id.bytes().enumerate().all(|(i, byte)|
        if [8, 13, 18, 23].contains(&i) { byte == b'-' } else { byte.is_ascii_hexdigit() }))
        .then(|| format!("codex://threads/{}", id.to_ascii_lowercase()))
}

fn gui_bundle(agent: AgentKind) -> Option<&'static str> {
    match agent { AgentKind::Codex => Some(CODEX), AgentKind::Claude => Some(CLAUDE), _ => None }
}

fn application(process: &Process) -> Application {
    Application { pid: process.pid, start_stamp: process.start_stamp, bundle_id: process.bundle_id.clone(), app_path: process.app_path.clone() }
}

fn known_gui(agent: AgentKind, environment: &Inspection, allow_install: bool) -> Result<Option<Application>, String> {
    let Some(bundle) = gui_bundle(agent) else { return Ok(None) };
    let running: Vec<_> = environment.running.iter().filter(|app| app.bundle_id == bundle).collect();
    if running.len() == 1 { return Ok(Some(running[0].clone())); }
    if running.len() > 1 {
        // Two Claude variants can have the same bundle ID. Only verified PID
        // ancestry can choose which instance owns a particular CLI session.
        return Err(format!("有多个 {} 应用正在运行，无法确认此会话所属窗口", agent.label()));
    }
    Ok(allow_install.then(|| environment.installed.iter().find(|app| app.bundle_id == bundle).cloned()).flatten())
}

fn is_agent_process(agent: AgentKind, process: &Process) -> bool {
    if gui_bundle(agent) == Some(process.bundle_id.as_str()) { return true; }
    let filename = Path::new(&process.executable).file_name().and_then(|name| name.to_str()).unwrap_or("");
    let filename = filename.strip_suffix(".exe").unwrap_or(filename);
    let expected = match agent {
        AgentKind::Codex => "codex", AgentKind::Claude => "claude", AgentKind::Hermes => "hermes",
        AgentKind::Gemini => "gemini", AgentKind::OpenCode => "opencode", AgentKind::Cursor => "cursor-agent", AgentKind::Aider => "aider",
    };
    filename == expected || agent == AgentKind::Claude && process.executable.contains("/.local/share/claude/versions/")
        && !filename.is_empty() && filename.bytes().all(|byte| byte.is_ascii_digit() || byte == b'.' || byte == b'-')
}

fn is_terminal_or_editor(bundle: &str) -> bool {
    matches!(bundle, "com.apple.Terminal" | "com.googlecode.iterm2" | "dev.warp.Warp-Stable" | "dev.warp.Warp"
        | "com.mitchellh.ghostty" | "net.kovidgoyal.kitty" | "org.alacritty" | "org.wezfurlong.wezterm"
        | "com.microsoft.VSCode" | "com.microsoft.VSCodeInsiders" | "com.todesktop.230313mzl4w4u92"
        | "dev.zed.Zed" | "com.exafunction.windsurf" | "com.visualstudio.code.oss")
}

fn verified_owner(agent: AgentKind, pid: u32, observed: Option<u64>, environment: &Inspection) -> Result<Option<(Process, Process)>, String> {
    let Some(source) = environment.chain.first().filter(|source| source.pid == pid) else { return Ok(None) };
    if source.start_stamp == 0 || observed.filter(|at| *at > 0).is_some_and(|at| source.start_stamp / 1000 > at.saturating_add(1_500)) {
        return Err("原会话进程已退出或编号已被复用，无法安全定位原窗口".into());
    }
    let own_gui = gui_bundle(agent);
    let source_verified = is_agent_process(agent, source);
    // Generic node/python processes do not prove agent identity. They may only
    // select the requested GUI itself, never an unrelated terminal or editor.
    for process in &environment.chain {
        if !process.regular { continue; }
        let own = own_gui == Some(process.bundle_id.as_str());
        if own || source_verified && is_terminal_or_editor(&process.bundle_id) {
            return Ok(Some((source.clone(), process.clone())));
        }
        return Err("无法核验此会话所属的应用窗口".into());
    }
    if !source_verified { return Err("无法核验此会话的当前进程身份".into()); }
    Err("原会话仍在运行，但无法定位它所属的应用或终端窗口".into())
}

fn select(agent: AgentKind, id: &str, pid: Option<u32>, observed: Option<u64>, environment: &Inspection) -> Result<Action, String> {
    if agent == AgentKind::Codex {
        if let Some(app) = known_gui(agent, environment, true)? {
            if let Some(url) = codex_url(id) {
                // An exact existing-thread route is independent of a child CLI's
                // lifetime; explicitly address the verified Codex bundle.
                return Ok(Action { kind: "url", application: app, source: None, session_id: id.into(), url: Some(url), tty: String::new() });
            }
            return Ok(Action { kind: if app.pid == 0 { "launch" } else { "activate" }, application: app, source: None,
                session_id: id.into(), url: None, tty: String::new() });
        }
    }
    if let Some(pid) = pid {
        if let Some((source, owner)) = verified_owner(agent, pid, observed, environment)? {
            let tty = source.tty.clone();
            return Ok(Action { kind: "activate", application: application(&owner), source: Some(source), session_id: id.into(), url: None, tty });
        }
    }
    if let Some(app) = known_gui(agent, environment, true)? {
        return Ok(Action { kind: if app.pid == 0 { "launch" } else { "activate" }, application: app, source: None,
            session_id: id.into(), url: None, tty: String::new() });
    }
    Err(format!("未找到此会话所属的 {} 应用或现有终端窗口", agent.label()))
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::ffi::{c_char, CStr, CString};
    extern "C" {
        fn qy_agent_open_inspect(pid: u32) -> *mut c_char;
        fn qy_agent_open_perform(json: *const c_char) -> *mut c_char;
        fn qy_agent_open_free(value: *mut c_char);
    }
    fn returned(pointer: *mut c_char) -> Result<String, String> {
        if pointer.is_null() { return Err("无法读取应用窗口信息".into()); }
        let value = unsafe { CStr::from_ptr(pointer).to_string_lossy().into_owned() };
        unsafe { qy_agent_open_free(pointer); }
        Ok(value)
    }
    pub(super) fn inspect(pid: u32) -> Result<Inspection, String> {
        let value = returned(unsafe { qy_agent_open_inspect(pid) })?;
        serde_json::from_str(&value).map_err(|_| "无法读取应用身份信息".into())
    }
    pub(super) fn perform(action: &Action) -> Result<(), String> {
        let value = CString::new(serde_json::to_vec(action).map_err(|_| "无法准备应用窗口操作")?).map_err(|_| "无效的应用窗口操作")?;
        let error = returned(unsafe { qy_agent_open_perform(value.as_ptr()) })?;
        if error.is_empty() { Ok(()) } else { Err(error) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "11111111-2222-4333-8444-555555555555";
    fn app(bundle: &str, pid: u32, path: &str) -> Application { Application { pid, start_stamp: 1_000_000, bundle_id: bundle.into(), app_path: path.into() } }
    fn cli(agent: &str) -> Process { Process { pid: 20, parent: 21, start_stamp: 1_000_000, executable: format!("/usr/local/bin/{agent}"), name: agent.into(), tty: "/dev/ttys003".into(), ..Default::default() } }
    fn owner(bundle: &str, path: &str) -> Process { Process { pid: 22, start_stamp: 500_000, bundle_id: bundle.into(), app_path: path.into(), regular: true, ..Default::default() } }

    #[test]
    fn only_exact_codex_uuid_can_create_an_existing_conversation_url() {
        assert_eq!(codex_url(ID).unwrap(), format!("codex://threads/{ID}"));
        for id in ["new", "threads/new", "", "../new", "11111111-2222-4333-8444-555555555555?prompt=hello", "11111111-2222-4333-8444-555555555555/new", "11111111-2222-4333-8444-55555555555g"] {
            assert!(codex_url(id).is_none(), "{id}");
        }
        assert_eq!(session_identity(AgentKind::Codex, &format!("codex:{ID}")).unwrap(), ID);
        assert!(session_identity(AgentKind::Claude, &format!("codex:{ID}")).is_err());
        assert!(session_identity(AgentKind::Claude, "claude:bad\0id").is_err());
        assert_eq!(session_identity(AgentKind::Claude, "claude-code:one").unwrap(), "one");
    }

    #[test]
    fn codex_routes_to_actual_bundle_even_when_installed_app_is_named_chatgpt() {
        let environment = Inspection { running: vec![app(CODEX, 42, "/Applications/ChatGPT.app")], ..Default::default() };
        let action = select(AgentKind::Codex, ID, Some(999), Some(5), &environment).unwrap();
        assert_eq!(action.kind, "url"); assert_eq!(action.application.app_path, "/Applications/ChatGPT.app");
        assert!(action.source.is_none()); assert_eq!(action.url.unwrap(), format!("codex://threads/{ID}"));
        let fallback = select(AgentKind::Codex, "invalid", None, None, &environment).unwrap();
        assert_eq!(fallback.kind, "activate"); assert!(fallback.url.is_none());
    }

    #[test]
    fn claude_cli_selects_its_verified_terminal_owner_without_new_session_routes() {
        let environment = Inspection { chain: vec![cli("claude"), Process { pid: 21, parent: 22, ..Default::default() }, owner("com.apple.Terminal", "/System/Applications/Utilities/Terminal.app")],
            running: vec![app(CLAUDE, 99, "/Applications/Claude.app")], ..Default::default() };
        let action = select(AgentKind::Claude, ID, Some(20), Some(1500), &environment).unwrap();
        assert_eq!(action.application.pid, 22); assert_eq!(action.application.bundle_id, "com.apple.Terminal");
        assert_eq!(action.tty, "/dev/ttys003"); assert!(action.url.is_none()); assert_eq!(action.source.unwrap().pid, 20);
    }

    #[test]
    fn exact_claude_owner_wins_over_other_running_apps_with_the_same_bundle_id() {
        let chinese = "/Users/example/Applications/Claude-Chinese.app";
        let environment = Inspection { chain: vec![cli("claude"), owner(CLAUDE, chinese)],
            running: vec![app(CLAUDE, 22, chinese), app(CLAUDE, 99, "/Applications/Claude.app")], ..Default::default() };
        assert_eq!(select(AgentKind::Claude, ID, Some(20), None, &environment).unwrap().application.app_path, chinese);
        assert!(select(AgentKind::Claude, ID, None, None, &environment).is_err());
    }

    #[test]
    fn reused_unrelated_or_unverifiable_processes_never_activate_their_owner() {
        let mut environment = Inspection { chain: vec![cli("claude"), owner("com.apple.Terminal", "/Terminal.app")], ..Default::default() };
        environment.chain[0].start_stamp = 10_000_000;
        assert!(select(AgentKind::Claude, ID, Some(20), Some(1000), &environment).is_err());
        environment.chain[0] = cli("node");
        assert!(select(AgentKind::Claude, ID, Some(20), None, &environment).is_err());
        environment.chain[0] = cli("claude"); environment.chain[1] = owner(CODEX, "/Applications/ChatGPT.app");
        assert!(select(AgentKind::Claude, ID, Some(20), None, &environment).is_err());
        environment.chain[1] = owner("com.apple.finder", "/Finder.app");
        assert!(select(AgentKind::Claude, ID, Some(20), None, &environment).is_err());
    }

    #[test]
    fn an_interpreter_can_only_select_the_requested_gui_it_belongs_to() {
        let environment = Inspection { chain: vec![cli("node"), owner(CLAUDE, "/Applications/Claude.app")], ..Default::default() };
        assert_eq!(select(AgentKind::Claude, ID, Some(20), None, &environment).unwrap().application.bundle_id, CLAUDE);
    }

    #[test]
    fn missing_process_can_use_known_gui_but_app_absence_and_other_agents_fail_safely() {
        let environment = Inspection { installed: vec![app(CLAUDE, 0, "/Applications/Claude.app")], ..Default::default() };
        let action = select(AgentKind::Claude, ID, Some(999), None, &environment).unwrap();
        assert_eq!(action.kind, "launch"); assert_eq!(action.application.bundle_id, CLAUDE); assert!(action.url.is_none());
        assert!(select(AgentKind::Claude, ID, None, None, &Inspection::default()).is_err());
        assert!(select(AgentKind::Hermes, ID, None, None, &environment).is_err());
        let environment = Inspection { chain: vec![cli("aider"), owner("com.googlecode.iterm2", "/iTerm.app")], ..Default::default() };
        assert_eq!(select(AgentKind::Aider, ID, Some(20), None, &environment).unwrap().application.pid, 22);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_inspection_reads_only_current_process_and_application_identity() {
        let environment = native::inspect(std::process::id()).unwrap();
        let source = environment.chain.first().unwrap();
        assert_eq!(source.pid, std::process::id()); assert!(source.start_stamp > 0); assert!(!source.executable.is_empty());
        assert!(environment.chain.len() <= 24);
        assert!(environment.installed.iter().all(|app| matches!(app.bundle_id.as_str(), CODEX | CLAUDE)));
        assert!(select(AgentKind::Claude, ID, Some(std::process::id()), None, &environment).is_err());
    }
}
