//! 拾光：采集端的内容记录与界面状态分开原子保存。
//!
//! `records/<id>.json` 只由采集端写，`states/<id>.json` 只由界面写。
//! id 是带命名空间的 source_id 的 SHA-256；URL 相同并不代表同一次采集。

use chrono::{DateTime, SecondsFormat, Utc};
use ring::{digest, rand::{SecureRandom, SystemRandom}};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, fs, io::Read, path::{Path, PathBuf}, sync::Arc, time::SystemTime};

const VERSION: u32 = 1;
const MAX_RECORD_BYTES: u64 = 256 * 1024;
const MAX_STATE_BYTES: u64 = 4096;
const MAX_ORIGINAL_BYTES: usize = 128 * 1024;
const MAX_FILES: usize = 10_000;
const MAX_REFRESH_BYTES: u64 = 32 * 1024 * 1024;
const SCRIPT: &str = include_str!("../scripts/library/shiguang.py");
const SKILL_TEMPLATE: &str = include_str!("../scripts/library/SKILL.md");
const SKILL_NAME: &str = "qingyu-shiguang";
const MANIFEST_NAME: &str = ".qingyu-managed.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Idea,
    Link,
    Action,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self { Self::Idea => "想法", Self::Link => "收藏", Self::Action => "行动" }
    }
    pub fn glyph(self) -> &'static str {
        match self { Self::Idea => "agent", Self::Link => "link", Self::Action => "todo" }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EntryState {
    pub pinned: bool,
    pub reviewed: bool,
    pub done: bool,
    pub archived: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub summary: String,
    pub original: String,
    pub url: Option<String>,
    pub tags: Vec<String>,
    pub topic: Option<String>,
    pub next_action: Option<String>,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
    pub state: EntryState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u32,
    id: String,
    source_id: String,
    kind: Kind,
    title: String,
    #[serde(default)]
    summary: String,
    original: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    next_action: Option<String>,
    source: String,
    created_at: String,
    updated_at: String,
}

impl Record {
    fn validate(&self, id: &str) -> Result<(), String> {
        if self.version != VERSION { return Err("不支持的记录版本".into()); }
        if !valid_id(id) || self.id != id || record_id(&self.source_id) != id {
            return Err("记录 ID 与文件名或来源 ID 不一致".into());
        }
        bounded_text(&self.source_id, "来源 ID", 1024, true)?;
        bounded_text(&self.title, "标题", 512, true)?;
        bounded_text(&self.original, "原文", MAX_ORIGINAL_BYTES, true)?;
        bounded_text(&self.summary, "摘要", 16 * 1024, false)?;
        bounded_text(&self.source, "来源", 256, true)?;
        if self.tags.len() > 24 { return Err("标签超过 24 个".into()); }
        for tag in &self.tags { bounded_text(tag, "标签", 128, true)?; }
        for (value, label, limit) in [(&self.topic, "主题", 512), (&self.next_action, "下一步", 2048)] {
            if let Some(value) = value { bounded_text(value, label, limit, true)?; }
        }
        if let Some(url) = &self.url { validate_url(url)?; }
        if self.kind == Kind::Link && self.url.is_none() { return Err("收藏缺少 http(s) 链接".into()); }
        let created = parse_time(&self.created_at)?;
        let updated = parse_time(&self.updated_at)?;
        if updated < created { return Err("更新时间早于创建时间".into()); }
        Ok(())
    }

    fn entry(&self, state: EntryState) -> Entry {
        Entry {
            id: self.id.clone(), kind: self.kind, title: self.title.clone(), summary: self.summary.clone(),
            original: self.original.clone(), url: self.url.clone(), tags: self.tags.clone(), topic: self.topic.clone(),
            next_action: self.next_action.clone(), source: self.source.clone(), created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(), state,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateFile { version: u32, id: String, state: EntryState }

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    inode: u64,
}

impl Stamp {
    fn from(metadata: &fs::Metadata) -> Self {
        Self {
            len: metadata.len(), modified: metadata.modified().ok(),
            #[cfg(unix)]
            inode: { use std::os::unix::fs::MetadataExt; metadata.ino() },
        }
    }
}

#[derive(Clone, Debug)]
struct Cached<T> { stamp: Stamp, good: Option<Arc<T>>, error: Option<String> }

pub struct Store {
    pub entries: Vec<Entry>,
    pub error: Option<String>,
    root: PathBuf,
    records: BTreeMap<String, Cached<Record>>,
    states: BTreeMap<String, Cached<StateFile>>,
}

impl Store {
    pub fn load() -> Self { Self::at(crate::model::config_dir().join("library")) }

    /// `root` 是 library 目录，不是 Qingyu 数据目录。读取不会创建目录。
    pub fn at(root: PathBuf) -> Self {
        let root = absolute(&root).unwrap_or(root);
        let mut store = Self { entries: Vec::new(), error: None, root, records: BTreeMap::new(), states: BTreeMap::new() };
        store.refresh();
        store
    }

    pub fn root(&self) -> &Path { &self.root }

    /// 每次仅扫描有限的文件元数据，只有变化的文件才读取和解析。
    /// 损坏或暂时不可读的文件保留进程内最后一份有效记录及状态，并显示错误。
    pub fn refresh(&mut self) -> bool {
        let mut errors = Vec::new();
        let mut budget = MAX_REFRESH_BYTES;
        let mut cache_changed = false;
        match record_paths(&self.root.join("records")) {
            Ok((paths, path_errors)) => {
                errors.extend(path_errors);
                let present: BTreeSet<_> = paths.iter().map(|(id, _)| id.clone()).collect();
                let old_count = self.records.len();
                self.records.retain(|id, _| present.contains(id));
                self.states.retain(|id, _| present.contains(id));
                cache_changed |= old_count != self.records.len();
                for (id, path) in paths {
                    let old = self.records.get(&id);
                    let cached = read_cached(&path, MAX_RECORD_BYTES, &mut budget, old, |bytes| {
                        let record: Record = serde_json::from_slice(bytes).map_err(|e| format!("JSON 格式无效：{e}"))?;
                        record.validate(&id)?;
                        Ok(record)
                    });
                    if let Some(cached) = cached {
                        cache_changed |= cache_differs(old, &cached);
                        if let Some(error) = &cached.error { errors.push(format!("记录 {}：{error}", short_id(&id))); }
                        self.records.insert(id.clone(), cached);
                    } else {
                        errors.push(format!("记录 {}：本次读取超过 32 MiB，稍后重试", short_id(&id)));
                    }
                    if self.records.get(&id).and_then(|c| c.good.as_ref()).is_none() { continue; }
                    let state_path = self.root.join("states").join(format!("{id}.json"));
                    if let Err(error) = ensure_directory_if_present(&self.root.join("states")) {
                        errors.push(error);
                        continue;
                    }
                    match fs::symlink_metadata(&state_path) {
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => { cache_changed |= self.states.remove(&id).is_some(); }
                        _ => {
                            let cached = read_cached(&state_path, MAX_STATE_BYTES, &mut budget, self.states.get(&id), |bytes| {
                                let state: StateFile = serde_json::from_slice(bytes).map_err(|e| format!("JSON 格式无效：{e}"))?;
                                if state.version != VERSION || state.id != id { return Err("状态版本或 ID 不一致".into()); }
                                Ok(state)
                            });
                            if let Some(cached) = cached {
                                cache_changed |= cache_differs(self.states.get(&id), &cached);
                                if let Some(error) = &cached.error { errors.push(format!("状态 {}：{error}", short_id(&id))); }
                                self.states.insert(id, cached);
                            } else { errors.push("状态读取超过本次上限，稍后重试".into()); }
                        }
                    }
                }
            }
            Err(error) => errors.push(error), // 无法完整列出目录时不删除缓存。
        }
        let error = if errors.is_empty() { None } else {
            let count = errors.len();
            errors.truncate(4);
            let mut message = errors.join("；");
            if count > 4 { message.push_str(&format!("；另有 {} 项读取错误", count - 4)); }
            Some(message)
        };
        // 没有文件变化时保留现有列表及原文分配，轮询只产生元数据开销。
        if !cache_changed {
            let changed = self.error != error;
            self.error = error;
            return changed;
        }
        let mut entries: Vec<_> = self.records.iter().filter_map(|(id, cached)| {
            let record = cached.good.as_ref()?;
            let state = self.states.get(id).and_then(|cached| cached.good.as_ref()).map(|s| s.state.clone()).unwrap_or_default();
            Some(record.entry(state))
        }).collect();
        entries.sort_by(|a, b| b.state.pinned.cmp(&a.state.pinned)
            .then_with(|| parse_time(&b.created_at).ok().cmp(&parse_time(&a.created_at).ok()))
            .then_with(|| a.id.cmp(&b.id)));
        let changed = self.entries != entries || self.error != error;
        self.entries = entries;
        self.error = error;
        changed
    }

    pub fn capture(&mut self, kind: Kind, text: &str) -> Result<String, String> {
        let result = self.capture_inner(kind, text);
        if let Err(error) = &result { self.error = Some(error.clone()); }
        result
    }

    fn capture_inner(&mut self, kind: Kind, text: &str) -> Result<String, String> {
        bounded_text(text, "内容", MAX_ORIGINAL_BYTES, true)?;
        let url = extract_url(text)?;
        if kind == Kind::Link && url.is_none() { return Err("请在收藏中填写有效的 http(s) 链接".into()); }
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
        let mut nonce = [0u8; 16];
        SystemRandom::new().fill(&mut nonce).map_err(|_| "无法生成安全的记录 ID".to_string())?;
        let source_id = format!("manual:{now}:{}", hex(&nonce));
        let id = record_id(&source_id);
        let first_line = text.trim().lines().next().unwrap_or_default().trim();
        let title: String = first_line.chars().take(100).collect();
        let record = Record { version: VERSION, id: id.clone(), source_id, kind, title,
            summary: String::new(), original: text.to_string(), url, tags: Vec::new(), topic: None,
            next_action: None, source: "手动".into(), created_at: now.clone(), updated_at: now };
        record.validate(&id)?;
        prepare_directory(&self.root)?;
        let records = self.root.join("records");
        prepare_directory(&records)?;
        let path = records.join(format!("{id}.json"));
        require_regular_or_missing(&path)?;
        save_json(&path, &record)?;
        self.refresh();
        Ok(id)
    }

    /// 写入独立的 UI 状态文件，绝不重写 agent 记录。
    pub fn set_state(&mut self, id: &str, state: EntryState) -> Result<(), String> {
        let result = self.set_state_inner(id, state);
        if let Err(error) = &result { self.error = Some(error.clone()); }
        result
    }

    fn set_state_inner(&mut self, id: &str, state: EntryState) -> Result<(), String> {
        if !valid_id(id) { return Err("无效的记录 ID".into()); }
        if !self.entries.iter().any(|entry| entry.id == id) { return Err("记录不存在，请刷新后重试".into()); }
        prepare_directory(&self.root)?;
        let states = self.root.join("states");
        prepare_directory(&states)?;
        let path = states.join(format!("{id}.json"));
        require_regular_or_missing(&path)?;
        save_json(&path, &StateFile { version: VERSION, id: id.into(), state })?;
        self.refresh();
        Ok(())
    }
}

fn read_cached<T: Clone>(path: &Path, limit: u64, budget: &mut u64, old: Option<&Cached<T>>,
    parse: impl FnOnce(&[u8]) -> Result<T, String>) -> Option<Cached<T>> {
    let metadata = fs::symlink_metadata(path);
    let stamp = metadata.as_ref().map(Stamp::from).unwrap_or(Stamp { len: 0, modified: None, #[cfg(unix)] inode: 0 });
    if old.is_some_and(|cached| cached.stamp == stamp) { return old.cloned(); }
    let result = match metadata {
        Ok(metadata) if !metadata.file_type().is_file() => Err("必须是普通文件，不能是目录或符号链接".into()),
        Ok(metadata) if metadata.len() > limit => Err(format!("文件超过 {} KiB 上限", limit / 1024)),
        Ok(metadata) if metadata.len() > *budget => return None,
        Ok(metadata) => { *budget -= metadata.len(); read_bounded(path, limit).and_then(|bytes| parse(&bytes)) }
        Err(error) => Err(format!("无法读取：{error}")),
    };
    Some(match result {
        Ok(good) => Cached { stamp, good: Some(Arc::new(good)), error: None },
        Err(error) => Cached { stamp, good: old.and_then(|c| c.good.clone()), error: Some(error) },
    })
}

fn cache_differs<T>(old: Option<&Cached<T>>, new: &Cached<T>) -> bool {
    old.map_or(true, |old| old.stamp != new.stamp || old.error != new.error || old.good.is_some() != new.good.is_some())
}

fn record_paths(directory: &Path) -> Result<(Vec<(String, PathBuf)>, Vec<String>), String> {
    ensure_directory_if_present(directory)?;
    let directory = match fs::read_dir(directory) {
        Ok(directory) => directory,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), Vec::new())),
        Err(error) => return Err(format!("无法读取拾光目录：{error}")),
    };
    let mut paths = Vec::new();
    let mut errors = Vec::new();
    for (count, item) in directory.enumerate() {
        if count >= MAX_FILES * 2 { return Err("拾光目录文件过多，最多扫描 20000 个文件".into()); }
        let item = item.map_err(|e| format!("无法列出拾光记录：{e}"))?;
        let path = item.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") { continue; }
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        if !valid_id(id) {
            errors.push("拾光目录含无效 JSON 文件名，记录 ID 应为 64 位小写十六进制".into());
            continue;
        }
        if paths.len() >= MAX_FILES { return Err("拾光记录超过 10000 条，请整理后重试".into()); }
        paths.push((id.to_string(), path));
    }
    paths.sort_by(|a, b| a.0.cmp(&b.0));
    Ok((paths, errors))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    require_regular_or_missing(path)?;
    let file = fs::File::open(path).map_err(|e| format!("无法打开 {}：{e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(|e| format!("无法读取文件：{e}"))?;
    if bytes.len() as u64 > limit { return Err("文件超过读取上限".into()); }
    Ok(bytes)
}

fn bounded_text(value: &str, name: &str, limit: usize, required: bool) -> Result<(), String> {
    if required && value.trim().is_empty() { return Err(format!("{name}不能为空")); }
    if value.len() > limit { return Err(format!("{name}超过 {limit} 字节上限")); }
    if value.contains('\0') { return Err(format!("{name}不能含空字符")); }
    Ok(())
}

fn parse_time(value: &str) -> Result<DateTime<chrono::FixedOffset>, String> {
    if value.len() > 64 { return Err("时间格式无效".into()); }
    DateTime::parse_from_rfc3339(value).map_err(|_| "时间应为 ISO 8601 格式并包含时区".into())
}

fn valid_id(id: &str) -> bool { id.len() == 64 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
fn short_id(id: &str) -> &str { &id[..id.len().min(8)] }
fn record_id(source_id: &str) -> String { hex(digest::digest(&digest::SHA256, format!("qingyu-library-v1\0{source_id}").as_bytes()).as_ref()) }
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|byte| format!("{byte:02x}")).collect() }

fn validate_url(value: &str) -> Result<(), String> {
    bounded_text(value, "链接", 8192, true)?;
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) { return Err("链接不能含空白或控制字符".into()); }
    let url = url::Url::parse(value).map_err(|_| "链接格式无效".to_string())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() { return Err("只支持有效的 http(s) 链接".into()); }
    Ok(())
}

fn extract_url(text: &str) -> Result<Option<String>, String> {
    let lowercase = text.to_ascii_lowercase();
    let Some(start) = [lowercase.find("https://"), lowercase.find("http://")].into_iter().flatten().min() else { return Ok(None); };
    let candidate = text[start..].split(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '“' | '”' | '「' | '」' | '『' | '』')).next().unwrap_or_default();
    let candidate = candidate.trim_end_matches(['.', ',', ';', '!', '?', ')', ']', '}', '\'', '。', '，', '；', '！', '？', '）', '】', '》']);
    validate_url(candidate)?;
    Ok(Some(candidate.into()))
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() { return Err("目录不能为空".into()); }
    if path.is_absolute() { Ok(path.to_path_buf()) } else { std::env::current_dir().map(|cwd| cwd.join(path)).map_err(|e| format!("无法确定当前目录：{e}")) }
}

fn ensure_directory_if_present(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_dir() => Err(format!("{} 必须是实际目录，不能是符号链接", path.display())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法检查目录 {}：{error}", path.display())),
    }
}

fn prepare_directory(path: &Path) -> Result<(), String> {
    ensure_directory_if_present(path)?;
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)] { use std::os::unix::fs::DirBuilderExt; builder.mode(0o700); }
    builder.create(path).map_err(|e| format!("无法创建目录 {}：{e}", path.display()))?;
    ensure_directory_if_present(path)
}

fn require_regular_or_missing(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => Err(format!("{} 不是普通文件", path.display())),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法检查文件 {}：{error}", path.display())),
    }
}

fn save_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| format!("无法编码拾光记录：{e}"))?;
    if bytes.len() as u64 > MAX_RECORD_BYTES { return Err("编码后的记录超过 256 KiB 上限，请缩小内容后重试".into()); }
    save_bytes(path, &bytes)
}

fn save_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    crate::model::atomic_write(path, bytes).map_err(|e| format!("无法保存 {}：{e}", path.display()))?;
    sync_directory(path.parent().ok_or("缺少父目录")?)
}

fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    fs::File::open(path).and_then(|f| f.sync_all()).map_err(|e| format!("无法同步目录 {}：{e}", path.display()))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallManifest { owner: String, version: u32, root: String, files: BTreeMap<String, String> }

fn hermes_home() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("HERMES_HOME").filter(|s| !s.is_empty()) {
        let path = PathBuf::from(path);
        if path.starts_with("~") {
            let home = std::env::var_os("HOME").ok_or("无法展开 HERMES_HOME，缺少 HOME")?;
            return absolute(&PathBuf::from(home).join(path.strip_prefix("~").map_err(|_| "HERMES_HOME 格式无效")?));
        }
        return absolute(&path);
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".hermes")).ok_or_else(|| "找不到 Hermes 主目录，请设置 HERMES_HOME".into())
}

/// 仅检查本地目录/可执行文件，不读取 Hermes 凭据或联网。
pub fn hermes_available() -> bool {
    hermes_home().is_ok_and(|home| home.join("hermes-agent").is_dir() || home.join("skills").is_dir())
        || std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|path| path.join(if cfg!(windows) { "hermes.exe" } else { "hermes" }).is_file()))
}

fn install_files(root: &Path, skill_dir: &Path) -> Result<(String, BTreeMap<String, Vec<u8>>), String> {
    let root = absolute(root)?;
    let root = root.to_str().ok_or("拾光目录不是有效 UTF-8 路径")?.to_string();
    let script = skill_dir.join("scripts/shiguang.py");
    let script = script.to_str().ok_or("Hermes 技能路径不是有效 UTF-8")?;
    let skill = SKILL_TEMPLATE.replace("__SCRIPT_SH__", &shell_quote(script));
    let config = serde_json::to_vec_pretty(&serde_json::json!({ "version": VERSION, "root": root })).map_err(|e| e.to_string())?;
    Ok((root, BTreeMap::from([
        ("SKILL.md".into(), skill.into_bytes()),
        ("scripts/shiguang.py".into(), SCRIPT.as_bytes().to_vec()),
        ("config.json".into(), config),
    ])))
}

fn shell_quote(value: &str) -> String { format!("'{}'", value.replace('\'', "'\"'\"'")) }
fn checksum(bytes: &[u8]) -> String { hex(digest::digest(&digest::SHA256, bytes).as_ref()) }

fn managed_manifest(dir: &Path) -> Result<InstallManifest, String> {
    ensure_directory_if_present(dir)?;
    let manifest: InstallManifest = serde_json::from_slice(&read_bounded(&dir.join(MANIFEST_NAME), 8192)?)
        .map_err(|_| "现有拾光技能不含有效的轻屿管理标记，已保留原文件".to_string())?;
    let expected: BTreeSet<_> = ["SKILL.md", "scripts/shiguang.py", "config.json"].into_iter().map(String::from).collect();
    if manifest.owner != "qingyu-shiguang" || manifest.version != VERSION || manifest.files.keys().cloned().collect::<BTreeSet<_>>() != expected {
        return Err("现有同名技能不由轻屿管理，已保留原文件".into());
    }
    for (name, hash) in &manifest.files {
        if checksum(&read_bounded(&dir.join(name), MAX_RECORD_BYTES)?) != *hash {
            return Err(format!("现有拾光技能 {name} 已被修改，已保留原文件；请先备份并移动该技能目录"));
        }
    }
    // 更新通过整个目录替换实现；拒绝删除用户后来添加的任何文件。
    for item in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        if !["SKILL.md", "scripts", "config.json", MANIFEST_NAME].iter().any(|name| item.file_name() == *name) {
            return Err("现有拾光技能含额外文件，已保留；请先备份并移动该技能目录".into());
        }
    }
    ensure_directory_if_present(&dir.join("scripts"))?;
    for item in fs::read_dir(dir.join("scripts")).map_err(|e| e.to_string())? {
        if item.map_err(|e| e.to_string())?.file_name() != "shiguang.py" {
            return Err("现有拾光技能 scripts 含额外文件，已保留原目录".into());
        }
    }
    Ok(manifest)
}

pub fn hermes_installed(root: &Path) -> bool {
    hermes_home().is_ok_and(|home| installed_at(root, &home))
}

fn installed_at(root: &Path, home: &Path) -> bool {
    let dir = home.join("skills").join(SKILL_NAME);
    let Ok((root, files)) = install_files(root, &dir) else { return false; };
    let Ok(manifest) = managed_manifest(&dir) else { return false; };
    manifest.root == root && files.iter().all(|(name, bytes)| manifest.files.get(name) == Some(&checksum(bytes)))
}

/// 安装到 HERMES_HOME/skills/qingyu-shiguang；不更改 gateway、插件或配置。
pub fn install_hermes(root: &Path) -> Result<String, String> { install_hermes_at(root, &hermes_home()?) }

fn install_hermes_at(root: &Path, home: &Path) -> Result<String, String> {
    let home = absolute(home)?;
    let skills = home.join("skills");
    prepare_directory(&home)?;
    prepare_directory(&skills)?;
    let dir = skills.join(SKILL_NAME);
    if installed_at(root, &home) {
        return Ok("Hermes 拾光技能已连接到当前数据目录。微信采集需由现有 Hermes 微信网关接收消息。".into());
    }
    let lock = skills.join(".qingyu-shiguang.install-lock");
    fs::create_dir(&lock).map_err(|e| format!("无法锁定 Hermes 技能安装（{}）：{e}；若上次安装被中断，请确认无安装进程后移除此空目录", lock.display()))?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup { fn drop(&mut self) { let _ = fs::remove_dir(&self.0); } }
    let _guard = Cleanup(lock);
    if fs::symlink_metadata(&dir).is_ok() {
        managed_manifest(&dir).map_err(|e| format!("无法覆盖 {}：{e}", dir.display()))?;
    }
    let (root_text, files) = install_files(root, &dir)?;
    prepare_directory(root)?;
    prepare_directory(&root.join("records"))?;
    prepare_directory(&root.join("states"))?;
    let mut random = [0u8; 8];
    SystemRandom::new().fill(&mut random).map_err(|_| "无法生成安装事务 ID".to_string())?;
    let stage = skills.join(format!(".qingyu-shiguang-stage-{}", hex(&random)));
    let backup = skills.join(format!(".qingyu-shiguang-backup-{}", hex(&random)));
    prepare_directory(&stage)?;
    let write_result = (|| {
        prepare_directory(&stage.join("scripts"))?;
        for (name, bytes) in &files { save_bytes(&stage.join(name), bytes)?; }
        let manifest = InstallManifest { owner: SKILL_NAME.into(), version: VERSION, root: root_text, files: files.iter().map(|(name, bytes)| (name.clone(), checksum(bytes))).collect() };
        save_json(&stage.join(MANIFEST_NAME), &manifest)
    })();
    if let Err(error) = write_result { let _ = fs::remove_dir_all(&stage); return Err(error); }
    let had_old = dir.exists();
    if had_old {
        if let Err(error) = fs::rename(&dir, &backup) { let _ = fs::remove_dir_all(&stage); return Err(format!("无法保留旧版拾光技能：{error}")); }
    }
    if let Err(error) = fs::rename(&stage, &dir) {
        let rollback = if had_old { fs::rename(&backup, &dir).err() } else { None };
        let _ = fs::remove_dir_all(&stage);
        return Err(format!("安装拾光技能失败：{error}{}", rollback.map(|e| format!("；原技能保留在 {}，恢复失败：{e}", backup.display())).unwrap_or_default()));
    }
    sync_directory(&skills)?;
    if had_old { fs::remove_dir_all(&backup).map_err(|e| format!("技能已安装，旧版备份仍在 {}：{e}", backup.display()))?; }
    if !installed_at(root, &home) { return Err("安装后校验失败，请保留技能目录并重试".into()); }
    Ok(format!("已安装 Hermes 拾光技能，连接到 {}。在 Hermes 新会话中加载 qingyu-shiguang；微信消息需通过已有微信网关送达，连接尚需实际验证。", root.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static SERIAL: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!("qingyu-library-{}-{}", std::process::id(), SERIAL.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    fn record(root: &Path, source_id: &str, time: &str) -> Record {
        let record = Record { version: VERSION, id: record_id(source_id), source_id: source_id.into(), kind: Kind::Idea,
            title: "一个想法".into(), summary: "已提炼的摘要".into(), original: "原始消息".into(), url: None,
            tags: vec!["设计".into()], topic: None, next_action: None, source: "微信 · Hermes".into(), created_at: time.into(), updated_at: time.into() };
        prepare_directory(&root.join("records")).unwrap();
        save_json(&root.join("records").join(format!("{}.json", record.id)), &record).unwrap();
        record
    }

    #[test]
    fn manual_capture_preserves_original_and_extracts_link_without_fake_summary() {
        let dir = Temp::new();
        let mut store = Store::at(dir.0.join("library"));
        let text = "看看这个 https://example.org/video?id=1\n下周做个原型";
        let id = store.capture(Kind::Link, text).unwrap();
        let entry = &store.entries[0];
        assert_eq!(entry.id, id);
        assert_eq!(entry.original, text);
        assert_eq!(entry.url.as_deref(), Some("https://example.org/video?id=1"));
        assert!(entry.summary.is_empty());
        assert_eq!(entry.source, "手动");
        let unchanged_allocation = store.entries.as_ptr();
        assert!(!store.refresh());
        assert_eq!(store.entries.as_ptr(), unchanged_allocation);
        assert!(store.capture(Kind::Link, "file:///tmp/private").is_err());
        assert!(store.capture(Kind::Idea, " \n ").is_err());
        assert!(store.capture(Kind::Idea, &"a".repeat(MAX_ORIGINAL_BYTES + 1)).is_err());
        assert_eq!(store.entries.len(), 1);
        assert!(store.capture(Kind::Idea, &"\u{1}".repeat(MAX_ORIGINAL_BYTES)).is_err());
        assert_eq!(store.entries.len(), 1);
    }

    #[test]
    fn agent_refresh_and_ui_state_never_rewrite_each_other() {
        let dir = Temp::new();
        let mut item = record(&dir.0, "wechat:chat:42", "2026-11-15T10:00:00Z");
        let path = dir.0.join("records").join(format!("{}.json", item.id));
        let mut store = Store::at(dir.0.clone());
        let before = fs::read(&path).unwrap();
        let state = EntryState { pinned: true, reviewed: true, done: true, archived: false };
        store.set_state(&item.id, state.clone()).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        item.summary = "新的 Hermes 摘要".into();
        item.updated_at = "2026-11-15T11:00:00Z".into();
        save_json(&path, &item).unwrap();
        assert!(store.refresh());
        assert_eq!(store.entries[0].summary, item.summary);
        assert_eq!(store.entries[0].state, state);
        assert_eq!(Store::at(dir.0.clone()).entries[0].state, state);
    }

    #[test]
    fn malformed_and_oversized_files_retain_last_good_data_and_state() {
        let dir = Temp::new();
        let item = record(&dir.0, "wechat:chat:43", "2026-11-15T10:00:00Z");
        let mut store = Store::at(dir.0.clone());
        store.set_state(&item.id, EntryState { pinned: true, ..Default::default() }).unwrap();
        let entry = store.entries[0].clone();
        let path = dir.0.join("records").join(format!("{}.json", item.id));
        save_bytes(&path, b"{").unwrap();
        assert!(store.refresh());
        assert_eq!(store.entries, vec![entry.clone()]);
        assert!(store.error.as_ref().unwrap().contains("JSON"));
        assert!(!store.refresh());
        save_bytes(&path, &vec![b' '; MAX_RECORD_BYTES as usize + 1]).unwrap();
        store.refresh();
        assert_eq!(store.entries, vec![entry.clone()]);
        assert!(store.error.as_ref().unwrap().contains("上限"));
        save_json(&path, &item).unwrap();
        save_bytes(&dir.0.join("states").join(format!("{}.json", item.id)), b"{\"version\":1}").unwrap();
        store.refresh();
        assert_eq!(store.entries[0].state, entry.state);
        assert!(store.error.as_ref().unwrap().contains("状态"));
    }

    #[test]
    fn records_are_pinned_then_newest_then_stable_id() {
        let dir = Temp::new();
        let a = record(&dir.0, "wechat:a", "2026-11-15T10:00:00+09:00");
        let b = record(&dir.0, "wechat:b", "2026-11-15T01:01:00Z");
        let c = record(&dir.0, "wechat:c", "2026-11-15T01:01:00Z");
        let mut store = Store::at(dir.0.clone());
        let mut expected = vec![b.id, c.id]; expected.sort(); expected.push(a.id.clone());
        assert_eq!(store.entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), expected);
        store.set_state(&a.id, EntryState { pinned: true, ..Default::default() }).unwrap();
        assert_eq!(store.entries[0].id, a.id);
        assert!(!store.refresh());
    }

    #[test]
    fn invalid_ids_and_failed_saves_do_not_report_success_or_change_memory() {
        let dir = Temp::new();
        let item = record(&dir.0, "wechat:one", "2026-11-15T10:00:00Z");
        let mut store = Store::at(dir.0.clone());
        assert!(store.set_state("../../outside", EntryState::default()).is_err());
        fs::write(dir.0.join("states"), b"blocked").unwrap();
        assert!(store.set_state(&item.id, EntryState { done: true, ..Default::default() }).is_err());
        assert!(!store.entries[0].state.done);
        let blocked = dir.0.join("not-a-directory"); fs::write(&blocked, b"blocked").unwrap();
        let mut other = Store::at(blocked);
        assert!(other.capture(Kind::Idea, "仍需保存的内容").is_err());
        assert!(other.entries.is_empty());
        assert!(other.error.is_some());
    }

    #[test]
    fn filename_id_and_record_identity_must_agree() {
        let dir = Temp::new();
        let mut item = record(&dir.0, "wechat:identity", "2026-11-15T10:00:00Z");
        let mut store = Store::at(dir.0.clone());
        item.source_id = "../../outside".into();
        save_json(&dir.0.join("records").join(format!("{}.json", item.id)), &item).unwrap();
        store.refresh();
        assert_eq!(store.entries.len(), 1);
        assert!(store.error.as_ref().unwrap().contains("ID"));
        assert_eq!(record_id("wechat:chat:42"), "e08dd90b146898e938a8bc19116068950563652bff5d38a2771193dd4de49042");
    }

    #[test]
    fn unknown_record_filename_does_not_hide_valid_records_on_first_load() {
        let dir = Temp::new();
        let item = record(&dir.0, "wechat:valid", "2026-11-15T10:00:00Z");
        fs::write(dir.0.join("records/bad-name.json"), b"{}").unwrap();
        let store = Store::at(dir.0.clone());
        assert_eq!(store.entries[0].id, item.id);
        assert!(store.error.as_ref().unwrap().contains("文件名"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_redirect_state_writes() {
        use std::os::unix::fs::symlink;
        let dir = Temp::new();
        let item = record(&dir.0, "wechat:symlink", "2026-11-15T10:00:00Z");
        let mut store = Store::at(dir.0.clone());
        let outside = dir.0.join("outside"); fs::create_dir(&outside).unwrap();
        symlink(&outside, dir.0.join("states")).unwrap();
        assert!(store.set_state(&item.id, EntryState::default()).is_err());
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }

    #[test]
    fn installer_is_exact_idempotent_binds_root_and_preserves_custom_skill() {
        let dir = Temp::new();
        let home = dir.0.join("hermes");
        let root = dir.0.join("Qingyu 空间's/library");
        install_hermes_at(&root, &home).unwrap();
        assert!(installed_at(&root, &home));
        assert!(!installed_at(&dir.0.join("other"), &home));
        install_hermes_at(&root, &home).unwrap();
        let other = dir.0.join("dev/library");
        install_hermes_at(&other, &home).unwrap();
        assert!(installed_at(&other, &home));
        let skill = home.join("skills/qingyu-shiguang/SKILL.md");
        fs::write(&skill, b"user customized skill").unwrap();
        assert!(install_hermes_at(&root, &home).is_err());
        assert_eq!(fs::read(&skill).unwrap(), b"user customized skill");
        assert!(!installed_at(&other, &home));
        let nonmanaged = dir.0.join("other-hermes/skills/qingyu-shiguang");
        fs::create_dir_all(&nonmanaged).unwrap();
        fs::write(nonmanaged.join("SKILL.md"), b"another author's skill").unwrap();
        assert!(install_hermes_at(&root, &dir.0.join("other-hermes")).is_err());
        assert_eq!(fs::read(nonmanaged.join("SKILL.md")).unwrap(), b"another author's skill");
    }

    #[test]
    fn installer_preserves_added_files_and_handles_failed_directory() {
        let dir = Temp::new();
        let home = dir.0.join("hermes");
        let root = dir.0.join("library");
        install_hermes_at(&root, &home).unwrap();
        let extra = home.join("skills/qingyu-shiguang/my-notes.txt");
        fs::write(&extra, b"keep").unwrap();
        assert!(install_hermes_at(&root, &home).is_err());
        assert_eq!(fs::read(extra).unwrap(), b"keep");
        let blocked = dir.0.join("blocked"); fs::write(&blocked, b"file").unwrap();
        assert!(install_hermes_at(&root, &blocked).is_err());
    }

    #[test]
    fn installed_python_cli_and_rust_store_share_records_and_independent_state() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let dir = Temp::new();
        let home = dir.0.join("hermes's $(not-a-command)");
        let root = dir.0.join("dev's library");
        install_hermes_at(&root, &home).unwrap();
        let script = home.join("skills/qingyu-shiguang/scripts/shiguang.py");
        let run_ingest = |summary: &str| {
            let mut command = Command::new("python3").arg(&script).arg("ingest")
                .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
            let payload = serde_json::json!({"source_id":"wechat:round-trip:42","kind":"link","title":"跨语言收藏",
                "original":"收藏 https://example.com/video", "summary":summary,"tags":["验证"]});
            command.stdin.take().unwrap().write_all(&serde_json::to_vec(&payload).unwrap()).unwrap();
            let output = command.wait_with_output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
        };
        let saved = run_ingest("视频内容未读取");
        let id = saved["id"].as_str().unwrap();
        let mut store = Store::at(root.clone());
        assert!(store.error.is_none(), "{:?}", store.error);
        assert_eq!(store.entries[0].id, id);
        assert_eq!(store.entries[0].url.as_deref(), Some("https://example.com/video"));
        let state = EntryState { pinned:true, reviewed:true, done:true, archived:false };
        store.set_state(id, state.clone()).unwrap();
        let refined = run_ingest("用户补充了用途；视频内容仍未读取");
        assert_eq!(refined["created"], false);
        store.refresh();
        assert_eq!(store.entries[0].summary, "用户补充了用途；视频内容仍未读取");
        assert_eq!(store.entries[0].state, state);
        let output = Command::new("python3").arg(&script).args(["search", "跨语言"]).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let found: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(found["entries"][0]["state"]["done"], true);
        // 验证 SKILL.md 中实际使用的 POSIX 引号，即使路径含单引号和 $() 也安全。
        #[cfg(unix)] {
            let output = Command::new("/bin/sh").args(["-c", &format!("python3 {} doctor", shell_quote(script.to_str().unwrap()))]).output().unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            let doctor: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(doctor["root"], root.to_str().unwrap());
        }
    }
}
