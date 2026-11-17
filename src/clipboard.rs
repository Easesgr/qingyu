//! 本机剪贴板历史：文字以及 macOS 复制的图片、视频。
//!
//! 媒体导入、哈希和缓存回收只在监听线程进行。记录指向自有缓存副本，
//! 原文件不会被移动或删除。重新复制带历史标记，不会改变列表顺序。

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) mod maccy;

/// 未固定条目的上限；固定的不占这个名额。
pub const MAX_RECENT: usize = 30;
pub const MAX_CHARS: usize = 20_000;
pub const MAX_PINNED: usize = 20;
pub const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_VIDEO_BYTES: u64 = 256 * 1024 * 1024;
/// 已保留媒体的总大小。固定内容不会为了新记录被回收；空间不足时跳过新媒体。
pub const MAX_MEDIA_CACHE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_FILES_PER_COPY: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind { Image, Video }

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Media {
    pub path: PathBuf,
    pub name: String,
    pub kind: MediaKind,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clip {
    pub text: String,
    /// 本地时间 `HH:MM`。
    pub at: String,
    pub pinned: bool,
    pub media: Option<Media>,
}

impl Clip {
    pub fn preview(&self) -> String { preview(self.media.as_ref().map_or(&self.text, |m| &m.name)) }
    pub fn label(&self) -> &'static str {
        match self.media.as_ref().map(|m| m.kind) { Some(MediaKind::Image) => "图片", Some(MediaKind::Video) => "视频", None => "文字" }
    }
    pub fn description(&self) -> String {
        if let Some(media) = &self.media {
            let bytes = media.bytes as f64;
            let size = if bytes >= 1024.0 * 1024.0 { format!("{:.1} MB", bytes / (1024.0 * 1024.0)) }
                else if bytes >= 1024.0 { format!("{:.0} KB", bytes / 1024.0) } else { format!("{} B", media.bytes) };
            format!("{} · {size}", self.label())
        } else { format!("文字 · {} 字", self.text.chars().count()) }
    }
    fn same_content(&self, other: &Self) -> bool {
        match (&self.media, &other.media) {
            (Some(a), Some(b)) => a.kind == b.kind && a.path == b.path,
            (None, None) => self.text == other.text,
            _ => false,
        }
    }
}

#[derive(Debug, Default)]
pub struct History {
    pub items: Vec<Clip>,
    pub version: u64,
    path: Option<PathBuf>,
    // A failed JSON write must not let pruning delete assets still referenced by the last saved history.
    persisted_media: HashSet<PathBuf>,
}

impl History {
    pub fn push(&mut self, text: &str, at: &str) -> bool {
        if text.trim().is_empty() || text.chars().count() > MAX_CHARS { return false; }
        self.insert(Clip { text: text.to_string(), at: at.to_string(), pinned: false, media: None }, MAX_MEDIA_CACHE_BYTES)
    }

    fn push_media(&mut self, media: Media, at: &str) -> bool {
        self.insert(Clip { text: media.name.clone(), at: at.to_string(), pinned: false, media: Some(media) }, MAX_MEDIA_CACHE_BYTES)
    }

    fn insert(&mut self, mut item: Clip, media_limit: u64) -> bool {
        let mut items = self.items.clone();
        match items.iter().position(|old| old.same_content(&item)) {
            Some(0) => return false,
            Some(i) => item.pinned = items.remove(i).pinned,
            None => {},
        }
        let new_media = item.media.as_ref().map(|m| m.path.clone());
        items.insert(0, item);
        trim_counts(&mut items);
        // Remove the oldest unpinned media first. Pinned assets and all text records survive this quota.
        while media_bytes(&items) > media_limit {
            let Some(index) = items.iter().rposition(|clip| !clip.pinned && clip.media.is_some()) else { return false; };
            if items[index].media.as_ref().map(|m| &m.path) == new_media.as_ref() { return false; }
            items.remove(index);
        }
        self.items = items;
        self.changed();
        true
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() { self.items.remove(index); self.changed(); }
    }

    /// UI rows are snapshots; locate their content again after a background insertion or promotion.
    pub fn remove_clip(&mut self, clip: &Clip) {
        if let Some(index) = self.items.iter().position(|item| item.same_content(clip)) { self.remove(index); }
    }

    pub fn clear_recent(&mut self) {
        self.items.retain(|c| c.pinned);
        self.changed();
    }

    pub fn toggle_pin(&mut self, index: usize) -> bool {
        let pinned_count = self.items.iter().filter(|c| c.pinned).count();
        let Some(item) = self.items.get_mut(index) else { return false; };
        if !item.pinned && pinned_count >= MAX_PINNED { return false; }
        item.pinned = !item.pinned;
        self.changed();
        true
    }

    pub fn toggle_pin_clip(&mut self, clip: &Clip) -> bool {
        self.items.iter().position(|item| item.same_content(clip)).is_some_and(|index| self.toggle_pin(index))
    }

    fn referenced_media(&self) -> HashSet<PathBuf> {
        self.items.iter().filter_map(|c| c.media.as_ref().map(|m| m.path.clone())).collect()
    }

    fn protected_media(&self) -> HashSet<PathBuf> {
        let mut paths = self.referenced_media();
        paths.extend(self.persisted_media.iter().cloned());
        paths
    }

    fn to_json(&self) -> Vec<u8> {
        let rows: Vec<_> = self.items.iter().map(|c| {
            let mut row = serde_json::json!({"pinned": c.pinned, "text": c.text, "at": c.at});
            if let Some(media) = &c.media { row["media"] = serde_json::to_value(media).unwrap_or_default(); }
            row
        }).collect();
        serde_json::to_vec(&rows).unwrap_or_default()
    }

    pub fn from_json(raw: &[u8]) -> Self {
        let rows: Vec<serde_json::Value> = serde_json::from_slice(raw).unwrap_or_default();
        let mut items: Vec<Clip> = Vec::new();
        for row in rows.iter().take(MAX_PINNED + MAX_RECENT + MAX_FILES_PER_COPY) {
            let media: Option<Media> = match row.get("media") {
                None | Some(serde_json::Value::Null) => None,
                Some(value) => match serde_json::from_value::<Media>(value.clone()) {
                    Ok(media) if !media.name.trim().is_empty() && media.bytes > 0 && media.bytes <= media_size_limit(media.kind) && owned_media_name(&media.path) &&
                        media_format(&media.path).is_some_and(|(kind, _)| kind == media.kind) => Some(media),
                    _ => continue,
                },
            };
            let Some(text) = row.get("text").and_then(|v| v.as_str()).map(str::to_owned).or_else(|| media.as_ref().map(|m| m.name.clone())) else { continue; };
            if text.trim().is_empty() || text.chars().count() > MAX_CHARS { continue; }
            let item = Clip { text, media, at: row.get("at").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                pinned: row.get("pinned").and_then(|v| v.as_bool()).unwrap_or(true) };
            if !items.iter().any(|old| old.same_content(&item)) { items.push(item); }
        }
        trim_counts(&mut items);
        // Media cannot exceed the on-disk quota, even if metadata was edited outside the application.
        let mut remaining = MAX_MEDIA_CACHE_BYTES;
        items.retain(|c| c.media.as_ref().is_none_or(|m| {
            if m.bytes > remaining { false } else { remaining -= m.bytes; true }
        }));
        Self { items, ..Self::default() }
    }

    fn path() -> PathBuf { crate::model::config_dir().join("clipboard.json") }
    pub fn load() -> Self { Self::load_from(Self::path()) }

    fn media_dir(&self) -> Option<PathBuf> {
        let root = self.path.as_ref()?.parent()?.join("clipboard-media");
        if std::fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink()) { return None; }
        Some(root.canonicalize().unwrap_or(root))
    }

    fn load_from(path: PathBuf) -> Self {
        let path = if path.is_absolute() { path } else { std::env::current_dir().unwrap_or_default().join(path) };
        let mut history = std::fs::read(&path).map(|raw| Self::from_json(&raw)).unwrap_or_default();
        history.path = Some(path);
        history.persisted_media = history.referenced_media();
        let root = history.media_dir();
        history.items.retain_mut(|clip| {
            let Some(media) = &mut clip.media else { return true; };
            let Some(root) = &root else { return false; };
            // Only direct, regular, locally owned cache files can be replayed from persisted metadata.
            if !media.path.is_absolute() && media.path.components().count() == 1 { media.path = root.join(&media.path); }
            media.path.parent() == Some(root.as_path()) && std::fs::symlink_metadata(&media.path)
                .is_ok_and(|m| m.is_file() && m.len() == media.bytes)
        });
        history
    }

    fn changed(&mut self) {
        self.version = self.version.wrapping_add(1);
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
            if crate::model::atomic_write(path, &self.to_json()).is_ok() { self.persisted_media = self.referenced_media(); }
        }
    }
}

fn trim_counts(items: &mut Vec<Clip>) {
    let (mut pinned, mut recent) = (0, 0);
    items.retain(|c| if c.pinned { pinned += 1; pinned <= MAX_PINNED } else { recent += 1; recent <= MAX_RECENT });
}
fn media_bytes(items: &[Clip]) -> u64 {
    let mut seen = HashSet::new();
    items.iter().filter_map(|c| c.media.as_ref()).filter(|m| seen.insert(&m.path)).map(|m| m.bytes).sum()
}
fn media_size_limit(kind: MediaKind) -> u64 { match kind { MediaKind::Image => MAX_IMAGE_BYTES, MediaKind::Video => MAX_VIDEO_BYTES } }

fn media_format(path: &Path) -> Option<(MediaKind, &'static str)> {
    Some(match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => (MediaKind::Image, "png"), "jpg" | "jpeg" | "jpe" => (MediaKind::Image, "jpg"),
        "tif" | "tiff" => (MediaKind::Image, "tiff"), "gif" => (MediaKind::Image, "gif"),
        "webp" => (MediaKind::Image, "webp"), "heic" => (MediaKind::Image, "heic"), "heif" => (MediaKind::Image, "heif"),
        "bmp" => (MediaKind::Image, "bmp"), "avif" => (MediaKind::Image, "avif"),
        "mp4" => (MediaKind::Video, "mp4"), "m4v" => (MediaKind::Video, "m4v"), "mov" => (MediaKind::Video, "mov"),
        "webm" => (MediaKind::Video, "webm"), "mkv" => (MediaKind::Video, "mkv"), "avi" => (MediaKind::Video, "avi"),
        "mpeg" | "mpg" => (MediaKind::Video, "mpeg"), "m2v" => (MediaKind::Video, "m2v"),
        "mts" => (MediaKind::Video, "mts"), "m2ts" => (MediaKind::Video, "m2ts"), "3gp" => (MediaKind::Video, "3gp"),
        _ => return None,
    })
}

/// Restrict cleanup to the exact content-addressed namespace generated below. Never recurse or follow links.
fn owned_media_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else { return false; };
    let (kind, rest) = if let Some(rest) = name.strip_prefix("qy-image-") { (MediaKind::Image, rest) }
        else if let Some(rest) = name.strip_prefix("qy-video-") { (MediaKind::Video, rest) } else { return false; };
    let Some((hash, _)) = rest.split_once('.') else { return false; };
    hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) && media_format(path).is_some_and(|(format, _)| format == kind)
}

fn owned_pending_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|s| s.to_str()).and_then(|s| s.strip_prefix(".qy-import-")).and_then(|s| s.strip_suffix(".tmp")) else { return false; };
    let Some((pid, serial)) = name.split_once('-') else { return false; };
    !pid.is_empty() && !serial.is_empty() && pid.bytes().chain(serial.bytes()).all(|b| b.is_ascii_digit())
}

/// Streaming disk import; this type is only used by the watcher (and isolated filesystem tests).
struct MediaCache { root: PathBuf }
struct PendingMedia(PathBuf);
impl Drop for PendingMedia { fn drop(&mut self) { let _ = std::fs::remove_file(&self.0); } }
impl MediaCache {
    fn open(root: PathBuf) -> std::io::Result<Self> {
        if std::fs::symlink_metadata(&root).is_ok_and(|m| m.file_type().is_symlink()) { return Err(std::io::Error::other("媒体缓存不能是符号链接")); }
        let mut builder = std::fs::DirBuilder::new(); builder.recursive(true);
        #[cfg(unix)] { use std::os::unix::fs::DirBuilderExt; builder.mode(0o700); }
        builder.create(&root)?;
        Ok(Self { root: root.canonicalize()? })
    }

    fn import_file(&self, source: &Path, remaining: u64) -> std::io::Result<Media> {
        self.import_file_until(source, remaining, None)
    }

    fn import_file_until(&self, source: &Path, remaining: u64, cancelled: Option<&AtomicBool>) -> std::io::Result<Media> {
        let (kind, extension) = media_format(source).ok_or_else(|| std::io::Error::other("不支持的媒体类型"))?;
        #[cfg(target_os = "macos")]
        if !crate::mac_services::local_media_file(source) { return Err(std::io::Error::other("媒体尚未下载或不在本机磁盘")); }
        let metadata = source.metadata()?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > media_size_limit(kind).min(remaining) {
            return Err(std::io::Error::other("媒体超过大小限制或不是普通文件"));
        }
        let mut file = std::fs::File::open(source)?;
        let before = file.metadata()?;
        if !before.is_file() { return Err(std::io::Error::other("不是普通文件")); }
        let name = source.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let media = self.import_reader_until(&mut file, kind, extension, name, media_size_limit(kind).min(remaining), cancelled)?;
        let after = file.metadata()?;
        if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
            // The new cache file is an orphan and will be reclaimed by the same watcher iteration.
            return Err(std::io::Error::other("导入期间文件发生变化"));
        }
        Ok(media)
    }

    fn import_image(&self, data: &[u8], extension: &str) -> std::io::Result<Media> {
        self.import_image_until(data, extension, None)
    }

    fn import_image_until(&self, data: &[u8], extension: &str, cancelled: Option<&AtomicBool>) -> std::io::Result<Media> {
        let (kind, extension) = media_format(&PathBuf::from(format!("image.{extension}"))).filter(|(k, _)| *k == MediaKind::Image)
            .ok_or_else(|| std::io::Error::other("不支持的图片格式"))?;
        self.import_reader_until(&mut &data[..], kind, extension, format!("剪贴板图片.{extension}"), MAX_IMAGE_BYTES, cancelled)
    }

    fn import_reader(&self, reader: &mut impl Read, kind: MediaKind, extension: &str, name: String, limit: u64) -> std::io::Result<Media> {
        self.import_reader_until(reader, kind, extension, name, limit, None)
    }

    fn import_reader_until(&self, reader: &mut impl Read, kind: MediaKind, extension: &str, name: String, limit: u64, cancelled: Option<&AtomicBool>) -> std::io::Result<Media> {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let pending = PendingMedia(self.root.join(format!(".qy-import-{}-{}.tmp", std::process::id(), SERIAL.fetch_add(1, Ordering::Relaxed))));
        let mut options = std::fs::OpenOptions::new(); options.write(true).create_new(true);
        #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
        let mut target = options.open(&pending.0)?;
        let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
        let mut source = reader.take(limit + 1);
        let mut buffer = [0_u8; 64 * 1024];
        let mut bytes = 0_u64;
        loop {
            if capture_cancelled(cancelled) { return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "已停止媒体收集")); }
            let count = source.read(&mut buffer)?;
            if count == 0 { break; }
            bytes += count as u64;
            if bytes > limit { return Err(std::io::Error::other("媒体超过大小限制")); }
            hasher.update(&buffer[..count]); target.write_all(&buffer[..count])?;
        }
        if bytes == 0 { return Err(std::io::Error::other("媒体为空")); }
        target.sync_all()?; drop(target);
        let digest: String = hasher.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect();
        let label = match kind { MediaKind::Image => "image", MediaKind::Video => "video" };
        let path = self.root.join(format!("qy-{label}-{digest}.{extension}"));
        // Replacing this owned path also repairs a corrupted cache entry with the same content identity.
        std::fs::rename(&pending.0, &path)?;
        Ok(Media { path, name, kind, bytes })
    }

    fn cleanup(&self, protected: &HashSet<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(&self.root) else { return; };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else { continue; };
            if !metadata.is_file() { continue; }
            let abandoned = owned_pending_name(&path) &&
                metadata.modified().ok().and_then(|time| time.elapsed().ok()).is_some_and(|age| age.as_secs() > 24 * 60 * 60);
            if abandoned || (owned_media_name(&path) && !protected.contains(&path)) { let _ = std::fs::remove_file(path); }
        }
    }
}

fn capture_cancelled(cancelled: Option<&AtomicBool>) -> bool { cancelled.is_some_and(|flag| flag.load(Ordering::Acquire)) }

// Dropping a watcher is intentionally nonblocking. Old and new watcher generations may overlap,
// so serialize their import+history commit and cleanup on a worker-only mutex, never REPLAY_LEASE.
#[cfg(any(target_os = "macos", test))]
static MEDIA_WORK: Mutex<()> = Mutex::new(());
#[cfg(any(target_os = "macos", test))]
fn media_work_lock() -> std::sync::MutexGuard<'static, ()> { MEDIA_WORK.lock().unwrap_or_else(|e| e.into_inner()) }

#[derive(Clone)]
pub struct Shared {
    pub history: Arc<Mutex<History>>,
    pub enabled: Arc<AtomicBool>,
}
impl Shared {
    pub fn new(enabled: bool) -> Self {
        Self { history: Arc::new(Mutex::new(History::load())), enabled: Arc::new(AtomicBool::new(enabled)) }
    }
    pub fn set_enabled(&self, on: bool) { let _history = self.lock(); self.enabled.store(on, Ordering::Relaxed); }
    pub fn is_enabled(&self) -> bool { self.enabled.load(Ordering::Relaxed) }
    pub fn lock(&self) -> std::sync::MutexGuard<'_, History> { self.history.lock().unwrap_or_else(|e| e.into_inner()) }
    #[cfg(any(windows, test))]
    fn record_update(&self, text: &str, from_history: bool) -> bool { !from_history && self.record(text) }
    pub fn record(&self, text: &str) -> bool {
        self.record_until(text, None)
    }
    fn record_until(&self, text: &str, cancelled: Option<&AtomicBool>) -> bool {
        let mut history = self.lock();
        if !self.is_enabled() || capture_cancelled(cancelled) { return false; }
        history.push(text, &chrono::Local::now().format("%H:%M").to_string())
    }
    fn record_media(&self, media: Media) -> bool {
        self.record_media_until(media, None)
    }
    fn record_media_until(&self, media: Media, cancelled: Option<&AtomicBool>) -> bool {
        let mut history = self.lock();
        if !self.is_enabled() || capture_cancelled(cancelled) { return false; }
        history.push_media(media, &chrono::Local::now().format("%H:%M").to_string())
    }

    #[cfg(target_os = "macos")]
    fn capture(&self, content: crate::mac_services::ClipboardContent, cache: Option<&MediaCache>) -> bool {
        self.capture_until(content, cache, None)
    }
    #[cfg(target_os = "macos")]
    fn capture_until(&self, content: crate::mac_services::ClipboardContent, cache: Option<&MediaCache>, cancelled: Option<&AtomicBool>) -> bool {
        use crate::mac_services::ClipboardContent;
        let _work = media_work_lock();
        if !self.is_enabled() || capture_cancelled(cancelled) { return false; }
        match content {
            ClipboardContent::Text(text) => self.record_until(&text, cancelled),
            ClipboardContent::Image { data, extension } => cache.and_then(|cache| cache.import_image_until(&data, &extension, cancelled).ok()).is_some_and(|media| self.record_media_until(media, cancelled)),
            ClipboardContent::Files(paths) => {
                let Some(cache) = cache else { return false; };
                let mut remaining = MAX_MEDIA_CACHE_BYTES;
                let mut changed = false;
                for source in paths.iter().take(MAX_FILES_PER_COPY).rev() {
                    if !self.is_enabled() || capture_cancelled(cancelled) { break; }
                    if let Ok(media) = cache.import_file_until(source, remaining, cancelled) {
                        remaining -= media.bytes;
                        changed |= self.record_media_until(media, cancelled);
                    }
                }
                changed
            }
        }
    }
}
impl Default for Shared { fn default() -> Self { Self::new(true) } }
pub fn preview(text: &str) -> String { text.split_whitespace().collect::<Vec<_>>().join(" ") }

pub struct Watcher {
    #[cfg(windows)] stop: Arc<win::Stop>,
    #[cfg(target_os = "macos")] stop: std::sync::mpsc::Sender<()>,
    #[cfg(target_os = "macos")] cancelled: Arc<AtomicBool>,
}
#[cfg(windows)]
impl Drop for Watcher { fn drop(&mut self) { self.stop.request(); } }
#[cfg(windows)]
pub fn watch(shared: Shared, repaint: impl Fn() + Send + 'static) -> Option<Watcher> {
    let stop = Arc::new(win::Stop::default()); let thread_stop = stop.clone();
    std::thread::Builder::new().name("qingyu-clipboard".into()).spawn(move || { let _ = win::run(shared, Box::new(repaint), &thread_stop); }).ok()?;
    Some(Watcher { stop })
}

#[cfg(target_os = "macos")]
static REPLAY_LEASE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

#[cfg(target_os = "macos")]
fn maintain_cache(shared: &Shared, cache: &MediaCache) {
    maintain_cache_with(shared, cache, crate::mac_services::clipboard_replay_path);
}

#[cfg(target_os = "macos")]
fn maintain_cache_with(shared: &Shared, cache: &MediaCache, read_replay: impl FnOnce() -> crate::mac_services::ReplayPath) {
    use crate::mac_services::ReplayPath;
    // Serialize native replay registration and pruning, including the interval before publication.
    let _work = media_work_lock();
    let mut lease = REPLAY_LEASE.lock().unwrap_or_else(|e| e.into_inner());
    match read_replay() {
        ReplayPath::Active(paths) => *lease = paths,
        ReplayPath::Empty => lease.clear(),
        ReplayPath::Unstable => {},
    }
    let mut protected = shared.lock().protected_media();
    protected.extend(lease.iter().cloned());
    cache.cleanup(&protected);
}

#[cfg(target_os = "macos")]
impl Drop for Watcher { fn drop(&mut self) { self.cancelled.store(true, Ordering::Release); let _ = self.stop.send(()); } }
#[cfg(target_os = "macos")]
pub fn watch(shared: Shared, repaint: impl Fn() + Send + 'static) -> Option<Watcher> {
    let (stop, stopped) = std::sync::mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false)); let thread_cancelled = cancelled.clone();
    std::thread::Builder::new().name("qingyu-clipboard".into()).spawn(move || {
        if thread_cancelled.load(Ordering::Acquire) { return; }
        let cache = shared.lock().media_dir().and_then(|root| MediaCache::open(root).ok());
        let mut change = -1;
        // Only observe new copies; disabled collection advances the counter without loading data.
        let _ = crate::mac_services::clipboard(&mut change, false);
        if let Some(cache) = &cache { maintain_cache(&shared, cache); }
        let mut cleaned_version = shared.lock().version;
        while stopped.recv_timeout(std::time::Duration::from_millis(300)) == Err(std::sync::mpsc::RecvTimeoutError::Timeout) {
            let before = change;
            let content = crate::mac_services::clipboard(&mut change, shared.is_enabled());
            let attempted = content.is_some();
            if content.is_some_and(|content| shared.capture_until(content, cache.as_ref(), Some(&thread_cancelled))) { repaint(); }
            let version = shared.lock().version;
            if attempted || change != before || version != cleaned_version {
                if let Some(cache) = &cache { maintain_cache(&shared, cache); }
                cleaned_version = version;
            }
        }
    }).ok()?;
    Some(Watcher { stop, cancelled })
}
#[cfg(not(any(windows, target_os = "macos")))]
pub fn watch(_shared: Shared, _repaint: impl Fn() + Send + 'static) -> Option<Watcher> { Some(Watcher {}) }

/// Media replay uses cached data, so moving or deleting the source does not affect a later paste.
pub fn copy_clip(clip: &Clip) -> bool {
    let Some(media) = &clip.media else { return copy_history(&clip.text); };
    #[cfg(target_os = "macos")] {
        let mut lease = REPLAY_LEASE.lock().unwrap_or_else(|e| e.into_inner());
        if crate::mac_services::copy_media(&media.path, media.kind == MediaKind::Image) { *lease = vec![media.path.clone()]; true } else { false }
    }
    #[cfg(not(target_os = "macos"))] { let _ = media; false }
}

/// Recopy a file selection in its original order, using one pasteboard item per cached file.
/// A single row keeps `copy_clip`'s text/image behavior; multiple rows must all contain media.
pub fn copy_clips(clips: &[Clip]) -> bool {
    if let [clip] = clips { return copy_clip(clip); }
    if clips.is_empty() || clips.len() > MAX_FILES_PER_COPY { return false; }
    let paths: Option<Vec<PathBuf>> = clips.iter().map(|clip| clip.media.as_ref().filter(|m| m.bytes > 0 && m.bytes <= media_size_limit(m.kind)).map(|m| m.path.clone())).collect();
    let Some(paths) = paths else { return false; };
    #[cfg(target_os = "macos")] {
        let mut lease = REPLAY_LEASE.lock().unwrap_or_else(|e| e.into_inner());
        if crate::mac_services::copy_files(&paths) { *lease = paths; true } else { false }
    }
    #[cfg(not(target_os = "macos"))] { let _ = paths; false }
}
#[cfg(target_os = "macos")]
pub fn copy_history(text: &str) -> bool { crate::mac_services::copy_history(text) }
#[cfg(not(any(windows, target_os = "macos")))]
pub fn copy_history(_text: &str) -> bool { false }
#[cfg(windows)]
const HISTORY_COPY_FORMAT: &str = "Qingyu.ClipboardHistoryCopy";
#[cfg(windows)]
pub fn copy_history(text: &str) -> bool {
    use clipboard_win::{Clipboard, raw, register_format};
    let Some(format) = register_format(HISTORY_COPY_FORMAT) else { return false };
    let Ok(_clipboard) = Clipboard::new_attempts(10) else { return false };
    raw::set_string(text).is_ok() && raw::set_without_clear(format.get(), &[1]).is_ok()
}

#[cfg(windows)]
mod win {
    use super::Shared;
    use std::cell::RefCell;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    use windows::Win32::UI::WindowsAndMessaging::*;

    const WM_CLIPBOARDUPDATE: u32 = 0x031D;

    struct State { shared: Shared, repaint: Box<dyn Fn() + Send> }

    /// 监听线程建好消息窗口后登记线程号；持有者销毁时先置位再读线程号，两边总有一方看到对方。
    #[derive(Default)]
    pub struct Stop { requested: std::sync::atomic::AtomicBool, thread: std::sync::atomic::AtomicU32 }
    impl Stop {
        pub fn request(&self) {
            use std::sync::atomic::Ordering::SeqCst;
            self.requested.store(true, SeqCst);
            let thread = self.thread.load(SeqCst);
            if thread != 0 { unsafe { let _ = PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0)); } }
        }
        fn register(&self) -> bool {
            use std::sync::atomic::Ordering::SeqCst;
            self.thread.store(unsafe { windows::Win32::System::Threading::GetCurrentThreadId() }, SeqCst);
            !self.requested.load(SeqCst)
        }
    }
    thread_local! { static STATE: RefCell<Option<State>> = const { RefCell::new(None) }; }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_CLIPBOARDUPDATE {
            STATE.with(|state| {
                if let Some(state) = state.borrow().as_ref() {
                    if state.shared.is_enabled() {
                        if let Some((text, from_history)) = read_text(hwnd) {
                            if state.shared.record_update(&text, from_history) { (state.repaint)(); }
                        }
                    }
                }
            });
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }

    /// 密码管理器、远程桌面等会给敏感内容加上这些格式，表示不要进入剪贴板历史。
    unsafe fn excluded() -> bool {
        let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
        let ignore = RegisterClipboardFormatW(w!("Clipboard Viewer Ignore"));
        if (exclude != 0 && IsClipboardFormatAvailable(exclude).is_ok()) || (ignore != 0 && IsClipboardFormatAvailable(ignore).is_ok()) {
            return true;
        }
        // 值为 DWORD 0 表示不允许进入历史。
        let history = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
        if history == 0 || IsClipboardFormatAvailable(history).is_err() { return false; }
        let Ok(handle) = GetClipboardData(history) else { return true; };
        let memory = HGLOBAL(handle.0);
        let ptr = GlobalLock(memory) as *const u32;
        if ptr.is_null() { return true; }
        let allowed = GlobalSize(memory) >= 4 && *ptr != 0;
        let _ = GlobalUnlock(memory);
        !allowed
    }

    unsafe fn read_text(hwnd: HWND) -> Option<(String, bool)> {
        if IsClipboardFormatAvailable(CF_UNICODETEXT.0 as u32).is_err() { return None; }
        // 其他程序可能正占用剪贴板，稍等重试几次。
        let mut opened = false;
        for _ in 0..5 {
            if OpenClipboard(hwnd).is_ok() { opened = true; break; }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !opened { return None; }
        let text = (|| {
            if excluded() { return None; }
            let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok()?;
            let memory = HGLOBAL(handle.0);
            let ptr = GlobalLock(memory) as *const u16;
            if ptr.is_null() { return None; }
            let max = (GlobalSize(memory) / 2).min(super::MAX_CHARS * 2 + 1);
            let mut len = 0;
            while len < max && *ptr.add(len) != 0 { len += 1; }
            let text = (len <= super::MAX_CHARS * 2).then(|| String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)));
            let _ = GlobalUnlock(memory);
            let from_history = clipboard_win::register_format(super::HISTORY_COPY_FORMAT)
                .is_some_and(|format| IsClipboardFormatAvailable(format.get()).is_ok());
            text.map(|text| (text, from_history))
        })();
        let _ = CloseClipboard();
        text
    }

    pub fn run(shared: Shared, repaint: Box<dyn Fn() + Send>, stop: &Stop) -> windows::core::Result<()> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = w!("QingyuClipboardWatcher");
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(WINDOW_EX_STYLE(0), class, PCWSTR::null(), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, instance, None)?;
            // 消息窗口建好后线程已有消息队列，此后投递的 WM_QUIT 不会丢。
            if !stop.register() { let _ = DestroyWindow(hwnd); return Ok(()); }
            STATE.with(|state| *state.borrow_mut() = Some(State { shared, repaint }));
            AddClipboardFormatListener(hwnd)?;
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = RemoveClipboardFormatListener(hwnd);
            let _ = DestroyWindow(hwnd);
            STATE.with(|state| *state.borrow_mut() = None);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!("qingyu-clipboard-test-{}-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir(&path).unwrap(); Self(path.canonicalize().unwrap())
        }
        fn cache(&self) -> MediaCache { MediaCache::open(self.0.join("clipboard-media")).unwrap() }
        fn history(&self) -> History { History::load_from(self.0.join("clipboard.json")) }
    }
    impl Drop for TempDir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

    #[cfg(windows)]
    #[test]
    fn dropping_the_watcher_ends_the_listener_thread(){
        // 先停后启和启动后再停两种顺序都必须让线程退出，不能留下常驻的监听窗口。
        for stop_first in [true,false] {
            let stop=Arc::new(win::Stop::default());let thread_stop=stop.clone();
            let shared=Shared{history:Default::default(),enabled:Arc::new(AtomicBool::new(false))};
            if stop_first {stop.request();}
            let (done,finished)=std::sync::mpsc::channel();
            std::thread::spawn(move||{let _=win::run(shared,Box::new(||{}),&thread_stop);let _=done.send(());});
            if !stop_first {std::thread::sleep(std::time::Duration::from_millis(200));stop.request();}
            finished.recv_timeout(std::time::Duration::from_secs(3)).expect("监听线程应在停止后退出");
        }
    }

    #[test]
    fn history_copy_preserves_order_but_external_copy_promotes() {
        let shared = Shared { history: Default::default(), enabled: Arc::new(AtomicBool::new(true)) };
        shared.lock().items.clear();
        shared.record("较早的一条");
        shared.record("最近的一条");
        let before = shared.lock().items.clone();
        assert!(!shared.record_update("较早的一条", true));
        assert_eq!(shared.lock().items, before);
        assert!(shared.record_update("较早的一条", false));
        assert_eq!(shared.lock().items[0].text, "较早的一条");
    }
    #[test]
    fn history_dedupes_caps_and_keeps_pins() {
        let mut h = History::default();
        assert!(!h.push("   \n", "09:00"), "空白不记录");
        assert!(h.push("a", "09:00") && h.push("b", "09:01"));
        assert!(!h.push("b", "09:02"), "与最新一条相同不重复记录");
        assert!(h.push("a", "09:03"));
        assert_eq!(h.items.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(h.items[0].at, "09:03");
        h.items[1].pinned = true; // b
        for i in 0..(MAX_RECENT + 5) { h.push(&format!("n{i}"), "10:00"); }
        assert_eq!(h.items.iter().filter(|c| !c.pinned).count(), MAX_RECENT);
        assert!(h.items.iter().any(|c| c.text == "b" && c.pinned), "固定的不会被挤掉");
        h.clear_recent();
        assert_eq!(h.items.len(), 1);
        assert!(!h.push(&"x".repeat(MAX_CHARS + 1), "10:00"), "超长内容不记录");
    }

    #[test]
    fn reopening_restores_history_and_respects_manual_cleanup() {
        let dir = TempDir::new();
        let path = dir.0.join("clipboard.json");
        std::fs::write(&path, br#"[{"text":"old","at":"08:00"}]"#).unwrap();
        let mut history = History::load_from(path.clone());
        assert!(history.items[0].pinned);
        history.push("保留", "09:00");
        history.push("删除", "09:01");
        history.remove(0);
        let mut reopened = History::load_from(path.clone());
        assert_eq!(reopened.items, history.items);
        reopened.clear_recent();
        let final_history = History::load_from(path.clone());
        assert_eq!(final_history.items.len(), 1);
        assert_eq!(final_history.items[0].text, "old");
    }

    #[test]
    fn pinned_round_trip_and_preview() {
        let mut h = History::default();
        h.push("第一行\n第二行  末尾", "08:00");
        h.push("临时", "08:01");
        h.items[1].pinned = true;
        let back = History::from_json(&h.to_json());
        assert_eq!(back.items, h.items);
        assert_eq!(preview(&back.items[1].text), "第一行 第二行 末尾");
        assert!(History::from_json(b"not json").items.is_empty());
    }

    #[test]
    fn media_survives_source_deletion_and_reopening_with_text_migration() {
        let dir = TempDir::new(); let cache = dir.cache();
        std::fs::write(dir.0.join("clipboard.json"), br#"[{"text":"legacy pin","at":"08:00"}]"#).unwrap();
        let mut history = dir.history();
        let image = dir.0.join("photo.PNG"); let video = dir.0.join("video.MOV");
        std::fs::write(&image, b"image file contents").unwrap();
        std::fs::write(&video, b"video file contents").unwrap();
        let image_media = cache.import_file(&image, MAX_MEDIA_CACHE_BYTES).unwrap();
        let video_media = cache.import_file(&video, MAX_MEDIA_CACHE_BYTES).unwrap();
        assert!(history.push_media(image_media.clone(), "09:00"));
        assert!(history.toggle_pin(0));
        assert!(history.push_media(video_media.clone(), "09:01"));
        std::fs::remove_file(image).unwrap(); std::fs::remove_file(video).unwrap();
        let mut reopened = dir.history();
        assert_eq!(reopened.items, history.items);
        assert_eq!(std::fs::read(&image_media.path).unwrap(), b"image file contents");
        assert_eq!(std::fs::read(&video_media.path).unwrap(), b"video file contents");
        assert_eq!(reopened.items.last().unwrap().text, "legacy pin");
        assert!(reopened.items.last().unwrap().pinned);
        reopened.clear_recent(); cache.cleanup(&reopened.protected_media());
        assert!(!video_media.path.exists()); assert!(image_media.path.exists());
        assert_eq!(dir.history().items, reopened.items);
    }

    #[test]
    fn media_dedupes_by_content_and_identity_actions_survive_new_insertions() {
        let dir = TempDir::new(); let cache = dir.cache(); let mut history = dir.history();
        let media = cache.import_image(b"same image bytes", "png").unwrap();
        assert!(history.push_media(media.clone(), "10:00"));
        let row = history.items[0].clone();
        history.push(&media.name, "10:01"); // A text filename is distinct from the actual media.
        assert!(history.toggle_pin_clip(&row));
        history.push("new background copy", "10:02");
        assert!(history.push_media(media.clone(), "10:03"));
        assert!(history.items[0].pinned);
        assert_eq!(history.items.iter().filter(|c| c.media.is_some()).count(), 1);
        assert!(!history.push_media(media, "10:04"));
        history.remove_clip(&row);
        assert!(history.items.iter().all(|c| c.media.is_none()));
        assert!(history.items.iter().any(|c| c.text == row.text));
        let before = history.items.clone(); history.remove_clip(&row);
        assert_eq!(history.items, before, "an already removed row must not delete its neighbor");
    }

    #[test]
    fn quota_evicts_old_unpinned_media_and_preserves_pins_and_text() {
        let dir = TempDir::new(); let cache = dir.cache(); let mut history = History::default();
        let pinned = cache.import_image(b"pin!", "png").unwrap();
        let older = cache.import_image(b"old!", "png").unwrap();
        let newer = cache.import_image(b"new!", "png").unwrap();
        history.push_media(pinned.clone(), "10:00"); history.toggle_pin(0);
        history.push_media(older.clone(), "10:01"); history.push("keep text", "10:02");
        let new_clip = Clip { text: newer.name.clone(), at: "10:03".into(), pinned: false, media: Some(newer.clone()) };
        assert!(history.insert(new_clip, 8));
        assert_eq!(media_bytes(&history.items), 8);
        assert!(history.items.iter().any(|c| c.media.as_ref() == Some(&pinned) && c.pinned));
        assert!(history.items.iter().any(|c| c.text == "keep text"));
        assert!(!history.items.iter().any(|c| c.media.as_ref() == Some(&older)));
        let too_large = cache.import_image(b"five!", "png").unwrap();
        let before = history.items.clone();
        assert!(!history.insert(Clip { text: too_large.name.clone(), at: "10:04".into(), pinned: false, media: Some(too_large) }, 8));
        assert_eq!(history.items, before, "a rejected import must not evict existing records");
        let mut protected = history.protected_media(); protected.insert(older.path.clone());
        cache.cleanup(&protected); assert!(older.path.exists(), "the active pasteboard replay is leased even without a row");
        cache.cleanup(&history.protected_media()); assert!(!older.path.exists());
        assert!(newer.path.exists() && pinned.path.exists());
    }

    #[test]
    fn media_import_is_bounded_and_cleanup_leaves_originals_and_foreign_files_alone() {
        let dir = TempDir::new(); let cache = dir.cache();
        for (name, limit) in [("oversize.png", MAX_IMAGE_BYTES), ("oversize.mov", MAX_VIDEO_BYTES)] {
            let path = dir.0.join(name); let file = std::fs::File::create(&path).unwrap(); file.set_len(limit + 1).unwrap();
            assert!(cache.import_file(&path, MAX_MEDIA_CACHE_BYTES).is_err());
            assert!(path.exists());
        }
        assert!(cache.import_reader(&mut b"oversized stream".as_slice(), MediaKind::Image, "png", "stream.png".into(), 4).is_err());
        assert!(std::fs::read_dir(&cache.root).unwrap().next().is_none(), "failed imports must remove staging files");
        let source = dir.0.join("original.png"); std::fs::write(&source, b"original bytes").unwrap();
        let media = cache.import_file(&source, MAX_MEDIA_CACHE_BYTES).unwrap();
        let foreign = cache.root.join("user-notes.txt"); std::fs::write(&foreign, b"leave me").unwrap();
        let foreign_tmp = cache.root.join(".qy-import-user-notes.tmp"); std::fs::write(&foreign_tmp, b"also leave me").unwrap();
        let abandoned = cache.root.join(".qy-import-999999-1.tmp"); std::fs::write(&abandoned, b"stale owned staging").unwrap();
        for path in [&foreign_tmp, &abandoned] {
            let old = std::time::SystemTime::now() - std::time::Duration::from_secs(48 * 60 * 60);
            std::fs::OpenOptions::new().write(true).open(path).unwrap().set_times(std::fs::FileTimes::new().set_modified(old)).unwrap();
        }
        let folder = cache.root.join(media.path.file_name().unwrap()).with_extension("mov");
        std::fs::create_dir(&folder).unwrap(); std::fs::write(folder.join("nested.mov"), b"never recurse").unwrap();
        cache.cleanup(&HashSet::new());
        assert!(!media.path.exists() && !abandoned.exists()); assert!(source.exists() && foreign.exists() && foreign_tmp.exists() && folder.join("nested.mov").exists());
    }

    #[test]
    fn failed_history_save_keeps_last_persisted_media_alive() {
        let dir = TempDir::new(); let cache = dir.cache(); let mut history = dir.history();
        let media = cache.import_image(b"persisted image", "png").unwrap(); history.push_media(media.clone(), "10:00");
        let not_directory = dir.0.join("not-a-directory"); std::fs::write(&not_directory, b"block writes").unwrap();
        history.path = Some(not_directory.join("clipboard.json")); history.remove(0);
        cache.cleanup(&history.protected_media());
        assert!(media.path.exists(), "save failure must not delete the last durable history's asset");
        assert_eq!(dir.history().items[0].media.as_ref(), Some(&media));
    }

    #[test]
    fn persisted_media_cannot_escape_the_cache_or_follow_symlinks() {
        let dir = TempDir::new(); let cache = dir.cache();
        let media = cache.import_image(b"owned image", "png").unwrap();
        let outside = dir.0.join(media.path.file_name().unwrap()); std::fs::copy(&media.path, &outside).unwrap();
        let mut bad = media.clone(); bad.path = outside.clone();
        let row = serde_json::json!([{"text":"do not read external", "media":bad}]);
        std::fs::write(dir.0.join("clipboard.json"), serde_json::to_vec(&row).unwrap()).unwrap();
        assert!(dir.history().items.is_empty());
        #[cfg(unix)] {
            std::fs::remove_file(&media.path).unwrap();
            std::os::unix::fs::symlink(&outside, &media.path).unwrap();
            bad.path = media.path.clone();
            let row = serde_json::json!([{"text":"no symlink", "media":bad}]);
            std::fs::write(dir.0.join("clipboard.json"), serde_json::to_vec(&row).unwrap()).unwrap();
            assert!(dir.history().items.is_empty());
        }
        cache.cleanup(&HashSet::new()); assert_eq!(std::fs::read(outside).unwrap(), b"owned image");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn disabled_media_collection_skips_imports_and_files_are_independent_records() {
        use crate::mac_services::ClipboardContent;
        let dir = TempDir::new(); let cache = dir.cache();
        let shared = Shared { history: Arc::new(Mutex::new(dir.history())), enabled: Arc::new(AtomicBool::new(false)) };
        assert!(!shared.capture(ClipboardContent::Image { data: b"not collected".to_vec(), extension: "png".into() }, Some(&cache)));
        assert!(std::fs::read_dir(&cache.root).unwrap().next().is_none());
        shared.set_enabled(true);
        let image = dir.0.join("one.png"); let video = dir.0.join("two.mp4"); let other = dir.0.join("ignore.txt");
        std::fs::write(&image, b"image").unwrap(); std::fs::write(&video, b"video").unwrap(); std::fs::write(&other, b"text file").unwrap();
        assert!(shared.capture(ClipboardContent::Files(vec![image, other, video]), Some(&cache)));
        let rows = shared.lock().items.clone();
        assert_eq!(rows.len(), 2); assert_eq!(rows[0].media.as_ref().unwrap().kind, MediaKind::Image);
        assert_eq!(rows[1].media.as_ref().unwrap().kind, MediaKind::Video);
        shared.set_enabled(false);
        assert!(!shared.record_media(cache.import_image(b"disabled after IO", "png").unwrap()));
        cache.cleanup(&shared.lock().protected_media());
        assert_eq!(std::fs::read_dir(&cache.root).unwrap().count(), 2);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn private_native_replay_restores_lazy_image_bytes_video_url_and_keeps_deleted_row_asset() {
        use crate::mac_services::clipboard_fixtures::Board;
        let dir = TempDir::new(); let cache = dir.cache(); let mut history = dir.history(); let board = Board::new();
        let image = cache.import_image(b"original image", "png").unwrap(); history.push_media(image.clone(), "10:00");
        assert!(board.copy_media(&image.path, true));
        let image_urls = board.values(maccy::FILE_URL); assert_eq!(image_urls.len(), 1, "image bytes and Finder URL share one item");
        assert_eq!(url::Url::parse(&image_urls[0]).unwrap().to_file_path().unwrap(), image.path);
        let mut change = -1; assert!(board.snapshot(&mut change, true).is_none());
        // Providers load when requested, not during the UI copy action or the history monitor's marker check.
        std::fs::write(&image.path, b"image bytes requested lazily").unwrap();
        history.remove(0);
        let active = board.replay_path().unwrap(); assert_eq!(active, image.path);
        let mut protected = history.protected_media(); protected.insert(active); cache.cleanup(&protected);
        assert_eq!(board.data("public.png"), b"image bytes requested lazily");
        assert_eq!(board.replay_path(), Some(image.path.clone()));
        let video_source = dir.0.join("original.mov"); std::fs::write(&video_source, b"video source").unwrap();
        let video = cache.import_file(&video_source, MAX_MEDIA_CACHE_BYTES).unwrap(); std::fs::remove_file(video_source).unwrap();
        assert!(board.copy_media(&video.path, false)); assert!(board.snapshot(&mut change, true).is_none());
        let url = String::from_utf8(board.data("public.file-url")).unwrap();
        assert_eq!(url::Url::parse(&url).unwrap().to_file_path().unwrap(), video.path);
        let active = board.replay_path().unwrap();
        cache.cleanup(&HashSet::from([active])); assert!(video.path.exists()); assert!(!image.path.exists());
        board.write(1, b"another external copy", None); assert!(board.replay_path().is_none());
        cache.cleanup(&HashSet::new()); assert!(!video.path.exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn maccy_multi_file_replay_preserves_order_and_leases_every_cached_file() {
        use crate::mac_services::clipboard_fixtures::Board;
        let dir = TempDir::new(); let cache = dir.cache(); let mut history = dir.history(); let board = Board::new();
        let first = cache.import_image(b"first file", "png").unwrap();
        let source = dir.0.join("second.mov"); std::fs::write(&source, b"second video").unwrap();
        let second = cache.import_file(&source, MAX_MEDIA_CACHE_BYTES).unwrap();
        history.push_media(first.clone(), "10:00"); history.push_media(second.clone(), "10:01");
        let paths = vec![first.path.clone(), second.path.clone()]; assert!(board.copy_files(&paths));
        assert_eq!(board.values(maccy::FILE_URL).iter().map(|url| url::Url::parse(url).unwrap().to_file_path().unwrap()).collect::<Vec<_>>(), paths);
        assert_eq!(board.values(maccy::HISTORY).len(), 2, "every file item must carry the suppression marker");
        assert_eq!(board.replay_paths(), paths);
        assert!(board.snapshot(&mut -1, true).is_none());
        history.clear_recent(); cache.cleanup(&board.replay_paths().into_iter().collect());
        assert!(paths.iter().all(|path| path.exists()));
        board.write(1, b"external replacement", None); assert!(board.replay_paths().is_empty());
        cache.cleanup(&history.protected_media()); assert!(paths.iter().all(|path| !path.exists())); assert!(source.exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn overlapping_watchers_cannot_prune_between_import_and_history_commit() {
        let dir = TempDir::new(); let cache = dir.cache();
        let shared = Shared { history: Arc::new(Mutex::new(dir.history())), enabled: Arc::new(AtomicBool::new(true)) };
        let first = shared.clone(); let root = cache.root.clone();
        let (imported, ready) = std::sync::mpsc::channel(); let (resume, resumed) = std::sync::mpsc::channel();
        let importer = std::thread::spawn(move || {
            let _work = media_work_lock();
            let cache = MediaCache::open(root).unwrap(); let media = cache.import_image(b"not committed yet", "png").unwrap();
            imported.send(media.path.clone()).unwrap(); resumed.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
            assert!(first.record_media(media));
        });
        let path = ready.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let second = shared.clone(); let root = cache.root.clone();
        let (started, began) = std::sync::mpsc::channel(); let (finished, done) = std::sync::mpsc::channel();
        let cleaner = std::thread::spawn(move || {
            started.send(()).unwrap();
            maintain_cache_with(&second, &MediaCache::open(root).unwrap(), || crate::mac_services::ReplayPath::Empty);
            finished.send(()).unwrap();
        });
        began.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(matches!(done.recv_timeout(std::time::Duration::from_millis(100)), Err(std::sync::mpsc::RecvTimeoutError::Timeout)), "new watcher cleanup must wait for the old import+commit");
        resume.send(()).unwrap(); importer.join().unwrap();
        done.recv_timeout(std::time::Duration::from_secs(5)).unwrap(); cleaner.join().unwrap();
        assert!(path.exists()); assert_eq!(shared.lock().items[0].media.as_ref().unwrap().path, path);
    }

    #[test]
    fn cancelled_watcher_import_is_cleaned_and_cannot_commit_after_reenable() {
        let dir = TempDir::new(); let cache = dir.cache(); let cancelled = AtomicBool::new(false);
        struct CancelReader<'a> { cancelled: &'a AtomicBool }
        impl Read for CancelReader<'_> {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                bytes[..4].copy_from_slice(b"part"); self.cancelled.store(true, Ordering::Release); Ok(4)
            }
        }
        let mut reader = CancelReader { cancelled: &cancelled };
        let result = cache.import_reader_until(&mut reader, MediaKind::Image, "png", "cancelled.png".into(), 1024, Some(&cancelled));
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert!(std::fs::read_dir(&cache.root).unwrap().next().is_none());
        let shared = Shared { history: Arc::new(Mutex::new(dir.history())), enabled: Arc::new(AtomicBool::new(false)) };
        shared.set_enabled(true);
        assert!(!shared.record_until("old generation's text", Some(&cancelled)));
        let media = cache.import_image(b"old completed import", "png").unwrap();
        assert!(!shared.record_media_until(media, Some(&cancelled)));
        cache.cleanup(&shared.lock().protected_media());
        assert!(shared.lock().items.is_empty()); assert!(std::fs::read_dir(&cache.root).unwrap().next().is_none());
    }
}
