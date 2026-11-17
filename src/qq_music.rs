//! QQ Music's official website, hosted by WebKit. Account data stays in the
//! website's WebKit data store; the media bridge reads public playback APIs only.
use crate::smtc::{NowPlaying, Transport};

pub const SOURCE: &str = "com.qingyu.qqmusic.web";
#[cfg(any(target_os="macos",test))]
#[path="qq_music_session.rs"]
mod session_store;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Library,
    Player,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionStatus {
    #[default]
    Unknown,
    Restoring,
    Connected,
    Expired,
}

#[derive(Clone, Default)]
pub struct Library {
    pub loading: bool,
    pub logged_in: bool,
    pub playlists: Vec<Playlist>,
    pub message: String,
    pub session: SessionStatus,
    pub resume: Option<Resume>,
    pub resuming: bool,
    pub playback_error: String,
    pub player_ready: bool,
    /// One fresh media/lyric pair from the same selected document. Reading this
    /// cache never evaluates WebKit or fetches artwork.
    pub live: Option<NowPlaying>,
    pub live_lyrics: Option<NativeLyrics>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resume {
    pub title:String,
    pub artist:String,
    pub album:String,
    pub position:f64,
    pub duration:f64,
    pub playlist_title:String,
}
#[derive(Clone)]
pub struct Playlist { pub id: String, pub title: String }
#[derive(Clone, Default, PartialEq)]
pub struct NativeLyrics { pub title: String, pub lines: Vec<String>, pub current: Option<usize> }

pub fn library() -> Library {
    #[cfg(target_os="macos")] { bridge::library() }
    #[cfg(not(target_os="macos"))] { Library {message:"QQ 音乐内置网页目前仅支持 macOS".into(),..Default::default()} }
}
pub fn lyrics() -> Option<NativeLyrics> {
    #[cfg(target_os="macos")] { bridge::lyrics() }
    #[cfg(not(target_os="macos"))] { None }
}
pub fn refresh_library() -> Result<(),String> {
    #[cfg(target_os="macos")] {
        let result=initialize(2);
        if let Err(error)=&result{bridge::restoration_failed(format!("暂时无法恢复 QQ 音乐连接：{error}"));}
        result
    }
    #[cfg(not(target_os="macos"))] { Err("QQ 音乐内置网页目前仅支持 macOS".into()) }
}
pub fn play_playlist(id: &str) -> Result<(),String> {
    #[cfg(target_os="macos")] { bridge::play_playlist(id) }
    #[cfg(not(target_os="macos"))] { let _=id; Err("QQ 音乐内置网页目前仅支持 macOS".into()) }
}
/// Restore remembered QQ playback only after an explicit user action.
pub fn resume() -> Result<(),String> {
    #[cfg(target_os="macos")] { bridge::resume() }
    #[cfg(not(target_os="macos"))] { Err("QQ 音乐内置网页目前仅支持 macOS".into()) }
}
/// Schedule an absolute playback state from the AppKit thread. The returned
/// receipt confirms the actual website state; the main thread never waits.
pub fn set_playing(playing:bool) -> Result<std::sync::mpsc::Receiver<bool>,String> {
    #[cfg(target_os="macos")] { bridge::set_playing(playing) }
    #[cfg(not(target_os="macos"))] { let _=playing; Err("QQ 音乐内置网页目前仅支持 macOS".into()) }
}
/// Optional UI wakeup for media snapshots and asynchronous playback receipts.
pub fn set_repaint_context(context:eframe::egui::Context) {
    #[cfg(target_os="macos")] { bridge::set_repaint_context(context); }
    #[cfg(not(target_os="macos"))] { let _=context; }
}

/// Called on the AppKit event-loop thread. Opening a window never signs in or
/// starts a track; both actions belong to the official website and its user.
pub fn open(page: Page) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    { initialize(i32::from(page == Page::Player)) }
    #[cfg(not(target_os = "macos"))]
    { let _=page; Err("QQ 音乐内置网页目前仅支持 macOS".into()) }
}
#[cfg(target_os="macos")]
fn initialize(page: i32) -> Result<(),String> {
    {
        use std::ffi::CString;
        let directory = crate::model::config_dir();
        std::fs::create_dir_all(&directory).map_err(|e| format!("无法建立 QQ 音乐数据目录：{e}"))?;
        let directory = directory.canonicalize().map_err(|e| format!("无法读取 QQ 音乐数据目录：{e}"))?;
        bridge::configure_profile(&directory);
        if page==2{bridge::begin_library_refresh();}
        let identifier = profile_identifier(&directory);
        let identifier = CString::new(identifier).map_err(|_| "QQ 音乐数据标识无效")?;
        let script = CString::new(include_str!("macos/qq_music.js")).map_err(|_| "QQ 音乐播放桥接脚本无效")?;
        let isolated = ["QINGYU_DATA_DIR", "DAOZAI_DATA_DIR"].iter()
            .any(|key| std::env::var_os(key).is_some_and(|v| !v.is_empty()));
        let result = unsafe { native::qy_qq_open(page, identifier.as_ptr(), isolated, script.as_ptr()) };
        match result {
            1 => Ok(()),
            -1 => Err("请在轻屿主线程打开 QQ 音乐".into()),
            -2 => Err("QQ 音乐窗口已关闭，请重新启动轻屿".into()),
            _ => Err("无法创建 QQ 音乐窗口，请重试".into()),
        }
    }
}

/// Call on the AppKit thread before destroying the app shell. Hidden web
/// windows deliberately remain alive until this point to preserve playback.
pub fn shutdown() {
    #[cfg(target_os = "macos")]
    unsafe { native::qy_qq_shutdown() }
}

#[cfg(any(target_os = "macos", test))]
fn profile_identifier(directory: &std::path::Path) -> String {
    let hash = ring::digest::digest(&ring::digest::SHA256, directory.as_os_str().as_encoded_bytes());
    let mut bytes: [u8; 16] = hash.as_ref()[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!("{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],bytes[1],bytes[2],bytes[3],bytes[4],bytes[5],bytes[6],bytes[7],
        bytes[8],bytes[9],bytes[10],bytes[11],bytes[12],bytes[13],bytes[14],bytes[15])
}

#[cfg(target_os = "macos")]
pub(crate) fn query() -> Option<NowPlaying> { bridge::query() }
#[cfg(not(target_os = "macos"))]
pub(crate) fn query() -> Option<NowPlaying> { None }

#[cfg(target_os = "macos")]
pub(crate) fn control(command: Transport) -> bool { bridge::control(command) }
#[cfg(not(target_os = "macos"))]
pub(crate) fn control(_command: Transport) -> bool { false }

#[cfg(target_os = "macos")]
pub(crate) fn volume() -> Option<(u32, bool)> { bridge::volume() }

#[cfg(target_os = "macos")]
mod native {
    unsafe extern "C" {
        pub(super) fn qy_qq_open(page: i32, identifier: *const std::ffi::c_char, isolated: bool, script: *const std::ffi::c_char) -> i32;
        pub(super) fn qy_qq_shutdown();
        pub(super) fn qy_qq_control(view: u64, epoch: u64, operation: i32, value: f64) -> bool;
        pub(super) fn qy_qq_play_playlist(url: *const std::ffi::c_char) -> bool;
        pub(super) fn qy_qq_set_playing(view:u64,epoch:u64,document:*const std::ffi::c_char,track:*const std::ffi::c_char,playing:bool,request:u64)->bool;
        pub(super) fn qy_qq_start(value:*const std::ffi::c_char,request:u64)->bool;
    }
}

#[cfg(any(target_os = "macos", test))]
mod bridge {
    use super::*;
    use serde::Deserialize;
    use std::{collections::BTreeMap, sync::Mutex, time::{Duration, Instant}};
    const MAX_VIEWS: usize = 4;
    const MAX_MESSAGE: usize = 96 * 1024;
    const FRESH_FOR: Duration = Duration::from_secs(8);

    #[derive(Clone, Deserialize)]
    struct Sample {
        origin: String,
        title: String,
        #[serde(default)] artist: String,
        #[serde(default)] album: String,
        playing: bool,
        position: f64,
        duration: f64,
        volume: Option<f64>,
        muted: bool,
        #[serde(default)] artwork: String,
        #[serde(default)] track_key: String,
        #[serde(default)] track_url: String,
    }
    #[derive(Deserialize)]
    struct RawPlaylist { id:String, title:String, url:String }
    #[derive(Deserialize)]
    struct RawLibrary {
        logged_in:bool,
        #[serde(default)] loading:bool,
        section:String,
        playlists:Vec<RawPlaylist>,
        #[serde(default)] message:String,
        #[serde(default)] auth:String,
    }
    #[derive(Deserialize)]
    struct RawLyrics { title:String, track_key:String, lines:Vec<String>, current:Option<usize> }
    #[derive(Deserialize)]
    struct Projection { origin:String, document:String, #[serde(default)] sequence:u64, media:Option<Sample>, library:Option<RawLibrary>, lyrics:Option<RawLyrics> }

    fn trusted_origin(origin: &str) -> bool {
        url::Url::parse(origin).is_ok_and(|url| url.scheme() == "https" && url.host_str() == Some("y.qq.com")
            && url.port_or_known_default() == Some(443) && url.username().is_empty() && url.password().is_none()
            && url.path() == "/" && url.query().is_none() && url.fragment().is_none())
    }

    fn parse(bytes: &[u8]) -> Option<Sample> {
        if bytes.len() > MAX_MESSAGE { return None; }
        validate_sample(serde_json::from_slice(bytes).ok()?)
    }
    fn validate_sample(mut sample:Sample) -> Option<Sample> {
        if !trusted_origin(&sample.origin) || sample.title.trim().is_empty()
            || [&sample.title, &sample.artist, &sample.album].iter().any(|text| text.len() > 4096)
            || !sample.position.is_finite() || !sample.duration.is_finite() || sample.volume.is_some_and(|volume|!volume.is_finite())
            || sample.position < 0.0 || sample.duration < 0.0 || sample.duration > 7.0 * 86400.0
            || sample.volume.is_some_and(|volume|!(0.0..=1.0).contains(&volume)) || sample.track_key.len()>4096 { return None; }
        if sample.track_key.is_empty() {sample.track_key=format!("{}\u{1f}{}\u{1f}{}",sample.title,sample.artist,sample.album);}
        sample.track_url=session_store::track_url(&sample.track_url).unwrap_or_default();
        sample.position = if sample.duration > 0.0 { sample.position.min(sample.duration) } else { sample.position.min(7.0 * 86400.0) };
        if artwork_url(&sample.artwork).is_none() { sample.artwork.clear(); }
        Some(sample)
    }

    fn artwork_url(value: &str) -> Option<url::Url> {
        if value.len() > 2048 { return None; }
        let url = url::Url::parse(value).ok()?;
        (url.scheme() == "https" && matches!(url.host_str()?, "y.gtimg.cn" | "y.qq.com" | "qpic.y.qq.com" | "p.qpic.cn")
            && url.port_or_known_default() == Some(443) && url.username().is_empty() && url.password().is_none()).then_some(url)
    }

    fn playlist_url(id:&str,value:&str)->Option<String> {
        session_store::playlist_url(id,value)
    }
    struct Slot { epoch: u64, sample: Option<Sample>, at: Instant, activity: u64, document:String, sequence:u64, lyrics:Option<NativeLyrics>, hydrating:bool }
    #[derive(Default)]
    struct State {
        slots: BTreeMap<u64, Slot>, selected: Option<u64>, activity: u64,
        library:Library, sections:BTreeMap<String,BTreeMap<String,(String,String)>>,
        library_at:Option<Instant>, library_view:Option<u64>, refresh_at:Option<Instant>,
        profile:Option<(std::path::PathBuf,String)>, remembered:bool, connected_at_ms:u64,
        persisted:Option<session_store::Cache>,
        last_playlist:Option<session_store::Playlist>, playback:Option<session_store::Playback>,
        progress_saved_at:Option<Instant>, last_playing:Option<bool>, start_request:Option<(u64,Instant)>,
        pending_playlist:Option<session_store::Playlist>,
    }
    impl State {
        fn load_profile(directory:&std::path::Path)->Self {
            let profile=profile_identifier(directory);
            let mut state=Self{profile:Some((directory.into(),profile.clone())),..Default::default()};
            if let Some(cache)=session_store::Cache::load(directory,&profile,session_store::now_ms()) {
                state.remembered=cache.connected_at_ms>0 && !cache.expired;state.connected_at_ms=cache.connected_at_ms;
                state.last_playlist=cache.last_playlist.clone();state.playback=cache.playback.clone();
                for(section,playlists)in &cache.sections {
                    state.sections.insert(section.clone(),playlists.iter().map(|p|(p.id.clone(),(p.title.clone(),p.url.clone()))).collect());
                }
                state.library.session=if cache.expired{SessionStatus::Expired}else{SessionStatus::Unknown};
                state.library.message=if cache.expired{"QQ 音乐官方登录已失效，请重新登录".into()}
                    else if state.remembered{"已保留上次连接和歌单，打开歌单时恢复连接".into()}else{String::new()};
                state.rebuild_playlists();state.rebuild_resume();state.persisted=Some(cache);
            }
            state
        }
        fn rebuild_playlists(&mut self) {
            let mut all=BTreeMap::new();for section in self.sections.values(){all.extend(section.clone());}
            self.library.playlists=all.into_iter().map(|(id,(title,_))|Playlist{id,title}).collect();
        }
        fn remembered_playlist(&self)->Option<session_store::Playlist> {
            self.last_playlist.clone().or_else(||self.sections.values().find_map(|section|section.get("favorites")
                .map(|(title,url)|session_store::Playlist{id:"favorites".into(),title:title.clone(),url:url.clone()})))
                .or_else(||self.sections.values().find_map(|section|section.iter().next()
                    .map(|(id,(title,url))|session_store::Playlist{id:id.clone(),title:title.clone(),url:url.clone()})))
        }
        fn rebuild_resume(&mut self) {
            self.library.resume=self.playback.as_ref().map(|playback|Resume{title:playback.title.clone(),artist:playback.artist.clone(),album:playback.album.clone(),
                position:playback.position,duration:playback.duration,playlist_title:playback.playlist.as_ref().map_or_else(String::new,|p|p.title.clone())})
                .or_else(||self.remembered_playlist().map(|playlist|Resume{playlist_title:playlist.title,..Default::default()}));
        }
        fn persist_session(&mut self) {
            let Some((directory,profile))=&self.profile else{return;};
            let mut cache=session_store::Cache::empty(profile);
            cache.connected_at_ms=self.connected_at_ms;cache.expired=self.library.session==SessionStatus::Expired;
            if !cache.expired {
                cache.sections=self.sections.iter().map(|(section,playlists)|(section.clone(),playlists.iter()
                    .map(|(id,(title,url))|session_store::Playlist{id:id.clone(),title:title.clone(),url:url.clone()}).collect())).collect();
                cache.playback=self.playback.clone();cache.last_playlist=self.last_playlist.clone();
            }
            if self.persisted.as_ref()!=Some(&cache) && cache.save(directory,session_store::now_ms()).is_ok(){self.persisted=Some(cache);}
        }
        fn begin_restore(&mut self,now:Instant) {
            self.refresh_at=Some(now);self.library.loading=true;
            if self.library.session!=SessionStatus::Connected {self.library.session=SessionStatus::Restoring;self.library.logged_in=false;}
            self.library.message="正在恢复 QQ 音乐连接和歌单…".into();
        }
        fn restoration_unconfirmed(&mut self,message:String) {
            self.refresh_at=None;self.library.loading=false;self.library.logged_in=false;
            if self.library.session!=SessionStatus::Expired {self.library.session=SessionStatus::Unknown;}
            self.library.message=message;
        }
        fn stop_engine(&mut self) {
            self.persist_session();self.library.resuming=false;self.start_request=None;self.pending_playlist=None;self.library.playback_error.clear();
            self.slots.clear();self.selected=None;self.activity=0;self.library_view=None;self.refresh_at=None;
            self.library.loading=false;self.library.logged_in=false;self.library_at=None;
            self.library.live=None;self.library.live_lyrics=None;self.library.player_ready=false;
            if self.library.session!=SessionStatus::Expired {
                self.library.session=SessionStatus::Unknown;
                self.library.message=if self.remembered{"已保留账号连接，打开歌单时恢复".into()}else{"打开歌单时检查 QQ 音乐连接".into()};
            }
        }
        fn reset(&mut self, view: u64, epoch: u64, now: Instant) {
            if view == 0 || epoch == 0 || (!self.slots.contains_key(&view) && self.slots.len() >= MAX_VIEWS) { return; }
            if self.slots.get(&view).is_some_and(|slot| epoch < slot.epoch) { return; }
            self.slots.insert(view, Slot { epoch, sample: None, at: now, activity: 0, document:String::new(),sequence:0,lyrics:None,hydrating:false });
        }
        fn remove(&mut self, view: u64) {
            self.slots.remove(&view);
            if self.selected == Some(view) { self.selected = None; }
            // Temporary profile views are released after enumeration. Their
            // validated playlist cache survives the view and expires by age;
            // an explicit official logout projection clears it. Engine shutdown
            // preserves this display cache for the next on-demand restoration.
            if self.library_view==Some(view) {self.library_view=None;}
        }
        fn update(&mut self, view: u64, epoch: u64, bytes: &[u8], now: Instant) {
            let Some(slot) = self.slots.get_mut(&view).filter(|slot| slot.epoch == epoch) else { return; };
            let sample = parse(bytes);
            if sample.as_ref().is_some_and(|new| new.playing && slot.sample.as_ref().is_none_or(|old|
                !old.playing || (old.title.as_str(),old.artist.as_str(),old.album.as_str()) != (new.title.as_str(),new.artist.as_str(),new.album.as_str()))) {
                self.activity = self.activity.saturating_add(1);
                slot.activity = self.activity;
            }
            slot.sample = sample;
            slot.at = now;
            slot.lyrics = None;
            slot.hydrating=false;
        }
        fn projection(&mut self,view:u64,epoch:u64,bytes:&[u8],now:Instant) {
            if bytes.len()>MAX_MESSAGE {return;}
            let Ok(projection)=serde_json::from_slice::<Projection>(bytes) else{return;};
            if !trusted_origin(&projection.origin) || projection.document.is_empty() || projection.document.len()>128 {return;}
            let Some(slot)=self.slots.get(&view).filter(|slot|slot.epoch==epoch) else{return;};
            if !slot.document.is_empty() && slot.document!=projection.document {return;}
            if projection.sequence>0 && projection.sequence<=slot.sequence {return;}
            let media=projection.media.and_then(validate_sample);
            let slot=self.slots.get_mut(&view).unwrap();
            if media.as_ref().is_some_and(|new|new.playing && slot.sample.as_ref().is_none_or(|old|!old.playing || old.track_key!=new.track_key)) {
                self.activity=self.activity.saturating_add(1);slot.activity=self.activity;
            }
            slot.document=projection.document;slot.sequence=projection.sequence;slot.lyrics=None;
            // A same-document hydration gap keeps the last clock for at most
            // 1.5 seconds. Missing samples never renew its freshness or lyrics.
            if let Some(media)=media {slot.sample=Some(media);slot.at=now;slot.hydrating=false;}
            else {slot.hydrating=true;}
            if let (Some(media),Some(raw))=(&slot.sample,projection.lyrics.filter(|_|!slot.hydrating)) {
                if raw.track_key==media.track_key && raw.title==media.title && raw.lines.len()<=256 && !raw.lines.is_empty()
                    && raw.lines.iter().all(|line|line.len()<=1024) && raw.lines.iter().map(String::len).sum::<usize>()<=48*1024
                    && raw.current.is_none_or(|i|i<raw.lines.len()) {
                    slot.lyrics=Some(NativeLyrics{title:raw.title,lines:raw.lines,current:raw.current});
                }
            }
            if let Some(raw)=projection.library {self.update_library(view,raw,now);}
            self.remember_playback(now);
        }
        fn remember_playback(&mut self,now:Instant) {
            let Some((_,_,sample,_))=self.selected(now) else{return;};
            if sample.track_url.is_empty(){return;}
            let changed=self.playback.as_ref().is_none_or(|playback|playback.track_url!=sample.track_url);
            // A selected playlist click may briefly leave the old song visible.
            // Keep confirmed memory unchanged until the new start is accepted.
            if self.library.resuming{return;}
            let playlist=self.last_playlist.clone().or_else(||self.playback.as_ref().and_then(|p|p.playlist.clone()));
            let playback=session_store::Playback{track_url:sample.track_url,title:sample.title,artist:sample.artist,album:sample.album,
                position:sample.position,duration:sample.duration,playlist};
            if !playback.valid(){return;}
            self.playback=Some(playback);self.rebuild_resume();
            let transition=self.last_playing!=Some(sample.playing);self.last_playing=Some(sample.playing);
            if changed || transition || self.progress_saved_at.is_none_or(|at|now.saturating_duration_since(at)>=Duration::from_secs(15)){
                self.persist_session();self.progress_saved_at=Some(now);
            }
        }
        fn update_library(&mut self,view:u64,raw:RawLibrary,now:Instant) {
            if !matches!(raw.section.as_str(),"favorites"|"created"|"collected") || raw.playlists.len()>120 || raw.message.len()>1024 {return;}
            if !raw.logged_in {
                if raw.auth=="expired" && !raw.loading {
                    self.sections.clear();self.remembered=false;self.connected_at_ms=0;self.library_at=None;self.refresh_at=None;
                    self.last_playlist=None;self.playback=None;self.start_request=None;
                    self.library=Library{session:SessionStatus::Expired,message:"QQ 音乐官方页面要求重新登录".into(),..Default::default()};
                    self.library_view=Some(view);self.persist_session();
                }else if self.library.session!=SessionStatus::Connected {
                    self.library.logged_in=false;self.library.loading=self.refresh_at.is_some()||raw.loading;
                    self.library.session=if self.library.loading{SessionStatus::Restoring}else{SessionStatus::Unknown};
                    if !raw.message.is_empty(){self.library.message=raw.message;}
                }
                return;
            }
            if self.library.session!=SessionStatus::Connected {self.connected_at_ms=session_store::now_ms();}
            self.remembered=true;
            let mut playlists=BTreeMap::new();
            for playlist in raw.playlists {
                if playlist.title.trim().is_empty() || playlist.title.len()>512 {continue;}
                if let Some(url)=playlist_url(&playlist.id,&playlist.url) {playlists.insert(playlist.id,(playlist.title,url));}
            }
            self.sections.insert(raw.section,playlists);
            self.library.logged_in=true;self.library.session=SessionStatus::Connected;self.library.loading=self.refresh_at.is_some()||raw.loading;
            self.library.message=raw.message;self.rebuild_playlists();self.rebuild_resume();self.persist_session();
            self.library_at=Some(now);self.library_view=Some(view);
        }
        fn selected(&mut self, now: Instant) -> Option<(u64, u64, Sample, Instant)> {
            let available = |slot: &&Slot| slot.sample.is_some() && now.saturating_duration_since(slot.at) <=
                if slot.hydrating {Duration::from_millis(1500)}else{FRESH_FOR};
            let chosen = self.slots.iter().filter(|(_,slot)| available(slot)).max_by_key(|(view,slot)|
                (slot.sample.as_ref().unwrap().playing, slot.activity, Some(**view) == self.selected, **view)).map(|(view,_)|*view)?;
            self.selected = Some(chosen);
            let slot = &self.slots[&chosen];
            Some((chosen, slot.epoch, slot.sample.as_ref()?.clone(), slot.at))
        }
        fn live_pair(&mut self,now:Instant)->(Option<NowPlaying>,Option<NativeLyrics>){
            let Some((view,_,sample,at))=self.selected(now)else{return(None,None);};
            let lyrics=self.slots.get(&view).and_then(|slot|slot.lyrics.clone());
            (Some(now_playing(sample,at,None)),lyrics)
        }
    }

    #[cfg(target_os = "macos")]
    static STATE: std::sync::OnceLock<Mutex<State>> = std::sync::OnceLock::new();
    #[cfg(target_os = "macos")]
    fn state() -> std::sync::MutexGuard<'static, State> {
        STATE.get_or_init(||{
            let state=crate::model::config_dir().canonicalize().ok().map_or_else(State::default,|directory|State::load_profile(&directory));
            Mutex::new(state)
        }).lock().unwrap_or_else(|e| e.into_inner())
    }
    #[cfg(target_os="macos")]
    pub(super) fn configure_profile(directory:&std::path::Path) {
        let mut state=state();
        if state.profile.as_ref().is_none_or(|(current,_)|current!=directory){*state=State::load_profile(directory);}
    }

    // These callbacks run on AppKit's thread. They never fetch artwork or wait
    // for a media worker, and accept only a bounded, registered document epoch.
    #[cfg(target_os = "macos")]
    #[no_mangle]
    extern "C" fn qy_qq_media_reset(view: u64, epoch: u64) { state().reset(view, epoch, Instant::now()); }
    #[cfg(target_os = "macos")]
    #[no_mangle]
    extern "C" fn qy_qq_media_remove(view: u64) { state().remove(view); }
    #[cfg(target_os="macos")]
    #[no_mangle]
    extern "C" fn qy_qq_cache_clear() { state().stop_engine(); receipts().cancel(); wake(); }
    #[cfg(target_os = "macos")]
    #[no_mangle]
    unsafe extern "C" fn qy_qq_media_update(view: u64, epoch: u64, bytes: *const u8, length: usize) {
        if bytes.is_null() || length > MAX_MESSAGE { return; }
        state().update(view, epoch, std::slice::from_raw_parts(bytes, length), Instant::now());
    }
    #[cfg(target_os="macos")]
    #[no_mangle]
    unsafe extern "C" fn qy_qq_projection_update(view:u64,epoch:u64,bytes:*const u8,length:usize) {
        if bytes.is_null() || length>MAX_MESSAGE {return;}
        let changed={
            let mut state=state();
            let status=|slot:&Slot|(slot.sample.as_ref().map(|media|(media.track_key.clone(),media.title.clone(),media.playing)),
                slot.lyrics.as_ref().map(|lyrics|(lyrics.current,lyrics.lines.len())));
            let before=state.slots.get(&view).map(status);
            state.projection(view,epoch,std::slice::from_raw_parts(bytes,length),Instant::now());
            let after=state.slots.get(&view).map(status);
            before!=after
        };
        if changed {wake();}
    }
    #[derive(Default)]
    struct Receipts { newest:Option<(u64,std::sync::mpsc::SyncSender<bool>)> }
    impl Receipts {
        fn replace(&mut self,id:u64)->std::sync::mpsc::Receiver<bool> {
            self.cancel();let (sender,receiver)=std::sync::mpsc::sync_channel(1);self.newest=Some((id,sender));receiver
        }
        fn complete(&mut self,id:u64,success:bool) {
            if self.newest.as_ref().is_some_and(|(current,_)|*current==id) {
                if let Some((_,sender))=self.newest.take(){let _=sender.try_send(success);}
            }
        }
        fn cancel(&mut self){if let Some((_,sender))=self.newest.take(){let _=sender.try_send(false);}}
    }
    #[cfg(target_os="macos")]
    static RECEIPTS:std::sync::OnceLock<Mutex<Receipts>>=std::sync::OnceLock::new();
    #[cfg(target_os="macos")]
    fn receipts()->std::sync::MutexGuard<'static,Receipts>{RECEIPTS.get_or_init(Mutex::default).lock().unwrap_or_else(|e|e.into_inner())}
    #[cfg(target_os="macos")]
    static REPAINT:std::sync::OnceLock<Mutex<Option<eframe::egui::Context>>>=std::sync::OnceLock::new();
    #[cfg(target_os="macos")]
    pub(super) fn set_repaint_context(context:eframe::egui::Context){*REPAINT.get_or_init(Mutex::default).lock().unwrap_or_else(|e|e.into_inner())=Some(context);}
    #[cfg(target_os="macos")]
    fn wake(){if let Some(context)=REPAINT.get().and_then(|mutex|mutex.lock().ok().and_then(|value|value.clone())){context.request_repaint();}}
    #[cfg(target_os="macos")]
    #[no_mangle]
    extern "C" fn qy_qq_playing_result(request:u64,success:bool){receipts().complete(request,success);wake();}
    #[cfg(target_os="macos")]
    pub(super) fn set_playing(playing:bool)->Result<std::sync::mpsc::Receiver<bool>,String>{
        let (view,epoch,document,track)={
            let mut state=state();let (view,epoch,media,_)=state.selected(Instant::now()).ok_or("QQ 音乐播放器尚未就绪")?;
            (view,epoch,state.slots[&view].document.clone(),media.track_key)
        };
        let document=std::ffi::CString::new(document).map_err(|_|"QQ 音乐页面标识无效")?;
        let track=std::ffi::CString::new(track).map_err(|_|"QQ 音乐歌曲标识无效")?;
        static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(1);
        let request=NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
        let receiver=receipts().replace(request);
        if unsafe{native::qy_qq_set_playing(view,epoch,document.as_ptr(),track.as_ptr(),playing,request)} {Ok(receiver)}
        else{receipts().complete(request,false);Err("无法提交 QQ 音乐播放控制，请重试".into())}
    }
    #[cfg(target_os="macos")]
    #[no_mangle]
    unsafe extern "C" fn qy_qq_library_status(message:*const std::ffi::c_char) {
        let mut state=state();state.refresh_at=None;state.library.loading=false;
        if !message.is_null() {let message=std::ffi::CStr::from_ptr(message).to_string_lossy(); if message.len()<=1024 {state.library.message=message.into_owned();}}
    }
    #[cfg(target_os="macos")]
    #[no_mangle]
    unsafe extern "C" fn qy_qq_restoration_failed(message:*const std::ffi::c_char){
        let message=if message.is_null(){"暂时无法确认 QQ 音乐连接，请重试".into()}else{std::ffi::CStr::from_ptr(message).to_string_lossy().chars().take(300).collect()};
        restoration_failed(message);
    }
    #[cfg(target_os="macos")]
    pub(super) fn restoration_failed(message:String){state().restoration_unconfirmed(message);wake();}
    #[cfg(target_os="macos")]
    pub(super) fn library()->Library {
        let mut state=state();
        if state.refresh_at.is_some_and(|at|at.elapsed()>Duration::from_secs(25)) {state.restoration_unconfirmed("恢复 QQ 音乐连接超时，账号记录已保留，请重试".into());}
        if state.start_request.is_some_and(|(_,at)|at.elapsed()>Duration::from_secs(24)) {
            state.start_request=None;state.pending_playlist=None;state.library.resuming=false;state.library.playback_error="QQ 音乐未能恢复播放，请重试".into();
        }
        let (live,lyrics)=state.live_pair(Instant::now());
        state.library.player_ready=live.is_some();state.library.live=live;state.library.live_lyrics=lyrics;
        state.library.clone()
    }
    #[cfg(target_os="macos")]
    pub(super) fn begin_library_refresh() {state().begin_restore(Instant::now());}
    #[cfg(target_os="macos")]
    pub(super) fn lyrics()->Option<NativeLyrics> {
        let mut state=state();let (view,_,_,_)=state.selected(Instant::now())?;state.slots.get(&view)?.lyrics.clone()
    }
    #[cfg(target_os="macos")]
    pub(super) fn play_playlist(id:&str)->Result<(),String> {
        let playlist={let state=state();
            if state.library.session==SessionStatus::Expired{return Err("QQ 音乐官方登录已失效，请重新登录".into());}
            state.sections.values().find_map(|section|section.get(id).map(|(title,url)|session_store::Playlist{id:id.into(),title:title.clone(),url:url.clone()}))
                .ok_or("这张歌单已不在当前列表中，请刷新后重试")?
        };
        start(None,Some(playlist))
    }
    #[cfg(target_os="macos")]
    pub(super) fn resume()->Result<(),String> {
        let (playback,playlist)={let state=state();
            if state.library.session==SessionStatus::Expired{return Err("QQ 音乐官方登录已失效，请重新登录".into());}
            (state.playback.clone(),state.remembered_playlist())
        };
        if playback.is_none() && playlist.is_none(){return Err("请先从我的歌单选择音乐".into());}
        let playlist=playback.as_ref().and_then(|p|p.playlist.clone()).or(playlist);
        start(playback,playlist)
    }
    #[cfg(target_os="macos")]
    fn start(playback:Option<session_store::Playback>,playlist:Option<session_store::Playlist>)->Result<(),String> {
        // Creating the coordinator alone makes no WebView and performs no
        // navigation. A validated, explicit user action supplies the target.
        initialize(3)?;
        static NEXT_START:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(1);
        let request=NEXT_START.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
        let descriptor=serde_json::json!({"track_url":playback.as_ref().map_or("",|p|p.track_url.as_str()),
            "position":playback.as_ref().map_or(0.0,|p|p.position),"duration":playback.as_ref().map_or(0.0,|p|p.duration),
            "playlist_url":playlist.as_ref().map_or("",|p|p.url.as_str())});
        let value=std::ffi::CString::new(descriptor.to_string()).map_err(|_|"播放记录无效")?;
        {let mut state=state();state.start_request=Some((request,Instant::now()));state.pending_playlist=playlist;
            state.library.resuming=true;state.library.playback_error.clear();}
        if unsafe{native::qy_qq_start(value.as_ptr(),request)}{Ok(())}else{
            let mut state=state();state.start_request=None;state.pending_playlist=None;state.library.resuming=false;
            state.library.playback_error="无法恢复 QQ 音乐，请重试".into();Err(state.library.playback_error.clone())
        }
    }
    #[cfg(target_os="macos")]
    #[no_mangle]
    unsafe extern "C" fn qy_qq_start_result(request:u64,success:bool,seek_status:i32,message:*const std::ffi::c_char){
        let mut state=state();
        if !state.start_request.is_some_and(|(current,_)|current==request){return;}
        if !success && seek_status==-2 {
            state.update_library(0,RawLibrary{logged_in:false,loading:false,section:"favorites".into(),playlists:Vec::new(),message:String::new(),auth:"expired".into()},Instant::now());
            drop(state);wake();return;
        }
        state.start_request=None;state.library.resuming=false;
        if success{
            if let Some(playlist)=state.pending_playlist.take(){state.last_playlist=Some(playlist);}
            state.library.playback_error.clear();state.remember_playback(Instant::now());state.persist_session();state.rebuild_resume();
            state.library.message=if seek_status<0{"已恢复歌曲，QQ 网页未能恢复到上次进度".into()}else{String::new()};
        }else{
            state.pending_playlist=None;
            state.library.playback_error=if message.is_null(){"QQ 音乐未能恢复播放，请重试".into()}
                else{std::ffi::CStr::from_ptr(message).to_string_lossy().chars().take(300).collect()};
        }
        drop(state);wake();
    }

    #[cfg(target_os = "macos")]
    #[derive(Default)]
    struct Artwork {
        key: String,
        image: Option<std::sync::Arc<eframe::egui::ColorImage>>,
        tried: Option<Instant>,
        pending: Option<std::sync::mpsc::Receiver<(String, Option<eframe::egui::ColorImage>)>>,
    }
    #[cfg(target_os = "macos")]
    impl Artwork {
        fn get(&mut self, key: &str) -> Option<std::sync::Arc<eframe::egui::ColorImage>> {
            if self.key != key { self.key = key.into(); self.image = None; self.tried = None; }
            if let Some(receiver) = &self.pending {
                match receiver.try_recv() {
                    Ok((loaded,image)) => {
                        if loaded == self.key { self.image = image.map(std::sync::Arc::new); }
                        self.pending = None;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => { self.pending = None; }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }
            if !key.is_empty() && self.pending.is_none() && self.image.is_none() && self.tried.is_none_or(|at| at.elapsed() >= Duration::from_secs(30)) {
                self.tried = Some(Instant::now());
                let (sender,receiver) = std::sync::mpsc::sync_channel(1);
                let key = key.to_owned();
                if crate::resources::COVER.spawn(move || { let image=load_artwork(&key); let _=sender.send((key,image)); }).is_some() {
                    self.pending = Some(receiver);
                }
            }
            self.image.clone()
        }
    }
    #[cfg(target_os = "macos")]
    fn load_artwork(value: &str) -> Option<eframe::egui::ColorImage> {
        use std::io::Read;
        let url = artwork_url(value)?;
        let response = ureq::AgentBuilder::new().timeout(Duration::from_secs(2)).redirects(0).build().get(url.as_str()).call().ok()?;
        let mut bytes = Vec::new();
        response.into_reader().take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 2 * 1024 * 1024 { return None; }
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096); limits.max_image_height = Some(4096); limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().ok()?.thumbnail(256,256).to_rgba8();
        Some(eframe::egui::ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize], image.as_raw()))
    }

    #[cfg(target_os = "macos")]
    pub(super) fn query() -> Option<NowPlaying> {
        let (view, epoch, sample, _) = state().selected(Instant::now())?;
        static ARTWORK: std::sync::OnceLock<Mutex<Artwork>> = std::sync::OnceLock::new();
        let artwork = ARTWORK.get_or_init(Mutex::default).lock().unwrap_or_else(|e|e.into_inner()).get(&sample.artwork);
        // Artwork only schedules bounded background work. Re-check the document
        // so a navigation or new track cannot inherit the old cover.
        let (current_view, current_epoch, current, current_at) = state().selected(Instant::now())?;
        if (view,epoch,sample.title.as_str(),sample.artist.as_str(),sample.artwork.as_str()) !=
            (current_view,current_epoch,current.title.as_str(),current.artist.as_str(),current.artwork.as_str()) {
            return Some(now_playing(current,current_at,None));
        }
        Some(now_playing(current,current_at,artwork))
    }
    fn now_playing(sample: Sample, at: Instant, artwork: Option<std::sync::Arc<eframe::egui::ColorImage>>) -> NowPlaying {
        NowPlaying { source: SOURCE.into(), title: sample.title, artist: sample.artist, album: sample.album,
            playing: sample.playing, position: sample.position, duration: sample.duration, sampled_at: Some(at), artwork,
            playback_type: crate::smtc::MediaKind::Music, volume: sample.volume.map(|volume|((volume*100.0).round() as u32,sample.muted)), ..Default::default() }
    }
    #[cfg(target_os = "macos")]
    pub(super) fn volume() -> Option<(u32, bool)> {
        let (_,_,sample,_) = state().selected(Instant::now())?;
        sample.volume.map(|volume|((volume * 100.0).round() as u32, sample.muted))
    }

    fn operation(command: Transport) -> Option<(i32,f64)> {
        Some(match command {
            Transport::Toggle => (0,0.0), Transport::Next => (1,0.0), Transport::Prev => (2,0.0),
            Transport::Seek(value) if value.is_finite() && (0.0..=7.0*86400.0).contains(&value) => (3,value),
            Transport::SetVolume(value) => (4,f64::from(value.min(100))/100.0),
            Transport::SetMute(value) => (5,f64::from(u8::from(value))),
            _ => return None,
        })
    }
    #[cfg(target_os = "macos")]
    pub(super) fn control(command: Transport) -> bool {
        let Some((operation,value)) = operation(command) else { return false; };
        let Some((view,epoch,_,_)) = state().selected(Instant::now()) else { return false; };
        // Native dispatch has a deadline, one in-flight slot, and refuses to
        // synchronously wait on AppKit itself.
        unsafe { native::qy_qq_control(view,epoch,operation,value) }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        fn sample(title: &str, playing: bool) -> Vec<u8> {
            serde_json::to_vec(&serde_json::json!({"origin":"https://y.qq.com","title":title,"artist":"歌手","album":"专辑",
                "playing":playing,"position":12.0,"duration":120.0,"volume":0.5,"muted":false,"artwork":""})).unwrap()
        }
        #[test]
        fn only_registered_current_documents_can_publish_and_stale_playback_expires() {
            let now=Instant::now(); let mut state=State::default();
            state.update(1,1,&sample("旧歌曲",true),now);
            assert!(state.selected(now).is_none());
            state.reset(1,1,now); state.update(1,1,&sample("旧歌曲",true),now);
            assert!(state.selected(now).unwrap().2.playing);
            state.reset(1,2,now); state.update(1,1,&sample("旧歌曲",true),now);
            assert!(state.selected(now).is_none());
            state.update(1,2,&sample("新歌曲",false),now);
            assert_eq!(state.selected(now).unwrap().2.title,"新歌曲");
            assert!(state.selected(now+FRESH_FOR+Duration::from_millis(1)).is_none());
            state.remove(1); state.update(1,2,&sample("新歌曲",true),now);
            assert!(state.selected(now).is_none());
        }
        #[test]
        fn player_events_win_over_library_and_heartbeats_do_not_steal_playback() {
            let now=Instant::now(); let mut state=State::default();
            state.reset(1,1,now);state.reset(2,1,now);
            state.update(1,1,&sample("浏览页",false),now);
            state.update(2,1,&sample("播放器",true),now);
            assert_eq!(state.selected(now).unwrap().0,2);
            state.update(1,1,&sample("浏览页",false),now+Duration::from_secs(1));
            assert_eq!(state.selected(now+Duration::from_secs(1)).unwrap().0,2);
            state.update(2,1,&sample("播放器",false),now+Duration::from_secs(2));
            assert_eq!(state.selected(now+Duration::from_secs(2)).unwrap().0,2);
            state.update(2,1,b"null",now+Duration::from_secs(3));
            assert_eq!(state.selected(now+Duration::from_secs(3)).unwrap().0,1);
        }
        #[test]
        fn untrusted_origins_and_unbounded_media_are_rejected() {
            for origin in ["http://y.qq.com","https://y.qq.com.evil.test","https://graph.qq.com","https://y.qq.com:444","https://user@y.qq.com","https://y.qq.com/token?x=y"] {
                let mut value:serde_json::Value=serde_json::from_slice(&sample("歌曲",true)).unwrap(); value["origin"]=origin.into();
                assert!(parse(&serde_json::to_vec(&value).unwrap()).is_none(),"{origin}");
            }
            let mut value:serde_json::Value=serde_json::from_slice(&sample("歌曲",true)).unwrap();
            value["duration"]=(-1).into(); assert!(parse(&serde_json::to_vec(&value).unwrap()).is_none());
            assert!(parse(&vec![b' ';MAX_MESSAGE+1]).is_none());
            for url in ["https://y.gtimg.cn.evil.test/cover.jpg","http://y.gtimg.cn/cover.jpg","https://user@y.gtimg.cn/cover.jpg","https://y.gtimg.cn:444/cover.jpg"] {assert!(artwork_url(url).is_none());}
        }
        #[test]
        fn controls_validate_values_before_entering_appkit() {
            for value in [f64::NAN,f64::INFINITY,-1.0,7.0*86400.0+1.0] {assert!(operation(Transport::Seek(value)).is_none());}
            assert_eq!(operation(Transport::SetVolume(200)),Some((4,1.0)));
            assert_eq!(operation(Transport::SetMute(false)),Some((5,0.0)));
        }
        #[test]
        fn profiles_are_stable_and_distinct_for_data_directories() {
            let a=std::path::Path::new("/data/qingyu");let b=std::path::Path::new("/data/qingyu-dev");
            assert_eq!(profile_identifier(a),profile_identifier(a));
            assert_ne!(profile_identifier(a),profile_identifier(b));
            assert_eq!(profile_identifier(a).len(),36);
        }
        fn projection(title:&str,key:&str,document:&str)->serde_json::Value {
            let mut media:serde_json::Value=serde_json::from_slice(&sample(title,true)).unwrap();media["track_key"]=key.into();media["volume"]=serde_json::Value::Null;
            serde_json::json!({"origin":"https://y.qq.com","document":document,"media":media,"library":null,
                "lyrics":{"title":title,"track_key":key,"lines":["第一行","第二行"],"current":1}})
        }
        #[test]
        fn dom_projection_supports_no_audio_and_rejects_old_documents_and_cross_track_lyrics(){
            let now=Instant::now();let mut state=State::default();state.reset(1,1,now);
            state.projection(1,1,&serde_json::to_vec(&projection("当前歌曲","song-a","doc-a")).unwrap(),now);
            assert_eq!(state.selected(now).unwrap().2.title,"当前歌曲");assert!(state.selected(now).unwrap().2.volume.is_none());
            assert_eq!(state.slots[&1].lyrics.as_ref().unwrap().current,Some(1));
            let mut next=projection("下一首","song-b","doc-a");next["lyrics"]["track_key"]="song-a".into();
            state.projection(1,1,&serde_json::to_vec(&next).unwrap(),now);assert!(state.slots[&1].lyrics.is_none());
            state.projection(1,1,&serde_json::to_vec(&projection("旧文档","song-x","doc-old")).unwrap(),now);
            assert_eq!(state.selected(now).unwrap().2.title,"下一首");
            state.reset(1,2,now);state.projection(1,1,&serde_json::to_vec(&projection("旧文档","song-x","doc-a")).unwrap(),now);
            assert!(state.selected(now).is_none());
        }
        #[test]
        fn library_projection_is_independent_from_playback_and_bounds_playlist_identity(){
            let now=Instant::now();let mut state=State::default();state.reset(2,1,now);
            let value=serde_json::json!({"origin":"https://y.qq.com","document":"library-doc","media":null,"lyrics":null,
                "library":{"logged_in":true,"section":"collected","playlists":[
                    {"id":"favorites","title":"我喜欢","url":"https://y.qq.com/n/ryqq_v2/profile/like/song"},
                    {"id":"123","title":"我的歌单","url":"https://y.qq.com/n/ryqq_v2/playlist/123"},
                    {"id":"234","title":"外部歌单","url":"https://evil.test/n/ryqq_v2/playlist/234"},
                    {"id":"345","title":"串号歌单","url":"https://y.qq.com/n/ryqq_v2/playlist/999"}]}});
            state.projection(2,1,&serde_json::to_vec(&value).unwrap(),now);
            assert!(state.selected(now).is_none());assert!(state.library.logged_in);assert_eq!(state.library.playlists.len(),2);
            assert!(state.library.playlists.iter().any(|p|p.id=="favorites"));
            for url in ["https://user@y.qq.com/n/ryqq_v2/playlist/123","https://y.qq.com/n/ryqq_v2/playlist/123?x=y","http://y.qq.com/n/ryqq_v2/playlist/123"]{assert!(playlist_url("123",url).is_none());}
            state.remove(2);assert_eq!(state.library.playlists.len(),2);assert!(state.library.logged_in);
            state=State::default();assert!(state.library.playlists.is_empty());assert!(!state.library.logged_in);
        }
        #[test]
        fn newest_absolute_receipt_supersedes_old_work_and_shutdown_completes_pending(){
            let mut receipts=Receipts::default();let first=receipts.replace(1);let second=receipts.replace(2);let latest=receipts.replace(3);
            assert_eq!(first.try_recv(),Ok(false));assert_eq!(second.try_recv(),Ok(false));
            receipts.complete(1,true);receipts.complete(2,false);assert!(latest.try_recv().is_err());
            receipts.complete(3,true);assert_eq!(latest.try_recv(),Ok(true));assert!(receipts.newest.is_none());
            let pending=receipts.replace(4);receipts.cancel();assert_eq!(pending.try_recv(),Ok(false));
        }
        #[test]
        fn a_late_poll_cannot_undo_a_confirmed_playback_projection(){
            let now=Instant::now();let mut state=State::default();state.reset(1,1,now);
            let mut playing=projection("歌曲","track","document");playing["sequence"]=1.into();
            state.projection(1,1,&serde_json::to_vec(&playing).unwrap(),now);
            let mut paused=playing.clone();paused["sequence"]=3.into();paused["media"]["playing"]=false.into();
            state.projection(1,1,&serde_json::to_vec(&paused).unwrap(),now);
            playing["sequence"]=2.into();state.projection(1,1,&serde_json::to_vec(&playing).unwrap(),now);
            assert!(!state.selected(now).unwrap().2.playing);assert_eq!(state.slots[&1].sequence,3);
        }
        fn library_sample(logged_in:bool,auth:&str,loading:bool)->RawLibrary {
            serde_json::from_value(serde_json::json!({"logged_in":logged_in,"auth":auth,"loading":loading,"section":"collected",
                "playlists":[{"id":"123","title":"已验证收藏","url":"https://y.qq.com/n/ryqq_v2/playlist/123"}]})).unwrap()
        }
        #[test]
        fn stop_restart_and_offline_restore_keep_connection_marker_without_claiming_live_auth(){
            static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
            let directory=std::env::temp_dir().join(format!("qingyu-session-state-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
            std::fs::create_dir_all(&directory).unwrap();let now=Instant::now();
            let mut state=State::load_profile(&directory);assert_eq!(state.library.session,SessionStatus::Unknown);assert!(!state.library.logged_in);
            state.update_library(1,library_sample(true,"connected",false),now);
            assert_eq!(state.library.session,SessionStatus::Connected);assert!(state.library.logged_in);
            let before=std::fs::read(directory.join(session_store::FILE)).unwrap();
            state.stop_engine();assert_eq!(state.library.session,SessionStatus::Unknown);assert!(!state.library.logged_in);assert_eq!(state.library.playlists.len(),1);
            let mut restored=State::load_profile(&directory);assert!(restored.remembered);assert!(!restored.library.logged_in);assert_eq!(restored.library.playlists.len(),1);
            restored.begin_restore(now);assert_eq!(restored.library.session,SessionStatus::Restoring);
            restored.update_library(2,library_sample(false,"unknown",true),now+Duration::from_secs(10));
            assert_eq!(restored.library.session,SessionStatus::Restoring);assert_eq!(restored.library.playlists.len(),1);
            restored.restoration_unconfirmed("网络暂时不可用，请重试".into());
            assert_eq!(restored.library.session,SessionStatus::Unknown);assert!(restored.remembered);
            assert_eq!(std::fs::read(directory.join(session_store::FILE)).unwrap(),before);
            let mut again=State::load_profile(&directory);assert_eq!(again.library.session,SessionStatus::Unknown);
            again.update_library(3,library_sample(true,"connected",false),now);
            assert_eq!(again.library.session,SessionStatus::Connected);assert!(again.library.logged_in);
            std::fs::remove_dir_all(directory).unwrap();
        }
        #[test]
        fn only_explicit_settled_official_expiry_clears_saved_connection_and_playlists(){
            let now=Instant::now();let mut state=State::default();state.update_library(1,library_sample(true,"connected",false),now);
            state.update_library(1,library_sample(false,"",false),now);assert_eq!(state.library.session,SessionStatus::Connected);
            state.update_library(1,library_sample(false,"expired",true),now);assert!(state.library.logged_in);assert_eq!(state.library.playlists.len(),1);
            state.update_library(1,library_sample(false,"expired",false),now);
            assert_eq!(state.library.session,SessionStatus::Expired);assert!(!state.library.logged_in);assert!(!state.remembered);assert!(state.sections.is_empty());assert!(state.library.playlists.is_empty());
            state.stop_engine();assert_eq!(state.library.session,SessionStatus::Expired);
            state.update_library(2,library_sample(true,"connected",false),now);assert_eq!(state.library.session,SessionStatus::Connected);assert!(state.library.logged_in);
        }
        #[test]
        fn playback_memory_survives_stop_and_cold_load_without_fabricating_live_media(){
            static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
            let directory=std::env::temp_dir().join(format!("qingyu-qq-resume-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
            std::fs::create_dir_all(&directory).unwrap();let now=Instant::now();let mut state=State::load_profile(&directory);
            state.update_library(2,library_sample(true,"connected",false),now);state.last_playlist=Some(session_store::Playlist{id:"123".into(),title:"已验证收藏".into(),url:"https://y.qq.com/n/ryqq_v2/playlist/123".into()});
            state.reset(1,1,now);let mut value=projection("上次歌曲","track-id","document");
            value["sequence"]=1.into();value["media"]["track_url"]="https://y.qq.com/n/ryqq_v2/songDetail/001Abc123".into();value["media"]["position"]=42.0.into();
            state.projection(1,1,&serde_json::to_vec(&value).unwrap(),now);
            assert_eq!(state.library.resume.as_ref().unwrap().title,"上次歌曲");assert_eq!(state.library.resume.as_ref().unwrap().playlist_title,"已验证收藏");
            let saved=std::fs::read(directory.join(session_store::FILE)).unwrap();
            value["sequence"]=2.into();value["media"]["position"]=43.0.into();
            state.projection(1,1,&serde_json::to_vec(&value).unwrap(),now+Duration::from_secs(1));assert_eq!(std::fs::read(directory.join(session_store::FILE)).unwrap(),saved,"progress writes are throttled");
            state.stop_engine();assert!(state.slots.is_empty());assert!(!state.library.resuming);assert_eq!(state.library.resume.as_ref().unwrap().position,43.0);
            let mut cold=State::load_profile(&directory);assert!(cold.slots.is_empty());assert!(cold.selected(now).is_none());assert!(!cold.library.player_ready);assert!(!cold.library.resuming);
            assert_eq!(cold.library.session,SessionStatus::Unknown);assert_eq!(cold.library.resume.as_ref().unwrap().position,43.0);
            assert_eq!(cold.playback.as_ref().unwrap().track_url,"https://y.qq.com/n/ryqq_v2/songDetail/001Abc123");assert_eq!(cold.library.playlists.len(),1);
            std::fs::remove_dir_all(directory).unwrap();
        }
        #[test]
        fn playlist_cache_without_track_exposes_honest_resume_fallback_and_expiry_removes_it(){
            let now=Instant::now();let mut state=State::default();state.update_library(1,library_sample(true,"connected",false),now);
            assert_eq!(state.library.resume.as_ref().unwrap().title,"");assert_eq!(state.library.resume.as_ref().unwrap().playlist_title,"已验证收藏");
            state.stop_engine();assert_eq!(state.library.session,SessionStatus::Unknown);assert!(state.remembered_playlist().is_some());
            state.update_library(1,library_sample(false,"expired",false),now);assert!(state.library.resume.is_none());assert!(state.remembered_playlist().is_none());
        }
        #[test]
        fn live_media_and_lyrics_are_one_fresh_selected_slot_never_persisted_resume(){
            let now=Instant::now();let mut state=State::default();
            state.library.resume=Some(Resume{title:"仅为上次记录".into(),..Default::default()});
            assert!(state.live_pair(now).0.is_none());
            state.reset(1,1,now);state.reset(2,1,now);
            let mut first=projection("风吹麦浪（现场长标题）","qq:first","first-document");first["sequence"]=1.into();
            state.projection(1,1,&serde_json::to_vec(&first).unwrap(),now);
            let mut second=projection("安和桥","qq:second","second-document");second["sequence"]=1.into();second["media"]["playing"]=false.into();second["lyrics"]["lines"]=serde_json::json!(["第二首第一行","第二首当前行"]);
            state.projection(2,1,&serde_json::to_vec(&second).unwrap(),now);
            assert_eq!(state.live_pair(now).0.unwrap().title,"风吹麦浪（现场长标题）");
            second["sequence"]=2.into();second["media"]["playing"]=true.into();
            state.projection(2,1,&serde_json::to_vec(&second).unwrap(),now+Duration::from_millis(100));
            first["sequence"]=2.into();state.projection(1,1,&serde_json::to_vec(&first).unwrap(),now+Duration::from_millis(200));
            let (live,lyrics)=state.live_pair(now+Duration::from_millis(300));
            assert_eq!(live.as_ref().unwrap().title,"安和桥");assert_eq!(live.as_ref().unwrap().sampled_at,Some(now+Duration::from_millis(100)));
            assert_eq!(lyrics.unwrap().lines[1],"第二首当前行");assert_eq!(state.library.resume.as_ref().unwrap().title,"仅为上次记录");
            assert!(state.live_pair(now+FRESH_FOR+Duration::from_secs(1)).0.is_none());
        }
        #[test]
        fn hydration_gap_never_renews_sample_freshness_or_reuses_lyrics(){
            let now=Instant::now();let mut state=State::default();state.reset(1,1,now);
            let mut valid=projection("当前歌曲","qq:song","document");valid["sequence"]=1.into();
            state.projection(1,1,&serde_json::to_vec(&valid).unwrap(),now);
            let mut missing=valid.clone();missing["media"]=serde_json::Value::Null;missing["sequence"]=2.into();
            state.projection(1,1,&serde_json::to_vec(&missing).unwrap(),now+Duration::from_secs(1));
            let (live,lyrics)=state.live_pair(now+Duration::from_millis(1400));assert_eq!(live.unwrap().sampled_at,Some(now));assert!(lyrics.is_none());
            missing["sequence"]=3.into();state.projection(1,1,&serde_json::to_vec(&missing).unwrap(),now+Duration::from_secs(2));
            assert!(state.live_pair(now+Duration::from_secs(2)).0.is_none(),"repeated hydration empties cannot keep old playback alive");
            valid["sequence"]=4.into();state.projection(1,1,&serde_json::to_vec(&valid).unwrap(),now+Duration::from_secs(30));
            let (live,lyrics)=state.live_pair(now+Duration::from_secs(30));assert_eq!(live.unwrap().title,"当前歌曲");assert!(lyrics.is_some());
            state.projection(1,1,&serde_json::to_vec(&missing).unwrap(),now+Duration::from_secs(31));assert!(state.live_pair(now+Duration::from_secs(31)).1.is_some(),"late older poll cannot clear fresh lyric snapshot");
        }
        #[test]
        fn repeated_track_changes_after_hidden_gaps_recover_without_cross_track_lyrics(){
            let now=Instant::now();let mut state=State::default();state.reset(1,1,now);
            for index in 0..40 {
                let at=now+Duration::from_secs(index*20);let title=format!("第 {index} 首 · 长标题");let key=format!("qq:track{index}");
                let mut value=projection(&title,&key,"persistent-hidden-document");value["sequence"]=(index*2+1).into();
                value["lyrics"]["track_key"]=format!("qq:track{}",index.saturating_sub(1)).into();
                state.projection(1,1,&serde_json::to_vec(&value).unwrap(),at);
                let (live,lyrics)=state.live_pair(at);assert_eq!(live.unwrap().title,title);if index>0{assert!(lyrics.is_none());}
                value["sequence"]=(index*2+2).into();value["lyrics"]["track_key"]=key.into();value["lyrics"]["lines"]=serde_json::json!([format!("第 {index} 首的当前歌词")]);value["lyrics"]["current"]=0.into();
                state.projection(1,1,&serde_json::to_vec(&value).unwrap(),at+Duration::from_millis(100));
                assert_eq!(state.live_pair(at+Duration::from_millis(100)).1.unwrap().title,title);
                assert!(state.live_pair(at+Duration::from_secs(10)).0.is_none());
            }
            assert_eq!(state.slots.len(),1);
        }
    }
}
