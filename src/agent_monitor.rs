//! Local, observe-only CLI agent monitoring. Lifecycle hooks provide precision; bounded
//! transcript tails supply metadata when hooks are unavailable. No conversation content
//! or tool arguments are retained. Home configuration changes require an explicit action.

#[cfg(test)]
#[path="agent_notification_tests.rs"]
mod notification_journal_tests;
#[cfg(test)]
pub(crate) use notification_journal_tests::fixture as notification_fixture;
#[cfg(test)]
pub(crate) use notification_journal_tests::{approval_fixture,approval_episode_fixture};

use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_SESSIONS: usize = 64;
const MAX_NOTICES: usize = MAX_SESSIONS * 4;
const MAX_FILES: usize = 32;
const MAX_LINE: usize = 64 * 1024;
const READ_BUDGET: usize = 1024 * 1024;
const MAX_CONFIG: u64 = 2 * 1024 * 1024;
const FRESH_LOG_MS: u64 = 90_000;
const STALE_ACTIVE_MS: u64 = 300_000;
const PROCESS_QUERY_LEASE_MS: u64 = 30_000;
const RETAIN_MS: u64 = 10 * 60_000;
const SINK: &str = include_str!("../scripts/agent-monitor/sink.py");
const HERMES_PLUGIN: &str = include_str!("../scripts/agent-monitor/hermes_plugin.py");
const HERMES_CONFIGURE: &str = include_str!("../scripts/agent-monitor/configure_hermes.py");
const READ_TITLES: &str = include_str!("../scripts/agent-monitor/read_titles.py");
const LEGACY_SINK: &str = include_str!("../scripts/agent-monitor/legacy_v1/sink.py");
const LEGACY_HERMES_PLUGIN: &str = include_str!("../scripts/agent-monitor/legacy_v1/hermes_plugin.py");
const PREVIOUS_SINK: &str = include_str!("../scripts/agent-monitor/legacy_v2/sink.py");
const PREVIOUS_HERMES_PLUGIN: &str = include_str!("../scripts/agent-monitor/legacy_v2/hermes_plugin.py");
const PREVIOUS_RELEASE_HERMES_PLUGIN: &str = include_str!("../scripts/agent-monitor/legacy_v3/hermes_plugin.py");
const HERMES_MANIFEST: &str = "name: qingyu-monitor\nversion: 4.0.0\ndescription: Local observe-only execution status for Qingyu\n";
const PREVIOUS_HERMES_MANIFEST: &str = "name: qingyu-monitor\nversion: 2.0.0\ndescription: Local observe-only execution status for Qingyu\n";
const PREVIOUS_RELEASE_HERMES_MANIFEST: &str = "name: qingyu-monitor\nversion: 3.0.0\ndescription: Local observe-only execution status for Qingyu\n";
const LEGACY_HERMES_MANIFEST: &str = "name: qingyu-monitor\nversion: 1.0.0\ndescription: Local observe-only execution status for Qingyu\n";
const HOOK_EVENTS: &[&str] = &[
    "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest",
    "Stop", "SessionEnd", "Interrupt",
];
const CLAUDE_EXTRA_EVENTS: &[&str] = &["PostToolUseFailure", "StopFailure", "Notification"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AgentKind {
    #[default]
    Codex,
    Claude,
    Hermes,
    Gemini,
    OpenCode,
    Cursor,
    Aider,
}

impl AgentKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Codex => "Codex", Self::Claude => "Claude", Self::Hermes => "Hermes",
            Self::Gemini => "Gemini CLI", Self::OpenCode => "OpenCode",
            Self::Cursor => "Cursor Agent", Self::Aider => "Aider",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Codex => "codex", Self::Claude => "claude", Self::Hermes => "hermes",
            Self::Gemini => "gemini", Self::OpenCode => "opencode", Self::Cursor => "cursor-agent",
            Self::Aider => "aider",
        }
    }
    fn parse(value: &str) -> Option<Self> {
        [Self::Codex, Self::Claude, Self::Hermes, Self::Gemini, Self::OpenCode, Self::Cursor, Self::Aider]
            .into_iter().find(|kind| kind.key() == value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    Running, Thinking, Working, Waiting, Approval, Completed, Error, Idle, Stopped,
    #[default]
    Unknown,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "已启动", Self::Thinking => "思考中", Self::Working => "执行中",
            Self::Waiting => "等待操作", Self::Approval => "需要审批", Self::Completed => "已完成", Self::Error => "发生错误",
            Self::Idle => "空闲", Self::Stopped => "已停止", Self::Unknown => "状态待更新",
        }
    }
    /// Process presence alone does not imply an executing task.
    pub fn is_active(&self) -> bool { matches!(self, Self::Thinking | Self::Working | Self::Waiting | Self::Approval) }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub agent: AgentKind,
    pub status: Status,
    pub project: String,
    pub detail: String,
    pub pid: Option<u32>,
    pub updated_at: u64,
    /// Start of the current turn; zero means no reliable start was observed.
    pub started_at: u64,
    /// Completion/error/exit time; retained when the process exits after completion.
    pub finished_at: Option<u64>,
    /// A process-presence row measures uptime, rather than task execution time.
    pub process_only: bool,
    /// Display identity from actual title/agent metadata, never from conversation content.
    pub title: String,
    /// Stable identity for this execution turn, independent of title, status and
    /// timer precision. Zero means no execution turn has been observed yet.
    pub turn_key: u64,
    /// Opaque identity of the current permission-request episode. Execution
    /// resuming then asking again creates a new key within the same turn.
    pub approval_key: u64,
}

impl Session {
    pub fn elapsed_seconds(&self, now: u64) -> Option<u64> {
        (self.started_at != 0).then(|| self.finished_at.unwrap_or(now).saturating_sub(self.started_at))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub sessions: Vec<Session>,
    pub scanning: bool,
    pub error: Option<String>,
    pub integration_message: Option<String>,
    pub integrations_installed: bool,
    pub integration_busy: bool,
    pub integrations: Vec<Integration>,
    /// Changes when a monitoring worker restarts. Consumers establish a quiet
    /// baseline instead of replaying the new worker's initial log tail.
    pub generation: u64,
    /// Bounded journal: several short turns between UI frames remain observable.
    pub completions: Vec<Completion>,
    /// Real lifecycle transitions, retained in event order between UI updates.
    pub notices: Vec<Notice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Integration {
    pub agent: AgentKind,
    /// A configured tool home or a verified live process has been found locally.
    pub detected: bool,
    /// All hooks/plugin files and their owned version match the current release.
    pub installed: bool,
    /// Recent real lifecycle/log metadata has been correlated with a live process.
    pub ready: bool,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub sequence: u64,
    pub at_ms: u64,
    pub session: Session,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoticeKind { Started, Waiting, Interrupted, Completed, Approval }

impl NoticeKind {
    fn bit(self) -> u8 { 1 << self as u8 }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub sequence: u64,
    pub at_ms: u64,
    pub kind: NoticeKind,
    pub session: Session,
}

struct Worker {
    stop: Arc<AtomicBool>,
    refresh: mpsc::Sender<()>,
    handle: JoinHandle<()>,
}

/// Default construction does no I/O and starts no threads (including in UI fixtures).
#[derive(Default)]
pub struct Monitor {
    shared: Arc<Mutex<Snapshot>>,
    worker: Option<Worker>,
    action: Option<JoinHandle<()>>,
    context: Option<eframe::egui::Context>,
    enabled: bool,
}

impl Monitor {
    pub fn snapshot(&self) -> Snapshot { self.shared.lock().unwrap_or_else(|e| e.into_inner()).clone() }

    pub fn update(&mut self, enabled: bool, ctx: &eframe::egui::Context) {
        self.context = Some(ctx.clone());
        if self.action.as_ref().is_some_and(|handle| handle.is_finished()) {
            if let Some(handle) = self.action.take() { let _ = handle.join(); }
        }
        if self.worker.as_ref().is_some_and(|worker| worker.handle.is_finished()) {
            if let Some(worker) = self.worker.take() { let _ = worker.handle.join(); }
        }
        if !enabled {
            if let Some(worker) = &self.worker {
                worker.stop.store(true, Ordering::Release);
                let _ = worker.refresh.send(());
            }
            if self.enabled {
                let mut snapshot = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                snapshot.sessions.clear(); snapshot.completions.clear(); snapshot.notices.clear(); snapshot.scanning = false; snapshot.error = None;
            }
        } else if self.worker.is_none() {
            let shared = Arc::clone(&self.shared);
            let repaint = ctx.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let worker_stop = Arc::clone(&stop);
            let (tx, rx) = mpsc::channel();
            {
                static GENERATION: AtomicU64 = AtomicU64::new(1);
                let mut snapshot = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                snapshot.scanning = true;
                snapshot.generation = GENERATION.fetch_add(1, Ordering::Relaxed);
                snapshot.completions.clear();
                snapshot.notices.clear();
            }
            match thread::Builder::new().name("qingyu-agents".into()).spawn(move || run_worker(shared, worker_stop, rx, repaint)) {
                Ok(handle) => self.worker = Some(Worker { stop, refresh: tx, handle }),
                Err(error) => {
                    let mut snapshot = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                    snapshot.scanning = false; snapshot.error = Some(format!("无法启动监控：{error}"));
                }
            }
        }
        self.enabled = enabled;
        // A fast disable/enable waits for the old worker to release its marker first.
        if enabled && self.worker.as_ref().is_some_and(|worker| worker.stop.load(Ordering::Acquire)) {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    pub fn refresh(&mut self) { if let Some(worker) = &self.worker { let _ = worker.refresh.send(()); } }
    pub fn install_integrations(&mut self) { self.integration_action(true); }
    pub fn uninstall_integrations(&mut self) { self.integration_action(false); }

    fn integration_action(&mut self, install: bool) {
        if self.action.as_ref().is_some_and(|handle| !handle.is_finished()) { return; }
        if let Some(handle) = self.action.take() { let _ = handle.join(); }
        {
            let mut snapshot = self.shared.lock().unwrap_or_else(|e| e.into_inner());
            snapshot.integration_busy = true;
            snapshot.integration_message = Some(if install { "正在安装本地状态集成…" } else { "正在移除状态集成…" }.into());
        }
        let shared = Arc::clone(&self.shared);
        let context = self.context.clone();
        match thread::Builder::new().name("qingyu-agent-hooks".into()).spawn(move || {
            let paths = Paths::environment();
            let result = integrations(&paths, install);
            let integrations_installed = installed(&paths);
            let detected = integration_configs(&paths);
            let mut snapshot = shared.lock().unwrap_or_else(|e| e.into_inner());
            snapshot.integration_busy = false;
            snapshot.integrations_installed = integrations_installed;
            if snapshot.integrations.is_empty() { snapshot.integrations = integration_status(detected, &[], &Core::default(), now_ms(), false); }
            else { for integration in &mut snapshot.integrations {
                if let Some(config) = detected.iter().find(|config| config.agent == integration.agent) { integration.installed = config.installed; }
            } }
            snapshot.integration_message = Some(result);
            drop(snapshot);
            if let Some(context) = context { context.request_repaint(); }
        }) {
            Ok(handle) => self.action = Some(handle),
            Err(error) => {
                let mut snapshot = self.shared.lock().unwrap_or_else(|e| e.into_inner());
                snapshot.integration_busy = false;
                snapshot.integration_message = Some(format!("无法启动集成操作：{error}"));
            }
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.stop.store(true, Ordering::Release);
            let _ = worker.refresh.send(());
            if worker.handle.is_finished() { let _ = worker.handle.join(); }
        }
    }
}

#[derive(Clone)]
struct Paths { root: PathBuf, codex: PathBuf, claude: PathBuf, hermes: PathBuf }

impl Paths {
    fn environment() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let dir = |name: &str, fallback: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(fallback));
        let absolute = |path: PathBuf| if path.is_absolute() { path } else { std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()).join(path) };
        Self { root: absolute(crate::model::config_dir().join("agent-monitor")), codex: absolute(dir("CODEX_HOME", ".codex")),
            claude: absolute(dir("CLAUDE_CONFIG_DIR", ".claude")), hermes: absolute(dir("HERMES_HOME", ".hermes")) }
    }
}

fn now_ms() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64 }
fn next_turn_key() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
fn clean(value: &str, max: usize) -> String { value.chars().filter(|c| !c.is_control()).take(max).collect() }
fn tool_name(value: &str) -> String {
    if value.len() <= 96 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.:/-".contains(&b)) { value.into() } else { String::new() }
}

fn title_text(value: &str) -> String {
    let value: String = value.chars().take(1024).filter_map(|ch| {
        if ch.is_whitespace() { Some(' ') }
        else if ch.is_control() || matches!(ch, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}') { None }
        else { Some(ch) }
    }).collect();
    value.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum ProcessRole { #[default] Cli, AppServer, Helper, Service }

#[derive(Clone, Debug)]
struct OpenRollout { path: PathBuf, modified_ms: u64 }

#[derive(Clone, Debug)]
struct Process {
    pid: u32, parent: u32, agent: AgentKind, birth: String, started_ms: u64,
    service: bool, role: ProcessRole, launcher: bool, project: String,
    sessions: HashSet<String>, rollouts: HashMap<String, OpenRollout>, session_scan_complete: bool,
}

fn process_role(command: &str, service: bool) -> ProcessRole {
    let words = command_prefix(command);
    if words.iter().skip(1).any(|word| word == "mcp-server") { ProcessRole::Helper }
    else if words.iter().skip(1).any(|word| word == "app-server") { ProcessRole::AppServer }
    else if service { ProcessRole::Service } else { ProcessRole::Cli }
}

/// Tokenize only a command prefix. Arguments elsewhere (including prompts) never match agents.
fn command_prefix(command: &str) -> Vec<String> {
    let mut words = Vec::new(); let mut word = String::new(); let mut quote = None; let mut escape = false;
    for ch in command.chars().take(4096) {
        if escape { word.push(ch); escape = false; continue; }
        if ch == '\\' && quote != Some('\'') { escape = true; continue; }
        if let Some(q) = quote { if ch == q { quote = None; } else { word.push(ch); } }
        else if ch == '\'' || ch == '"' { quote = Some(ch); }
        else if ch.is_whitespace() { if !word.is_empty() { words.push(std::mem::take(&mut word)); if words.len() == 5 { break; } } }
        else { word.push(ch); }
    }
    if !word.is_empty() && words.len() < 5 { words.push(word); }
    words
}

fn detect_command(command: &str) -> Option<(AgentKind, bool)> {
    let words = command_prefix(command);
    let executable = Path::new(words.first()?).file_name()?.to_str()?.trim_end_matches(".exe");
    let python = executable.to_ascii_lowercase().starts_with("python");
    let direct = match executable {
        "codex" => Some(AgentKind::Codex), "claude" => Some(AgentKind::Claude), "hermes" => Some(AgentKind::Hermes),
        "gemini" => Some(AgentKind::Gemini), "opencode" => Some(AgentKind::OpenCode),
        "cursor-agent" => Some(AgentKind::Cursor), "aider" => Some(AgentKind::Aider), _ => None,
    };
    let agent = direct.or_else(|| {
        if executable == "node" || python {
            let mut index = 1;
            while words.get(index).is_some_and(|word| matches!(word.as_str(), "-I" | "-u" | "-B" | "-s" | "-S" | "-E" | "--no-warnings" | "--enable-source-maps")) { index += 1; }
            let script = words.get(index)?;
            if python && script == "-c" {
                // Hermes' managed Python launcher runs an inline bootstrap. Match
                // actual import/call statements, never an arbitrary 'hermes' argument.
                let entry = regex::Regex::new(r"(?m)^from hermes_cli\.main import main\s*$").unwrap();
                let call = regex::Regex::new(r"(?m)^(?:sys\.exit\(main\(\)\)|main\(\))(?:\s|$)").unwrap();
                let runpy = regex::Regex::new(r#"(?:^|;\s*)runpy\.run_module\((?:'hermes_cli\.main'|"hermes_cli\.main"),\s*run_name=(?:'__main__'|"__main__"),\s*alter_sys=True\)(?:\s|$)"#).unwrap();
                return (command.len() <= 64 * 1024 && (entry.is_match(command) && call.is_match(command)
                    || command.contains("import hermes_bootstrap;") && runpy.is_match(command))).then_some(AgentKind::Hermes);
            }
            let name = Path::new(script).file_name()?.to_str()?;
            if python && script == "-m" {
                return match words.get(index + 1)?.as_str() { "hermes_cli" | "hermes_cli.main" => Some(AgentKind::Hermes), "aider" | "aider.main" => Some(AgentKind::Aider), _ => None };
            }
            match name {
                "hermes" | "hermes_cli.py" => Some(AgentKind::Hermes), "aider" => Some(AgentKind::Aider),
                "cli.py" if script.contains("/hermes-agent/") => Some(AgentKind::Hermes),
                "main.py" if script.contains("/hermes_cli/") => Some(AgentKind::Hermes),
                "codex.js" if script.contains("/@openai/codex/") => Some(AgentKind::Codex),
                "cli.js" if script.contains("/@anthropic-ai/claude-code/") => Some(AgentKind::Claude),
                "index.js" | "gemini.js" if script.contains("/@google/gemini-cli/") => Some(AgentKind::Gemini),
                _ => None,
            }
        } else { None }
    })?;
    let service = words.iter().skip(1).any(|arg| matches!(arg.as_str(), "app-server" | "mcp-server" | "gateway" | "serve" | "web"))
        || agent == AgentKind::Hermes && python && words.iter().any(|word| word == "-c")
            && command.lines().last().is_some_and(|line| line.split_whitespace().any(|word| matches!(word, "gateway" | "serve" | "web")));
    Some((agent, service))
}

/// BSD ps renders newlines in argv as \012 and literal backslashes as \\.
/// Decode once at the ps boundary, so Python string escapes remain literal and
/// cannot masquerade as executable import/call lines.
fn process_command_text(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len()); let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' { text.push(ch); continue; }
        if chars.peek() == Some(&'\\') { chars.next(); text.push('\\'); continue; }
        let mut look = chars.clone(); let octal: String = look.by_ref().take(3).collect();
        if octal.len() == 3 && octal.bytes().all(|byte| matches!(byte, b'0'..=b'7')) {
            let value = u32::from_str_radix(&octal, 8).unwrap_or(0);
            if matches!(value, 9 | 10 | 13) {
                for _ in 0..3 { chars.next(); }
                text.push(char::from_u32(value).unwrap()); continue;
            }
        }
        text.push('\\');
    }
    text
}

/// Bound child lifetime and output, rather than waiting synchronously on `Command::output`.
fn command_output(command: &mut Command, timeout: Duration, limit: usize) -> io::Result<Vec<u8>> {
    let mut child = command.stdin(Stdio::null()).stderr(Stdio::null()).stdout(Stdio::piped()).spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| io::Error::other("缺少进程输出"))?;
    let reader = thread::spawn(move || { let mut bytes = Vec::new(); stdout.take(limit as u64 + 1).read_to_end(&mut bytes).map(|_| bytes) });
    let start = Instant::now();
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None if start.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            None => { let _ = child.kill(); let _ = child.wait(); return Err(io::Error::new(io::ErrorKind::TimedOut, "本地进程查询超时")); }
        }
    };
    let bytes = reader.join().map_err(|_| io::Error::other("进程输出读取失败"))??;
    if bytes.len() > limit { return Err(io::Error::other("本地进程输出超过限制")); }
    if !status.success() { return Err(io::Error::other("本地命令未成功完成")); }
    Ok(bytes)
}

#[cfg(unix)]
fn processes() -> io::Result<(Vec<Process>, HashMap<u32, u32>)> {
    // `comm` does not contain prompts or multiline inline programs. Read argv only
    // for plausible executables belonging to the current user, under one deadline.
    // `lstart` is a machine-readable part of this protocol. Finder-launched
    // apps inherit the user's locale: Chinese ps dates have four fields rather
    // than the five English fields below, which otherwise consumes `comm` too.
    // Override only this child; leave the app and user environment unchanged.
    let raw = command_output(Command::new("/bin/ps").env("LC_ALL", "C").args(["-ww", "-x", "-o", "pid=,ppid=,lstart=,comm="]), Duration::from_millis(600), 1024 * 1024)?;
    let mut found = Vec::new(); let mut parents = HashMap::new(); let mut candidates = Vec::new();
    for line in String::from_utf8_lossy(&raw).lines().take(16_384) {
        let mut fields = line.split_whitespace();
        let (Some(pid), Some(parent)) = (fields.next().and_then(|s| s.parse::<u32>().ok()), fields.next().and_then(|s| s.parse::<u32>().ok())) else { continue };
        let birth = fields.by_ref().take(5).collect::<Vec<_>>().join(" ");
        parents.insert(pid, parent);
        let executable = fields.collect::<Vec<_>>().join(" ");
        let basename = Path::new(&executable).file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let direct = detect_command(&shell_quote(&executable));
        let interpreter = basename == "node" || basename.starts_with("python") || basename.starts_with("Python");
        if direct.is_none() && !interpreter { continue; }
        let started_ms = NaiveDateTime::parse_from_str(&birth, "%a %b %e %H:%M:%S %Y").ok()
            .and_then(|date| date.and_local_timezone(Local).earliest()).map(|date| date.timestamp_millis().max(0) as u64).unwrap_or(0);
        candidates.push((direct, pid, parent, birth, started_ms, interpreter));
    }
    candidates.sort_by_key(|candidate| candidate.0.is_none());
    let query_start = Instant::now();
    for (direct, pid, parent, birth, started_ms, launcher) in candidates.into_iter().take(64) {
        let detected = if query_start.elapsed() < Duration::from_millis(1200) {
            command_output(Command::new("/bin/ps").args(["-ww", "-p", &pid.to_string(), "-o", "args="]), Duration::from_millis(180), 64 * 1024)
                .ok().and_then(|bytes| {
                    let command = process_command_text(&String::from_utf8_lossy(&bytes));
                    detect_command(command.trim()).map(|(agent, service)| (agent, service, process_role(&command, service)))
                }).or_else(|| direct.map(|(agent, service)| (agent, service, ProcessRole::Cli)))
        } else { direct.map(|(agent, service)| (agent, service, ProcessRole::Cli)) };
        if let Some((agent, service, role)) = detected { found.push(Process { pid, parent, agent, birth, started_ms, service, role, launcher,
            project: String::new(), sessions: HashSet::new(), rollouts: HashMap::new(), session_scan_complete: false }); }
    }
    // Node launchers and their native executable represent one process, not two agents.
    remove_launch_wrappers(&mut found);
    process_projects(&mut found);
    Ok((found, parents))
}

fn remove_launch_wrappers(found: &mut Vec<Process>) {
    let wrappers: HashSet<u32> = found.iter().filter_map(|child| found.iter().any(|parent| parent.launcher && parent.pid == child.parent && parent.agent == child.agent).then_some(child.parent)).collect();
    found.retain(|process| !wrappers.contains(&process.pid));
}

fn record_open_file(process: &mut Process, path: &Path) {
    if path.extension().is_none_or(|extension| extension != "jsonl") { return; }
    if process.agent == AgentKind::Codex {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if !name.starts_with("rollout-") || !uuid_session_id(&session_file_id(path)) { return; }
        let Ok(metadata) = fs::symlink_metadata(path) else { return };
        if !metadata.is_file() { return; }
        let id = session_file_id(path);
        if process.sessions.len() >= MAX_SESSIONS && !process.sessions.contains(&id) { process.session_scan_complete = false; return; }
        let modified_ms = metadata.modified().unwrap_or(UNIX_EPOCH).duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
        process.rollouts.insert(id.clone(), OpenRollout { path: path.to_path_buf(), modified_ms });
        process.sessions.insert(id);
    } else if process.sessions.len() < MAX_SESSIONS { process.sessions.insert(session_file_id(path)); }
}

#[cfg(target_os = "macos")]
fn process_projects(found: &mut [Process]) {
    if found.is_empty() { return; }
    let ids = found.iter().map(|process| process.pid.to_string()).collect::<Vec<_>>().join(",");
    let mut command = Command::new("/usr/sbin/lsof"); command.args(["-n", "-a", "-p", &ids, "-Fpfn"]);
    if let Ok(raw) = command_output(&mut command, Duration::from_millis(650), 512 * 1024) {
        parse_open_files(found, &raw);
    }
}

fn parse_open_files(found: &mut [Process], raw: &[u8]) {
        let mut pid = 0; let mut descriptor = String::new();
        for line in String::from_utf8_lossy(raw).lines() {
            if let Some(value) = line.strip_prefix('p') {
                pid = value.parse().unwrap_or(0); descriptor.clear();
                if let Some(process) = found.iter_mut().find(|process| process.pid == pid) { process.session_scan_complete = true; }
            }
            if let Some(value) = line.strip_prefix('f') { descriptor = value.into(); }
            if let Some(path) = line.strip_prefix('n') {
                if let Some(process) = found.iter_mut().find(|process| process.pid == pid) {
                    if descriptor == "cwd" { process.project = clean(path, 1024); }
                    else { record_open_file(process, Path::new(path)); }
                }
            }
        }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn process_projects(found: &mut [Process]) {
    for process in found {
        if let Ok(path) = fs::read_link(format!("/proc/{}/cwd", process.pid)) { process.project = clean(&path.to_string_lossy(), 1024); }
        if let Ok(entries) = fs::read_dir(format!("/proc/{}/fd", process.pid)) {
            process.session_scan_complete = true;
            let mut entries = entries;
            for entry in entries.by_ref().take(256).flatten() { if let Ok(path) = fs::read_link(entry.path()) { record_open_file(process, &path); } }
            if entries.next().is_some() { process.session_scan_complete = false; }
        }
    }
}

#[cfg(not(unix))]
fn processes() -> io::Result<(Vec<Process>, HashMap<u32, u32>)> { Err(io::Error::new(io::ErrorKind::Unsupported, "此平台暂不支持本地代理进程监控")) }

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source { Loaded, Log, Hook }

#[derive(Clone, Debug)]
struct Event {
    id: String, agent: AgentKind, status: Status, at_ms: u64, source: Source,
    turn: String, project: String, tool: String, pid: Option<u32>, heartbeat: bool,
    starts_turn: bool,
    /// Explicit termination, distinct from a recoverable tool error or metadata.
    ends_turn: bool,
    title: String,
    start_ms: Option<u64>,
}

fn event_status(event: &str) -> Option<Status> {
    Some(match event {
        "SessionStart" => Status::Idle, "UserPromptSubmit" | "Resume" => Status::Thinking,
        "PreToolUse" | "PostToolUse" => Status::Working, "PermissionRequest" => Status::Approval, "Waiting" => Status::Waiting,
        "Stop" => Status::Completed, "StopFailure" | "PostToolUseFailure" => Status::Error,
        "SessionEnd" => Status::Stopped, "Interrupt" => Status::Idle, _ => return None,
    })
}

#[derive(Deserialize)]
struct SpoolEvent {
    version: u8, agent: String, event: String, at_ms: u64,
    #[serde(default)] session_id: String, #[serde(default)] turn_id: String,
    #[serde(default)] project: String, #[serde(default)] tool: String,
    #[serde(default)] pid: Option<u32>, #[serde(default)] heartbeat: bool,
    #[serde(default)] title: String,
    #[serde(default)] started_at_ms: Option<u64>,
    #[serde(default)] transcript: String,
    #[serde(default)] notification_type: String,
}

fn decode_spool(bytes: &[u8], now: u64) -> Option<Event> {
    let record: SpoolEvent = serde_json::from_slice(bytes).ok()?;
    if record.version != 1 || record.at_ms > now + 5_000 || now.saturating_sub(record.at_ms) > FRESH_LOG_MS { return None; }
    let agent = AgentKind::parse(&record.agent)?;
    let status = if record.event == "Notification" && matches!(record.notification_type.as_str(), "permission_prompt" | "elicitation_dialog") {
        Status::Approval
    } else { event_status(&record.event)? };
    let id = if record.session_id.is_empty() { format!("hook-process-{}", record.pid?) } else { clean(&record.session_id, 160) };
    Some(Event { id, agent, status, at_ms: record.at_ms, source: Source::Hook, turn: clean(&record.turn_id, 160),
        project: clean(&record.project, 1024), tool: tool_name(&record.tool), pid: record.pid.filter(|pid| *pid > 0), heartbeat: record.heartbeat,
        starts_turn: record.event == "UserPromptSubmit" && !record.heartbeat, title: title_text(&record.title),
        ends_turn: matches!(record.event.as_str(), "Stop" | "StopFailure" | "SessionEnd" | "Interrupt"),
        start_ms: record.started_at_ms.filter(|start| *start > 0 && *start <= record.at_ms) })
}

struct State {
    session: Session, at_ms: u64, source: Source, turn: String, seen_ms: u64,
    process_birth: String, metadata_project: String,
    started_ms: u64, retired_turns: Vec<String>,
    process_started_ms: u64,
    terminal: bool,
    notice_kinds: u8,
    approval_open: bool,
    loaded: bool,
    turn_aliases: Vec<String>,
}

#[derive(Default)]
struct Core { states: HashMap<String, State>, next_turn: u64, sequence: u64, completions: VecDeque<Completion>,
    notice_sequence: u64, notices: VecDeque<Notice>, rejected: HashSet<String>, ended_hermes: HashSet<String>, received: HashMap<AgentKind, u64> }

impl State {
    fn notice(&mut self, kind: NoticeKind) -> Option<Session> {
        if self.session.turn_key == 0 || self.notice_kinds & kind.bit() != 0 { return None; }
        self.notice_kinds |= kind.bit();
        Some(self.session.clone())
    }

    fn interrupt(&mut self, at_ms: u64, reason: &str) -> Option<Session> {
        if self.terminal || self.session.turn_key == 0 { return None; }
        self.terminal = true;
        self.approval_open = false;
        self.session.status = Status::Stopped;
        self.session.detail = reason.into();
        self.session.updated_at = at_ms / 1000;
        finish_time(&mut self.session, Status::Stopped, at_ms / 1000);
        self.notice(NoticeKind::Interrupted)
    }
}

impl Core {
    /// An open rollout proves a loaded identity, not an executing turn or timer.
    fn recover_loaded(&mut self, found: &[Process], now: u64) {
        let mut ids: Vec<_> = found.iter().filter(|process| process.agent == AgentKind::Codex)
            .flat_map(|process| process.rollouts.keys().cloned()).collect::<HashSet<_>>().into_iter().collect();
        ids.sort_by(|a, b| {
            let active = |id: &str| self.states.get(&format!("codex:{id}")).is_some_and(|state| state.session.status.is_active());
            let modified = |id: &str| found.iter().filter_map(|process| process.rollouts.get(id)).map(|rollout| rollout.modified_ms).max().unwrap_or(0);
            active(b).cmp(&active(a)).then_with(|| modified(b).cmp(&modified(a))).then_with(|| a.cmp(b))
        });
        for id in ids.into_iter().take(MAX_SESSIONS) {
            let key = format!("codex:{id}"); if self.rejected.contains(&key) { continue; }
            let mut owners: Vec<_> = found.iter().filter(|process| process.agent == AgentKind::Codex && process.rollouts.contains_key(&id)).collect();
            owners.sort_by_key(|process| (role_rank(process.role), process.started_ms, process.pid));
            let preferred = self.states.get(&key).and_then(|state| owners.iter().copied().find(|process| Some(process.pid) == state.session.pid && process.birth == state.process_birth));
            let Some(process) = preferred.or_else(|| owners.first().copied()) else { continue };
            if let Some(state) = self.states.get_mut(&key) {
                if state.process_birth == process.birth && state.session.pid == Some(process.pid) { state.loaded = true; continue; }
            }
            let interrupted = self.states.get_mut(&key).and_then(|state| state.interrupt(now, "原执行进程已退出"));
            if let Some(session) = interrupted { self.record_notice(NoticeKind::Interrupted, session, now); }
            let title = self.states.get(&key).map(|state| state.session.title.clone()).unwrap_or_default();
            if self.states.len() >= MAX_SESSIONS && !self.states.contains_key(&key) {
                if let Some(oldest) = self.states.iter().min_by_key(|(_, state)| (state.session.status.is_active(), live_loaded(state, found), state.seen_ms)).map(|(key, _)| key.clone()) { self.states.remove(&oldest); }
            }
            let session = Session { id: key.clone(), agent: AgentKind::Codex, status: Status::Idle,
                project: process.project.clone(), detail: "对话已加载，暂无近期执行事件".into(), pid: Some(process.pid), title,
                started_at: 0, finished_at: None, updated_at: 0, process_only: false, turn_key: 0, approval_key: 0 };
            self.states.insert(key, State { session, at_ms: 0, source: Source::Loaded, turn: String::new(), seen_ms: now,
                process_birth: process.birth.clone(), metadata_project: process.project.clone(), started_ms: 0, retired_turns: Vec::new(),
                process_started_ms: process.started_ms, terminal: false, notice_kinds: 0, approval_open: false, loaded: true, turn_aliases: Vec::new() });
        }
    }

    fn apply(&mut self, event: Event, found: &[Process], parents: &HashMap<u32, u32>, now: u64) {
        if event.at_ms > now + 5_000 || now.saturating_sub(event.at_ms) > FRESH_LOG_MS { return; }
        let key = format!("{}:{}", event.agent.key(), event.id);
        if event.agent == AgentKind::Hermes && self.ended_hermes.contains(&key) {
            if event.starts_turn && !event.heartbeat {
                self.ended_hermes.remove(&key);
            } else { return; }
        }
        if self.rejected.contains(&key) {
            return;
        }
        let mut process = match_process(&event, found, parents);
        if process.is_none() && event.agent == AgentKind::Claude && event.pid.is_none() {
            process = self.states.get(&key).and_then(|state| found.iter().find(|process| process.agent == AgentKind::Claude
                && Some(process.pid) == state.session.pid && process.birth == state.process_birth));
        }
        // A queued terminal event may arrive in the same poll that verifies an
        // exit. It can close an already verified turn, but cannot introduce or
        // reactivate an exited/historical session.
        let exited = if event.ends_turn {
            self.states.get(&key).filter(|state| state.session.turn_key != 0 && !state.terminal
                && event.pid.is_none_or(|pid| Some(pid) == state.session.pid)
                && (event.turn.is_empty() || event.turn == state.turn || state.turn_aliases.contains(&event.turn))
                && !found.iter().any(|process| Some(process.pid) == state.session.pid))
                .and_then(|state| Some(Process { pid: state.session.pid?, parent: 0, agent: state.session.agent,
                    birth: state.process_birth.clone(), started_ms: state.process_started_ms, service: false, role: ProcessRole::Cli,
                    launcher: false, project: state.session.project.clone(), sessions: HashSet::new(), rollouts: HashMap::new(), session_scan_complete: false }))
        } else { None };
        let Some(process) = exited.as_ref().or(process) else { return };
        if process.started_ms > event.at_ms + 2_000 { return; }
        let execution = event.status.is_active() || matches!(event.status, Status::Completed | Status::Error)
            || event.ends_turn && self.states.get(&key).is_some_and(|state| state.session.turn_key != 0);
        if execution { self.received.insert(event.agent, event.at_ms.max(*self.received.get(&event.agent).unwrap_or(&0))); }
        let reported_start = event.start_ms.filter(|start| !event.turn.is_empty() && *start + 2_000 >= process.started_ms && *start <= event.at_ms);
        let resumed = self.states.get(&key).is_some_and(|state| state.process_birth != process.birth
            && !found.iter().any(|candidate| Some(candidate.pid) == state.session.pid && candidate.birth == state.process_birth));
        if resumed {
            let interrupted = self.states.get_mut(&key).and_then(|state| state.interrupt(now, "原执行进程已退出"));
            if let Some(session) = interrupted { self.record_notice(NoticeKind::Interrupted, session, now); }
            self.states.remove(&key);
        }
        if !event.title.is_empty() { self.update_title(event.agent, &event.id, &event.title); }
        let notice_kind = if event.starts_turn && !event.heartbeat { Some(NoticeKind::Started) }
            else if !event.heartbeat { match event.status {
                Status::Waiting => Some(NoticeKind::Waiting),
                Status::Approval => Some(NoticeKind::Approval),
                Status::Completed => Some(NoticeKind::Completed),
                Status::Error => Some(NoticeKind::Interrupted),
                Status::Idle | Status::Stopped if event.ends_turn => Some(NoticeKind::Interrupted),
                _ => None,
            } } else { None };
        if let Some(state) = self.states.get_mut(&key) {
            if state.process_birth != process.birth { return; }
            if !event.turn.is_empty() && state.retired_turns.contains(&event.turn) { return; }
            let synthetic_pair = (event.turn.starts_with("hook-"), state.turn.starts_with("hook-"));
            if event.agent == AgentKind::Claude && event.starts_turn && state.started_ms != 0 && (!state.terminal || event.at_ms <= state.at_ms)
                && synthetic_pair.0 != synthetic_pair.1 && !event.turn.is_empty() && !state.turn.is_empty()
                && event.at_ms.abs_diff(state.started_ms) <= 2_000 && !state.turn_aliases.contains(&event.turn) && state.turn_aliases.len() < 4 {
                state.turn_aliases.push(event.turn.clone());
            }
            let same_known_turn = !event.turn.is_empty() && (event.turn == state.turn || state.turn_aliases.contains(&event.turn));
            let changed_turn = !event.turn.is_empty() && !state.turn.is_empty() && !same_known_turn;
            if changed_turn
                && !(event.starts_turn || (event.status == Status::Working && !state.session.status.is_active() && event.at_ms > state.at_ms)) { return; }
            if event.heartbeat {
                if event.turn.is_empty() || state.turn.is_empty() || same_known_turn {
                    state.seen_ms = now;
                    // A verified current heartbeat restores an uncertain task after
                    // a process-query outage; it cannot reopen a terminal turn.
                    if state.session.status == Status::Unknown && !state.terminal && state.session.finished_at.is_none()
                        && event.status.is_active() && event.at_ms >= state.at_ms {
                        state.session.status = event.status; state.session.detail = detail(event.status, &event.tool);
                        state.session.updated_at = event.at_ms / 1000;
                    }
                    if state.turn.is_empty() && !event.turn.is_empty() { state.turn = event.turn; }
                    if state.started_ms == 0 && state.session.status.is_active() {
                        if let Some(start) = reported_start { state.started_ms = start; state.session.started_at = start / 1000; }
                    }
                }
                return;
            }
            if (event.at_ms, event.source) < (state.at_ms, state.source) { return; }
            // Session headers and SessionStart establish metadata. They do not
            // cancel a turn which has already supplied execution evidence.
            if event.status == Status::Idle && !event.ends_turn && state.session.turn_key != 0 { return; }
            let unknown_new_turn = changed_turn && !event.starts_turn;
            let initial_start = event.starts_turn && state.session.turn_key == 0 && !state.terminal;
            let new_turn = initial_start || event.starts_turn && !same_known_turn && (state.started_ms == 0 || state.terminal || changed_turn
                || (event.turn.is_empty() && event.at_ms.saturating_sub(state.started_ms) > 2_000)
                || (state.turn.is_empty() && !event.turn.is_empty() && event.at_ms.saturating_sub(state.started_ms) > 2_000));
            let mut notices = Vec::new();
            if new_turn || unknown_new_turn {
                if let Some(session) = state.interrupt(event.at_ms, "新一轮已开始，上轮执行已中断") { notices.push((NoticeKind::Interrupted, session)); }
                for alias in state.turn_aliases.drain(..) { if state.retired_turns.len() >= 8 { state.retired_turns.remove(0); } state.retired_turns.push(alias); }
                if !state.turn.is_empty() && state.turn != event.turn {
                    if state.retired_turns.len() >= 8 { state.retired_turns.remove(0); }
                    state.retired_turns.push(state.turn.clone());
                }
                state.turn = event.turn.clone();
                state.started_ms = if new_turn { event.at_ms } else { 0 };
                state.session.started_at = state.started_ms / 1000;
                state.session.finished_at = None;
                self.next_turn = next_turn_key();
                state.session.turn_key = self.next_turn;
                state.terminal = false;
                state.notice_kinds = 0;
                state.approval_open = false;
                state.session.approval_key = 0;
            }
            // A duplicate hook/log start or delayed tool event cannot reopen a
            // terminal turn. Only a genuinely new turn may do that.
            if state.terminal && !new_turn && !unknown_new_turn && event.status != Status::Stopped { return; }
            if event.status == Status::Approval {
                if !state.approval_open {
                    state.session.approval_key = next_turn_key();
                    state.notice_kinds &= !NoticeKind::Approval.bit();
                }
                state.approval_open = true;
            } else { state.approval_open = false; }
            if state.session.turn_key == 0 && (event.status.is_active() || matches!(event.status, Status::Completed | Status::Error)) { self.next_turn = next_turn_key(); state.session.turn_key = self.next_turn; }
            state.at_ms = event.at_ms; state.source = event.source; state.seen_ms = now;
            if !event.turn.is_empty() && !state.turn_aliases.contains(&event.turn) { state.turn = event.turn.clone(); }
            state.session.status = event.status;
            state.session.updated_at = event.at_ms / 1000;
            if !event.project.is_empty() { state.metadata_project = event.project.clone(); state.session.project = event.project.clone(); }
            state.session.detail = event_detail(&event);
            state.session.pid = Some(process.pid);
            if !event.title.is_empty() { state.session.title = event.title.clone(); }
            if state.started_ms == 0 && event.starts_turn {
                state.started_ms = reported_start.unwrap_or(event.at_ms);
                state.session.started_at = state.started_ms / 1000;
            }
            if state.started_ms == 0 && event.status.is_active() {
                if let Some(start) = reported_start { state.started_ms = start; state.session.started_at = start / 1000; }
            }
            if event.ends_turn {
                finish_time(&mut state.session, event.status, event.at_ms / 1000);
                state.terminal = true;
            }
            if let Some(kind) = notice_kind {
                if let Some(mut session) = state.notice(kind) {
                    // A recoverable tool error freezes the notice's elapsed
                    // time while the live turn can still continue afterwards.
                    if kind == NoticeKind::Interrupted { finish_time(&mut session, Status::Stopped, event.at_ms / 1000); }
                    notices.push((kind, session));
                }
            }
            let completed = (event.status == Status::Completed).then(|| state.session.clone());
            for (kind, session) in notices { self.record_notice(kind, session, event.at_ms); }
            if let Some(session) = completed { self.record_completion(session, event.at_ms); }
            return;
        }
        // Bound retained states even when a long-lived server creates many threads.
        if self.states.len() >= MAX_SESSIONS {
            if let Some(oldest) = self.states.iter().min_by_key(|(_, state)| (state.session.status.is_active(), state.seen_ms)).map(|(key, _)| key.clone()) { self.states.remove(&oldest); }
        }
        let project = if event.project.is_empty() { process.project.clone() } else { event.project.clone() };
        let started_ms = if event.starts_turn && !event.heartbeat { event.at_ms } else if event.status.is_active() { reported_start.unwrap_or(0) } else { 0 };
        let has_turn = event.status.is_active() || matches!(event.status, Status::Completed | Status::Error);
        if has_turn { self.next_turn = next_turn_key(); }
        let mut session = Session { id: key.clone(), agent: event.agent, status: event.status, project, detail: detail(event.status, &event.tool), pid: Some(process.pid), updated_at: event.at_ms / 1000,
            started_at: started_ms / 1000, finished_at: None, process_only: false, title: event.title.clone(), turn_key: if has_turn { self.next_turn } else { 0 },
            approval_key: if event.status == Status::Approval { next_turn_key() } else { 0 } };
        session.detail = event_detail(&event);
        if event.ends_turn { finish_time(&mut session, event.status, event.at_ms / 1000); }
        if event.status == Status::Completed { self.record_completion(session.clone(), event.at_ms); }
        let mut state = State { session, at_ms: event.at_ms, source: event.source, turn: event.turn,
            seen_ms: now, process_birth: process.birth.clone(), metadata_project: event.project, started_ms, retired_turns: Vec::new(), process_started_ms: process.started_ms,
            terminal: event.ends_turn, notice_kinds: 0, approval_open: event.status == Status::Approval, loaded: process.rollouts.contains_key(&event.id), turn_aliases: Vec::new() };
        if let Some(kind) = notice_kind {
            if let Some(mut session) = state.notice(kind) {
                if kind == NoticeKind::Interrupted { finish_time(&mut session, Status::Stopped, event.at_ms / 1000); }
                self.record_notice(kind, session, event.at_ms);
            }
        }
        self.states.insert(key, state);
    }

    fn record_completion(&mut self, session: Session, at_ms: u64) {
        self.sequence += 1;
        self.completions.push_back(Completion { sequence: self.sequence, at_ms, session });
        while self.completions.len() > MAX_SESSIONS { self.completions.pop_front(); }
    }

    fn record_notice(&mut self, kind: NoticeKind, session: Session, at_ms: u64) {
        self.notice_sequence += 1;
        self.notices.push_back(Notice { sequence: self.notice_sequence, at_ms, kind, session });
        while self.notices.len() > MAX_NOTICES { self.notices.pop_front(); }
    }

    fn process_query_unavailable(&mut self, now: u64, unavailable_ms: u64) -> Vec<Session> {
        for state in self.states.values_mut() {
            if state.session.status.is_active() && (unavailable_ms >= PROCESS_QUERY_LEASE_MS || now.saturating_sub(state.seen_ms) > STALE_ACTIVE_MS) {
                state.session.status = Status::Unknown;
                state.session.detail = "暂时无法核验进程状态，等待恢复".into();
                state.session.updated_at = now / 1000;
            }
        }
        let mut sessions: Vec<_> = self.states.values().map(|state| state.session.clone()).collect();
        sessions.sort_by(|a, b| b.status.is_active().cmp(&a.status.is_active()).then_with(|| a.id.cmp(&b.id)));
        sessions.truncate(MAX_SESSIONS); sessions
    }

    fn reconcile(&mut self, found: &[Process], now: u64) -> Vec<Session> {
        self.recover_loaded(found, now);
        self.states.retain(|_, state| !(state.source == Source::Loaded && loaded_closed(state, found))
            && (live_loaded(state, found) || now.saturating_sub(state.seen_ms) < RETAIN_MS));
        let mut interrupted = Vec::new();
        for state in self.states.values_mut() {
            let present = found.iter().any(|process| Some(process.pid) == state.session.pid && process.birth == state.process_birth);
            if !present && state.session.status != Status::Stopped {
                if let Some(session) = state.interrupt(now, "执行进程已退出") { interrupted.push(session); }
                finish_time(&mut state.session, Status::Stopped, now / 1000);
                state.session.status = Status::Stopped; state.session.detail = "进程已退出".into(); state.session.updated_at = now / 1000;
            } else if present && loaded_closed(state, found) && !(state.source == Source::Hook && now.saturating_sub(state.at_ms) < 2000) {
                if let Some(session) = state.interrupt(now, "执行中的对话已关闭") { interrupted.push(session); }
                finish_time(&mut state.session, Status::Stopped, now / 1000);
                state.session.status = Status::Stopped; state.session.detail = "对话已关闭".into();
            } else if present && state.session.status.is_active() && now.saturating_sub(state.seen_ms) > STALE_ACTIVE_MS {
                state.session.status = Status::Unknown; state.session.detail = "进程仍在，等待新的状态事件".into(); state.session.updated_at = now / 1000;
            }
        }
        interrupted.sort_by(|a, b| a.id.cmp(&b.id));
        for session in interrupted { self.record_notice(NoticeKind::Interrupted, session, now); }
        let mut sessions: Vec<_> = self.states.values().map(|state| state.session.clone()).collect();
        let mut groups = HashMap::<(AgentKind, ProcessRole, u32), Vec<&Process>>::new();
        for process in found {
            if sessions.iter().any(|session| session.pid == Some(process.pid) && session.status != Status::Stopped) { continue; }
            groups.entry((process.agent, process.role, if process.role == ProcessRole::Cli { process.pid } else { 0 })).or_default().push(process);
        }
        for ((_, role, _), mut processes) in groups {
            processes.sort_by_key(|process| (process.started_ms, process.pid)); let process = processes[0];
            let description = match role {
                ProcessRole::Helper => "MCP 辅助服务，未关联对话",
                ProcessRole::AppServer if !process.session_scan_complete => "应用服务已启动，暂无法读取已打开的会话文件",
                ProcessRole::AppServer => "应用服务已启动，暂无已加载对话",
                ProcessRole::Service => "后台服务已启动，未关联对话",
                ProcessRole::Cli => "检测到进程，尚未确认对话 ID",
            };
            let detail = if processes.len() > 1 { format!("{description} · {} 个进程", processes.len()) } else { description.into() };
            sessions.push(Session { id: format!("process:{}:{role:?}:{}:{}", process.agent.key(), process.pid, process.birth), agent: process.agent,
                status: if process.service { Status::Idle } else { Status::Running }, project: process.project.clone(),
                detail, pid: Some(process.pid), updated_at: process.started_ms / 1000,
                started_at: process.started_ms / 1000, finished_at: None, process_only: true, title: String::new(), turn_key: 0, approval_key: 0 });
        }
        sessions.sort_by(|a, b| a.process_only.cmp(&b.process_only).then_with(|| b.status.is_active().cmp(&a.status.is_active())).then_with(|| (a.status == Status::Stopped).cmp(&(b.status == Status::Stopped))).then_with(|| b.updated_at.cmp(&a.updated_at)).then_with(|| a.id.cmp(&b.id)));
        sessions.truncate(MAX_SESSIONS);
        sessions
    }

    fn update_title(&mut self, agent: AgentKind, id: &str, title: &str) {
        if let Some(state) = self.states.get_mut(&format!("{}:{id}", agent.key())) {
            state.session.title = title_text(title);
            for completion in &mut self.completions {
                if completion.session.id == state.session.id { completion.session.title = state.session.title.clone(); }
            }
            for notice in &mut self.notices {
                if notice.session.id == state.session.id { notice.session.title = state.session.title.clone(); }
            }
        }
    }

    fn ids(&self, agent: AgentKind) -> HashSet<String> {
        let prefix = format!("{}:", agent.key());
        self.states.values().filter(|state| state.session.agent == agent).filter_map(|state| state.session.id.strip_prefix(&prefix).map(str::to_owned)).collect()
    }
}

fn role_rank(role: ProcessRole) -> u8 { match role { ProcessRole::Cli => 0, ProcessRole::AppServer => 1, ProcessRole::Service => 2, ProcessRole::Helper => 3 } }

fn live_loaded(state: &State, found: &[Process]) -> bool {
    state.loaded && found.iter().any(|process| Some(process.pid) == state.session.pid && process.birth == state.process_birth
        && state.session.id.strip_prefix("codex:").is_some_and(|id| process.rollouts.contains_key(id)))
}

fn loaded_closed(state: &State, found: &[Process]) -> bool {
    state.loaded && found.iter().any(|process| Some(process.pid) == state.session.pid && process.birth == state.process_birth && process.session_scan_complete
        && state.session.id.strip_prefix("codex:").is_some_and(|id| !process.rollouts.contains_key(id)))
}

fn finish_time(session: &mut Session, status: Status, at: u64) {
    if status == Status::Completed {session.finished_at=Some(at);return;}
    if session.finished_at.is_none() && (matches!(status, Status::Completed | Status::Error | Status::Stopped)
        || (status == Status::Idle && session.started_at != 0)) { session.finished_at = Some(at); }
}

fn detail(status: Status, tool: &str) -> String {
    match status {
        Status::Working if !tool.is_empty() => format!("工具 · {tool}"),
        Status::Waiting if !tool.is_empty() => format!("等待操作 · {tool}"),
        Status::Approval if !tool.is_empty() => format!("需要审批 · {tool}"),
        Status::Error if !tool.is_empty() => format!("工具执行失败 · {tool}"),
        _ => status.label().into(),
    }
}

fn event_detail(event: &Event) -> String {
    match event.status {
        Status::Idle if event.ends_turn => "本轮执行已中断".into(),
        Status::Error if event.ends_turn => "本轮执行因错误中止".into(),
        Status::Stopped => "对话已结束".into(),
        _ => detail(event.status, &event.tool),
    }
}

fn match_process<'a>(event: &Event, found: &'a [Process], parents: &HashMap<u32, u32>) -> Option<&'a Process> {
    if let Some(mut pid) = event.pid {
        for _ in 0..8 {
            if let Some(process) = found.iter().find(|process| process.pid == pid && process.agent == event.agent) { return Some(process); }
            let next = parents.get(&pid).copied().unwrap_or(0);
            if next == 0 || next == pid { break; } pid = next;
        }
        // A provided PID is authoritative; never attach an exited hook to a sibling.
        return None;
    }
    let candidates: Vec<_> = found.iter().filter(|process| process.agent == event.agent).collect();
    let owners: Vec<_> = candidates.iter().filter(|process| process.sessions.contains(&event.id)).copied().collect();
    if owners.len() == 1 { return owners.first().copied(); }
    if !owners.is_empty() {
        let best = owners.iter().map(|process| role_rank(process.role)).min()?;
        let preferred: Vec<_> = owners.into_iter().filter(|process| role_rank(process.role) == best).collect();
        return (preferred.len() == 1).then(|| preferred[0]);
    }
    // Transcript metadata contains no reliable PID. Project paths (even a unique
    // one) cannot establish which Claude instance owns a conversation.
    if event.agent == AgentKind::Claude { return None; }
    if event.agent == AgentKind::Codex && !candidates.is_empty() && candidates.iter().all(|process| process.session_scan_complete) { return None; }
    let same_project: Vec<_> = candidates.iter().filter(|process| !event.project.is_empty() && process.project == event.project).copied().collect();
    if same_project.len() == 1 { return same_project.first().copied(); }
    if candidates.len() == 1 { return candidates.first().copied(); }
    // Codex desktop hosts many sessions in a single app-server, whose cwd is often '/'.
    let services: Vec<_> = candidates.into_iter().filter(|process| process.service).collect();
    if services.len() == 1 && event.agent == AgentKind::Codex { return services.first().copied(); }
    None
}

// serde ignores all fields not listed here, including prompt/response text, arguments,
// tool output, credentials and environment. They never enter the persistent state.
#[derive(Default, Deserialize)]
struct LogPayload {
    #[serde(default, rename = "type")] kind: String,
    #[serde(default)] id: String, #[serde(default)] cwd: String,
    #[serde(default)] turn_id: String, #[serde(default)] name: String,
    #[serde(default)] tool_name: String,
}

#[derive(Default, Deserialize)]
struct ContentMeta {
    #[serde(default, rename = "type")] kind: String,
    #[serde(default)] name: String,
    #[serde(default)] is_error: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content { Blocks(Vec<ContentMeta>), Other(serde::de::IgnoredAny) }

#[derive(Default, Deserialize)]
struct MessageMeta {
    #[serde(default)] content: Option<Content>,
    #[serde(default)] stop_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct LogRecord {
    #[serde(default, rename = "type")] kind: String,
    #[serde(default)] subtype: String,
    #[serde(default)] timestamp: Option<String>,
    #[serde(default)] payload: LogPayload,
    #[serde(default, rename = "sessionId")] session_id: String,
    #[serde(default)] cwd: String,
    #[serde(default)] message: Option<MessageMeta>,
    #[serde(default)] is_error: bool,
    #[serde(default)] uuid: String,
    #[serde(default)] hook_event_name: String,
    #[serde(default)] notification_type: String,
    #[serde(default)] tool_name: String,
}

#[derive(Default)]
struct LogMeta { id: String, project: String, turn: String }

fn log_event(bytes: &[u8], agent: AgentKind, metadata: &mut LogMeta) -> Option<Event> {
    let record: LogRecord = serde_json::from_slice(bytes).ok()?;
    if record.kind == "session_meta" {
        if !record.payload.id.is_empty() { metadata.id = clean(&record.payload.id, 160); }
        if !record.payload.cwd.is_empty() { metadata.project = clean(&record.payload.cwd, 1024); }
    }
    if !record.session_id.is_empty() { metadata.id = clean(&record.session_id, 160); }
    if !record.cwd.is_empty() { metadata.project = clean(&record.cwd, 1024); }
    if !record.payload.cwd.is_empty() { metadata.project = clean(&record.payload.cwd, 1024); }
    let is_turn_start = record.kind == "turn_context" || (record.kind == "event_msg" && record.payload.kind == "task_started");
    if is_turn_start && !record.payload.turn_id.is_empty() { metadata.turn = clean(&record.payload.turn_id, 160); }
    let mut tool = String::new();
    let mut starts_turn = false;
    let status = match agent {
        AgentKind::Codex => match (record.kind.as_str(), record.payload.kind.as_str()) {
            ("session_meta", _) => Status::Idle,
            ("event_msg", "task_started" | "user_message") => { starts_turn = true; Status::Thinking },
            ("response_item", "function_call" | "custom_tool_call" | "web_search_call") => {
                tool = tool_name(&record.payload.name); Status::Working
            }
            ("event_msg", "exec_command_begin" | "exec_command_end" | "patch_apply_begin" | "patch_apply_end" | "custom_tool_call_output" | "guardian_assessment") => {
                tool = tool_name(&record.payload.tool_name); Status::Working
            }
            ("event_msg", "task_complete") => Status::Completed,
            ("event_msg", "exec_approval_request" | "apply_patch_approval_request" | "mcp_elicitation_request") => {
                tool = tool_name(&record.payload.tool_name); Status::Approval
            }
            ("event_msg", "request_user_input") => Status::Waiting,
            ("event_msg", "turn_aborted") => Status::Idle,
            ("event_msg", "error") => Status::Error,
            _ => return None,
        },
        AgentKind::Claude => {
            let blocks = record.message.as_ref().and_then(|message| match message.content.as_ref() { Some(Content::Blocks(blocks)) => Some(blocks.as_slice()), _ => None }).unwrap_or_default();
            match record.kind.as_str() {
                "assistant" if blocks.iter().any(|block| block.kind == "tool_use") => {
                    tool = blocks.iter().find(|block| block.kind == "tool_use").map(|block| tool_name(&block.name)).unwrap_or_default(); Status::Working
                }
                "assistant" if record.message.as_ref().and_then(|message| message.stop_reason.as_deref()) == Some("end_turn") => Status::Completed,
                "assistant" if record.message.as_ref().and_then(|message| message.stop_reason.as_deref()) == Some("refusal") => Status::Error,
                "user" if blocks.iter().any(|block| block.kind == "tool_result" && block.is_error) => Status::Error,
                "user" => {
                    starts_turn = !blocks.iter().any(|block| block.kind == "tool_result");
                    if starts_turn && !record.uuid.is_empty() { metadata.turn = clean(&record.uuid, 160); }
                    Status::Thinking
                }
                "result" => if record.is_error { Status::Error } else { Status::Completed },
                "system" if record.subtype == "turn_duration" => Status::Completed,
                "system" | "hook" if record.hook_event_name == "PermissionRequest"
                    || (record.hook_event_name == "Notification" && matches!(record.notification_type.as_str(), "permission_prompt" | "elicitation_dialog")) => {
                    tool = tool_name(&record.tool_name); Status::Approval
                }
                "system" | "hook" if record.hook_event_name == "Stop" => Status::Completed,
                "system" | "hook" if record.hook_event_name == "StopFailure" => Status::Error,
                "system" | "hook" if record.hook_event_name == "Interrupt" => Status::Idle,
                _ => return None,
            }
        }
        _ => return None,
    };
    let at_ms = DateTime::parse_from_rfc3339(record.timestamp.as_deref()?).ok()?.timestamp_millis();
    if at_ms < 0 || metadata.id.is_empty() { return None; }
    let turn = if record.payload.turn_id.is_empty() { metadata.turn.clone() } else { clean(&record.payload.turn_id, 160) };
    let ends_turn = matches!(status, Status::Completed | Status::Stopped)
        || (agent == AgentKind::Codex && record.kind == "event_msg" && matches!(record.payload.kind.as_str(), "turn_aborted" | "error"))
        || (agent == AgentKind::Claude && (matches!(record.hook_event_name.as_str(), "StopFailure" | "Interrupt")
            || record.kind == "result" || record.message.as_ref().and_then(|message| message.stop_reason.as_deref()) == Some("refusal")));
    Some(Event { id: metadata.id.clone(), agent, status, at_ms: at_ms as u64, source: Source::Log,
        turn, project: metadata.project.clone(), tool, pid: None, heartbeat: false, starts_turn, ends_turn, title: String::new(), start_ms: None })
}

struct Tail {
    agent: AgentKind, identity: (u64, u64), offset: u64, pending: Vec<u8>, discard: bool,
    metadata: LogMeta, touched: u64,
    gap: bool,
}

fn session_file_id(path: &Path) -> String {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let suffix = stem.get(stem.len().saturating_sub(36)..).unwrap_or_default();
    let uuid = uuid_session_id(suffix);
    if uuid { suffix.into() } else { clean(&stem, 160) }
}

fn uuid_session_id(value: &str) -> bool {
    value.len() == 36 && value.bytes().enumerate().all(|(index, byte)| if [8, 13, 18, 23].contains(&index) { byte == b'-' } else { byte.is_ascii_hexdigit() })
}

#[cfg(unix)]
fn identity(metadata: &fs::Metadata) -> (u64, u64) { use std::os::unix::fs::MetadataExt; (metadata.dev(), metadata.ino()) }
#[cfg(not(unix))]
fn identity(metadata: &fs::Metadata) -> (u64, u64) {
    (metadata.created().unwrap_or(UNIX_EPOCH).duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64, 0)
}

impl Tail {
    fn open(path: &Path, agent: AgentKind, file: &mut File, metadata: &fs::Metadata, now: u64) -> io::Result<Self> {
        let mut prefix = Vec::new();
        Read::by_ref(file).take(MAX_LINE as u64).read_to_end(&mut prefix)?;
        // A very large session header can exceed the read cap. Codex rollout names
        // still contain the thread UUID, allowing hooks and tails to share one state.
        let mut log_meta = LogMeta { id: session_file_id(path), ..Default::default() };
        if let Some(end) = prefix.iter().position(|b| *b == b'\n') { let _ = log_event(&prefix[..end], agent, &mut log_meta); }
        log_meta.turn.clear(); // A header is metadata, not proof of the current turn.
        let offset = metadata.len().saturating_sub(MAX_LINE as u64);
        file.seek(SeekFrom::Start(offset))?;
        Ok(Self { agent, identity: identity(metadata), offset, pending: Vec::new(), discard: offset > 0, metadata: log_meta, touched: now, gap: offset > 0 })
    }

    fn read(&mut self, file: &mut File, budget: &mut usize, now: u64) -> io::Result<Vec<Event>> {
        if *budget == 0 { return Ok(Vec::new()); }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        file.take((*budget).min(128 * 1024) as u64).read_to_end(&mut bytes)?;
        *budget = budget.saturating_sub(bytes.len());
        self.offset += bytes.len() as u64;
        if !bytes.is_empty() { self.touched = now; }
        let mut events = Vec::new();
        for byte in bytes {
            if byte == b'\n' {
                if !self.discard && !self.pending.is_empty() {
                    if serde_json::from_slice::<LogRecord>(&self.pending).is_err() { self.gap = true; }
                    else if let Some(event) = log_event(&self.pending, self.agent, &mut self.metadata) { self.gap = false; events.push(event); }
                }
                self.pending.clear(); self.discard = false;
            } else if !self.discard {
                if self.pending.len() < MAX_LINE { self.pending.push(byte); }
                else { self.pending.clear(); self.discard = true; self.gap = true; }
            }
        }
        Ok(events)
    }
}

#[derive(Default)]
struct Logs { tails: HashMap<PathBuf, Tail>, bound_claude: HashMap<String, PathBuf>, backfill_cursor: usize, poll_cursor: usize, discovery: Discovery }

impl Logs {
    fn poll(&mut self, paths: &Paths, now: u64) -> Vec<Event> {
        let cold = self.discovery.next(&paths.codex);
        let discovered = discover_logs_with(paths, now, &cold);
        let mut files: Vec<_> = self.tails.iter().filter_map(|(path, tail)| {
            fs::symlink_metadata(path).ok().filter(|metadata| metadata.is_file() && recent_file(metadata, now))
                .map(|_| (path.clone(), tail.agent))
        }).collect();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        for item in discovered { if !files.iter().any(|(path, _)| *path == item.0) { files.push(item); } }
        for path in self.bound_claude.values() {
            if files.len() < MAX_FILES * 2 && !files.iter().any(|(other, _)| other == path)
                && fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file() && recent_file(&metadata, now)) { files.push((path.clone(), AgentKind::Claude)); }
        }
        let mut ranked: Vec<_> = files.into_iter().map(|item| {
            let modified = fs::metadata(&item.0).ok().and_then(|metadata| metadata.modified().ok()).unwrap_or(UNIX_EPOCH).duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
            let pinned = self.tails.get(&item.0).is_some_and(|tail| now.saturating_sub(tail.touched) <= FRESH_LOG_MS && now.saturating_sub(modified) <= FRESH_LOG_MS);
            (pinned, modified, item)
        }).collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)).then_with(|| a.2.0.cmp(&b.2.0)));
        let mut files: Vec<_> = ranked.into_iter().take(MAX_FILES).map(|(_, _, item)| item).collect();
        let selected: HashSet<_> = files.iter().map(|(path, _)| path.clone()).collect();
        self.tails.retain(|path, _| selected.contains(path));
        // Each busy file receives a turn within four polls under the byte budget.
        // Newest-first iteration alone starves quieter files behind large streams.
        if !files.is_empty() { let length = files.len(); files.rotate_left(self.poll_cursor % length); }
        let mut budget = READ_BUDGET; let mut events = Vec::new();
        let mut visited = 0;
        for (path, agent) in files {
            if budget == 0 { break; }
            visited += 1;
            let Ok(metadata) = fs::symlink_metadata(&path) else { continue };
            if !metadata.is_file() { continue; }
            let Ok(mut file) = File::open(&path) else { continue };
            let replace = self.tails.get(&path).is_none_or(|tail| tail.identity != identity(&metadata) || metadata.len() < tail.offset);
            if replace {
                let Ok(tail) = Tail::open(&path, agent, &mut file, &metadata, now) else { continue };
                self.tails.insert(path.clone(), tail);
            }
            if let Some(tail) = self.tails.get_mut(&path) {
                if let Ok(mut read) = tail.read(&mut file, &mut budget, now) { events.append(&mut read); }
            }
        }
        self.poll_cursor = (self.poll_cursor + visited) % MAX_FILES;
        events
    }

    fn bind_claude(&mut self, path: &str, event: &Event, paths: &Paths) {
        if event.agent != AgentKind::Claude || path.len() > 4096 { return; }
        let path = PathBuf::from(path);
        let Ok(projects) = fs::canonicalize(paths.claude.join("projects")) else { return };
        let Ok(resolved) = fs::canonicalize(&path) else { return };
        if !resolved.starts_with(projects) || resolved.extension().is_none_or(|extension| extension != "jsonl")
            || resolved.file_stem().is_none_or(|stem| stem != event.id.as_str()) || !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) { return; }
        if self.bound_claude.len() >= MAX_FILES && !self.bound_claude.contains_key(&event.id) {
            if let Some(key) = self.bound_claude.keys().min().cloned() { self.bound_claude.remove(&key); }
        }
        self.bound_claude.insert(event.id.clone(), resolved);
    }

    fn backfill_timers(&mut self, core: &mut Core, now: u64) {
        let mut eligible: Vec<_> = self.tails.iter().filter_map(|(path, tail)| {
            let key = format!("{}:{}", tail.agent.key(), tail.metadata.id);
            core.states.get(&key).filter(|state| state.started_ms == 0 && state.session.status.is_active())
                .map(|_| (path.clone(), tail.agent, tail.metadata.id.clone(), key))
        }).collect();
        eligible.sort_by(|a, b| a.0.cmp(&b.0));
        if eligible.is_empty() { self.backfill_cursor = 0; return; }
        let start = self.backfill_cursor % eligible.len();
        // At most 1 MiB extra per cycle, with a rotating cursor across sessions.
        let count = eligible.len().min(2);
        for step in 0..count {
            let (path, agent, id, key) = &eligible[(start + step) % eligible.len()];
            let mut metadata = LogMeta { id: id.clone(), ..Default::default() };
            let mut starts = HashMap::<String, u64>::new(); let mut last = None;
            if let Ok(lines) = title_lines(path, 512 * 1024, false) {
                for line in lines {
                    if line.is_empty() { starts.clear(); metadata.turn.clear(); last = None; continue; }
                    if serde_json::from_slice::<LogRecord>(&line).is_err() { starts.clear(); metadata.turn.clear(); last = None; continue; }
                    if let Some(event) = log_event(&line, *agent, &mut metadata) {
                        if event.starts_turn && !event.turn.is_empty() {
                            if starts.len() >= 64 { if let Some(oldest) = starts.iter().min_by_key(|(_, at)| **at).map(|(turn, _)| turn.clone()) { starts.remove(&oldest); } }
                            starts.entry(event.turn.clone()).and_modify(|at| *at = (*at).min(event.at_ms)).or_insert(event.at_ms);
                        }
                        last = Some(event);
                    }
                }
            }
            let Some(last) = last else { continue };
            let Some(at) = starts.get(&last.turn).copied() else { continue };
            let Some(state) = core.states.get_mut(key) else { continue };
            if state.started_ms == 0 && state.session.status.is_active() && last.status.is_active()
                && !last.turn.is_empty() && (state.turn.is_empty() || state.turn == last.turn) && !state.retired_turns.contains(&last.turn)
                && now.saturating_sub(last.at_ms) <= FRESH_LOG_MS && state.at_ms.abs_diff(last.at_ms) <= 5_000
                && at > 0 && at <= last.at_ms && at + 2_000 >= state.process_started_ms {
                state.started_ms = at; state.session.started_at = at / 1000;
                if state.turn.is_empty() { state.turn = last.turn; }
            }
        }
        self.backfill_cursor = (start + count) % eligible.len();
    }

    fn apply_uncertainty(&self, core: &mut Core) {
        for tail in self.tails.values().filter(|tail| tail.gap) {
            if let Some(state) = core.states.get_mut(&format!("{}:{}", tail.agent.key(), tail.metadata.id)) {
                if state.source == Source::Log && state.session.status.is_active() {
                    state.session.status = Status::Unknown;
                    state.session.detail = "日志条目不完整或过大，等待新的状态事件".into();
                }
            }
        }
    }
}

#[cfg(test)]
fn discover_logs(paths: &Paths, now: u64) -> Vec<(PathBuf, AgentKind)> {
    discover_logs_with(paths, now, &[])
}

fn recent_file(metadata: &fs::Metadata, now: u64) -> bool {
    let modified = metadata.modified().unwrap_or(UNIX_EPOCH).duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
    modified <= now + 5_000 && now.saturating_sub(modified) <= 6 * 60 * 60_000
}

#[derive(Default)]
struct Discovery { directories: Vec<PathBuf>, cursor: usize, next_refresh: Option<Instant> }
impl Discovery {
    fn next(&mut self, codex: &Path) -> Vec<PathBuf> {
        if self.next_refresh.is_none_or(|at| Instant::now() >= at) {
            self.directories.clear(); self.cursor = 0;
            let children = |path: &Path, digits: usize, cap: usize| -> Vec<PathBuf> {
                let mut paths = Vec::new();
                if let Ok(entries) = fs::read_dir(path) {
                    for entry in entries.take(128).flatten() {
                        let name = entry.file_name(); let name = name.to_string_lossy();
                        if name.len() == digits && name.bytes().all(|byte| byte.is_ascii_digit()) && entry.file_type().is_ok_and(|kind| kind.is_dir()) { paths.push(entry.path()); }
                    }
                }
                paths.sort_by(|a, b| b.cmp(a)); paths.truncate(cap); paths
            };
            // Enumerate date directory names, never transcript bodies or arbitrary trees.
            for year in children(&codex.join("sessions"), 4, 16) {
                for month in children(&year, 2, 12) {
                    for day in children(&month, 2, 31) { if self.directories.len() < 4096 { self.directories.push(day); } }
                }
            }
            self.directories.sort_by(|a, b| b.cmp(a));
            self.next_refresh = Some(Instant::now() + Duration::from_secs(120));
        }
        if self.directories.is_empty() { return Vec::new(); }
        let count = self.directories.len().min(12); let start = self.cursor % self.directories.len();
        let selected = (0..count).map(|step| self.directories[(start + step) % self.directories.len()].clone()).collect();
        self.cursor = (start + count) % self.directories.len(); selected
    }
}

fn discover_logs_with(paths: &Paths, now: u64, extra: &[PathBuf]) -> Vec<(PathBuf, AgentKind)> {
    let mut candidates = Vec::new(); let mut budget = 2048usize;
    let mut visited = HashSet::new();
    let mut collect = |directory: &Path, agent: AgentKind, codex: bool| {
        if !visited.insert(directory.to_path_buf()) { return; }
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.take(512).flatten() {
                if budget == 0 { break; } budget -= 1;
                let path = entry.path();
                if path.extension().is_none_or(|extension| extension != "jsonl") || (codex && !path.file_name().unwrap_or_default().to_string_lossy().starts_with("rollout-")) { continue; }
                let Ok(metadata) = fs::symlink_metadata(&path) else { continue };
                if !metadata.is_file() { continue; }
                let modified = metadata.modified().unwrap_or(UNIX_EPOCH).duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                if modified <= now + 5_000 && now.saturating_sub(modified) <= 6 * 60 * 60_000 { candidates.push((modified, path, agent)); }
            }
        }
    };
    // Visit known date directories, never recursively walk the full Codex archive.
    for days in 0..3 {
        let date = Utc::now() - ChronoDuration::days(days);
        collect(&paths.codex.join("sessions").join(date.format("%Y/%m/%d").to_string()), AgentKind::Codex, true);
    }
    for directory in extra { collect(directory, AgentKind::Codex, true); }
    if let Ok(projects) = fs::read_dir(paths.claude.join("projects")) {
        for project in projects.take(128).flatten() {
            if project.file_type().is_ok_and(|kind| kind.is_dir()) { collect(&project.path(), AgentKind::Claude, false); }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    candidates.truncate(MAX_FILES);
    candidates.into_iter().map(|(_, path, agent)| (path, agent)).collect()
}

#[derive(Default, Deserialize)]
struct TitleRecord {
    #[serde(default, rename = "type")] kind: String,
    #[serde(default, alias = "thread_id", alias = "session_id", alias = "sessionId")] id: String,
    #[serde(default)] thread_name: Option<String>,
    #[serde(default)] title: Option<String>,
    #[serde(default, rename = "customTitle")] custom_title: Option<String>,
    #[serde(default, rename = "aiTitle")] ai_title: Option<String>,
}

/// Read complete lines from a bounded tail, optionally also a bounded header.
/// Title refresh deliberately does not feed those lines back into the status reducer.
fn title_lines(path: &Path, limit: usize, header: bool) -> io::Result<Vec<Vec<u8>>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() { return Ok(Vec::new()); }
    let mut file = File::open(path)?; let length = metadata.len();
    let windows = if length <= limit as u64 { vec![(0, limit)] }
        else if header { vec![(0, limit / 2), (length.saturating_sub((limit / 2) as u64), limit / 2)] }
        else { vec![(length.saturating_sub(limit as u64), limit)] };
    let mut lines = Vec::new();
    for (offset, count) in windows {
        file.seek(SeekFrom::Start(offset))?;
        let mut bytes = Vec::new(); Read::by_ref(&mut file).take(count as u64).read_to_end(&mut bytes)?;
        let mut start = if offset > 0 { bytes.iter().position(|byte| *byte == b'\n').map(|index| index + 1).unwrap_or(bytes.len()) } else { 0 };
        for index in start..bytes.len() {
            if bytes[index] == b'\n' { if index > start { lines.push(if index - start <= MAX_LINE { bytes[start..index].to_vec() } else { Vec::new() }); } start = index + 1; }
        }
    }
    Ok(lines)
}

fn codex_index_titles(path: &Path, ids: &HashSet<String>) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    if ids.is_empty() { return titles; }
    if let Ok(lines) = title_lines(path, 512 * 1024, false) {
        for line in lines {
            let Ok(record) = serde_json::from_slice::<TitleRecord>(&line) else { continue };
            if ids.contains(&record.id) { if let Some(title) = record.thread_name.or(record.title) { titles.insert(record.id, title_text(&title)); } }
        }
    }
    titles
}

fn claude_title(path: &Path, expected: &str) -> Option<String> {
    let mut custom = None; let mut automatic = None;
    for line in title_lines(path, MAX_LINE, true).ok()? {
        let Ok(record) = serde_json::from_slice::<TitleRecord>(&line) else { continue };
        if !record.id.is_empty() && record.id != expected { continue; }
        match record.kind.as_str() {
            "custom-title" => if let Some(title) = record.custom_title.or(record.title) { custom = Some(title_text(&title)); },
            "ai-title" => if let Some(title) = record.ai_title.or(record.title) { automatic = Some(title_text(&title)); },
            _ => {}, // Conversation text, summaries and tool input are never titles.
        }
    }
    match custom { Some(title) if !title.is_empty() => Some(title), Some(_) => automatic.or_else(|| Some(String::new())), None => automatic }
}

#[derive(Clone, Debug, Deserialize)]
struct DatabaseTitle {
    agent: String, id: String, title: String,
    #[serde(default)] has_name: bool,
    #[serde(default)] name: Option<String>,
    #[serde(default)] archived: Option<bool>,
    #[serde(default)] rollout_path: Option<String>,
    #[serde(default)] parent_id: Option<String>,
    #[serde(default)] parent_title: Option<String>,
    #[serde(default)] is_subagent: bool,
    #[serde(default)] agent_nickname: Option<String>,
    #[serde(default)] agent_role: Option<String>,
    #[serde(default)] ended: Option<bool>,
}

impl DatabaseTitle {
    fn child(&self) -> bool {
        self.agent == "codex" && (self.is_subagent || self.parent_id.as_deref().is_some_and(uuid_session_id))
    }

    fn own_title(&self, index: Option<&String>) -> String {
        let manual = self.name.as_deref().map(title_text).unwrap_or_default();
        if self.has_name && !manual.is_empty() { return manual; }
        let title = title_text(&self.title);
        if !title.is_empty() { return title; }
        if !self.has_name { if let Some(title) = index.filter(|title| !title.is_empty()) { return title_text(title); } }
        String::new()
    }

    fn effective_title(&self, index: Option<&String>) -> String {
        if self.child() { self.child_title(index, false) } else { self.own_title(index) }
    }

    fn child_title(&self, index: Option<&String>, full_id: bool) -> String {
        let own = self.own_title(None);
        let nickname = self.agent_nickname.as_deref().map(title_text).unwrap_or_default();
        let legacy = if !self.has_name { index.map(|title| title_text(title)).unwrap_or_default() } else { String::new() };
        let role = self.agent_role.as_deref().map(title_text).unwrap_or_default();
        let label = [&own, &nickname, &legacy, &role].into_iter().find(|value| !value.is_empty()).map(String::as_str).unwrap_or_default();
        let identity = if full_id { clean(&self.id, 160) }
            else if uuid_session_id(&self.id) { self.id[self.id.len() - 8..].to_ascii_lowercase() }
            else { asset_hash(self.id.as_bytes())[..8].to_string() };
        let suffix = format!(" · {identity}");
        let available = 160usize.saturating_sub("子 Agent · ".chars().count() + suffix.chars().count());
        let label: String = label.chars().take(available).collect();
        if label.is_empty() { format!("子 Agent{suffix}") } else { format!("子 Agent · {label}{suffix}") }
    }
}

fn distinct_codex_titles(ids: &HashSet<String>, database: &HashMap<String, DatabaseTitle>, index: &HashMap<String, String>) -> HashMap<String, String> {
    let mut titles: HashMap<_, _> = ids.iter().filter_map(|id| {
        database.get(&format!("codex:{id}")).map(|metadata| (id.clone(), metadata.effective_title(index.get(id))))
            .or_else(|| index.get(id).map(|title| (id.clone(), title.clone())))
    }).collect();
    let mut counts = HashMap::<String, usize>::new();
    for title in titles.values() { if !title.is_empty() { *counts.entry(title.clone()).or_default() += 1; } }
    // UUID time prefixes repeat, so short identities use the random suffix.
    // In the rare suffix/name collision, the full ID provides deterministic
    // distinction without changing any parent or sibling's lifecycle identity.
    for id in ids {
        if let Some(metadata) = database.get(&format!("codex:{id}")) {
            if metadata.child() && titles.get(id).is_some_and(|title| counts.get(title).copied().unwrap_or(0) > 1) {
                titles.insert(id.clone(), metadata.child_title(index.get(id), true));
            }
        }
    }
    titles
}

fn database_titles(paths: &Paths, codex: &HashSet<String>, hermes: &HashSet<String>) -> Vec<DatabaseTitle> {
    let mut requests = Vec::new();
    if !codex.is_empty() {
        let mut databases = Vec::new();
        if let Ok(entries) = fs::read_dir(&paths.codex) {
            for entry in entries.take(128).flatten() {
                let path = entry.path(); let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with("state_") && name.ends_with(".sqlite") && fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
                    let version = name.trim_start_matches("state_").trim_end_matches(".sqlite").parse::<u32>().unwrap_or(0);
                    databases.push((version, path));
                }
            }
        }
        databases.sort_by(|a, b| b.0.cmp(&a.0));
        let mut ids: Vec<_> = codex.iter().cloned().collect(); ids.sort(); ids.truncate(MAX_SESSIONS);
        for (_, path) in databases.into_iter().take(2) { requests.push(json!(["codex", path, ids])); }
    }
    if !hermes.is_empty() && paths.hermes.join("state.db").is_file() {
        let mut ids: Vec<_> = hermes.iter().cloned().collect(); ids.sort(); ids.truncate(MAX_SESSIONS);
        requests.push(json!(["hermes", paths.hermes.join("state.db"), ids]));
    }
    if requests.is_empty() { return Vec::new(); }
    let Some(python) = python_candidates(paths).into_iter().next() else { return Vec::new() };
    let Ok(argument) = serde_json::to_string(&requests) else { return Vec::new() };
    command_output(Command::new(python).arg("-c").arg(READ_TITLES).arg(argument), Duration::from_secs(1), 128 * 1024)
        .ok().and_then(|bytes| serde_json::from_slice::<Vec<DatabaseTitle>>(&bytes).ok()).unwrap_or_default()
}

#[derive(Default)]
struct Titles { next: Option<Instant>, requested: HashSet<String>, database: HashMap<String, DatabaseTitle>, child_titles: HashMap<String, String> }
impl Titles {
    #[cfg(test)]
    fn refresh(&mut self, paths: &Paths, logs: &Logs, core: &mut Core) {
        self.refresh_loaded(paths, logs, core, &[], now_ms());
    }

    fn refresh_loaded(&mut self, paths: &Paths, logs: &Logs, core: &mut Core, found: &[Process], now: u64) {
        core.recover_loaded(found, now);
        let mut codex = core.ids(AgentKind::Codex); let claude = core.ids(AgentKind::Claude); let hermes = core.ids(AgentKind::Hermes);
        codex.extend(found.iter().filter(|process| process.agent == AgentKind::Codex).flat_map(|process| process.rollouts.keys().cloned()));
        if codex.len() > MAX_SESSIONS {
            let mut prioritized: Vec<_> = codex.into_iter().collect();
            prioritized.sort_by(|a, b| {
                let rank = |id: &str| {
                    let key = format!("codex:{id}"); let active = core.states.get(&key).is_some_and(|state| state.session.status.is_active());
                    let modified = found.iter().filter_map(|process| process.rollouts.get(id)).map(|rollout| rollout.modified_ms).max();
                    (active, modified.is_some(), modified.unwrap_or(0))
                };
                rank(b).cmp(&rank(a)).then_with(|| a.cmp(b))
            });
            codex = prioritized.into_iter().take(MAX_SESSIONS).collect();
        }
        let requested: HashSet<_> = codex.iter().map(|id| format!("codex:{id}")).chain(hermes.iter().map(|id| format!("hermes:{id}"))).collect();
        if self.next.is_some_and(|next| Instant::now() < next) && requested == self.requested {
            self.suppress_ended_hermes(core);
            // Hooks may carry a raw thread/parent name between metadata polls.
            // Keep the already verified child identity on snapshots and notices.
            for (id, title) in &self.child_titles { core.update_title(AgentKind::Codex, id, title); }
            return;
        }
        self.next = Some(Instant::now() + Duration::from_secs(5));
        self.requested = requested;
        self.database.retain(|key, _| self.requested.contains(key));
        let index = codex_index_titles(&paths.codex.join("session_index.jsonl"), &codex);
        for title in database_titles(paths, &codex, &hermes) {
            if (title.agent == "codex" && codex.contains(&title.id)) || (title.agent == "hermes" && hermes.contains(&title.id)) {
                self.database.insert(format!("{}:{}", title.agent, title.id), title);
            }
        }
        core.rejected.retain(|key| self.requested.contains(key));
        for id in &codex {
            let key = format!("codex:{id}");
            if let Some(metadata) = self.database.get(&key) {
                let owners: Vec<_> = found.iter().filter_map(|process| process.rollouts.get(id)).collect();
                let wrong_path = metadata.rollout_path.as_deref().filter(|path| !path.is_empty()).is_some_and(|path|
                    !owners.is_empty() && !owners.iter().any(|rollout| same_file(&rollout.path, Path::new(path))));
                if metadata.archived == Some(true) || wrong_path { core.rejected.insert(key.clone()); core.states.remove(&key); }
                else { core.rejected.remove(&key); }
            }
        }
        core.recover_loaded(found, now);
        self.child_titles.clear();
        for (id, title) in distinct_codex_titles(&codex, &self.database, &index) {
            if self.database.get(&format!("codex:{id}")).is_some_and(DatabaseTitle::child) { self.child_titles.insert(id.clone(), title.clone()); }
            core.update_title(AgentKind::Codex, &id, &title);
        }
        for (path, tail) in &logs.tails {
            if tail.agent == AgentKind::Claude && claude.contains(&tail.metadata.id) {
                if let Some(title) = claude_title(path, &tail.metadata.id) { core.update_title(AgentKind::Claude, &tail.metadata.id, &title); }
            }
        }
        for (id, path) in &logs.bound_claude {
            if claude.contains(id) { if let Some(title) = claude_title(path, id) { core.update_title(AgentKind::Claude, id, &title); } }
        }
        self.suppress_ended_hermes(core);
        for id in &hermes {
            if let Some(metadata) = self.database.get(&format!("hermes:{id}")) { core.update_title(AgentKind::Hermes, id, &metadata.title); }
        }
    }

    fn suppress_ended_hermes(&self, core: &mut Core) {
        for (key, metadata) in self.database.iter().filter(|(_, metadata)| metadata.agent == "hermes") {
            if metadata.ended == Some(true) {
                core.states.remove(key);
                core.ended_hermes.insert(key.clone());
                while core.ended_hermes.len() > MAX_NOTICES {
                    if let Some(oldest) = core.ended_hermes.iter().next().cloned() { core.ended_hermes.remove(&oldest); }
                }
            } else if metadata.ended == Some(false) {
                core.ended_hermes.remove(key);
            }
        }
    }
}

fn same_file(first: &Path, second: &Path) -> bool {
    first == second || fs::metadata(first).ok().zip(fs::metadata(second).ok()).is_some_and(|(a, b)| a.is_file() && b.is_file() && identity(&a) == identity(&b))
}

fn private_dir(path: &Path) -> io::Result<()> {
    if !path.exists() {
        let mut builder = fs::DirBuilder::new(); builder.recursive(true);
        #[cfg(unix)] { use std::os::unix::fs::DirBuilderExt; builder.mode(0o700); }
        builder.create(path)?;
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() { return Err(io::Error::other("状态目录不是普通目录")); }
    #[cfg(unix)] {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        unsafe extern "C" { fn getuid() -> u32; }
        if metadata.uid() != unsafe { getuid() } { return Err(io::Error::other("状态目录不属于当前用户")); }
        if metadata.permissions().mode() & 0o077 != 0 { fs::set_permissions(path, fs::Permissions::from_mode(0o700))?; }
    }
    Ok(())
}

fn private_file(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > limit { return Err(io::Error::other("文件类型或大小不受支持")); }
    #[cfg(unix)] {
        use std::os::unix::fs::MetadataExt;
        unsafe extern "C" { fn getuid() -> u32; }
        if metadata.uid() != unsafe { getuid() } || metadata.mode() & 0o077 != 0 { return Err(io::Error::other("状态文件权限不安全")); }
    }
    let mut bytes = Vec::new(); File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit { return Err(io::Error::other("文件超过大小限制")); }
    Ok(bytes)
}

fn spool(paths: &Paths, now: u64) -> Vec<Event> { spool_metadata(paths, now, |_, _| {}) }

fn spool_metadata(paths: &Paths, now: u64, mut transcript: impl FnMut(&Event, &str)) -> Vec<Event> {
    let directory = paths.root.join("events");
    if fs::symlink_metadata(&directory).is_err() { return Vec::new(); }
    let mut events = Vec::new();
    if let Ok(entries) = fs::read_dir(directory) {
        // Lifecycle callbacks can share a millisecond. The sink's nanosecond
        // filenames retain their queue order; filesystem enumeration does not.
        let mut entries: Vec<_> = entries.take(256).flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") { continue; }
            if let Ok(bytes) = private_file(&path, 4096) {
                if let Some(event) = decode_spool(&bytes, now) {
                    if let Ok(metadata) = serde_json::from_slice::<SpoolEvent>(&bytes) { if !metadata.transcript.is_empty() { transcript(&event, &metadata.transcript); } }
                    events.push(event);
                }
            }
            // Only entries in our private transport directory are consumed.
            let _ = fs::remove_file(path);
        }
    }
    events
}

struct Marker { path: PathBuf, token: String }
impl Marker {
    fn start(paths: &Paths) -> io::Result<Self> {
        private_dir(&paths.root)?; private_dir(&paths.root.join("events"))?;
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let marker = Self { path: paths.root.join("enabled"), token: format!("{}:{}:{}", std::process::id(), now_ms(), SERIAL.fetch_add(1, Ordering::Relaxed)) };
        marker.touch()?; Ok(marker)
    }
    fn touch(&self) -> io::Result<()> { write_private(&self.path, self.token.as_bytes()) }
}
impl Drop for Marker {
    fn drop(&mut self) { if fs::read(&self.path).is_ok_and(|raw| raw == self.token.as_bytes()) { let _ = fs::remove_file(&self.path); } }
}

fn run_worker(shared: Arc<Mutex<Snapshot>>, stop: Arc<AtomicBool>, refresh: mpsc::Receiver<()>, context: eframe::egui::Context) {
    let paths = Paths::environment();
    let marker = Marker::start(&paths);
    let marker_error = marker.as_ref().err().map(|error| format!("状态事件目录不可用：{error}"));
    let mut logs = Logs::default(); let mut core = Core::default();
    let mut titles = Titles::default();
    let initially_installed = installed(&paths);
    {
        let mut snapshot = shared.lock().unwrap_or_else(|e| e.into_inner());
        initialize_integration_state(&mut snapshot, initially_installed);
    }
    let mut next_marker = Instant::now();
    let mut process_failure_since: Option<Instant> = None;
    let mut integration_state = integration_configs(&paths);
    let mut next_config = Instant::now() + Duration::from_secs(5);
    let mut last_processes = Vec::new();
    while !stop.load(Ordering::Acquire) {
        let now = now_ms();
        if Instant::now() >= next_config { integration_state = integration_configs(&paths); next_config = Instant::now() + Duration::from_secs(5); }
        let mut verified = false;
        let (sessions, error) = match processes() {
            Ok((found, parents)) => {
                verified = true; last_processes = found.clone();
                process_failure_since = None;
                let mut events = logs.poll(&paths, now);
                // Logs and hooks share one event-time ordering, independent of polling order.
                if marker.is_ok() { events.extend(spool_metadata(&paths, now, |event, transcript| {
                    if event.pid.is_some() && match_process(event, &found, &parents).is_some() { logs.bind_claude(transcript, event, &paths); }
                })); }
                events.sort_by_key(|event| (event.at_ms, event.source));
                for event in events { core.apply(event, &found, &parents, now); }
                logs.apply_uncertainty(&mut core);
                logs.backfill_timers(&mut core, now);
                titles.refresh_loaded(&paths, &logs, &mut core, &found, now);
                (core.reconcile(&found, now), marker_error.clone())
            }
            Err(error) => {
                let since = process_failure_since.get_or_insert_with(Instant::now);
                (core.process_query_unavailable(now, since.elapsed().as_millis().min(u64::MAX as u128) as u64), Some(format!("无法读取本地进程：{error}")))
            }
        };
        if stop.load(Ordering::Acquire) { break; }
        let integrations = integration_status(integration_state.clone(), &last_processes, &core, now, verified);
        {
            let mut snapshot = shared.lock().unwrap_or_else(|e| e.into_inner());
            let completions: Vec<_> = core.completions.iter().cloned().collect();
            let notices: Vec<_> = core.notices.iter().cloned().collect();
            if snapshot.sessions != sessions || snapshot.completions != completions || snapshot.notices != notices || snapshot.integrations != integrations || snapshot.error != error || snapshot.scanning {
                snapshot.sessions = sessions; snapshot.completions = completions; snapshot.notices = notices; snapshot.error = error; snapshot.scanning = false;
                snapshot.integrations = integrations;
                context.request_repaint();
            }
        }
        if Instant::now() >= next_marker {
            if let Ok(marker) = &marker { let _ = marker.touch(); }
            next_marker = Instant::now() + Duration::from_secs(15);
        }
        match refresh.recv_timeout(Duration::from_secs(2)) { Ok(()) => while refresh.try_recv().is_ok() {}, Err(mpsc::RecvTimeoutError::Disconnected) => break, Err(mpsc::RecvTimeoutError::Timeout) => {} }
    }
    // Marker drop disables installed transports immediately on normal shutdown.
}

fn initialize_integration_state(snapshot: &mut Snapshot, initially_installed: bool) {
    // An explicit operation can finish while startup is checking tool configuration.
    // Its result always wins, including across monitor disable/enable cycles.
    if !snapshot.integration_busy && snapshot.integration_message.is_none() { snapshot.integrations_installed = initially_installed; }
}

fn write_private(path: &Path, body: &[u8]) -> io::Result<()> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let temporary = path.with_file_name(format!(".qingyu-{}-{}.tmp", std::process::id(), SERIAL.fetch_add(1, Ordering::Relaxed)));
    let mut options = OpenOptions::new(); options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut file = options.open(&temporary)?;
    let result = file.write_all(body).and_then(|_| file.sync_all());
    drop(file);
    let result = result.and_then(|_| fs::rename(&temporary, path));
    if result.is_err() { let _ = fs::remove_file(temporary); }
    result
}

fn read_configuration(path: &Path) -> io::Result<(Vec<u8>, Value)> {
    if !path.exists() && !path.is_symlink() { return Ok((Vec::new(), json!({}))); }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG { return Err(io::Error::other("配置不是普通文件或超过大小限制")); }
    let mut raw = Vec::new(); File::open(path)?.take(MAX_CONFIG + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > MAX_CONFIG { return Err(io::Error::other("配置超过大小限制")); }
    let value = serde_json::from_slice::<UniqueJson>(&raw).map_err(|_| io::Error::other("配置 JSON 无法解析或包含重复设置名，原文件已保留"))?.0;
    if !value.is_object() { return Err(io::Error::other("配置顶层必须是设置对象，原文件已保留")); }
    if let Some(hooks) = value.get("hooks") {
        let hooks = hooks.as_object().ok_or_else(|| io::Error::other("hooks 必须是设置对象"))?;
        for entries in hooks.values() {
            let entries = entries.as_array().ok_or_else(|| io::Error::other("hooks 事件必须是列表"))?;
            for entry in entries {
                let hooks = entry.get("hooks").and_then(Value::as_array).ok_or_else(|| io::Error::other("hook 项目格式不受支持，原文件已保留"))?;
                if hooks.iter().any(|hook| !hook.is_object()) { return Err(io::Error::other("hook 命令格式不受支持，原文件已保留")); }
            }
        }
    }
    Ok((raw, value))
}

// serde_json::Value normally discards duplicate keys. Refuse that ambiguous input
// rather than risk dropping a user's hook or permission during a config rewrite.
struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Value;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { formatter.write_str("JSON with unique setting names") }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Value, E> { Ok(Value::Bool(value)) }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Value, E> { Ok(value.into()) }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Value, E> { Ok(value.into()) }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Value, E> { serde_json::Number::from_f64(value).map(Value::Number).ok_or_else(|| E::custom("invalid number")) }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Value, E> { Ok(Value::String(value.into())) }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Value, E> { Ok(Value::String(value)) }
            fn visit_none<E: serde::de::Error>(self) -> Result<Value, E> { Ok(Value::Null) }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> { Ok(Value::Null) }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
                let mut values = Vec::new(); while let Some(value) = sequence.next_element::<UniqueJson>()? { values.push(value.0); } Ok(Value::Array(values))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut mapping: A) -> Result<Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = mapping.next_key::<String>()? {
                    if values.contains_key(&key) { return Err(serde::de::Error::custom("duplicate setting name")); }
                    values.insert(key, mapping.next_value::<UniqueJson>()?.0);
                }
                Ok(Value::Object(values))
            }
        }
        deserializer.deserialize_any(Visitor).map(Self)
    }
}

fn hook_events(agent: AgentKind) -> Vec<&'static str> {
    let mut events: Vec<_> = HOOK_EVENTS.iter().copied().filter(|event| agent != AgentKind::Claude || *event != "Interrupt").collect();
    if agent == AgentKind::Claude { events.extend_from_slice(CLAUDE_EXTRA_EVENTS); }
    events
}

fn shell_quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\"'\"'")) }

fn hook_command(python: &Path, paths: &Paths, agent: AgentKind, event: &str) -> String {
    format!("{} {} {} {} {} >/dev/null 2>&1 || true", shell_quote(&python.to_string_lossy()), shell_quote(&paths.root.join("sink.py").to_string_lossy()),
        shell_quote(&paths.root.to_string_lossy()), shell_quote(agent.key()), shell_quote(event))
}

fn owned_hook(hook: &Value, paths: &Paths, agent: AgentKind, event: &str) -> bool {
    if hook.get("type").and_then(Value::as_str) != Some("command") { return false; }
    let Some(command) = hook.get("command").and_then(Value::as_str) else { return false };
    let prefix = command_prefix(command);
    if prefix.len() != 5 || prefix[3] != agent.key() || prefix[4] != event || !same_location(Path::new(&prefix[2]), &paths.root) { return false; }
    let alias = Paths { root: PathBuf::from(&prefix[2]), ..paths.clone() };
    prefix.first().is_some_and(|python| {
        let expected = hook_command(Path::new(python), &alias, agent, event);
        expected == command || expected.strip_suffix(" >/dev/null 2>&1 || true").is_some_and(|prefix| format!("{prefix} || true") == command)
    })
}

fn same_location(left: &Path, right: &Path) -> bool {
    left == right || fs::canonicalize(left).ok().zip(fs::canonicalize(right).ok()).is_some_and(|(left, right)| left == right)
}

fn merge_hooks(value: &mut Value, paths: &Paths, agent: AgentKind, python: &Path, install: bool) -> io::Result<bool> {
    let before = value.clone();
    let object = value.as_object_mut().ok_or_else(|| io::Error::other("配置顶层必须是对象"))?;
    if !object.contains_key("hooks") && !install { return Ok(false); }
    let hooks = object.entry("hooks").or_insert_with(|| json!({})).as_object_mut().ok_or_else(|| io::Error::other("hooks 必须是对象"))?;
    for event in hook_events(agent) {
        if !hooks.contains_key(event) && !install { continue; }
        let entries = hooks.entry(event).or_insert_with(|| json!([])).as_array_mut().ok_or_else(|| io::Error::other("hooks 事件必须是列表"))?;
        let mut removed = false;
        let mut owned_empty = Vec::new();
        for (index, entry) in entries.iter_mut().enumerate() {
            let Some(commands) = entry.get_mut("hooks").and_then(Value::as_array_mut) else { return Err(io::Error::other("hook 项目格式不受支持")) };
            let count = commands.len(); commands.retain(|hook| !owned_hook(hook, paths, agent, event));
            if count != commands.len() { removed = true; if commands.is_empty() { owned_empty.push(index); } }
        }
        for index in owned_empty.into_iter().rev() { entries.remove(index); }
        if install {
            entries.push(json!({"hooks":[{"type":"command","command":hook_command(python, paths, agent, event),"timeout":2}]}));
        } else if removed && entries.is_empty() { hooks.remove(event); }
    }
    // An existing empty user hooks object is harmless and remains intact.
    Ok(*value != before)
}

fn hooks_present(path: &Path, paths: &Paths, agent: AgentKind) -> bool {
    let Ok((_, value)) = read_configuration(path) else { return false };
    hook_events(agent).into_iter().any(|event| value.get("hooks").and_then(|hooks| hooks.get(event)).and_then(Value::as_array).is_some_and(|entries|
        entries.iter().filter_map(|entry| entry.get("hooks").and_then(Value::as_array)).flatten().any(|hook| owned_hook(hook, paths, agent, event))))
}

fn hooks_complete(path: &Path, paths: &Paths, agent: AgentKind) -> bool {
    let Ok((_, value)) = read_configuration(path) else { return false };
    hook_events(agent).into_iter().all(|event| value.get("hooks").and_then(|hooks| hooks.get(event)).and_then(Value::as_array).is_some_and(|entries|
        entries.iter().filter_map(|entry| entry.get("hooks").and_then(Value::as_array)).flatten().any(|hook| owned_hook(hook, paths, agent, event))))
}

fn integration_configs(paths: &Paths) -> Vec<Integration> {
    let sink_current = fs::read(paths.root.join("sink.py")).is_ok_and(|bytes| bytes == SINK.as_bytes());
    [AgentKind::Codex, AgentKind::Claude, AgentKind::Hermes, AgentKind::Gemini, AgentKind::OpenCode, AgentKind::Cursor, AgentKind::Aider].into_iter().map(|agent| {
        let (detected, installed) = match agent {
            AgentKind::Codex => (paths.codex.is_dir(), sink_current && hooks_complete(&paths.codex.join("hooks.json"), paths, agent)),
            AgentKind::Claude => (paths.claude.is_dir(), sink_current && hooks_complete(&paths.claude.join("settings.json"), paths, agent)),
            AgentKind::Hermes => {
                let directory = paths.hermes.join("plugins/qingyu-monitor");
                let current = fs::read(directory.join("__init__.py")).is_ok_and(|bytes| plugin_release_matches(HERMES_PLUGIN, &bytes, &paths.root))
                    && fs::read(directory.join("sink.py")).is_ok_and(|bytes| bytes == SINK.as_bytes())
                    && fs::read(directory.join("plugin.yaml")).is_ok_and(|bytes| bytes == HERMES_MANIFEST.as_bytes());
                (paths.hermes.is_dir(), current && configure_hermes(paths, "check").unwrap_or(false))
            }
            _ => (false, false),
        };
        Integration { agent, detected, installed, ready: false, message: String::new() }
    }).collect()
}

fn integration_status(mut configured: Vec<Integration>, found: &[Process], core: &Core, now: u64, verified: bool) -> Vec<Integration> {
    for integration in &mut configured {
        let running = found.iter().any(|process| process.agent == integration.agent);
        integration.detected |= running;
        integration.ready = verified && running && core.received.get(&integration.agent).is_some_and(|at| *at <= now.saturating_add(5000) && now.saturating_sub(*at) <= FRESH_LOG_MS);
        let detail_supported = matches!(integration.agent, AgentKind::Codex | AgentKind::Claude | AgentKind::Hermes);
        integration.message = if !detail_supported {
            if running { "已发现进程；此工具暂仅支持进程识别，不推断任务状态" } else { "此工具暂仅支持进程识别" }
        } else if integration.ready {
            if integration.installed { "已接收并核验真实任务事件" } else if integration.agent == AgentKind::Hermes { "已接收真实事件；集成文件需要修复" } else { "已读取真实会话状态；安装集成可补充权限等待等事件" }
        } else if integration.installed && running { "集成已安装；尚无近期事件，安装后需重启此工具" }
        else if integration.installed { "集成已安装；工具未运行或暂无任务" }
        else if running && integration.agent == AgentKind::Hermes { "已发现进程；需安装状态集成并重启 Hermes 才能读取任务" }
        else if running { "已发现进程；等待本机会话日志，可安装状态集成" }
        else if integration.detected { "已发现配置；工具未运行，状态集成尚未安装" }
        else { "尚未发现此工具的进程或配置" }.into();
    }
    configured
}

fn python_candidates(paths: &Paths) -> Vec<PathBuf> {
    let mut candidates = vec![paths.hermes.join("hermes-agent/venv/bin/python"), paths.hermes.join("hermes-agent/.venv/bin/python"), paths.hermes.join("venv/bin/python")];
    if let Some(search) = std::env::var_os("PATH") { for directory in std::env::split_paths(&search) { candidates.push(directory.join("python3")); } }
    candidates.extend([PathBuf::from("/opt/homebrew/bin/python3"), PathBuf::from("/usr/local/bin/python3"), PathBuf::from("/usr/bin/python3")]);
    let mut seen = HashSet::new(); candidates.retain(|path| path.is_file() && seen.insert(path.clone())); candidates
}

fn hermes_configuration_command(paths: &Paths) -> io::Result<Command> {
    let source = paths.hermes.join("hermes-agent");
    let launcher = source.join(".hermes/bin/hermes");
    if launcher.is_file() && source.join("hermes_bootstrap.py").is_file() {
        // The current Hermes launcher owns its interpreter and dependency generation.
        // Its store Python alone intentionally cannot import Hermes' YAML library.
        let output = command_output(Command::new(&launcher).arg("--print-runtime-command"), Duration::from_secs(2), 16 * 1024)?;
        let runtime: Vec<String> = serde_json::from_slice(&output).map_err(|_| io::Error::other("Hermes 运行时命令无法解析"))?;
        let python = runtime.first().filter(|python| Path::new(python).is_absolute() && Path::new(python).is_file())
            .ok_or_else(|| io::Error::other("Hermes 运行时缺少可用的 Python"))?;
        let bootstrap = format!("import os,sys;os.environ.pop('PYTHONHOME',None);os.environ.pop('PYTHONPATH',None);os.environ.pop('VIRTUAL_ENV',None);sys.path.insert(0,{});import hermes_bootstrap;exec({})",
            serde_json::to_string(&source.to_string_lossy()).map_err(io::Error::other)?, serde_json::to_string(HERMES_CONFIGURE).map_err(io::Error::other)?);
        let mut command = Command::new(python); command.args(["-I", "-c", &bootstrap]);
        return Ok(command);
    }
    let python = python_candidates(paths).into_iter().take(8).find(|python|
        command_output(Command::new(python).args(["-c", "import yaml"]), Duration::from_millis(800), 1024).is_ok())
        .ok_or_else(|| io::Error::other("未找到 Hermes 的 YAML 解析环境；请确认 Hermes 已完成安装"))?;
    let mut command = Command::new(python); command.arg("-c").arg(HERMES_CONFIGURE); Ok(command)
}

fn configure_hermes(paths: &Paths, operation: &str) -> io::Result<bool> {
    let output = command_output(hermes_configuration_command(paths)?.arg(paths.hermes.join("config.yaml")).arg(operation), Duration::from_secs(3), 4096)
        .map_err(|_| io::Error::other("Hermes 配置无法解析或安全更新，原文件已保留"))?;
    Ok(String::from_utf8_lossy(&output).trim() == "installed")
}

fn plugin_source(paths: &Paths) -> String { HERMES_PLUGIN.replace("__QINGYU_MONITOR_ROOT__", &serde_json::to_string(&paths.root.to_string_lossy()).unwrap_or_default()) }

fn legacy_plugin_source(paths: &Paths) -> String { LEGACY_HERMES_PLUGIN.replace("__QINGYU_MONITOR_ROOT__", &serde_json::to_string(&paths.root.to_string_lossy()).unwrap_or_default()) }

#[derive(serde::Serialize, Deserialize)]
struct OwnedAssets { owner: String, version: u8, files: HashMap<String, String> }

fn asset_hash(body: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, body).as_ref().iter().map(|byte| format!("{byte:02x}")).collect()
}

fn plugin_source_root(body: &[u8]) -> Option<String> {
    let source = std::str::from_utf8(body).ok()?;
    let line = source.lines().find_map(|line| line.strip_prefix("_ROOT = Path(")?.strip_suffix(')'))?;
    serde_json::from_str(line).ok()
}

fn plugin_release_matches(template: &str, body: &[u8], expected_root: &Path) -> bool {
    let Some(root) = plugin_source_root(body) else { return false };
    same_location(Path::new(&root), expected_root)
        && template.replace("__QINGYU_MONITOR_ROOT__", &serde_json::to_string(&root).unwrap_or_default()).as_bytes() == body
}

fn known_asset_release(name: &str, body: &[u8], current: &[u8], legacy: &[u8]) -> bool {
    if body == current || body == legacy { return true; }
    match name {
        "sink.py" => body == PREVIOUS_SINK.as_bytes(),
        "plugin.yaml" => body == PREVIOUS_HERMES_MANIFEST.as_bytes() || body == PREVIOUS_RELEASE_HERMES_MANIFEST.as_bytes(),
        "__init__.py" => {
            let Some(expected) = plugin_source_root(current) else { return false };
            [HERMES_PLUGIN, PREVIOUS_RELEASE_HERMES_PLUGIN, PREVIOUS_HERMES_PLUGIN, LEGACY_HERMES_PLUGIN].iter()
                .any(|template| plugin_release_matches(template, body, Path::new(&expected)))
        }
        _ => false,
    }
}

/// Only exact embedded releases or files matching our saved ownership hashes can
/// be repaired. A user edit to either file or manifest never grants replacement.
fn asset_preflight(directory: &Path, files: &[(&str, &[u8], &[u8])]) -> io::Result<()> {
    let manifest = directory.join("qingyu-owned-assets.json");
    let ownership = if manifest.exists() || manifest.is_symlink() {
        let bytes = private_file(&manifest, 8192)?;
        let value: OwnedAssets = serde_json::from_slice(&bytes).map_err(|_| io::Error::other("集成归属清单损坏，文件已保留"))?;
        if value.owner != "qingyu-agent-monitor" || ![1, 2, 3, 4].contains(&value.version) { return Err(io::Error::other("集成归属清单不受支持，文件已保留")); }
        Some(value)
    } else { None };
    for (name, current, legacy) in files {
        let path = directory.join(name);
        if !path.exists() && !path.is_symlink() { continue; }
        let info = fs::symlink_metadata(&path)?;
        if !info.is_file() || info.len() > MAX_CONFIG { return Err(io::Error::other("同名集成不是普通文件，已保留")); }
        let body = fs::read(&path)?; let hash = asset_hash(&body);
        let release = known_asset_release(name, &body, current, legacy);
        let marked = body.starts_with(b"# qingyu-monitor:v") || body.starts_with(b"#!/usr/bin/env python3\n# qingyu-monitor:v") || *name == "plugin.yaml";
        let recorded = ownership.as_ref().and_then(|owner| owner.files.get(*name));
        if !marked || !release || recorded.is_some_and(|known| *known != hash) {
            return Err(io::Error::other("集成文件曾被修改或来源不明，已保留；请先检查该文件"));
        }
    }
    Ok(())
}

fn install_assets(directory: &Path, files: &[(&str, &[u8], &[u8])]) -> io::Result<()> {
    asset_preflight(directory, files)?;
    for (name, body, _) in files { write_private(&directory.join(name), body)?; }
    let ownership = OwnedAssets { owner: "qingyu-agent-monitor".into(), version: 4,
        files: files.iter().map(|(name, body, _)| ((*name).into(), asset_hash(body))).collect() };
    write_private(&directory.join("qingyu-owned-assets.json"), &serde_json::to_vec(&ownership).map_err(io::Error::other)?)
}

fn remove_assets(directory: &Path, files: &[(&str, &[u8], &[u8])]) -> io::Result<()> {
    asset_preflight(directory, files)?;
    for (name, _, _) in files { let path = directory.join(name); if path.exists() { fs::remove_file(path)?; } }
    let manifest = directory.join("qingyu-owned-assets.json"); if manifest.exists() { fs::remove_file(manifest)?; } Ok(())
}

fn change_hooks(paths: &Paths, agent: AgentKind, python: &Path, install: bool) -> io::Result<()> {
    let path = match agent { AgentKind::Codex => paths.codex.join("hooks.json"), AgentKind::Claude => paths.claude.join("settings.json"), _ => return Ok(()) };
    let (raw, mut value) = read_configuration(&path)?;
    if !merge_hooks(&mut value, paths, agent, python, install)? { return Ok(()); }
    let body = serde_json::to_vec_pretty(&value).map_err(io::Error::other)?;
    let current = if path.exists() { fs::read(&path)? } else { Vec::new() };
    if current != raw { return Err(io::Error::other("配置已被其他程序修改，请重试")); }
    backup_configuration(&path, &raw)?;
    write_private(&path, &body)
}

fn backup_configuration(path: &Path, raw: &[u8]) -> io::Result<()> {
    if raw.is_empty() { return Ok(()); }
    let directory = path.parent().ok_or_else(|| io::Error::other("配置路径缺少父目录"))?.join(".qingyu-backups");
    private_dir(&directory)?;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let backup = directory.join(format!("{}.{}-{}-{}.bak", path.file_name().unwrap_or_default().to_string_lossy(), now_ms(), std::process::id(), SERIAL.fetch_add(1, Ordering::Relaxed)));
    write_private(&backup, raw)
}

fn change_hermes(paths: &Paths, install: bool) -> io::Result<()> {
    // Preflight the complete YAML before touching the plugin directory.
    let _ = configure_hermes(paths, "check")?;
    let directory = paths.hermes.join("plugins/qingyu-monitor");
    let source = plugin_source(paths);
    let legacy = legacy_plugin_source(paths);
    let files = [("__init__.py", source.as_bytes(), legacy.as_bytes()), ("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes()), ("plugin.yaml", HERMES_MANIFEST.as_bytes(), LEGACY_HERMES_MANIFEST.as_bytes())];
    if install {
        if directory.exists() {
            if !fs::symlink_metadata(&directory)?.is_dir() { return Err(io::Error::other("同名 Hermes 插件不是普通目录，已保留")); }
            // Refuse to reuse a user-owned plugin directory based on the name alone.
            asset_preflight(&directory, &files)?;
            if !directory.join("qingyu-owned-assets.json").is_file() && files.iter().any(|(name, _, _)| !directory.join(name).is_file()) { return Err(io::Error::other("同名 Hermes 插件不完整且未有归属清单，已保留")); }
        } else {
            private_dir(&directory)?;
        }
        install_assets(&directory, &files)?;
        configure_hermes(paths, "install")?;
    } else {
        configure_hermes(paths, "uninstall")?;
        remove_assets(&directory, &files)?;
        remove_owned_plugin_cache(&directory);
        let _ = fs::remove_dir(&directory); // Unrelated bytecode and user files are preserved.
    }
    Ok(())
}

fn remove_owned_plugin_cache(directory: &Path) {
    let cache = directory.join("__pycache__");
    if !fs::symlink_metadata(&cache).is_ok_and(|metadata| metadata.is_dir()) { return; }
    if let Ok(entries) = fs::read_dir(&cache) {
        for entry in entries.take(32).flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if (name.starts_with("__init__.cpython-") || name.starts_with("sink.cpython-")) && name.ends_with(".pyc")
                && entry.file_type().is_ok_and(|kind| kind.is_file()) { let _ = fs::remove_file(entry.path()); }
        }
    }
    let _ = fs::remove_dir(cache);
}

fn installed(paths: &Paths) -> bool {
    // This means at least one owned integration is installed. Partial installation
    // must still expose Remove/Repair when another tool has an invalid config.
    if paths.codex.is_dir() && hooks_present(&paths.codex.join("hooks.json"), paths, AgentKind::Codex) { return true; }
    if paths.claude.is_dir() && hooks_present(&paths.claude.join("settings.json"), paths, AgentKind::Claude) { return true; }
    if paths.hermes.is_dir() {
        return fs::read(paths.hermes.join("plugins/qingyu-monitor/plugin.yaml")).is_ok_and(|bytes| bytes == HERMES_MANIFEST.as_bytes()) && configure_hermes(paths, "check").unwrap_or(false);
    }
    false
}

fn integrations(paths: &Paths, install: bool) -> String {
    let prepare = || -> io::Result<()> {
        private_dir(&paths.root)?; private_dir(&paths.root.join("events"))?;
        if install { install_assets(&paths.root, &[("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes())])?; }
        Ok(())
    };
    if let Err(error) = prepare() { return format!("无法准备本地状态集成：{error}"); }
    let python = python_candidates(paths).into_iter().next();
    let mut changed = Vec::new(); let mut errors = Vec::new();
    for (agent, directory) in [(AgentKind::Codex, &paths.codex), (AgentKind::Claude, &paths.claude), (AgentKind::Hermes, &paths.hermes)] {
        if !directory.is_dir() { continue; }
        let result = if agent == AgentKind::Hermes { change_hermes(paths, install) }
            else if let Some(python) = &python { change_hooks(paths, agent, python, install) }
            else { Err(io::Error::other("未找到 Python 3；原配置已保留")) };
        match result { Ok(()) => changed.push(agent.label()), Err(error) => errors.push(format!("{}：{error}", agent.label())) }
    }
    if !install && errors.is_empty() {
        if let Err(error) = remove_assets(&paths.root, &[("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes())]) { errors.push(error.to_string()); }
    }
    let mut message = if changed.is_empty() { String::new() } else {
        format!("{}{}。{}", changed.join("、"), if install { " 状态集成已安装" } else { " 状态集成已移除" }, if install { "重新启动对应工具后生效。" } else { "运行中的 Hermes 会话重启后完全卸载。" })
    };
    if !errors.is_empty() { if !message.is_empty() { message.push('\n'); } message.push_str(&errors.join("\n")); }
    if message.is_empty() { message = "未找到 Codex、Claude 或 Hermes 的配置目录，请先启动对应工具。".into(); }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temporary(PathBuf);
    impl Temporary {
        fn new() -> Self {
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!("qingyu-agent-test-{}-{}-{}", std::process::id(), now_ms(), SERIAL.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir_all(&path).unwrap(); Self(path)
        }
        fn paths(&self) -> Paths { Paths { root: self.0.join("data/agent-monitor"), codex: self.0.join("codex"), claude: self.0.join("claude"), hermes: self.0.join("hermes") } }
    }
    impl Drop for Temporary { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

    fn process(agent: AgentKind, pid: u32) -> Process { Process { pid, parent: 1, agent, birth: format!("birth-{pid}"), started_ms: 1,
        service: false, role: ProcessRole::Cli, launcher: false, project: "/project".into(), sessions: HashSet::new(), rollouts: HashMap::new(), session_scan_complete: false } }
    fn event(status: Status, at_ms: u64, turn: &str) -> Event { Event { id: "session".into(), agent: AgentKind::Codex, status, at_ms, source: Source::Log, turn: turn.into(), project: "/project".into(), tool: String::new(), pid: None, heartbeat: false, starts_turn: status == Status::Thinking, ends_turn: matches!(status, Status::Completed | Status::Error | Status::Idle | Status::Stopped), title: String::new(), start_ms: None } }
    fn timestamp(at_ms: u64) -> String { DateTime::<Utc>::from_timestamp_millis(at_ms as i64).unwrap().to_rfc3339() }
    fn codex_line(kind: &str, subtype: &str, at_ms: u64) -> Vec<u8> { let mut body = serde_json::to_vec(&json!({"type":kind,"timestamp":timestamp(at_ms),"payload":{"type":subtype,"turn_id":"turn"}})).unwrap(); body.push(b'\n'); body }

    #[test]
    fn lifecycle_journal_keeps_short_aborts_completions_and_actual_metadata() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
        let mut metadata = LogMeta { id: "session".into(), ..Default::default() };
        for (kind, turn, at) in [("task_started", "one", 10_000), ("turn_aborted", "one", 10_010),
            ("task_started", "two", 10_020), ("task_complete", "two", 12_000)] {
            let body = json!({"type":"event_msg","timestamp":timestamp(at),"payload":{"type":kind,"turn_id":turn,
                "message":"PRIVATE PROMPT AND RESPONSE","reason":"PRIVATE FREE TEXT"}});
            let value = log_event(&serde_json::to_vec(&body).unwrap(), AgentKind::Codex, &mut metadata).unwrap();
            core.apply(value, &found, &parents, 12_000);
        }
        core.update_title(AgentKind::Codex, "session", "Actual conversation title");
        assert_eq!(core.notices.iter().map(|event| event.kind).collect::<Vec<_>>(),
            [NoticeKind::Started, NoticeKind::Interrupted, NoticeKind::Started, NoticeKind::Completed]);
        let notices: Vec<_> = core.notices.iter().collect();
        assert_eq!(notices[0].session.turn_key, notices[1].session.turn_key);
        assert_ne!(notices[1].session.turn_key, notices[2].session.turn_key);
        assert_eq!(notices[2].session.turn_key, notices[3].session.turn_key);
        assert_eq!(notices[3].session.elapsed_seconds(999), Some(2));
        assert!(notices.iter().all(|event| event.session.id == "codex:session" && event.session.title == "Actual conversation title"));
        assert_eq!(notices[1].session.detail, "本轮执行已中断");
        assert_eq!(core.completions.len(), 1); assert!(!format!("{:?}", core.notices).contains("PRIVATE"));
    }

    #[test]
    fn every_agent_terminal_interrupt_is_once_and_late_events_cannot_reopen_it() {
        for agent in [AgentKind::Codex, AgentKind::Claude, AgentKind::Hermes, AgentKind::Gemini, AgentKind::OpenCode, AgentKind::Cursor, AgentKind::Aider] {
            let found = [process(agent, 20)]; let parents = HashMap::new(); let mut core = Core::default();
            let make = |status, at, turn: &str| Event { agent, pid: Some(20), ..event(status, at, turn) };
            core.apply(make(Status::Thinking, 10_000, "one"), &found, &parents, 10_000);
            let key = core.states[&format!("{}:session", agent.key())].session.turn_key;
            core.apply(make(Status::Idle, 11_000, "one"), &found, &parents, 11_000);
            core.apply(make(Status::Idle, 11_100, "one"), &found, &parents, 11_100);
            let mut heartbeat = make(Status::Working, 11_200, "one"); heartbeat.heartbeat = true;
            core.apply(heartbeat, &found, &parents, 11_200);
            core.apply(make(Status::Working, 11_300, "one"), &found, &parents, 11_300);
            core.apply(make(Status::Thinking, 11_400, "one"), &found, &parents, 11_400);
            core.apply(make(Status::Completed, 11_500, "one"), &found, &parents, 11_500);
            let session = &core.states[&format!("{}:session", agent.key())].session;
            assert_eq!(session.status, Status::Idle); assert_eq!(session.turn_key, key); assert_eq!(session.finished_at, Some(11));
            assert!(core.completions.is_empty());
            assert_eq!(core.notices.iter().map(|event| event.kind).collect::<Vec<_>>(), [NoticeKind::Started, NoticeKind::Interrupted]);
            core.apply(make(Status::Thinking, 12_000, "two"), &found, &parents, 12_000);
            core.apply(make(Status::Completed, 13_000, "two"), &found, &parents, 13_000);
            assert_eq!(core.completions.len(), 1); assert_ne!(core.completions[0].session.turn_key, key);
        }
    }

    #[test]
    fn claude_hook_log_aliases_share_start_and_interrupt_notices() {
        let mut core = Core::default(); let found = [process(AgentKind::Claude, 20)]; let parents = HashMap::new();
        let make = |status, at, turn: &str, source| Event { agent: AgentKind::Claude, pid: Some(20), source, ..event(status, at, turn) };
        core.apply(make(Status::Thinking, 10_000, "hook-one", Source::Hook), &found, &parents, 10_000);
        core.apply(make(Status::Thinking, 10_100, "transcript-one", Source::Log), &found, &parents, 10_100);
        core.apply(make(Status::Idle, 11_000, "transcript-one", Source::Log), &found, &parents, 11_000);
        core.apply(make(Status::Idle, 11_100, "hook-one", Source::Hook), &found, &parents, 11_100);
        let mut heartbeat = make(Status::Working, 11_200, "hook-one", Source::Hook); heartbeat.heartbeat = true;
        core.apply(heartbeat, &found, &parents, 11_200);
        assert_eq!(core.notices.len(), 2); assert_eq!(core.notices[0].kind, NoticeKind::Started); assert_eq!(core.notices[1].kind, NoticeKind::Interrupted);
        assert_eq!(core.notices[0].session.turn_key, core.notices[1].session.turn_key); assert!(core.completions.is_empty());
        core.apply(make(Status::Thinking, 12_000, "hook-two", Source::Hook), &found, &parents, 12_000);
        assert_eq!(core.notices.len(), 3); assert_ne!(core.notices[1].session.turn_key, core.notices[2].session.turn_key);
        core.apply(make(Status::Completed, 13_000, "transcript-one", Source::Log), &found, &parents, 13_000);
        assert_eq!(core.notices.len(), 3); assert!(core.completions.is_empty());
    }

    #[test]
    fn permission_waits_and_recoverable_tool_failures_do_not_finish_or_restart_a_turn() {
        let mut core = Core::default(); let found = [process(AgentKind::Claude, 20)]; let parents = HashMap::new();
        let make = |hook: &str, at| decode_spool(&serde_json::to_vec(&json!({"version":1,"agent":"claude","event":hook,
            "session_id":"session","turn_id":"one","at_ms":at,"pid":20,"tool":"Bash","prompt":"PRIVATE"})).unwrap(), at).unwrap();
        for (hook, at) in [("UserPromptSubmit", 10_000), ("PermissionRequest", 11_000), ("PreToolUse", 12_000),
            ("PermissionRequest", 13_000), ("PostToolUseFailure", 14_000), ("PostToolUseFailure", 14_100), ("PreToolUse", 15_000)] {
            core.apply(make(hook, at), &found, &parents, at);
        }
        let before = core.states["claude:session"].session.clone();
        assert_eq!(before.status, Status::Working); assert_eq!(before.started_at, 10); assert_eq!(before.finished_at, None);
        assert!(core.completions.is_empty());
        assert_eq!(core.notices.iter().map(|event| event.kind).collect::<Vec<_>>(), [NoticeKind::Started, NoticeKind::Approval, NoticeKind::Approval, NoticeKind::Interrupted]);
        assert_eq!(core.notices[1].session.detail, "需要审批 · Bash");
        assert_ne!(core.notices[1].session.approval_key,core.notices[2].session.approval_key);
        assert_eq!(core.notices[3].session.detail, "工具执行失败 · Bash");
        assert_eq!(core.notices[3].session.elapsed_seconds(999), Some(4));
        core.apply(make("Stop", 20_000), &found, &parents, 20_000);
        assert_eq!(core.completions.len(), 1); assert_eq!(core.completions[0].session.turn_key, before.turn_key);
        assert_eq!(core.completions[0].session.elapsed_seconds(999), Some(10));
        assert_eq!(core.notices.back().unwrap().kind, NoticeKind::Completed);
        assert!(!format!("{:?}", core.notices).contains("PRIVATE"));
    }

    #[test]
    fn explicit_failures_session_end_and_real_exit_notify_without_success() {
        for terminal in ["StopFailure", "SessionEnd", "Interrupt"] {
            let mut core = Core::default(); let found = [process(AgentKind::Hermes, 20)]; let parents = HashMap::new();
            let make = |hook: &str, at| decode_spool(&serde_json::to_vec(&json!({"version":1,"agent":"hermes","event":hook,
                "session_id":"session","turn_id":"one","at_ms":at,"pid":20})).unwrap(), at).unwrap();
            core.apply(make("UserPromptSubmit", 10_000), &found, &parents, 10_000);
            core.apply(make(terminal, 12_000), &found, &parents, 12_000);
            core.apply(make("Stop", 13_000), &found, &parents, 13_000);
            core.reconcile(&[], 14_000); core.reconcile(&[], 15_000);
            assert!(core.completions.is_empty()); assert_eq!(core.notices.len(), 2);
            assert_eq!(core.notices[1].kind, NoticeKind::Interrupted); assert_eq!(core.notices[1].session.elapsed_seconds(999), Some(2));
        }
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)];
        core.apply(event(Status::Thinking, 10_000, "one"), &found, &HashMap::new(), 10_000);
        core.process_query_unavailable(40_000, PROCESS_QUERY_LEASE_MS);
        assert_eq!(core.notices.len(), 1);
        core.reconcile(&[], 42_000); core.reconcile(&[], 43_000);
        assert_eq!(core.notices.len(), 2); assert_eq!(core.notices[1].session.detail, "执行进程已退出");
        assert_eq!(core.notices[1].session.elapsed_seconds(999), Some(32)); assert!(core.completions.is_empty());
    }

    #[test]
    fn terminal_events_queued_before_exit_close_verified_turns_without_resurrecting_history() {
        for terminal in [Status::Completed, Status::Idle, Status::Error] {
            let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
            let mut start = event(Status::Thinking, 10_000, "one"); start.pid = Some(20);
            core.apply(start, &found, &parents, 10_000);
            let mut end = event(terminal, 12_000, "one"); end.pid = Some(20);
            core.apply(end.clone(), &[], &parents, 13_000);
            core.reconcile(&[], 13_000);
            assert_eq!(core.notices.len(), 2);
            assert_eq!(core.notices[1].kind, if terminal == Status::Completed { NoticeKind::Completed } else { NoticeKind::Interrupted });
            assert_eq!(core.notices[1].session.finished_at, Some(12));
            assert_eq!(core.completions.len(), usize::from(terminal == Status::Completed));
            core.apply(end.clone(), &[], &parents, 14_000); assert_eq!(core.notices.len(), 2);
            let mut unknown = Core::default(); unknown.apply(end.clone(), &[], &parents, 14_000);
            assert!(unknown.states.is_empty()); assert!(unknown.notices.is_empty());
            let mut wrong_owner = Core::default(); wrong_owner.apply(event(Status::Thinking, 10_000, "one"), &found, &parents, 10_000);
            end.pid = Some(21); wrong_owner.apply(end, &[], &parents, 14_000);
            assert_eq!(wrong_owner.notices.len(), 1); assert!(wrong_owner.completions.is_empty());
        }
    }

    #[test]
    fn loaded_session_close_interrupts_a_real_turn_and_process_replacement_keeps_its_completion() {
        let temporary = Temporary::new(); let now = now_ms(); let found = [loaded_process(&temporary, &[LOADED_A])];
        let parents = HashMap::new(); let mut core = Core::default();
        core.recover_loaded(&found, now); assert!(core.notices.is_empty());
        let mut start = event(Status::Thinking, now, "one"); start.id = LOADED_A.into();
        core.apply(start.clone(), &found, &parents, now);
        let mut closed = found.clone(); closed[0].rollouts.clear(); closed[0].sessions.clear();
        core.reconcile(&closed, now + 1000); core.reconcile(&closed, now + 2000);
        assert_eq!(core.notices.len(), 2); assert_eq!(core.notices[1].kind, NoticeKind::Interrupted);
        assert_eq!(core.notices[1].session.detail, "执行中的对话已关闭"); assert!(core.completions.is_empty());

        let mut core = Core::default(); core.apply(start, &found, &parents, now);
        let key = core.notices[0].session.turn_key;
        let mut replacement = found.clone(); replacement[0].pid += 1; replacement[0].birth = "new-process".into(); replacement[0].started_ms = now + 1000;
        let mut done = event(Status::Completed, now + 2000, "one"); done.id = LOADED_A.into();
        core.apply(done, &replacement, &parents, now + 2000);
        // Worker event ingestion precedes the loaded-row metadata refresh, so
        // an old owner's terminal evidence keeps the original turn identity.
        core.recover_loaded(&replacement, now + 2000);
        assert_eq!(core.completions.len(), 1); assert_eq!(core.completions[0].session.turn_key, key);
        assert_eq!(core.notices.len(), 2); assert_eq!(core.notices[1].kind, NoticeKind::Completed);
        assert_eq!(core.notices[1].session.elapsed_seconds(now / 1000 + 999), Some(2));
    }

    #[test]
    fn metadata_process_presence_heartbeats_and_uncertainty_never_create_start_notices() {
        let mut core = Core::default(); let found = [process(AgentKind::Hermes, 20)];
        core.reconcile(&found, 10_000); assert!(core.notices.is_empty());
        let make = |hook: &str, at, heartbeat| decode_spool(&serde_json::to_vec(&json!({"version":1,"agent":"hermes","event":hook,
            "session_id":"session","turn_id":"one","at_ms":at,"pid":20,"heartbeat":heartbeat,"started_at_ms":10_000})).unwrap(), at).unwrap();
        core.apply(make("SessionStart", 10_000, false), &found, &HashMap::new(), 10_000);
        assert_eq!(core.states["hermes:session"].session.turn_key, 0);
        core.apply(make("PreToolUse", 11_000, true), &found, &HashMap::new(), 11_000);
        core.process_query_unavailable(42_000, PROCESS_QUERY_LEASE_MS);
        core.apply(make("PreToolUse", 43_000, true), &found, &HashMap::new(), 43_000);
        assert!(core.notices.is_empty());
        core.apply(make("UserPromptSubmit", 50_000, false), &found, &HashMap::new(), 50_000);
        assert_eq!(core.notices.len(), 1); assert_eq!(core.notices[0].kind, NoticeKind::Started);
        core.apply(make("SessionStart", 51_000, false), &found, &HashMap::new(), 51_000);
        assert_eq!(core.states["hermes:session"].session.status, Status::Thinking); assert_eq!(core.notices.len(), 1);
    }

    #[test]
    fn ended_hermes_database_sessions_reject_orphan_heartbeats_and_allow_a_new_real_turn() {
        let found = [process(AgentKind::Hermes, 20)]; let parents = HashMap::new();
        let make = |hook: &str, turn: &str, at, heartbeat| decode_spool(&serde_json::to_vec(&json!({"version":1,"agent":"hermes","event":hook,
            "session_id":"cron-session","turn_id":turn,"at_ms":at,"pid":20,"heartbeat":heartbeat,"started_at_ms":10_000})).unwrap(), at).unwrap();
        let mut core = Core::default(); core.apply(make("UserPromptSubmit", "cron-turn", 10_000, false), &found, &parents, 10_000);
        let mut titles = Titles::default();
        titles.database.insert("hermes:cron-session".into(), DatabaseTitle {
            agent: "hermes".into(), id: "cron-session".into(), title: "Finished cron".into(), has_name: false,
            name: None, archived: None, rollout_path: None, parent_id: None, parent_title: None, is_subagent: false,
            agent_nickname: None, agent_role: None, ended: Some(true),
        });
        let notices_before = core.notices.len();
        titles.suppress_ended_hermes(&mut core);
        assert!(!core.states.contains_key("hermes:cron-session")); assert!(core.ended_hermes.contains("hermes:cron-session"));
        core.apply(make("PreToolUse", "cron-turn", 11_000, true), &found, &parents, 11_000);
        core.apply(make("Stop", "cron-turn", 12_000, false), &found, &parents, 12_000);
        assert!(!core.states.contains_key("hermes:cron-session")); assert_eq!(core.completions.len(), 0);
        assert_eq!(core.notices.len(), notices_before);

        titles.database.get_mut("hermes:cron-session").unwrap().ended = Some(false);
        titles.suppress_ended_hermes(&mut core);
        assert!(!core.ended_hermes.contains("hermes:cron-session"));
        core.apply(make("UserPromptSubmit", "next-turn", 13_000, false), &found, &parents, 13_000);
        assert_eq!(core.states["hermes:cron-session"].session.status, Status::Thinking);
        assert_eq!(core.states["hermes:cron-session"].turn, "next-turn");
    }

    #[test]
    fn hermes_database_end_marker_filters_stale_cron_heartbeat_before_task_readiness() {
        let temporary = Temporary::new(); let paths = temporary.paths(); fs::create_dir_all(&paths.hermes).unwrap();
        let database = paths.hermes.join("state.db");
        let python = python_candidates(&paths).into_iter().next().expect("Hermes Python is required for bounded metadata reads");
        let create = "import sqlite3,sys;db=sqlite3.connect(sys.argv[1]);db.execute('CREATE TABLE sessions(id TEXT PRIMARY KEY,title TEXT,ended_at REAL,end_reason TEXT,system_prompt TEXT)');db.executemany('INSERT INTO sessions VALUES(?,?,?,?,?)',[('cron-ended','Finished cron',100.0,'cron_complete','PRIVATE PROMPT'),('live-session','Live session',None,None,'PRIVATE PROMPT')]);db.commit();db.close()";
        command_output(Command::new(&python).arg("-c").arg(create).arg(&database), Duration::from_secs(2), 1024).unwrap();
        let found = [process(AgentKind::Hermes, 20)]; let parents = HashMap::new(); let now = now_ms();
        let make = |session: &str, hook: &str, turn: &str, at, heartbeat| decode_spool(&serde_json::to_vec(&json!({"version":1,"agent":"hermes","event":hook,
            "session_id":session,"turn_id":turn,"at_ms":at,"pid":20,"heartbeat":heartbeat,"started_at_ms":now})).unwrap(), at).unwrap();
        let mut core = Core::default();
        core.apply(make("cron-ended", "UserPromptSubmit", "old-cron-turn", now, false), &found, &parents, now);
        core.apply(make("live-session", "UserPromptSubmit", "live-turn", now + 1, false), &found, &parents, now + 1);
        let mut titles = Titles::default(); titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 1);
        assert!(!core.states.contains_key("hermes:cron-ended")); assert!(core.ended_hermes.contains("hermes:cron-ended"));
        assert_eq!(titles.database["hermes:cron-ended"].ended, Some(true)); assert_eq!(titles.database["hermes:live-session"].ended, Some(false));
        let received = core.received[&AgentKind::Hermes]; let notices = core.notices.len();
        core.apply(make("cron-ended", "PreToolUse", "old-cron-turn", now + 2, true), &found, &parents, now + 2);
        assert!(!core.states.contains_key("hermes:cron-ended")); assert_eq!(core.received[&AgentKind::Hermes], received);
        assert_eq!(core.notices.len(), notices); assert!(core.states["hermes:live-session"].session.status.is_active());

        let reopen = "import sqlite3,sys;db=sqlite3.connect(sys.argv[1]);db.execute(\"UPDATE sessions SET ended_at=NULL,end_reason=NULL WHERE id='cron-ended'\");db.commit();db.close()";
        command_output(Command::new(&python).arg("-c").arg(reopen).arg(&database), Duration::from_secs(2), 1024).unwrap();
        core.apply(make("cron-ended", "UserPromptSubmit", "new-turn", now + 4, false), &found, &parents, now + 4);
        titles.next = None; titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 5);
        assert_eq!(titles.database["hermes:cron-ended"].ended, Some(false)); assert!(!core.ended_hermes.contains("hermes:cron-ended"));
        assert_eq!(core.states["hermes:cron-ended"].turn, "new-turn");
    }

    #[test]
    fn lifecycle_journal_is_bounded_and_renames_refresh_every_retained_turn() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
        for index in 0..200 {
            let at = 10_000 + index * 2; let turn = format!("turn-{index}");
            core.apply(event(Status::Thinking, at, &turn), &found, &parents, at);
            core.apply(event(Status::Completed, at + 1, &turn), &found, &parents, at + 1);
        }
        assert_eq!(core.notices.len(), MAX_NOTICES); assert_eq!(core.notice_sequence, 400); assert_eq!(core.completions.len(), MAX_SESSIONS);
        core.update_title(AgentKind::Codex, "session", "Renamed conversation");
        assert!(core.notices.iter().all(|event| event.session.title == "Renamed conversation"));
        assert!(core.completions.iter().all(|event| event.session.title == "Renamed conversation"));
        let before = core.notices.len(); core.reconcile(&[], 12_000); assert_eq!(core.notices.len(), before);
    }

    #[test]
    fn process_detection_requires_an_executable_or_known_interpreter_entrypoint() {
        for (command, kind) in [
            ("/Applications/Codex.app/Contents/Resources/codex app-server --telemetry", AgentKind::Codex),
            ("/usr/bin/node /opt/node_modules/@anthropic-ai/claude-code/cli.js --resume", AgentKind::Claude),
            ("/usr/bin/python3 -m hermes_cli.main", AgentKind::Hermes),
            ("/usr/bin/python3 -I -m hermes_cli.main", AgentKind::Hermes),
            ("/managed/python3 -I -c import os, sys\nfrom hermes_cli.main import main\nsys.exit(main()) gateway", AgentKind::Hermes),
            ("/usr/bin/node /opt/node_modules/@google/gemini-cli/dist/index.js", AgentKind::Gemini),
            ("/usr/local/bin/opencode", AgentKind::OpenCode), ("cursor-agent resume", AgentKind::Cursor),
            ("python3 -m aider", AgentKind::Aider),
        ] { assert_eq!(detect_command(command).unwrap().0, kind, "{command}"); }
        for command in ["rg codex agents", "sh -c codex", "python3 script.py hermes", "node cli.js claude", "python3 -m unrelated codex", "python3 -c print('hermes_cli.main')", "python3 -c print('from hermes_cli.main import main; main()')", "vim /usr/bin/codex", "echo claude", "claude-helper", "Codex.app"] { assert!(detect_command(command).is_none(), "{command}"); }
        assert!(detect_command("codex app-server").unwrap().1);
        assert!(!Status::Running.is_active());
    }

    #[test]
    fn macos_ps_escaped_hermes_launchers_keep_real_newlines_literals_and_gateway_role_separate() {
        let command = "/managed/python3 -I -c import os,sys\nimport hermes_bootstrap\nfrom hermes_cli.main import main\nsys.argv[0] = r'(-script\\.pyw|\\.exe)'\nsys.exit(main())\n gateway run --external-supervisor";
        let escaped = command.replace('\\', "\\\\").replace('\n', "\\012");
        let decoded = process_command_text(&escaped); assert_eq!(decoded, command);
        assert_eq!(detect_command(&decoded), Some((AgentKind::Hermes, true)));
        let literal = "python3 -c print('from hermes_cli.main import main\\012main()')";
        assert!(detect_command(&process_command_text(&literal.replace('\\', "\\\\"))).is_none());
        let runtime = "/managed/python3 -I -c import os, sys, runpy; sys.path.insert(0, '/managed/hermes-agent'); import hermes_bootstrap; runpy.run_module('hermes_cli.main', run_name='__main__', alter_sys=True) gateway run";
        assert_eq!(detect_command(runtime), Some((AgentKind::Hermes, true)));
        assert!(detect_command("python3 -c print(\"import hermes_bootstrap; runpy.run_module('hermes_cli.main', run_name='__main__', alter_sys=True)\")").is_none());
    }

    #[test]
    fn old_completion_cannot_clear_a_new_turn_even_when_it_arrives_later() {
        let mut core = Core::default(); let processes = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
        core.apply(event(Status::Thinking, 1000, "old"), &processes, &parents, 1000);
        core.apply(event(Status::Thinking, 2000, "new"), &processes, &parents, 2000);
        core.apply(event(Status::Completed, 2500, "old"), &processes, &parents, 2500);
        assert_eq!(core.reconcile(&processes, 2500)[0].status, Status::Thinking);
        core.apply(event(Status::Completed, 3000, "new"), &processes, &parents, 3000);
        assert_eq!(core.reconcile(&processes, 3000)[0].status, Status::Completed);
    }

    #[test]
    fn source_order_and_timestamps_are_independent_of_poll_order() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
        let mut waiting = event(Status::Waiting, 2000, "turn"); waiting.source = Source::Hook;
        core.apply(waiting, &found, &parents, 2000);
        core.apply(event(Status::Working, 1000, "turn"), &found, &parents, 2000);
        core.apply(event(Status::Working, 2000, "turn"), &found, &parents, 2000);
        assert_eq!(core.reconcile(&found, 2000)[0].status, Status::Waiting);
        core.apply(event(Status::Working, 2001, "turn"), &found, &parents, 2001);
        assert_eq!(core.reconcile(&found, 2001)[0].status, Status::Working);
    }

    #[test]
    fn historical_logs_missing_processes_and_reused_pids_never_become_active() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
        core.apply(event(Status::Working, 1000, "turn"), &found, &parents, FRESH_LOG_MS + 1001);
        assert!(core.states.is_empty());
        core.apply(event(Status::Working, 1000, "turn"), &[], &parents, 1000);
        assert!(core.states.is_empty());
        let mut wrong_pid = event(Status::Working, 1000, "turn"); wrong_pid.pid = Some(99);
        core.apply(wrong_pid, &found, &parents, 1000); assert!(core.states.is_empty());
        let mut recent = found[0].clone(); recent.started_ms = 10_000;
        core.apply(event(Status::Working, 1000, "turn"), &[recent], &parents, 10_000); assert!(core.states.is_empty());
        let fallback = core.reconcile(&found, 10_000); assert_eq!(fallback.len(), 1); assert_eq!(fallback[0].status, Status::Running);
        let mut service = found[0].clone(); service.service = true;
        assert_eq!(core.reconcile(&[service], 10_000)[0].status, Status::Idle);
    }

    #[test]
    fn hook_shell_ancestry_is_correlated_and_exited_pids_are_authoritative() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)];
        let mut hook = event(Status::Working, 1000, "turn"); hook.pid = Some(22);
        core.apply(hook, &found, &HashMap::from([(22, 21), (21, 20)]), 1000);
        assert_eq!(core.reconcile(&found, 1000)[0].pid, Some(20));
        assert_eq!(core.reconcile(&[], 2000)[0].status, Status::Stopped);
        let mut reused = found[0].clone(); reused.birth = "different".into();
        assert!(core.reconcile(&[reused], 2000).iter().any(|session| session.status == Status::Running));
    }

    #[test]
    fn open_rollout_file_metadata_disambiguates_multiple_app_servers() {
        let mut first = process(AgentKind::Codex, 20); first.project = "/".into(); first.service = true;
        let mut second = process(AgentKind::Codex, 21); second.project = "/".into(); second.service = true; second.sessions.insert("session".into());
        let found = [first, second]; let mut core = Core::default(); core.apply(event(Status::Working, 1000, "turn"), &found, &HashMap::new(), 1000);
        assert_eq!(core.states["codex:session"].session.pid, Some(21));
        let mut ambiguous = found.clone(); ambiguous[0].sessions.insert("session".into()); let mut core = Core::default(); core.apply(event(Status::Working, 1000, "turn"), &ambiguous, &HashMap::new(), 1000); assert!(core.states.is_empty());
    }

    #[test]
    fn stale_states_expire_and_hermes_heartbeats_preserve_the_current_turn() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
        core.apply(event(Status::Working, 1000, "turn"), &found, &parents, 1000);
        let mut heartbeat = event(Status::Working, 290_000, "turn"); heartbeat.heartbeat = true; heartbeat.source = Source::Hook;
        core.apply(heartbeat, &found, &parents, 290_000);
        assert_eq!(core.reconcile(&found, 301_001)[0].status, Status::Working);
        assert_eq!(core.reconcile(&found, 590_001)[0].status, Status::Unknown);
        assert_eq!(core.reconcile(&found, 900_001)[0].status, Status::Running);
    }

    #[test]
    fn session_count_is_bounded() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)];
        for i in 0..200 { let mut value = event(Status::Working, 1000 + i, "turn"); value.id = format!("session-{i}"); core.apply(value, &found, &HashMap::new(), 1000 + i); }
        assert_eq!(core.states.len(), MAX_SESSIONS); assert!(core.reconcile(&found, 2000).len() <= MAX_SESSIONS);
    }

    #[test]
    fn codex_and_claude_parse_only_status_metadata() {
        let now = now_ms(); let mut metadata = LogMeta::default();
        let record = json!({"type":"session_meta","timestamp":timestamp(now),"payload":{"id":"session","cwd":"/project","instructions":"secret instructions"}});
        assert_eq!(log_event(&serde_json::to_vec(&record).unwrap(), AgentKind::Codex, &mut metadata).unwrap().status, Status::Idle);
        let record = json!({"type":"response_item","timestamp":timestamp(now),"payload":{"type":"function_call","name":"exec_command","arguments":"SECRET INPUT"}});
        let parsed = log_event(&serde_json::to_vec(&record).unwrap(), AgentKind::Codex, &mut metadata).unwrap();
        assert_eq!(parsed.tool, "exec_command"); assert!(!format!("{parsed:?}").contains("SECRET"));
        let record = json!({"type":"assistant","timestamp":timestamp(now),"sessionId":"claude-session","cwd":"/project","message":{"content":[{"type":"text","text":"SECRET RESPONSE"},{"type":"tool_use","name":"Bash","input":{"command":"SECRET INPUT"}}]}});
        let parsed = log_event(&serde_json::to_vec(&record).unwrap(), AgentKind::Claude, &mut metadata).unwrap();
        assert_eq!(parsed.status, Status::Working); assert_eq!(parsed.tool, "Bash"); assert!(!format!("{parsed:?}").contains("SECRET"));
        let record = json!({"type":"user","timestamp":timestamp(now),"sessionId":"claude-session","message":{"content":"SECRET PROMPT"}});
        assert_eq!(log_event(&serde_json::to_vec(&record).unwrap(), AgentKind::Claude, &mut metadata).unwrap().status, Status::Thinking);
    }

    #[test]
    fn tail_handles_partial_lines_oversize_records_and_bounded_reads() {
        let temporary = Temporary::new(); let path = temporary.0.join("session.jsonl"); let now = now_ms();
        let line = codex_line("event_msg", "task_started", now); let split = line.len() / 2;
        fs::write(&path, &line[..split]).unwrap(); let mut file = File::open(&path).unwrap(); let info = file.metadata().unwrap();
        let mut tail = Tail::open(&path, AgentKind::Codex, &mut file, &info, now).unwrap();
        assert!(tail.read(&mut file, &mut READ_BUDGET.clone(), now).unwrap().is_empty());
        OpenOptions::new().append(true).open(&path).unwrap().write_all(&line[split..]).unwrap();
        assert_eq!(tail.read(&mut file, &mut READ_BUDGET.clone(), now).unwrap().len(), 1);
        assert!(tail.read(&mut file, &mut READ_BUDGET.clone(), now).unwrap().is_empty());
        let mut oversized = vec![b'x'; MAX_LINE + 100]; oversized.push(b'\n'); oversized.extend(codex_line("event_msg", "task_complete", now));
        OpenOptions::new().append(true).open(&path).unwrap().write_all(&oversized).unwrap();
        let parsed = tail.read(&mut file, &mut READ_BUDGET.clone(), now).unwrap(); assert_eq!(parsed.len(), 1); assert_eq!(parsed[0].status, Status::Completed);
        assert!(tail.pending.len() <= MAX_LINE);
        OpenOptions::new().append(true).open(&path).unwrap().write_all(&vec![b'x'; 200_000]).unwrap();
        let before = tail.offset; let mut budget = 100; tail.read(&mut file, &mut budget, now).unwrap(); assert_eq!(tail.offset - before, 100); assert_eq!(budget, 0);
    }

    #[test]
    fn incremental_poll_recovers_from_truncation_and_replacement() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let directory = paths.claude.join("projects/project"); fs::create_dir_all(&directory).unwrap();
        let path = directory.join("session.jsonl"); let now = now_ms();
        let make = |kind: &str, subtype: &str| { let mut bytes = serde_json::to_vec(&json!({"type":kind,"subtype":subtype,"sessionId":"session","timestamp":timestamp(now),"message":{"content":"ignored"}})).unwrap(); bytes.push(b'\n'); bytes };
        fs::write(&path, make("user", "")).unwrap(); let mut logs = Logs::default(); assert_eq!(logs.poll(&paths, now)[0].status, Status::Thinking); assert!(logs.poll(&paths, now).is_empty());
        fs::write(&path, b"{}\n").unwrap(); assert!(logs.poll(&paths, now).is_empty());
        OpenOptions::new().append(true).open(&path).unwrap().write_all(&make("system", "turn_duration")).unwrap();
        assert_eq!(logs.poll(&paths, now)[0].status, Status::Completed);
        let replacement = directory.join("replacement.tmp"); fs::write(&replacement, make("user", "")).unwrap(); fs::rename(&replacement, &path).unwrap();
        assert_eq!(logs.poll(&paths, now)[0].status, Status::Thinking);
    }

    #[test]
    fn log_discovery_and_initial_tail_are_bounded() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let directory = paths.claude.join("projects/project"); fs::create_dir_all(&directory).unwrap(); let now = now_ms();
        for i in 0..90 { fs::write(directory.join(format!("{i}.jsonl")), b"{}\n").unwrap(); }
        assert_eq!(discover_logs(&paths, now).len(), MAX_FILES);
        let path = temporary.0.join("large.jsonl"); let mut body = vec![b'x'; MAX_LINE * 3]; body.push(b'\n'); body.extend(codex_line("event_msg", "task_started", now)); fs::write(&path, body).unwrap();
        let mut file = File::open(&path).unwrap(); let info = file.metadata().unwrap(); let mut tail = Tail::open(&path, AgentKind::Codex, &mut file, &info, now).unwrap();
        assert!(info.len() - tail.offset <= MAX_LINE as u64); assert_eq!(tail.read(&mut file, &mut READ_BUDGET.clone(), now).unwrap().len(), 1);
    }

    #[test]
    fn hook_install_is_idempotent_preserves_permissions_and_removes_only_owned_commands() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let python = Path::new("/path with 'quote/python3");
        let original = json!({"permissions":{"allow":["Read"],"deny":["Bash(secret)"]},"env":{"EXAMPLE":"value"},"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"my-existing-hook"}]},{"hooks":[]}]}});
        let mut value = original.clone(); assert!(merge_hooks(&mut value, &paths, AgentKind::Claude, python, true).unwrap());
        let once = value.clone(); assert!(!merge_hooks(&mut value, &paths, AgentKind::Claude, python, true).unwrap()); assert_eq!(value, once);
        assert_eq!(value["permissions"], original["permissions"]); assert_eq!(value["env"], original["env"]);
        assert!(merge_hooks(&mut value, &paths, AgentKind::Claude, python, false).unwrap());
        assert_eq!(value, original);
        assert!(!merge_hooks(&mut value, &paths, AgentKind::Claude, python, false).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn hook_repair_collapses_symlink_aliases_without_claiming_other_commands() {
        let temporary = Temporary::new(); let paths = temporary.paths(); private_dir(&paths.root).unwrap();
        let alias = Paths { root: temporary.0.join("installed-data"), ..paths.clone() };
        std::os::unix::fs::symlink(&paths.root, &alias.root).unwrap();
        let python = Path::new("/existing/python3"); let mut value = json!({"permissions":{"allow":["Read"]}});
        merge_hooks(&mut value, &paths, AgentKind::Claude, python, true).unwrap();
        for event in hook_events(AgentKind::Claude) {
            value["hooks"][event].as_array_mut().unwrap().push(json!({"hooks":[{"type":"command","command":hook_command(python,&alias,AgentKind::Claude,event)}]}));
        }
        let foreign = format!("{}; echo user-customization", hook_command(python, &paths, AgentKind::Claude, "Stop"));
        value["hooks"]["Stop"].as_array_mut().unwrap().push(json!({"matcher":"user","hooks":[{"type":"command","command":foreign}]}));
        assert!(merge_hooks(&mut value, &alias, AgentKind::Claude, python, true).unwrap());
        for event in hook_events(AgentKind::Claude) {
            let owned = value["hooks"][event].as_array().unwrap().iter().flat_map(|entry| entry["hooks"].as_array().unwrap())
                .filter(|hook| owned_hook(hook, &paths, AgentKind::Claude, event)).count();
            assert_eq!(owned, 1, "{event}");
        }
        let once = value.clone(); assert!(!merge_hooks(&mut value, &alias, AgentKind::Claude, python, true).unwrap()); assert_eq!(value, once);
        merge_hooks(&mut value, &paths, AgentKind::Claude, python, false).unwrap();
        assert_eq!(value["permissions"], json!({"allow":["Read"]}));
        assert_eq!(value["hooks"]["Stop"][0]["hooks"][0]["command"], foreign);
    }

    #[test]
    fn hook_configuration_changes_have_private_backups_and_idempotent_repair_does_not_write() {
        let temporary = Temporary::new(); let paths = temporary.paths(); fs::create_dir_all(&paths.claude).unwrap();
        let path = paths.claude.join("settings.json"); let original = b"{\"permissions\":{\"allow\":[\"Read\"]}}";
        fs::write(&path, original).unwrap();
        change_hooks(&paths, AgentKind::Claude, Path::new("python3"), true).unwrap();
        let directory = paths.claude.join(".qingyu-backups"); let backup = fs::read_dir(&directory).unwrap().next().unwrap().unwrap().path();
        assert_eq!(private_file(&backup, MAX_CONFIG).unwrap(), original);
        let once = fs::read(&path).unwrap(); change_hooks(&paths, AgentKind::Claude, Path::new("python3"), true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), once); assert_eq!(fs::read_dir(directory).unwrap().count(), 1);
    }

    #[test]
    fn malformed_duplicate_and_symlink_configurations_are_preserved() {
        let temporary = Temporary::new(); let paths = temporary.paths(); fs::create_dir_all(&paths.codex).unwrap(); let path = paths.codex.join("hooks.json");
        for bytes in [b"{ malformed".as_slice(), b"[]", b"{\"hooks\":[]}", b"{\"permissions\":{\"allow\":[1],\"allow\":[2]}}"] {
            fs::write(&path, bytes).unwrap(); assert!(change_hooks(&paths, AgentKind::Codex, Path::new("python3"), true).is_err()); assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        #[cfg(unix)] {
            fs::remove_file(&path).unwrap(); let target = temporary.0.join("target"); fs::write(&target, b"{}").unwrap(); std::os::unix::fs::symlink(&target, &path).unwrap();
            assert!(change_hooks(&paths, AgentKind::Codex, Path::new("python3"), true).is_err()); assert_eq!(fs::read(target).unwrap(), b"{}");
        }
    }

    #[test]
    fn spool_and_marker_are_private_bounded_and_removed_on_shutdown() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let now = now_ms(); let marker = Marker::start(&paths).unwrap();
        assert!(paths.root.join("enabled").exists());
        let body = serde_json::to_vec(&json!({"version":1,"agent":"hermes","event":"PreToolUse","session_id":"session","at_ms":now,"pid":20,"tool":"terminal"})).unwrap();
        write_private(&paths.root.join("events/event.json"), &body).unwrap(); let events = spool(&paths, now); assert_eq!(events.len(), 1); assert_eq!(events[0].agent, AgentKind::Hermes); assert_eq!(events[0].status, Status::Working);
        assert!(!paths.root.join("events/event.json").exists());
        assert!(decode_spool(&body, now + FRESH_LOG_MS + 1).is_none());
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&paths.root).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(fs::metadata(paths.root.join("enabled")).unwrap().permissions().mode() & 0o777, 0o600);
            let target = temporary.0.join("unrelated.json"); fs::write(&target, &body).unwrap(); std::os::unix::fs::symlink(&target, paths.root.join("events/link.json")).unwrap(); assert!(spool(&paths, now).is_empty()); assert!(target.exists());
        }
        drop(marker); assert!(!paths.root.join("enabled").exists());
    }

    #[test]
    fn same_millisecond_spool_order_preserves_concurrent_hermes_waits_approvals_and_terminal_deduplication() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let now = 10_000;
        let _marker = Marker::start(&paths).unwrap();
        let steps = [("one", "UserPromptSubmit"), ("two", "UserPromptSubmit"), ("one", "Waiting"),
            ("two", "PermissionRequest"), ("two", "PermissionRequest"), ("one", "Resume"),
            ("two", "Resume"), ("one", "Stop"), ("one", "Stop"), ("two", "Interrupt"), ("two", "Stop")];
        // Insert in reverse so a filesystem's incidental enumeration cannot
        // stand in for the sink's explicit emission order.
        for (index, (session, hook)) in steps.iter().enumerate().rev() {
            let body = serde_json::to_vec(&json!({"version":1,"agent":"hermes","event":hook,"session_id":session,
                "turn_id":format!("turn-{session}"),"at_ms":now,"pid":20,"started_at_ms":now,"title":format!("Hermes {session}")})).unwrap();
            write_private(&paths.root.join("events").join(format!("10000000000{index:03}-20-test.json")), &body).unwrap();
        }
        let mut core = Core::default(); let mut gateway = process(AgentKind::Hermes, 20); gateway.service = true;
        for event in spool(&paths, now) { core.apply(event, std::slice::from_ref(&gateway), &HashMap::new(), now); }
        assert_eq!(core.states.len(), 2); assert_eq!(core.completions.len(), 1);
        assert_eq!(core.completions[0].session.id, "hermes:one");
        assert_eq!(core.states["hermes:one"].session.status, Status::Completed);
        assert_eq!(core.states["hermes:two"].session.status, Status::Idle);
        assert_eq!(core.notices.iter().map(|notice| notice.kind).collect::<Vec<_>>(),
            [NoticeKind::Started, NoticeKind::Started, NoticeKind::Waiting, NoticeKind::Approval, NoticeKind::Completed, NoticeKind::Interrupted]);
        assert_eq!(core.states["hermes:one"].session.started_at, 10);
        assert_eq!(core.states["hermes:two"].session.started_at, 10);
        assert!(core.notices.iter().all(|notice| notice.session.title.starts_with("Hermes ")));
    }

    #[test]
    fn disabled_monitor_never_starts_a_worker() {
        let mut monitor = Monitor::default(); assert!(monitor.worker.is_none()); assert_eq!(monitor.snapshot(), Snapshot::default());
        monitor.update(false, &eframe::egui::Context::default()); monitor.refresh(); assert!(monitor.worker.is_none());
    }

    #[test]
    fn initial_scan_cannot_overwrite_an_in_progress_or_completed_integration_action() {
        let mut snapshot = Snapshot::default(); initialize_integration_state(&mut snapshot, true); assert!(snapshot.integrations_installed);
        snapshot.integration_busy = true; snapshot.integration_message = Some("installing".into());
        initialize_integration_state(&mut snapshot, false); assert!(snapshot.integrations_installed);
        snapshot.integration_busy = false; snapshot.integration_message = Some("installed".into());
        initialize_integration_state(&mut snapshot, false); assert!(snapshot.integrations_installed);
        snapshot.integrations_installed = false; snapshot.integration_message = Some("uninstalled".into());
        initialize_integration_state(&mut snapshot, true); assert!(!snapshot.integrations_installed);
    }

    #[test]
    fn task_timer_survives_tools_heartbeats_and_duplicate_starts_then_freezes() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
        core.apply(event(Status::Thinking, 10_000, "turn"), &found, &parents, 10_000);
        core.apply(event(Status::Thinking, 11_000, "turn"), &found, &parents, 11_000);
        core.apply(event(Status::Working, 25_000, "turn"), &found, &parents, 25_000);
        let mut heartbeat = event(Status::Working, 35_000, "turn"); heartbeat.heartbeat = true;
        core.apply(heartbeat, &found, &parents, 35_000);
        let session = &core.reconcile(&found, 35_000)[0]; assert_eq!(session.started_at, 10); assert_eq!(session.elapsed_seconds(35), Some(25)); assert!(!session.process_only);
        core.apply(event(Status::Completed, 40_000, "turn"), &found, &parents, 40_000);
        let session = &core.reconcile(&found, 50_000)[0]; assert_eq!(session.finished_at, Some(40)); assert_eq!(session.elapsed_seconds(500), Some(30));
        let session = &core.reconcile(&[], 60_000)[0]; assert_eq!(session.status, Status::Stopped); assert_eq!(session.finished_at, Some(40));
        core.apply(event(Status::Thinking, 70_000, "next"), &found, &parents, 70_000);
        let session = &core.reconcile(&found, 75_000)[0]; assert_eq!(session.started_at, 70); assert_eq!(session.finished_at, None); assert_eq!(session.elapsed_seconds(75), Some(5));
    }

    #[test]
    fn unknown_task_start_stays_unknown_and_process_uptime_is_explicit() {
        let mut core = Core::default(); let mut native = process(AgentKind::Codex, 20); native.started_ms = 10_000; let found = [native];
        let session = &core.reconcile(&found, 50_000)[0]; assert!(session.process_only); assert_eq!(session.started_at, 10); assert_eq!(session.elapsed_seconds(50), Some(40));
        core.apply(event(Status::Working, 50_000, "turn"), &found, &HashMap::new(), 50_000);
        let session = &core.reconcile(&found, 50_000)[0]; assert!(!session.process_only); assert_eq!(session.started_at, 0); assert_eq!(session.elapsed_seconds(50), None);
        let mut core = Core::default(); core.apply(event(Status::Thinking, 20_000, "turn"), &found, &HashMap::new(), 20_000);
        let session = &core.reconcile(&[], 30_000)[0]; assert_eq!(session.finished_at, Some(30)); assert_eq!(session.elapsed_seconds(999), Some(10));
        let mut rolled_back = session.clone(); rolled_back.finished_at = None; assert_eq!(rolled_back.elapsed_seconds(1), Some(0)); rolled_back.finished_at = Some(1); assert_eq!(rolled_back.elapsed_seconds(999), Some(0));
    }

    #[test]
    fn resuming_the_same_conversation_in_a_new_process_replaces_the_exited_instance() {
        let mut core = Core::default(); let old = [process(AgentKind::Codex, 20)];
        core.apply(event(Status::Thinking, 10_000, "old"), &old, &HashMap::new(), 10_000);
        let mut restarted = process(AgentKind::Codex, 21); restarted.started_ms = 15_000; let found = [restarted];
        core.apply(event(Status::Thinking, 20_000, "new"), &found, &HashMap::new(), 20_000);
        let session = &core.reconcile(&found, 20_000)[0]; assert_eq!(session.pid, Some(21)); assert_eq!(session.started_at, 20); assert_eq!(session.status, Status::Thinking);
        let mut delayed = event(Status::Completed, 21_000, "old"); delayed.pid = Some(20); delayed.source = Source::Hook;
        core.apply(delayed, &found, &HashMap::new(), 21_000); assert_eq!(core.reconcile(&found, 21_000)[0].status, Status::Thinking);
    }

    #[test]
    fn error_timer_freezes_and_tool_results_are_not_new_prompt_starts() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)];
        core.apply(event(Status::Thinking, 10_000, "turn"), &found, &HashMap::new(), 10_000);
        core.apply(event(Status::Error, 20_000, "turn"), &found, &HashMap::new(), 20_000);
        assert_eq!(core.reconcile(&found, 20_000)[0].elapsed_seconds(999), Some(10));
        let mut metadata = LogMeta { id: "session".into(), turn: "prompt-1".into(), ..Default::default() };
        let result = json!({"type":"user","timestamp":timestamp(30_000),"sessionId":"session","uuid":"tool-result-id","message":{"content":[{"type":"tool_result","content":"not a title or prompt start"}]}});
        let parsed = log_event(&serde_json::to_vec(&result).unwrap(), AgentKind::Claude, &mut metadata).unwrap(); assert!(!parsed.starts_turn); assert_eq!(parsed.turn, "prompt-1");
        let prompt = json!({"type":"user","timestamp":timestamp(40_000),"sessionId":"session","uuid":"prompt-2","message":{"content":"private"}});
        let parsed = log_event(&serde_json::to_vec(&prompt).unwrap(), AgentKind::Claude, &mut metadata).unwrap(); assert!(parsed.starts_turn); assert_eq!(parsed.turn, "prompt-2");
    }

    #[test]
    fn hermes_heartbeat_backfills_the_real_start_before_monitoring_began() {
        let mut core = Core::default(); let found = [process(AgentKind::Hermes, 20)];
        let body = serde_json::to_vec(&json!({"version":1,"agent":"hermes","event":"PreToolUse","at_ms":600_000,"started_at_ms":10_000,"session_id":"session","turn_id":"turn","pid":20,"heartbeat":true})).unwrap();
        let value = decode_spool(&body, 600_000).unwrap(); core.apply(value, &found, &HashMap::new(), 600_000);
        let session = &core.reconcile(&found, 600_000)[0]; assert_eq!(session.started_at, 10); assert_eq!(session.elapsed_seconds(600), Some(590)); assert!(session.status.is_active());
    }

    fn backfill_fixture(temporary: &Temporary, now: u64, completed: bool) -> (Logs, Core, Vec<Process>) {
        let paths = temporary.paths(); let directory = paths.codex.join("sessions").join(Utc::now().format("%Y/%m/%d").to_string()); fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rollout-test.jsonl"); let mut bytes = serde_json::to_vec(&json!({"type":"session_meta","timestamp":timestamp(now - 500_000),"payload":{"id":"session","cwd":"/project"}})).unwrap(); bytes.push(b'\n');
        bytes.extend(codex_line("event_msg", "task_started", now - 400_000));
        for _ in 0..80 { let mut ignored = serde_json::to_vec(&json!({"type":"response_item","payload":{"type":"message","content":"ignored".repeat(150)}})).unwrap(); ignored.push(b'\n'); bytes.extend(ignored); }
        bytes.extend(codex_line("response_item", "function_call", now)); if completed { bytes.extend(codex_line("event_msg", "task_complete", now + 1)); }
        fs::write(path, bytes).unwrap(); let mut logs = Logs::default(); let mut core = Core::default(); let found = vec![process(AgentKind::Codex, 20)];
        for value in logs.poll(&paths, now) { core.apply(value, &found, &HashMap::new(), now); }
        (logs, core, found)
    }

    #[test]
    fn fresh_activity_allows_old_current_turn_timer_metadata_without_activating_history() {
        let temporary = Temporary::new(); let now = now_ms(); let (mut logs, mut core, found) = backfill_fixture(&temporary, now, false);
        assert_eq!(core.reconcile(&found, now)[0].started_at, 0); let before = core.states["codex:session"].at_ms;
        logs.backfill_timers(&mut core, now);
        let session = &core.reconcile(&found, now)[0]; assert_eq!(session.started_at, (now - 400_000) / 1000); assert!(session.status.is_active()); assert_eq!(core.states["codex:session"].at_ms, before);
        let temporary = Temporary::new(); let (mut logs, mut core, found) = backfill_fixture(&temporary, now, true); logs.backfill_timers(&mut core, now);
        let session = &core.reconcile(&found, now)[0]; assert_eq!(session.status, Status::Completed); assert_eq!(session.started_at, 0);
    }

    #[test]
    fn backfill_rejects_another_turn_and_overlarge_unreadable_boundaries() {
        let temporary = Temporary::new(); let now = now_ms(); let (mut logs, mut core, _) = backfill_fixture(&temporary, now, false);
        core.states.get_mut("codex:session").unwrap().turn = "different-current-turn".into(); logs.backfill_timers(&mut core, now); assert_eq!(core.states["codex:session"].session.started_at, 0);
        core.states.get_mut("codex:session").unwrap().turn = "turn".into();
        let path = logs.tails.keys().next().unwrap().clone(); let mut bytes = codex_line("event_msg", "task_started", now - 400_000); bytes.extend(vec![b'x'; MAX_LINE + 100]); bytes.push(b'\n'); bytes.extend(codex_line("response_item", "function_call", now)); fs::write(path, bytes).unwrap();
        logs.backfill_timers(&mut core, now); assert_eq!(core.states["codex:session"].session.started_at, 0);
    }

    #[test]
    fn title_metadata_is_scoped_sanitized_and_independent_of_state_or_duration() {
        let temporary = Temporary::new(); let paths = temporary.paths(); fs::create_dir_all(&paths.codex).unwrap();
        let path = paths.codex.join("session_index.jsonl"); fs::write(&path, b"{\"id\":\"session\",\"thread_name\":\"Real name\"}\n{\"id\":\"other\",\"thread_name\":\"Other session\"}\n").unwrap();
        let ids = HashSet::from(["session".into()]); assert_eq!(codex_index_titles(&path, &ids), HashMap::from([("session".into(), "Real name".into())]));
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; core.apply(event(Status::Thinking, 10_000, "turn"), &found, &HashMap::new(), 10_000); let before = core.reconcile(&found, 10_000)[0].clone();
        let mut titles = Titles::default(); titles.refresh(&paths, &Logs::default(), &mut core); let after = core.reconcile(&found, 10_000)[0].clone(); assert_eq!(after.title, "Real name"); assert_eq!(after.status, before.status); assert_eq!(after.started_at, before.started_at); assert_eq!(after.updated_at, before.updated_at);
        fs::write(&path, b"{\"id\":\"session\",\"thread_name\":\"New title\"}\n").unwrap(); titles.next = None; titles.refresh(&paths, &Logs::default(), &mut core); assert_eq!(core.reconcile(&found, 20_000)[0].title, "New title");
        core.update_title(AgentKind::Claude, "session", "wrong agent"); assert_eq!(core.reconcile(&found, 20_000)[0].title, "New title");
        assert_eq!(title_text("\u{202e} A\n B\0\u{200b}"), "A B"); assert_eq!(title_text(&"长".repeat(300)).chars().count(), 160);
    }

    #[test]
    fn claude_titles_use_explicit_metadata_and_refresh_without_appending_log_records() {
        let temporary = Temporary::new(); let path = temporary.0.join("session.jsonl");
        fs::write(&path, b"{\"type\":\"user\",\"message\":{\"content\":\"PRIVATE PROMPT\"}}\n{\"type\":\"assistant\",\"message\":{\"content\":\"PRIVATE RESPONSE\"}}\n").unwrap(); assert_eq!(claude_title(&path, "session"), None);
        let make = |custom: &str| format!("{{\"type\":\"ai-title\",\"sessionId\":\"session\",\"aiTitle\":\"AI name\"}}\n{{\"type\":\"custom-title\",\"sessionId\":\"session\",\"customTitle\":\"{custom}\"}}\n{{\"type\":\"custom-title\",\"sessionId\":\"other\",\"customTitle\":\"wrong session\"}}\n");
        fs::write(&path, make("Name one")).unwrap(); assert_eq!(claude_title(&path, "session").as_deref(), Some("Name one"));
        let size = fs::metadata(&path).unwrap().len(); fs::write(&path, make("Name two")).unwrap(); assert_eq!(fs::metadata(&path).unwrap().len(), size); assert_eq!(claude_title(&path, "session").as_deref(), Some("Name two"));
    }

    #[test]
    fn old_date_directories_can_contain_fresh_active_logs() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let directory = paths.codex.join("sessions/2020/01/02"); fs::create_dir_all(&directory).unwrap(); let now = now_ms();
        let path = directory.join("rollout-old-session.jsonl"); fs::write(&path, codex_line("event_msg", "task_started", now)).unwrap();
        let mut logs = Logs::default(); let values = logs.poll(&paths, now); assert_eq!(values.len(), 1); assert_eq!(values[0].status, Status::Thinking); assert!(logs.tails.contains_key(&path));
    }

    #[test]
    fn tracked_fresh_tail_survives_leaving_recent_date_discovery() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let directory = paths.codex.join("sessions/2020/01/02"); fs::create_dir_all(&directory).unwrap(); let now = now_ms(); let path = directory.join("rollout-long-lived.jsonl");
        fs::write(&path, codex_line("event_msg", "task_started", now)).unwrap(); let mut logs = Logs::default(); assert_eq!(logs.poll(&paths, now).len(), 1);
        let original_offset = logs.tails[&path].offset;
        logs.discovery.directories.clear(); logs.discovery.next_refresh = Some(Instant::now() + Duration::from_secs(999));
        OpenOptions::new().append(true).open(&path).unwrap().write_all(&codex_line("response_item", "function_call", now + 1000)).unwrap();
        let values = logs.poll(&paths, now + 1000); assert_eq!(values.len(), 1); assert_eq!(values[0].status, Status::Working); assert!(logs.tails[&path].offset > original_offset);
        assert!(logs.poll(&paths, now + 1000).is_empty());
    }

    #[test]
    fn an_unreadable_terminal_record_cannot_leave_log_only_activity_running() {
        let temporary = Temporary::new(); let now = now_ms(); let (mut logs, mut core, found) = backfill_fixture(&temporary, now, false);
        assert!(core.reconcile(&found, now)[0].status.is_active());
        let path = logs.tails.keys().next().unwrap().clone();
        let mut bytes = serde_json::to_vec(&json!({"type":"event_msg","timestamp":timestamp(now + 1),"payload":{"type":"task_complete","last_agent_message":"private".repeat(MAX_LINE / 2)}})).unwrap(); bytes.push(b'\n');
        OpenOptions::new().append(true).open(&path).unwrap().write_all(&bytes).unwrap();
        for value in logs.poll(&temporary.paths(), now + 1) { core.apply(value, &found, &HashMap::new(), now + 1); }
        logs.apply_uncertainty(&mut core); assert_eq!(core.reconcile(&found, now + 1)[0].status, Status::Unknown);
        let mut precise = event(Status::Working, now + 2, "turn"); precise.source = Source::Hook; core.apply(precise, &found, &HashMap::new(), now + 2);
        logs.apply_uncertainty(&mut core); assert_eq!(core.reconcile(&found, now + 2)[0].status, Status::Working);
    }

    #[test]
    fn actual_active_title_metadata_smoke() {
        if std::env::var("QINGYU_AGENT_METADATA_SMOKE").as_deref() != Ok("1") { return; }
        let paths = Paths::environment(); let mut core = Core::default(); let mut titles = Titles::default();
        for cycle in 0..2 {
            if cycle > 0 { thread::sleep(Duration::from_secs(6)); }
            let now = now_ms(); let (found, _) = processes().unwrap();
            // No transcript/event polling: this explicitly verifies pre-existing,
            // quiet conversations without fresh activity or any home mutation.
            titles.next = None; titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now);
            let snapshot = core.reconcile(&found, now);
            let opened: HashSet<_> = found.iter().filter(|process| process.agent == AgentKind::Codex).flat_map(|process| process.rollouts.keys().cloned()).collect();
            let expected: HashSet<_> = opened.iter().filter(|id| !core.rejected.contains(&format!("codex:{id}"))).cloned().collect();
            let recovered: HashSet<_> = snapshot.iter().filter(|session| session.agent == AgentKind::Codex && !session.process_only)
                .filter_map(|session| session.id.strip_prefix("codex:").map(str::to_owned)).collect();
            let indexed = codex_index_titles(&paths.codex.join("session_index.jsonl"), &opened);
            let mut named_expected = 0; let mut named_recovered = 0; let mut manual = 0; let mut match_index = 0; let mut inherited = 0;
            for id in &expected {
                let metadata = titles.database.get(&format!("codex:{id}"));
                let expected_name = metadata.map(|row| row.effective_title(indexed.get(id))).unwrap_or_else(|| indexed.get(id).cloned().unwrap_or_default());
                let actual = snapshot.iter().find(|session| session.id == format!("codex:{id}")).expect("an open non-archived rollout must restore its identity");
                assert_eq!(actual.title == expected_name, true, "resolved title metadata must match the displayed title");
                assert!(!actual.status.is_active()); assert_eq!(actual.started_at, 0); assert_eq!(actual.turn_key, 0);
                if !expected_name.is_empty() { named_expected += 1; if !actual.title.is_empty() { named_recovered += 1; } }
                if metadata.is_some_and(|row| row.has_name && row.name.as_deref().is_some_and(|name| !title_text(name).is_empty())) {
                    manual += 1; if indexed.get(id).is_some_and(|name| *name == actual.title) { match_index += 1; }
                }
                if metadata.is_some_and(|row| row.name.as_deref().is_none_or(|name| title_text(name).is_empty()) && title_text(&row.title).is_empty()
                    && row.parent_title.as_deref().is_some_and(|name| !title_text(name).is_empty())) { inherited += 1; }
            }
            let quiet = |age: u64| -> HashSet<String> { expected.iter().filter(|id| found.iter().filter_map(|process| process.rollouts.get(*id)).any(|rollout| now.saturating_sub(rollout.modified_ms) > age)).cloned().collect() };
            let silent90 = quiet(FRESH_LOG_MS); let silent6h = quiet(6 * 3600_000);
            let generic = snapshot.iter().filter(|session| session.agent == AgentKind::Codex && session.process_only).count();
            let roles = [ProcessRole::Cli, ProcessRole::AppServer, ProcessRole::Helper, ProcessRole::Service].map(|role| found.iter().filter(|process| process.agent == AgentKind::Codex && process.role == role && process.rollouts.is_empty()).count());
            eprintln!("cycle={} opened_rollouts={} recovered={} nonempty_names={}/{} manual_names={} inherited_actual_parent_names={} manual_index_matches={} generic_application_rows={} quiet_90s={}/{} quiet_6h={}/{} excluded_archived_or_path_mismatch={} unmatched_cli_app_helper_service={:?} incomplete_fd_scans={}",
                cycle + 1, opened.len(), recovered.intersection(&expected).count(), named_recovered, named_expected, manual, inherited, match_index, generic,
                recovered.intersection(&silent90).count(), silent90.len(), recovered.intersection(&silent6h).count(), silent6h.len(),
                opened.len() - expected.len(), roles, found.iter().filter(|process| process.agent == AgentKind::Codex && !process.session_scan_complete).count());
            assert!(expected.len() > 0, "local smoke needs at least one real open rollout");
            assert_eq!(recovered.intersection(&expected).count(), expected.len().min(MAX_SESSIONS));
            assert_eq!(named_recovered, named_expected); assert!(core.completions.is_empty());
        }
    }

    #[test]
    fn actual_child_title_identity_metadata_smoke() {
        if std::env::var("QINGYU_AGENT_CHILD_TITLE_SMOKE").as_deref() != Ok("1") { return; }
        let paths = Paths::environment(); let mut core = Core::default(); let mut titles = Titles::default();
        let mut previous = HashMap::<String, (String, String)>::new();
        for cycle in 0..2 {
            if cycle > 0 { thread::sleep(Duration::from_secs(6)); }
            let now = now_ms(); let (found, _) = processes().unwrap();
            titles.next = None; titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now);
            let snapshot = core.reconcile(&found, now);
            let child_ids: HashSet<_> = titles.database.values().filter(|metadata| metadata.child())
                .filter(|metadata| core.states.contains_key(&format!("codex:{}", metadata.id)))
                .map(|metadata| metadata.id.clone()).collect();
            let mut real_nicknames = 0; let mut own_names = 0; let mut parent_matches = 0; let mut sibling_collisions = 0;
            let mut stable = 0; let mut unchanged = 0; let mut siblings = HashSet::new();
            for id in &child_ids {
                let metadata = &titles.database[&format!("codex:{id}")];
                let session = snapshot.iter().find(|session| session.id == format!("codex:{id}")).expect("loaded child identity must be retained");
                if metadata.agent_nickname.as_deref().is_some_and(|name| !title_text(name).is_empty()) { real_nicknames += 1; }
                if !metadata.own_title(None).is_empty() { own_names += 1; }
                assert!(session.title.starts_with("子 Agent · ")); assert!(!session.process_only);
                let parent_title = metadata.parent_id.as_deref().and_then(|parent| snapshot.iter().find(|value| value.id == format!("codex:{parent}")).map(|value| value.title.as_str()))
                    .or(metadata.parent_title.as_deref());
                if parent_title.is_some_and(|parent| !parent.is_empty() && parent == session.title) { parent_matches += 1; }
                if !siblings.insert((metadata.parent_id.clone(), session.title.clone())) { sibling_collisions += 1; }
                let fingerprint = serde_json::to_string(&(metadata.name.as_deref(), metadata.title.as_str(), metadata.has_name,
                    metadata.agent_nickname.as_deref(), metadata.agent_role.as_deref(), metadata.parent_id.as_deref(), metadata.is_subagent)).unwrap();
                if let Some((before, title)) = previous.get(id) {
                    if *before == fingerprint { unchanged += 1; if *title == session.title { stable += 1; } }
                }
                previous.insert(id.clone(), (fingerprint, session.title.clone()));
                assert_eq!(session.started_at, 0); assert_eq!(session.turn_key, 0); assert_eq!(session.approval_key, 0);
            }
            let opened: HashSet<_> = found.iter().filter(|process| process.agent == AgentKind::Codex).flat_map(|process| process.rollouts.keys().cloned()).collect();
            eprintln!("child_title_cycle={} opened_rollout_ids={} recovered_children={} children_with_real_nickname={} children_with_own_name_or_title={} parent_name_matches={} sibling_name_collisions={} stable_unchanged_names={}/{}",
                cycle + 1, opened.len(), child_ids.len(), real_nicknames, own_names, parent_matches, sibling_collisions, stable, unchanged);
            assert!(!child_ids.is_empty(), "local identity smoke requires loaded child metadata");
            assert_eq!(parent_matches, 0); assert_eq!(sibling_collisions, 0); assert_eq!(stable, unchanged);
            assert!(core.notices.is_empty() && core.completions.is_empty(), "metadata refresh cannot create lifecycle events");
        }
    }

    #[test]
    fn actual_lifecycle_metadata_smoke() {
        if std::env::var("QINGYU_AGENT_LIFECYCLE_SMOKE").as_deref() != Ok("1") { return; }
        let paths = Paths::environment(); let mut core = Core::default(); let mut logs = Logs::default();
        let mut found = Vec::new(); let mut latest = HashMap::<String, Event>::new();
        for _ in 0..4 {
            let now = now_ms(); let (processes, parents) = processes().unwrap(); found = processes;
            core.recover_loaded(&found, now);
            let mut events = logs.poll(&paths, now); events.sort_by_key(|event| (event.at_ms, event.source));
            for event in events { core.apply(event, &found, &parents, now); }
            logs.apply_uncertainty(&mut core); core.reconcile(&found, now);
        }
        let now = now_ms();
        let mut paths: Vec<_> = found.iter().flat_map(|process| process.rollouts.iter()).map(|(id, rollout)| (id.clone(), rollout.path.clone())).collect();
        paths.sort_by_key(|(id, _)| !core.states.get(&format!("codex:{id}")).is_some_and(|state| state.session.status.is_active()));
        paths.dedup_by(|a, b| a.0 == b.0);
        let mut terminal = 0; let mut mismatch = 0; let mut unknown_turn = 0; let mut event_counts = [0usize; 4];
        for (id, path) in paths.into_iter().take(8) {
            let mut metadata = LogMeta { id: id.clone(), ..Default::default() };
            for line in title_lines(&path, 256 * 1024, false).unwrap_or_default() {
                if let Some(event) = log_event(&line, AgentKind::Codex, &mut metadata) {
                    let index = if event.starts_turn { 0 } else if event.status == Status::Completed { 1 } else if matches!(event.status, Status::Idle | Status::Error | Status::Stopped) { 2 } else { 3 };
                    event_counts[index] += 1;
                    latest.insert(id.clone(), event);
                }
            }
        }
        for (id, event) in &latest {
            if matches!(event.status, Status::Completed | Status::Idle | Status::Error | Status::Stopped) {
                terminal += 1;
                if core.states.get(&format!("codex:{id}")).is_some_and(|state| state.session.status.is_active()
                    && (state.turn.is_empty() || event.turn.is_empty() || state.turn == event.turn) && event.at_ms >= state.at_ms) { mismatch += 1; }
            }
        }
        for state in core.states.values() { if state.session.status.is_active() && state.turn.is_empty() { unknown_turn += 1; } }
        let active = core.states.values().filter(|state| state.session.status.is_active()).count();
        let backlog = logs.tails.iter().filter(|(path, tail)| fs::metadata(path).is_ok_and(|metadata| metadata.len() > tail.offset)).count();
        eprintln!("lifecycle_probe loaded={} active={} audited={} latest_terminal={} terminal_but_core_active={} active_without_turn_id={} log_backlog_files={} structured_start_complete_cancel_or_error_tool_counts={:?} stale_active={}",
            core.states.len(), active, latest.len(), terminal, mismatch, unknown_turn, backlog, event_counts,
            core.states.values().filter(|state| state.session.status.is_active() && now.saturating_sub(state.seen_ms) > STALE_ACTIVE_MS).count());
        assert_eq!(mismatch, 0, "explicit terminal metadata must end the matching current turn");
    }

    #[test]
    fn a_partial_install_remains_removable_and_malformed_sibling_config_stays_intact() {
        let temporary = Temporary::new(); let paths = temporary.paths(); fs::create_dir_all(&paths.codex).unwrap(); fs::create_dir_all(&paths.claude).unwrap();
        let malformed = b"{unparseable"; fs::write(paths.claude.join("settings.json"), malformed).unwrap();
        let message = integrations(&paths, true);
        assert!(message.contains("Codex 状态集成已安装")); assert!(message.contains("Claude"));
        assert!(installed(&paths)); assert_eq!(fs::read(paths.claude.join("settings.json")).unwrap(), malformed);
        integrations(&paths, false); assert!(!installed(&paths)); assert_eq!(fs::read(paths.claude.join("settings.json")).unwrap(), malformed);
    }

    #[test]
    fn uninstall_cleans_only_generated_owned_bytecode_so_reinstall_is_possible() {
        let temporary = Temporary::new(); let directory = temporary.0.join("plugin"); let cache = directory.join("__pycache__"); fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("__init__.cpython-314.pyc"), b"generated").unwrap(); fs::write(cache.join("sink.cpython-311.pyc"), b"generated").unwrap();
        remove_owned_plugin_cache(&directory); assert!(!cache.exists()); fs::remove_dir(&directory).unwrap(); assert!(!directory.exists());
        fs::create_dir_all(&cache).unwrap(); fs::write(cache.join("user_module.cpython-314.pyc"), b"user file").unwrap(); remove_owned_plugin_cache(&directory); assert!(cache.join("user_module.cpython-314.pyc").exists());
    }

    const LOADED_A: &str = "11111111-1111-4111-8111-111111111111";
    const LOADED_B: &str = "22222222-2222-4222-8222-222222222222";

    fn rollout_file(temporary: &Temporary, id: &str, age_seconds: u64) -> PathBuf {
        let directory = temporary.paths().codex.join("sessions/2020/01/02"); fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("rollout-2020-01-02T00-00-00-{id}.jsonl"));
        let mut file = File::create(&path).unwrap(); file.write_all(b"private body is never opened during identity recovery").unwrap();
        file.set_times(fs::FileTimes::new().set_modified(SystemTime::now() - Duration::from_secs(age_seconds))).unwrap();
        path
    }

    fn loaded_process(temporary: &Temporary, ids: &[&str]) -> Process {
        let mut process = process(AgentKind::Codex, 20); process.role = ProcessRole::AppServer; process.service = true; process.session_scan_complete = true;
        for id in ids { record_open_file(&mut process, &rollout_file(temporary, id, 7 * 3600)); }
        process
    }

    fn thread_metadata(id: &str, name: Option<&str>, title: &str, has_name: bool) -> DatabaseTitle {
        DatabaseTitle { agent: "codex".into(), id: id.into(), name: name.map(str::to_owned), title: title.into(), has_name, archived: Some(false), rollout_path: None,
            parent_id: None, parent_title: None, is_subagent: false, agent_nickname: None, agent_role: None, ended: None }
    }

    #[test]
    fn one_server_recovers_multiple_quiet_loaded_conversations_without_fabricating_turns() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let now = now_ms(); let found = [loaded_process(&temporary, &[LOADED_A, LOADED_B])];
        let mut core = Core::default(); let mut titles = Titles::default();
        titles.database.insert(format!("codex:{LOADED_A}"), thread_metadata(LOADED_A, Some("Manual A"), "Old automatic", true));
        titles.database.insert(format!("codex:{LOADED_B}"), thread_metadata(LOADED_B, Some("Manual B"), "", true));
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now);
        let first = core.reconcile(&found, now);
        assert_eq!(first.len(), 2); assert!(first.iter().all(|session| !session.process_only && session.status == Status::Idle && !session.title.is_empty()
            && session.started_at == 0 && session.finished_at.is_none() && session.elapsed_seconds(now / 1000).is_none() && session.turn_key == 0));
        assert!(core.completions.is_empty());
        for state in core.states.values_mut() { state.seen_ms = now - RETAIN_MS * 2; }
        let second = core.reconcile(&found, now + 7 * 3600_000);
        assert_eq!(second, first); assert!(core.completions.is_empty());
    }

    #[test]
    fn manual_name_wins_and_clear_falls_back_without_stale_index_or_timer_resets() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let now = now_ms(); let found = [loaded_process(&temporary, &[LOADED_A])];
        fs::write(paths.codex.join("session_index.jsonl"), format!("{{\"id\":\"{LOADED_A}\",\"thread_name\":\"Stale indexed name\"}}\n")).unwrap();
        let mut core = Core::default(); core.recover_loaded(&found, now);
        let mut start = event(Status::Thinking, now, "turn"); start.id = LOADED_A.into(); core.apply(start, &found, &HashMap::new(), now);
        let before = core.states[&format!("codex:{LOADED_A}")].session.clone();
        let mut titles = Titles::default();
        for (name, automatic, expected) in [(Some("Manual one"), "Old automatic", "Manual one"), (Some("Manual two"), "Old automatic", "Manual two"), (None, "Automatic fallback", "Automatic fallback"), (Some("  "), "", "")] {
            titles.database.insert(format!("codex:{LOADED_A}"), thread_metadata(LOADED_A, name, automatic, true)); titles.next = None;
            titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 5000);
            let after = &core.states[&format!("codex:{LOADED_A}")].session;
            assert_eq!(after.title, expected); assert_eq!(after.started_at, before.started_at); assert_eq!(after.finished_at, before.finished_at);
            assert_eq!(after.status, before.status); assert_eq!(after.updated_at, before.updated_at); assert_eq!(after.turn_key, before.turn_key); assert!(core.completions.is_empty());
        }
        assert_eq!(thread_metadata(LOADED_A, None, "Legacy title", false).effective_title(Some(&"Index".into())), "Legacy title");
        assert_eq!(thread_metadata(LOADED_A, None, "", false).effective_title(Some(&"Index".into())), "Index");
    }

    #[test]
    fn closed_and_archived_loaded_conversations_are_not_revived_from_cached_names() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let now = now_ms(); let mut found = [loaded_process(&temporary, &[LOADED_A])];
        let mut core = Core::default(); let mut titles = Titles::default();
        let mut metadata = thread_metadata(LOADED_A, Some("Manual"), "", true); metadata.archived = Some(true);
        titles.database.insert(format!("codex:{LOADED_A}"), metadata.clone());
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now);
        assert!(core.reconcile(&found, now).iter().all(|session| session.process_only));
        let mut fresh = event(Status::Working, now, "turn"); fresh.id = LOADED_A.into(); fresh.pid = Some(20); fresh.source = Source::Hook;
        core.apply(fresh, &found, &HashMap::new(), now); assert!(core.states.is_empty());
        metadata.archived = Some(false); titles.database.insert(format!("codex:{LOADED_A}"), metadata); titles.next = None;
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 1); assert_eq!(core.reconcile(&found, now + 1).iter().filter(|session| !session.process_only).count(), 1);
        found[0].sessions.clear(); found[0].rollouts.clear();
        assert!(core.reconcile(&found, now + 2).iter().all(|session| session.process_only));
        titles.next = None; titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 3);
        let mut late = event(Status::Working, now + 3, "turn"); late.id = LOADED_A.into(); core.apply(late, &found, &HashMap::new(), now + 3);
        assert!(core.states.is_empty()); assert!(core.completions.is_empty());
    }

    #[test]
    fn pid_reuse_and_a_different_metadata_rollout_cannot_reuse_old_execution_state() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let now = now_ms(); let mut found = [loaded_process(&temporary, &[LOADED_A])];
        let mut core = Core::default(); core.recover_loaded(&found, now);
        let mut start = event(Status::Thinking, now, "turn"); start.id = LOADED_A.into(); core.apply(start, &found, &HashMap::new(), now);
        found[0].birth = "reused PID".into(); found[0].started_ms = now + 10_000;
        let session = core.reconcile(&found, now + 10_000).remove(0); assert_eq!(session.status, Status::Idle); assert_eq!(session.started_at, 0); assert_eq!(session.turn_key, 0);
        let mut metadata = thread_metadata(LOADED_A, Some("Name"), "", true); metadata.rollout_path = Some(rollout_file(&temporary, LOADED_B, 0).to_string_lossy().into_owned());
        let mut titles = Titles::default(); titles.database.insert(format!("codex:{LOADED_A}"), metadata);
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 10_000); assert!(core.reconcile(&found, now + 10_000).iter().all(|session| session.process_only));
    }

    #[test]
    fn native_app_server_is_not_removed_when_it_starts_helpers_and_helpers_are_application_rows() {
        let mut app = process(AgentKind::Codex, 20); app.role = ProcessRole::AppServer; app.service = true; app.session_scan_complete = true;
        let mut helper = process(AgentKind::Codex, 21); helper.parent = 20; helper.role = ProcessRole::Helper; helper.service = true;
        let mut another = helper.clone(); another.pid = 22; another.birth = "other helper".into();
        let mut launcher = process(AgentKind::Codex, 30); launcher.launcher = true;
        let mut native = process(AgentKind::Codex, 31); native.parent = 30;
        let mut found = vec![app, helper, another, launcher, native]; remove_launch_wrappers(&mut found);
        assert!(found.iter().any(|process| process.pid == 20)); assert!(!found.iter().any(|process| process.pid == 30));
        let rows = Core::default().reconcile(&found, 1000); assert!(rows.iter().all(|session| session.process_only));
        assert_eq!(rows.iter().filter(|session| session.detail.contains("MCP")).count(), 1);
        assert!(rows.iter().any(|session| session.detail.contains("2 个进程")));
    }

    #[test]
    fn a_single_process_file_scan_recovers_multiple_rollouts_and_ignores_other_jsonl_files() {
        let temporary = Temporary::new(); let a = rollout_file(&temporary, LOADED_A, 7 * 3600); let b = rollout_file(&temporary, LOADED_B, 180);
        let unrelated = temporary.0.join("unrelated.jsonl"); fs::write(&unrelated, b"not a rollout").unwrap();
        let mut found = [process(AgentKind::Codex, 20)];
        let raw = format!("p20\nfcwd\nn/project\nf10u\nn{}\nf11u\nn{}\nf12u\nn{}\n", a.display(), b.display(), unrelated.display());
        parse_open_files(&mut found, raw.as_bytes()); assert!(found[0].session_scan_complete); assert_eq!(found[0].rollouts.len(), 2);
        let rows = Core::default().reconcile(&found, now_ms()); assert_eq!(rows.len(), 2); assert!(rows.iter().all(|session| !session.process_only));
        assert!(found[0].rollouts.values().any(|rollout| now_ms().saturating_sub(rollout.modified_ms) > 6 * 3600_000));
    }

    #[test]
    fn restored_identity_and_rename_do_not_emit_completion_or_reopen_completed_turns() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let now = now_ms(); let found = [loaded_process(&temporary, &[LOADED_A])];
        let mut core = Core::default(); core.recover_loaded(&found, now); assert!(core.completions.is_empty());
        for (status, at) in [(Status::Thinking, now), (Status::Working, now + 10), (Status::Completed, now + 20)] {
            let mut value = event(status, at, "turn"); value.id = LOADED_A.into(); core.apply(value, &found, &HashMap::new(), at);
        }
        let before = core.states[&format!("codex:{LOADED_A}")].session.clone(); assert!(before.turn_key > 0); assert_eq!(core.completions.len(), 1);
        let mut titles = Titles::default(); titles.database.insert(format!("codex:{LOADED_A}"), thread_metadata(LOADED_A, Some("Renamed"), "", true));
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 1000); core.reconcile(&found, now + 1000);
        let after = &core.states[&format!("codex:{LOADED_A}")].session; assert_eq!(after.title, "Renamed"); assert_eq!(after.status, Status::Completed);
        assert_eq!(after.started_at, before.started_at); assert_eq!(after.finished_at, before.finished_at); assert_eq!(after.turn_key, before.turn_key); assert_eq!(core.completions.len(), 1);
    }

    #[test]
    fn failed_process_queries_expire_activity_without_claiming_exit_or_success() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
        core.apply(event(Status::Thinking, 100_000, "turn"), &found, &parents, 100_000);
        let before = core.states["codex:session"].session.clone();
        assert_eq!(core.process_query_unavailable(100_001, PROCESS_QUERY_LEASE_MS - 1)[0].status, Status::Thinking);
        let expired = core.process_query_unavailable(100_002, PROCESS_QUERY_LEASE_MS).remove(0);
        assert_eq!(expired.status, Status::Unknown); assert_eq!(expired.started_at, before.started_at); assert_eq!(expired.finished_at, None); assert_eq!(expired.turn_key, before.turn_key);
        assert!(core.completions.is_empty());
        let later = core.process_query_unavailable(90_000, PROCESS_QUERY_LEASE_MS + 10_000).remove(0); assert_eq!(later.status, Status::Unknown);
        assert_eq!(core.reconcile(&found, 100_003)[0].status, Status::Unknown, "process presence is not evidence of resumed work");
        core.apply(event(Status::Working, 100_004, "turn"), &found, &parents, 100_004);
        let resumed = &core.reconcile(&found, 100_004)[0]; assert_eq!(resumed.status, Status::Working); assert_eq!(resumed.started_at, before.started_at); assert_eq!(resumed.turn_key, before.turn_key);
    }

    #[test]
    fn verified_heartbeat_restores_a_long_task_after_an_outage_without_reopening_a_completed_turn() {
        let mut core = Core::default(); let found = [process(AgentKind::Codex, 20)]; let parents = HashMap::new();
        core.apply(event(Status::Thinking, 100_000, "turn"), &found, &parents, 100_000);
        core.process_query_unavailable(130_000, PROCESS_QUERY_LEASE_MS);
        let mut heartbeat = event(Status::Working, 140_000, "turn"); heartbeat.heartbeat = true; heartbeat.source = Source::Hook;
        core.apply(heartbeat.clone(), &found, &parents, 140_000); assert_eq!(core.reconcile(&found, 140_000)[0].status, Status::Working); assert_eq!(core.reconcile(&found, 140_000)[0].started_at, 100);
        core.apply(event(Status::Completed, 150_000, "turn"), &found, &parents, 150_000);
        heartbeat.at_ms = 160_000; core.apply(heartbeat, &found, &parents, 160_000); assert_eq!(core.reconcile(&found, 160_000)[0].status, Status::Completed); assert_eq!(core.completions.len(), 1);
        let mut active = event(Status::Thinking, 200_000, "next"); core.apply(active.clone(), &found, &parents, 200_000);
        for step in 1..100 {
            active.at_ms = 200_000 + step * 60_000; active.starts_turn = false; active.heartbeat = true; active.source = Source::Hook;
            core.apply(active.clone(), &found, &parents, active.at_ms); assert!(core.reconcile(&found, active.at_ms)[0].status.is_active());
        }
    }

    #[test]
    fn busy_logs_cannot_starve_a_quiet_files_explicit_completion() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let directory = paths.claude.join("projects/project"); fs::create_dir_all(&directory).unwrap(); let now = now_ms();
        let line = |id: usize, status: &str| {
            let mut value = serde_json::to_vec(&json!({"type":status,"timestamp":timestamp(now),"sessionId":format!("session-{id}"),"message":{"content":"ignored"}})).unwrap(); value.push(b'\n'); value
        };
        let files: Vec<_> = (0..MAX_FILES).map(|id| directory.join(format!("{id:02}.jsonl"))).collect();
        for (id, path) in files.iter().enumerate() { fs::write(path, line(id, "user")).unwrap(); }
        let mut logs = Logs::default(); logs.poll(&paths, now);
        for path in &files { let mut file = OpenOptions::new().append(true).open(path).unwrap(); file.write_all(&vec![b' '; 512 * 1024]).unwrap(); file.write_all(b"\n").unwrap(); }
        OpenOptions::new().append(true).open(&files[MAX_FILES - 1]).unwrap().write_all(&line(MAX_FILES - 1, "result")).unwrap();
        let mut seen = false;
        for _ in 0..24 {
            let events = logs.poll(&paths, now);
            if events.iter().any(|event| event.id == format!("session-{}", MAX_FILES - 1) && event.status == Status::Completed) { seen = true; break; }
            // The front streams stay permanently busy while the quiet tail closes.
            for path in files.iter().take(8) { OpenOptions::new().append(true).open(path).unwrap().write_all(&vec![b' '; 128 * 1024]).unwrap(); }
        }
        assert!(seen, "bounded round-robin polling must reach the terminal record behind sustained traffic");
    }

    #[test]
    fn parent_conversation_names_do_not_merge_thread_ids_timers_or_completion_deduplication() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let now = now_ms(); let found = [loaded_process(&temporary, &[LOADED_A, LOADED_B])];
        let mut core = Core::default(); core.recover_loaded(&found, now);
        for (index, id) in [LOADED_A, LOADED_B].into_iter().enumerate() { let mut value = event(Status::Thinking, now + index as u64 * 1000, &format!("turn-{index}")); value.id = id.into(); core.apply(value, &found, &HashMap::new(), value_time(now, index)); }
        let a = core.states[&format!("codex:{LOADED_A}")].session.clone(); let b = core.states[&format!("codex:{LOADED_B}")].session.clone();
        let mut titles = Titles::default();
        for id in [LOADED_A, LOADED_B] {
            let mut metadata = thread_metadata(id, None, "", true); metadata.parent_id = Some("33333333-3333-4333-8333-333333333333".into()); metadata.parent_title = Some("Actual parent conversation".into());
            titles.database.insert(format!("codex:{id}"), metadata);
        }
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 2000);
        let after_a = &core.states[&format!("codex:{LOADED_A}")].session; let after_b = &core.states[&format!("codex:{LOADED_B}")].session;
        assert_eq!(after_a.title, "子 Agent · 11111111"); assert_eq!(after_b.title, "子 Agent · 22222222"); assert_ne!(after_b.title, after_a.title);
        assert_eq!(after_a.started_at, a.started_at); assert_eq!(after_b.started_at, b.started_at); assert_ne!(after_a.id, after_b.id); assert_ne!(after_a.turn_key, after_b.turn_key);
        assert!(core.completions.is_empty());
        let mut value = event(Status::Completed, now + 3000, "turn-0"); value.id = LOADED_A.into(); core.apply(value, &found, &HashMap::new(), now + 3000);
        assert_eq!(core.completions.len(), 1); assert_eq!(core.completions[0].session.id, format!("codex:{LOADED_A}")); assert!(core.states[&format!("codex:{LOADED_B}")].session.status.is_active());
        let mut own = thread_metadata(LOADED_A, Some("Own manual name"), "Own old title", true); own.parent_id = Some(LOADED_B.into()); own.parent_title = Some("Parent".into()); assert_eq!(own.effective_title(None), "子 Agent · Own manual name · 11111111");
        own.name = None; assert_eq!(own.effective_title(None), "子 Agent · Own old title · 11111111"); own.title.clear(); assert_eq!(own.effective_title(None), "子 Agent · 11111111");
    }

    #[test]
    fn child_titles_use_their_own_names_or_nicknames_and_never_the_bare_parent_title() {
        let parent = "33333333-3333-4333-8333-333333333333";
        let root = thread_metadata(parent, Some("Real parent conversation"), "Older automatic", true);
        let mut a = thread_metadata(LOADED_A, None, "", true); a.parent_id = Some(parent.into()); a.parent_title = Some("Real parent conversation".into());
        a.agent_nickname = Some("First agent".into()); a.agent_role = Some("Researcher".into());
        let mut b = thread_metadata(LOADED_B, None, "", true); b.parent_id = Some(parent.into()); b.parent_title = a.parent_title.clone();
        b.agent_nickname = Some("Second agent".into()); b.agent_role = Some("Researcher".into());
        let ids = HashSet::from([parent.into(), LOADED_A.into(), LOADED_B.into()]);
        let database = HashMap::from([(format!("codex:{parent}"), root), (format!("codex:{LOADED_A}"), a.clone()), (format!("codex:{LOADED_B}"), b.clone())]);
        let names = distinct_codex_titles(&ids, &database, &HashMap::new());
        assert_eq!(names[parent], "Real parent conversation");
        assert_eq!(names[LOADED_A], "子 Agent · First agent · 11111111");
        assert_eq!(names[LOADED_B], "子 Agent · Second agent · 22222222");
        assert_eq!(names.values().collect::<HashSet<_>>().len(), 3);
        a.name = Some(names[parent].clone()); a.title = "Own automatic".into();
        assert_eq!(a.effective_title(None), "子 Agent · Real parent conversation · 11111111");
        a.name = None; assert_eq!(a.effective_title(None), "子 Agent · Own automatic · 11111111");
        a.title.clear(); assert_eq!(a.effective_title(None), names[LOADED_A]);
        b.parent_id = Some(LOADED_A.into()); b.parent_title = Some(names[LOADED_A].clone());
        assert_eq!(b.effective_title(None), names[LOADED_B], "nested children keep their own identity");
    }

    #[test]
    fn missing_nickname_and_equal_sibling_names_have_stable_distinct_fallbacks() {
        let mut a = thread_metadata(LOADED_A, None, "", false); a.parent_id = Some(LOADED_B.into()); a.parent_title = Some("Parent".into()); a.agent_role = Some("Worker".into());
        let mut b = thread_metadata(LOADED_B, None, "", false); b.parent_id = Some(LOADED_A.into()); b.parent_title = Some("Parent".into()); b.agent_role = Some("Worker".into());
        assert_eq!(a.effective_title(None), "子 Agent · Worker · 11111111");
        assert_ne!(a.effective_title(None), b.effective_title(None));
        a.agent_role = None; assert_eq!(a.effective_title(None), "子 Agent · 11111111");
        a.parent_id = None; a.is_subagent = true; assert_eq!(a.effective_title(None), "子 Agent · 11111111", "a missing/cyclic parent cannot replace the child's identity");
        a.agent_nickname = Some("\u{202e} agent\nname\u{200b}".into()); assert_eq!(a.effective_title(None), "子 Agent · agent name · 11111111");
        a.name = Some("长".repeat(300)); a.has_name = true;
        let label = a.effective_title(None); assert!(label.chars().count() <= 160); assert!(label.ends_with(" · 11111111"));
        assert_eq!(label, a.effective_title(None));
    }

    #[test]
    fn rare_short_id_collisions_expand_only_the_children_and_keep_parent_names_unchanged() {
        let second = "22222222-2222-4222-8222-111111111111";
        let parent = "33333333-3333-4333-8333-333333333333";
        let mut a = thread_metadata(LOADED_A, None, "", true); a.parent_id = Some(parent.into()); a.agent_nickname = Some("Same nickname".into());
        let mut b = thread_metadata(second, None, "", true); b.parent_id = Some(parent.into()); b.agent_nickname = a.agent_nickname.clone();
        let root = thread_metadata(parent, Some("Parent"), "", true);
        let ids = HashSet::from([LOADED_A.into(), second.into(), parent.into()]);
        let database = HashMap::from([(format!("codex:{LOADED_A}"), a), (format!("codex:{second}"), b), (format!("codex:{parent}"), root)]);
        let first = distinct_codex_titles(&ids, &database, &HashMap::new());
        assert_ne!(first[LOADED_A], first[second]); assert_eq!(first[parent], "Parent");
        assert!(first[LOADED_A].ends_with(LOADED_A)); assert!(first[second].ends_with(second));
        assert_eq!(first, distinct_codex_titles(&ids, &database, &HashMap::new()));
    }

    #[test]
    fn child_identity_refresh_updates_approval_and_completion_journals_without_new_lifecycle_events() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let now = now_ms(); let found = [loaded_process(&temporary, &[LOADED_A])];
        let mut core = Core::default(); core.recover_loaded(&found, now);
        let mut metadata = thread_metadata(LOADED_A, None, "", true); metadata.parent_id = Some(LOADED_B.into());
        metadata.parent_title = Some("Parent before".into()); metadata.agent_nickname = Some("Child before".into());
        let mut titles = Titles::default(); titles.database.insert(format!("codex:{LOADED_A}"), metadata.clone());
        let make = |status, at| { let mut value = event(status, at, "child-turn"); value.id = LOADED_A.into(); value };
        core.apply(make(Status::Thinking, now), &found, &HashMap::new(), now);
        core.apply(make(Status::Approval, now + 1000), &found, &HashMap::new(), now + 1000);
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 1000);
        let before = core.states[&format!("codex:{LOADED_A}")].session.clone();
        let notices = core.notices.len(); let notice_sequence = core.notice_sequence; let completed_sequence = core.sequence;
        assert!(before.approval_key > 0); assert_eq!(before.title, "子 Agent · Child before · 11111111");
        metadata.agent_nickname = Some("Child after".into()); metadata.agent_role = Some("Changed role".into()); metadata.parent_title = Some("Parent renamed".into());
        titles.database.insert(format!("codex:{LOADED_A}"), metadata.clone()); titles.next = None;
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 2000);
        let after = core.states[&format!("codex:{LOADED_A}")].session.clone();
        let mut expected = before.clone(); expected.title = "子 Agent · Child after · 11111111".into(); assert_eq!(after, expected);
        assert_eq!(core.notices.len(), notices); assert_eq!(core.notice_sequence, notice_sequence); assert_eq!(core.sequence, completed_sequence);
        assert!(core.notices.iter().all(|notice| notice.session.title == after.title));
        let approval_key = after.approval_key;
        let mut raw = make(Status::Approval, now + 2500); raw.title = "Parent renamed".into();
        core.apply(raw, &found, &HashMap::new(), now + 2500);
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 2500);
        assert_eq!(core.states[&format!("codex:{LOADED_A}")].session.title, after.title, "cached metadata prevents bare hook names returning between polls");
        assert_eq!(core.states[&format!("codex:{LOADED_A}")].session.approval_key, approval_key); assert_eq!(core.notices.len(), notices);
        core.apply(make(Status::Working, now + 3000), &found, &HashMap::new(), now + 3000);
        core.apply(make(Status::Completed, now + 4000), &found, &HashMap::new(), now + 4000);
        let completed = core.states[&format!("codex:{LOADED_A}")].session.clone();
        let notice_count = core.notices.len(); let sequence = core.sequence; let notice_sequence = core.notice_sequence;
        metadata.name = Some("Own manual rename".into()); titles.database.insert(format!("codex:{LOADED_A}"), metadata); titles.next = None;
        titles.refresh_loaded(&paths, &Logs::default(), &mut core, &found, now + 5000);
        let final_session = &core.states[&format!("codex:{LOADED_A}")].session;
        let mut expected = completed.clone(); expected.title = "子 Agent · Own manual rename · 11111111".into(); assert_eq!(*final_session, expected);
        assert_eq!(core.completions.len(), 1); assert_eq!(core.completions[0].session.title, expected.title);
        assert_eq!(core.sequence, sequence); assert_eq!(core.notice_sequence, notice_sequence); assert_eq!(core.notices.len(), notice_count);
        assert!(core.notices.iter().all(|notice| notice.session.id == expected.id && notice.session.turn_key == expected.turn_key && notice.session.title == expected.title));
        assert!(core.notices.iter().filter(|notice| notice.kind == NoticeKind::Approval).all(|notice| notice.session.approval_key == approval_key));
        core.apply(make(Status::Completed, now + 6000), &found, &HashMap::new(), now + 6000); assert_eq!(core.completions.len(), 1);
    }

    fn hook_fixture(paths: &Paths, agent: AgentKind, event: &str, value: Value) {
        let python = python_candidates(paths).into_iter().next().expect("Python 3 is required for the installed hook transport");
        let mut child = Command::new(python).arg(paths.root.join("sink.py")).arg(&paths.root).arg(agent.key()).arg(event)
            .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(&serde_json::to_vec(&value).unwrap()).unwrap();
        let started = Instant::now();
        loop { if let Some(status) = child.try_wait().unwrap() { assert!(status.success()); break; }
            if started.elapsed() > Duration::from_secs(3) { let _ = child.kill(); let _ = child.wait(); panic!("hook fixture exceeded its deadline"); }
            thread::sleep(Duration::from_millis(5)); }
    }

    #[test]
    fn claude_official_hooks_flow_through_transport_core_journal_titles_and_task_list() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let _marker = Marker::start(&paths).unwrap();
        install_assets(&paths.root, &[("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes())]).unwrap();
        let directory = paths.claude.join("projects/project"); fs::create_dir_all(&directory).unwrap(); let transcript = directory.join("shared.jsonl");
        fs::write(&transcript, b"{\"type\":\"custom-title\",\"sessionId\":\"shared\",\"customTitle\":\"Real Claude conversation\"}\n").unwrap();
        let found = [process(AgentKind::Claude, 20)]; let mut core = Core::default(); let mut logs = Logs::default(); let mut titles = Titles::default();
        let mut list = crate::agent_tasks::List::default(); let base = now_ms(); list.update(&Snapshot { generation: 100, ..Default::default() }, 0.0, base);
        let mut start = 0; let mut turn = String::new(); let mut key = 0;
        for (index, (event, expected)) in [("UserPromptSubmit", Status::Thinking), ("PreToolUse", Status::Working), ("PermissionRequest", Status::Approval), ("PostToolUse", Status::Working), ("Stop", Status::Completed)].into_iter().enumerate() {
            hook_fixture(&paths, AgentKind::Claude, event, json!({"session_id":"shared","pid":20,"cwd":"/project","transcript_path":transcript,"tool_name":"Bash","prompt":"PRIVATE PROMPT","tool_input":{"command":"PRIVATE"}}));
            let mut values = spool_metadata(&paths, now_ms(), |event, path| logs.bind_claude(path, event, &paths)); assert_eq!(values.len(), 1);
            let mut value = values.remove(0); value.at_ms = base + index as u64 * 1000;
            if index == 0 { start = value.start_ms.unwrap(); turn = value.turn.clone(); }
            assert_eq!(value.turn, turn); assert_eq!(value.start_ms, Some(start));
            core.apply(value, &found, &HashMap::new(), base + index as u64 * 1000); titles.next = None;
            titles.refresh_loaded(&paths, &logs, &mut core, &found, base + index as u64 * 1000);
            let snapshot = Snapshot { generation: 100, sessions: core.reconcile(&found, base + index as u64 * 1000), completions: core.completions.iter().cloned().collect(), ..Default::default() };
            let session = &snapshot.sessions[0]; assert_eq!(session.agent, AgentKind::Claude); assert_eq!(session.status, expected); assert_eq!(session.title, "Real Claude conversation");
            if index == 0 { key = session.turn_key; }
            assert_eq!(session.turn_key, key); assert_eq!(session.started_at, base / 1000);
            list.update(&snapshot, index as f64 + 1.0, base + index as u64 * 1000); assert_eq!(list.sessions().len(), 1); assert_eq!(list.sessions()[0].status, expected);
        }
        assert_eq!(core.completions.len(), 1); assert_eq!(core.completions[0].session.elapsed_seconds(base / 1000 + 999), Some(4));
        assert!(!format!("{:?}", core.completions).contains("PRIVATE"));
        let snapshot = Snapshot { generation: 100, sessions: core.reconcile(&found, base + 4000), completions: core.completions.iter().cloned().collect(), ..Default::default() };
        list.update(&snapshot, 64.9, base + 63_900); assert_eq!(list.sessions().len(), 1); list.update(&snapshot, 65.0, base + 64_000); assert!(list.sessions().is_empty());
    }

    #[test]
    fn claude_transcripts_require_pid_ownership_and_do_not_confuse_tool_results_with_prompts() {
        let now = now_ms(); let mut first = process(AgentKind::Claude, 20); let mut second = process(AgentKind::Claude, 21); second.sessions.insert("session".into());
        let found = [first.clone(), second.clone()]; let mut core = Core::default(); let mut metadata = LogMeta { id: "session".into(), ..Default::default() };
        let parse = |body: Value, metadata: &mut LogMeta| log_event(&serde_json::to_vec(&body).unwrap(), AgentKind::Claude, metadata).unwrap();
        let start = parse(json!({"type":"user","timestamp":timestamp(now),"sessionId":"session","uuid":"prompt-uuid","cwd":"/project","message":{"content":"PRIVATE"}}), &mut metadata);
        let mut unknown = found.clone(); unknown[1].sessions.clear(); core.apply(start.clone(), &unknown, &HashMap::new(), now); assert!(core.states.is_empty(), "same project or unique Claude PID does not establish ownership");
        core.apply(start, &found, &HashMap::new(), now); let before = core.states["claude:session"].session.clone(); assert_eq!(before.pid, Some(21));
        let result = parse(json!({"type":"user","timestamp":timestamp(now + 5000),"sessionId":"session","uuid":"tool-result","message":{"content":[{"type":"tool_result","content":"PRIVATE OUTPUT"}]}}), &mut metadata);
        assert!(!result.starts_turn); assert_eq!(result.turn, "prompt-uuid"); core.apply(result, &found, &HashMap::new(), now + 5000); assert_eq!(core.states["claude:session"].session.started_at, before.started_at);
        let end = parse(json!({"type":"assistant","timestamp":timestamp(now + 6000),"sessionId":"session","message":{"stop_reason":"end_turn","content":"PRIVATE RESPONSE"}}), &mut metadata);
        core.apply(end, &found, &HashMap::new(), now + 6000); assert_eq!(core.states["claude:session"].session.status, Status::Completed); assert_eq!(core.completions.len(), 1);
        first.sessions.insert("session".into()); let mut ambiguous = Core::default(); let value = event(Status::Working, now, "turn");
        let mut value = Event { agent: AgentKind::Claude, ..value }; value.id = "session".into(); ambiguous.apply(value, &[first, second], &HashMap::new(), now); assert!(ambiguous.states.is_empty());
    }

    #[test]
    fn claude_hook_and_transcript_start_aliases_keep_one_turn_and_terminal_does_not_reactivate() {
        let now = now_ms(); let found = [process(AgentKind::Claude, 20)]; let parents = HashMap::new(); let mut core = Core::default();
        let make = |status, at, turn: &str, source| Event { agent: AgentKind::Claude, source, pid: Some(20), ..event(status, at, turn) };
        core.apply(make(Status::Thinking, now, "hook-123", Source::Hook), &found, &parents, now); let key = core.states["claude:session"].session.turn_key;
        core.apply(make(Status::Thinking, now + 100, "transcript-user-uuid", Source::Log), &found, &parents, now + 100);
        core.apply(make(Status::Working, now + 200, "hook-123", Source::Hook), &found, &parents, now + 200);
        core.apply(make(Status::Completed, now + 300, "transcript-user-uuid", Source::Log), &found, &parents, now + 300);
        core.apply(make(Status::Completed, now + 400, "hook-123", Source::Hook), &found, &parents, now + 400);
        assert_eq!(core.completions.len(), 1); assert_eq!(core.states["claude:session"].session.turn_key, key); assert_eq!(core.states["claude:session"].session.status, Status::Completed);
        core.apply(make(Status::Thinking, now + 5000, "hook-next", Source::Hook), &found, &parents, now + 5000);
        let next = core.states["claude:session"].session.turn_key; assert_ne!(next, key);
        core.apply(make(Status::Completed, now + 6000, "transcript-user-uuid", Source::Log), &found, &parents, now + 6000);
        assert_eq!(core.states["claude:session"].session.status, Status::Thinking); assert_eq!(core.completions.len(), 1);
    }

    #[test]
    fn the_same_session_and_turn_ids_remain_isolated_across_all_supported_agents() {
        let now = now_ms(); let found = [process(AgentKind::Codex, 20), process(AgentKind::Claude, 21), process(AgentKind::Hermes, 22)]; let mut core = Core::default();
        for process in &found { let mut value = event(Status::Thinking, now, "same-turn"); value.agent = process.agent; value.pid = Some(process.pid); core.apply(value, &found, &HashMap::new(), now); }
        let mut done = event(Status::Completed, now + 1000, "same-turn"); done.agent = AgentKind::Claude; done.pid = Some(21); core.apply(done, &found, &HashMap::new(), now + 1000);
        assert_eq!(core.states.len(), 3); assert_eq!(core.states["claude:session"].session.status, Status::Completed);
        assert_eq!(core.states["hermes:session"].session.status, Status::Thinking); assert_eq!(core.states["codex:session"].session.status, Status::Thinking); assert_eq!(core.completions[0].session.agent, AgentKind::Claude);
    }

    #[test]
    fn hermes_plugin_hooks_flow_through_transport_core_journal_titles_and_task_list() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let _marker = Marker::start(&paths).unwrap();
        let directory = temporary.0.join("plugin-fixture"); private_dir(&directory).unwrap();
        fs::write(directory.join("__init__.py"), plugin_source(&paths)).unwrap(); fs::write(directory.join("sink.py"), SINK).unwrap();
        let base = now_ms();
        let driver = r#"
import importlib.util, pathlib, sys, time
sys.dont_write_bytecode = True
directory=pathlib.Path(sys.argv[1]); base=int(sys.argv[2])
spec=importlib.util.spec_from_file_location('qingyu_hermes_fixture',directory/'__init__.py',submodule_search_locations=[str(directory)])
plugin=importlib.util.module_from_spec(spec);sys.modules[spec.name]=plugin;spec.loader.exec_module(plugin);plugin._STARTED=True
class Context:
    def __init__(self):self.hooks={}
    def register_hook(self,name,callback):self.hooks[name]=callback
ctx=Context();plugin.register(ctx)
events=[('on_session_start',{}),('pre_llm_call',{'session_title':'Actual Hermes conversation','user_message':'PRIVATE PROMPT'}),('pre_tool_call',{'tool_name':'terminal','args':{'command':'PRIVATE INPUT'}}),('pre_approval_request',{'session_key':'routing-key'}),('post_approval_response',{}),('on_session_end',{'completed':True})]
for index,(hook,extra) in enumerate(events):
    plugin.time.time_ns=lambda at=base+index*1000:at*1000000
    assert ctx.hooks[hook](session_id='shared',turn_id='hermes-turn',**extra) is None
    event,data=plugin._QUEUE.get_nowait();plugin.emit(plugin._ROOT,'hermes',event,data)
"#;
        let python = python_candidates(&paths).into_iter().next().unwrap();
        command_output(Command::new(python).arg("-c").arg(driver).arg(&directory).arg(base.to_string()), Duration::from_secs(2), 1024).unwrap();
        let mut events = spool(&paths, base + 6000); events.sort_by_key(|event| event.at_ms); assert_eq!(events.len(), 6);
        let mut process = process(AgentKind::Hermes, 20); let mut core = Core::default(); let mut list = crate::agent_tasks::List::default();
        list.update(&Snapshot { generation: 101, ..Default::default() }, 0.0, base);
        let expected = [Status::Idle, Status::Thinking, Status::Working, Status::Approval, Status::Thinking, Status::Completed]; let mut start = 0; let mut turn_key = 0;
        for (index, mut event) in events.into_iter().enumerate() {
            // The actual script's PID is short-lived; attach the fixture's verified
            // Hermes process while preserving the exact plugin event metadata.
            event.pid = Some(process.pid); let at = event.at_ms;
            core.apply(event, std::slice::from_ref(&process), &HashMap::new(), at);
            let snapshot = Snapshot { generation: 101, sessions: core.reconcile(std::slice::from_ref(&process), at), completions: core.completions.iter().cloned().collect(), ..Default::default() };
            let session = &snapshot.sessions[0]; assert_eq!(session.agent, AgentKind::Hermes); assert_eq!(session.status, expected[index]);
            if index == 1 { start = session.started_at; turn_key = session.turn_key; }
            if index > 0 { assert_eq!(session.title, "Actual Hermes conversation"); assert_eq!(session.started_at, start); assert_eq!(session.turn_key, turn_key); }
            list.update(&snapshot, index as f64, at);
            assert_eq!(list.sessions().len(), usize::from(index > 0));
            process.project = "/fixture".into();
        }
        assert_eq!(core.completions.len(), 1); assert_eq!(core.completions[0].session.elapsed_seconds(base / 1000 + 999), Some(4));
        assert!(!format!("{:?}", core.completions).contains("PRIVATE"));
    }

    #[test]
    fn integration_readiness_requires_real_events_and_reports_unsupported_processes_honestly() {
        let temporary = Temporary::new(); let paths = temporary.paths(); fs::create_dir_all(&paths.claude).unwrap(); fs::create_dir_all(&paths.hermes).unwrap();
        let mut core = Core::default(); let found = [process(AgentKind::Claude, 20), process(AgentKind::Hermes, 21), process(AgentKind::Gemini, 22)];
        let configs = integration_configs(&paths); let statuses = integration_status(configs.clone(), &found, &core, 10_000, true);
        assert!(statuses.iter().all(|integration| !integration.ready));
        assert!(statuses.iter().find(|integration| integration.agent == AgentKind::Hermes).unwrap().message.contains("需安装"));
        let session_start = decode_spool(&serde_json::to_vec(&json!({"version":1,"agent":"hermes","event":"SessionStart","session_id":"idle-session","at_ms":10_000,"pid":21})).unwrap(), 10_000).unwrap();
        core.apply(session_start, &found, &HashMap::new(), 10_000);
        assert!(integration_status(configs.clone(), &found, &core, 10_000, true).iter().all(|integration| !integration.ready));
        let mut value = event(Status::Working, 10_000, "turn"); value.agent = AgentKind::Claude; value.pid = Some(20); core.apply(value, &found, &HashMap::new(), 10_000);
        let statuses = integration_status(configs.clone(), &found, &core, 10_001, true);
        assert!(statuses.iter().find(|integration| integration.agent == AgentKind::Claude).unwrap().ready);
        assert!(!statuses.iter().find(|integration| integration.agent == AgentKind::Hermes).unwrap().ready);
        assert!(statuses.iter().find(|integration| integration.agent == AgentKind::Gemini).unwrap().message.contains("仅支持进程识别"));
        assert!(integration_status(configs.clone(), &found, &core, 10_001, false).iter().all(|integration| !integration.ready));
        assert!(integration_status(configs, &found, &core, 10_001 + FRESH_LOG_MS, true).iter().all(|integration| !integration.ready));
    }

    #[test]
    fn self_owned_repair_upgrades_known_releases_and_refuses_user_edits_or_changed_manifests() {
        let temporary = Temporary::new(); let paths = temporary.paths(); private_dir(&paths.root).unwrap();
        let file = paths.root.join("sink.py"); fs::write(&file, LEGACY_SINK).unwrap();
        install_assets(&paths.root, &[("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes())]).unwrap(); assert_eq!(fs::read(&file).unwrap(), SINK.as_bytes());
        let manifest = paths.root.join("qingyu-owned-assets.json"); let first = fs::read(&manifest).unwrap();
        install_assets(&paths.root, &[("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes())]).unwrap(); assert_eq!(fs::read(&manifest).unwrap(), first);
        let modified = format!("{SINK}\n# user customization\n"); fs::write(&file, &modified).unwrap();
        assert!(install_assets(&paths.root, &[("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes())]).is_err()); assert_eq!(fs::read_to_string(&file).unwrap(), modified);
        assert!(remove_assets(&paths.root, &[("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes())]).is_err());
        fs::write(&file, SINK).unwrap(); let mut owner: OwnedAssets = serde_json::from_slice(&first).unwrap(); owner.files.insert("sink.py".into(), "changed digest".into()); write_private(&manifest, &serde_json::to_vec(&owner).unwrap()).unwrap();
        assert!(install_assets(&paths.root, &[("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes())]).is_err()); assert_eq!(fs::read(&file).unwrap(), SINK.as_bytes());
    }

    #[test]
    fn legacy_hermes_assets_upgrade_as_one_owned_namespace_and_keep_unrelated_files() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let directory = paths.hermes.join("plugins/qingyu-monitor"); private_dir(&directory).unwrap();
        let legacy = legacy_plugin_source(&paths); let source = plugin_source(&paths);
        fs::write(directory.join("__init__.py"), &legacy).unwrap(); fs::write(directory.join("sink.py"), LEGACY_SINK).unwrap(); fs::write(directory.join("plugin.yaml"), LEGACY_HERMES_MANIFEST).unwrap(); fs::write(directory.join("user-notes.txt"), "keep this").unwrap();
        let files = [("__init__.py", source.as_bytes(), legacy.as_bytes()), ("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes()), ("plugin.yaml", HERMES_MANIFEST.as_bytes(), LEGACY_HERMES_MANIFEST.as_bytes())];
        install_assets(&directory, &files).unwrap(); assert_eq!(fs::read(directory.join("__init__.py")).unwrap(), source.as_bytes());
        remove_assets(&directory, &files).unwrap(); assert!(directory.join("user-notes.txt").exists()); assert!(!directory.join("__init__.py").exists());
    }

    #[test]
    fn installed_v3_hermes_observer_upgrades_to_unload_safe_v4_and_keeps_user_files() {
        let temporary = Temporary::new(); let paths = temporary.paths(); let directory = paths.hermes.join("plugins/qingyu-monitor"); private_dir(&directory).unwrap();
        let previous = PREVIOUS_RELEASE_HERMES_PLUGIN.replace("__QINGYU_MONITOR_ROOT__", &serde_json::to_string(&paths.root.to_string_lossy()).unwrap());
        fs::write(directory.join("__init__.py"), &previous).unwrap();
        fs::write(directory.join("sink.py"), SINK).unwrap();
        fs::write(directory.join("plugin.yaml"), PREVIOUS_RELEASE_HERMES_MANIFEST).unwrap();
        fs::write(directory.join("user-notes.txt"), "preserve this file").unwrap();
        let source = plugin_source(&paths);
        let files = [("__init__.py", source.as_bytes(), previous.as_bytes()), ("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes()),
            ("plugin.yaml", HERMES_MANIFEST.as_bytes(), PREVIOUS_RELEASE_HERMES_MANIFEST.as_bytes())];
        install_assets(&directory, &files).unwrap();
        assert_eq!(fs::read(directory.join("__init__.py")).unwrap(), source.as_bytes());
        assert_eq!(fs::read_to_string(directory.join("plugin.yaml")).unwrap(), HERMES_MANIFEST);
        assert_eq!(fs::read_to_string(directory.join("user-notes.txt")).unwrap(), "preserve this file");
    }

    #[cfg(unix)]
    #[test]
    fn previous_hermes_plugin_root_alias_is_repaired_but_custom_sources_are_preserved() {
        let temporary = Temporary::new(); let paths = temporary.paths(); private_dir(&paths.root).unwrap();
        let alias = Paths { root: temporary.0.join("support-data"), ..paths.clone() };
        std::os::unix::fs::symlink(&paths.root, &alias.root).unwrap();
        let previous = PREVIOUS_HERMES_PLUGIN.replace("__QINGYU_MONITOR_ROOT__", &serde_json::to_string(&paths.root.to_string_lossy()).unwrap());
        let current = plugin_source(&alias);
        assert!(known_asset_release("__init__.py", previous.as_bytes(), current.as_bytes(), b""));
        let installed_v3 = PREVIOUS_RELEASE_HERMES_PLUGIN.replace("__QINGYU_MONITOR_ROOT__", &serde_json::to_string(&paths.root.to_string_lossy()).unwrap());
        assert!(known_asset_release("__init__.py", installed_v3.as_bytes(), current.as_bytes(), b""));
        assert!(plugin_release_matches(HERMES_PLUGIN, current.as_bytes(), &paths.root));
        assert!(!plugin_release_matches(HERMES_PLUGIN, previous.as_bytes(), &paths.root));
        assert!(known_asset_release("plugin.yaml", PREVIOUS_RELEASE_HERMES_MANIFEST.as_bytes(), HERMES_MANIFEST.as_bytes(), b""));
        assert!(known_asset_release("sink.py", PREVIOUS_SINK.as_bytes(), SINK.as_bytes(), LEGACY_SINK.as_bytes()));
        let changed = previous + "\n# customization\n";
        assert!(!known_asset_release("__init__.py", changed.as_bytes(), current.as_bytes(), b""));
    }

    /// Explicit local repair diagnostic. Never runs in normal test suites or during app startup.
    #[test]
    #[ignore]
    fn install_current_claude_and_hermes_integrations() {
        assert_eq!(std::env::var("QINGYU_INSTALL_CLAUDE_HERMES").as_deref(), Ok("1"));
        let paths = Paths::environment(); private_dir(&paths.root).unwrap();
        install_assets(&paths.root, &[("sink.py", SINK.as_bytes(), LEGACY_SINK.as_bytes())]).unwrap();
        let python = python_candidates(&paths).into_iter().find(|python|
            command_output(Command::new(python).args(["-c", "import sys;assert sys.version_info.major == 3"]), Duration::from_secs(1), 1024).is_ok()).unwrap();
        change_hooks(&paths, AgentKind::Claude, &python, true).unwrap(); change_hermes(&paths, true).unwrap();
        let claude = paths.claude.join("settings.json"); let hermes = paths.hermes.join("config.yaml");
        let first = (fs::read(&claude).unwrap(), fs::read(&hermes).unwrap());
        change_hooks(&paths, AgentKind::Claude, &python, true).unwrap(); change_hermes(&paths, true).unwrap();
        assert!(first.0 == fs::read(&claude).unwrap() && first.1 == fs::read(&hermes).unwrap(), "Repeated repair changed configuration bytes");
        let configs = integration_configs(&paths);
        assert!(configs.iter().filter(|integration| matches!(integration.agent, AgentKind::Claude | AgentKind::Hermes)).all(|integration| integration.installed));
        eprintln!("Claude and Hermes integrations installed; repeated repair preserved configuration bytes. Running agent processes were not restarted.");
    }

    #[test]
    #[ignore]
    fn actual_hermes_gateway_process_correlates_without_creating_or_finishing_a_task() {
        assert_eq!(std::env::var("QINGYU_HERMES_RUNTIME_SMOKE").as_deref(), Ok("1"));
        let paths = Paths::environment();
        let status: Value = serde_json::from_slice(&fs::read(paths.hermes.join("gateway_state.json")).unwrap()).unwrap();
        let pid = status.get("pid").and_then(Value::as_u64).unwrap() as u32;
        let (found, parents) = processes().unwrap();
        let gateway = found.iter().find(|process| process.agent == AgentKind::Hermes && process.pid == pid)
            .expect("Current gateway PID must be detected by its actual launch command");
        assert!(gateway.service); let mut core = Core::default(); let now = now_ms();
        assert!(core.reconcile(&found, now).iter().filter(|session| session.agent == AgentKind::Hermes).all(|session| !session.status.is_active()));
        let metadata = decode_spool(&serde_json::to_vec(&json!({"version":1,"agent":"hermes","event":"SessionStart","session_id":"offline-correlation-only","at_ms":now,"pid":pid})).unwrap(), now).unwrap();
        core.apply(metadata, &found, &parents, now);
        assert!(core.completions.is_empty() && core.notices.is_empty());
        assert!(!core.received.contains_key(&AgentKind::Hermes));
        assert_eq!(core.states["hermes:offline-correlation-only"].session.pid, Some(pid));
        eprintln!("Current Hermes gateway detected and PID-correlated; process/session presence created no task or completion.");
    }

    #[test]
    fn actual_multi_agent_metadata_smoke() {
        if std::env::var("QINGYU_MULTI_AGENT_METADATA_SMOKE").as_deref() != Ok("1") { return; }
        let paths = Paths::environment(); let now = now_ms(); let (found, parents) = processes().unwrap();
        let mut core = Core::default(); let mut logs = Logs::default(); let mut titles = Titles::default(); core.recover_loaded(&found, now);
        let mut events = logs.poll(&paths, now); events.sort_by_key(|event| (event.at_ms, event.source));
        for event in events { core.apply(event, &found, &parents, now); }
        logs.apply_uncertainty(&mut core); titles.refresh_loaded(&paths, &logs, &mut core, &found, now); core.reconcile(&found, now);
        let status = integration_status(integration_configs(&paths), &found, &core, now, true);
        for integration in status {
            let processes = found.iter().filter(|process| process.agent == integration.agent).count();
            let sessions = core.states.values().filter(|state| state.session.agent == integration.agent).count();
            let active = core.states.values().filter(|state| state.session.agent == integration.agent && state.session.status.is_active()).count();
            eprintln!("agent={} detected={} verified_processes={} installed_current_integration={} recent_detailed_events={} correlated_sessions={} active_tasks={}",
                integration.agent.key(), integration.detected, processes, integration.installed, integration.ready, sessions, active);
            assert!(!integration.ready || processes > 0 && core.received.contains_key(&integration.agent));
        }
        assert!(core.states.values().filter(|state| state.session.agent == AgentKind::Hermes).all(|state|
            !titles.database.get(&state.session.id).is_some_and(|metadata| metadata.ended == Some(true))));
    }

    fn value_time(now: u64, index: usize) -> u64 { now + index as u64 * 1000 }

    #[cfg(unix)]
    #[test]
    fn actual_local_process_query_is_bounded_and_does_not_mark_services_as_busy() {
        let started = Instant::now(); let (found, _) = processes().unwrap(); assert!(started.elapsed() < Duration::from_secs(4));
        let mut core = Core::default(); let snapshot = core.reconcile(&found, now_ms()); assert!(snapshot.iter().all(|session| !session.status.is_active()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn process_query_recognizes_agents_in_a_localized_gui_environment() {
        const CHILD_PID: &str = "QINGYU_AGENT_LOCALE_TEST_PID";
        if let Ok(pid) = std::env::var(CHILD_PID) {
            let pid = pid.parse::<u32>().unwrap();
            let (found, _) = processes().unwrap();
            let process = found.iter().find(|process| process.agent == AgentKind::Codex && process.pid == pid)
                .expect("localized ps dates must not swallow the agent executable");
            assert!(process.started_ms > 0, "the process birth date must remain parseable");
            let mut core = Core::default();
            assert!(core.reconcile(&found, now_ms()).iter().all(|session| !session.status.is_active()),
                "recognizing an executable cannot create task activity");
            return;
        }

        // Use a harmless real process with an agent executable name. A child
        // test process provides the Finder locale without mutating the shared
        // test runner's environment or depending on an installed agent.
        let temporary = Temporary::new(); let executable = temporary.0.join("codex");
        fs::copy("/bin/sleep", &executable).unwrap();
        let mut sleeper = Command::new(executable).arg("10").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        let result = command_output(Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "agent_monitor::tests::process_query_recognizes_agents_in_a_localized_gui_environment", "--test-threads=1"])
            .env(CHILD_PID, sleeper.id().to_string()).env("LANG", "zh_CN.UTF-8").env("LC_TIME", "zh_CN.UTF-8").env_remove("LC_ALL"),
            Duration::from_secs(6), 16 * 1024);
        let _ = sleeper.kill(); let _ = sleeper.wait();
        assert!(result.is_ok(), "the process query failed in the GUI locale: {result:?}");
    }
}
