//! Public Automation for Apple Music/Spotify plus the official QQ web player.
//! Automation has its own bounded worker so a slow permission dialog cannot
//! stall cached WebKit snapshots or QQ playback controls.
use crate::smtc::{MediaKind, NowPlaying, Transport};
use serde::Deserialize;
use std::{collections::HashMap, io::Read, process::{Command, Stdio}, sync::{Arc, Mutex}, time::{Duration, Instant}};

const READ: &str = r#"
function run(argv) {
    var choices = [];
    var specs = [['com.apple.Music', 1], ['com.spotify.client', 1000]];
    for (var i = 0; i < specs.length; i++) {
        try {
            var app = Application(specs[i][0]);
            if (!app.running()) continue;
            var state = String(app.playerState());
            if (state !== 'playing' && state !== 'paused') continue;
            var track = app.currentTrack;
            var value = {source: specs[i][0], title: String(track.name()).slice(0, 1024),
                artist: String(track.artist()).slice(0, 1024), album: String(track.album()).slice(0, 1024),
                playing: state === 'playing', duration: Number(track.duration()) / specs[i][1],
                position: Number(app.playerPosition()), volume: Number(app.soundVolume())};
            if (specs[i][0] === 'com.spotify.client') value.artwork = String(track.artworkUrl());
            if (value.title.length) choices.push(value);
        } catch (e) {}
    }
    choices.sort(function(a, b) { return Number(b.playing) - Number(a.playing); });
    return JSON.stringify(choices.length ? choices[0] : null);
}
"#;
const CONTROL: &str = r#"
function run(argv) {
    var app = Application(argv[0]);
    if (!app.running()) return 'false';
    switch (argv[1]) {
        case 'toggle': app.playpause(); break;
        case 'next': app.nextTrack(); break;
        case 'prev': app.previousTrack(); break;
        case 'seek': app.playerPosition = Number(argv[2]); break;
        case 'volume': app.soundVolume = Number(argv[2]); break;
        default: return 'false';
    }
    return 'true';
}
"#;

fn script(source: &str, arguments: &[&str]) -> Option<Vec<u8>> {
    let mut child = Command::new("/usr/bin/osascript").args(["-l", "JavaScript", "-e", source])
        .args(arguments).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() { return None; }
                let mut output = Vec::new();
                child.stdout.take()?.take(16385).read_to_end(&mut output).ok()?;
                return (output.len() <= 16384).then_some(output);
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            _ => { let _ = child.kill(); let _ = child.wait(); return None; }
        }
    }
}

#[derive(Deserialize)]
struct Track {
    source: String, title: String, artist: String, album: String,
    playing: bool, duration: f64, position: f64, volume: u32,
    #[serde(default)] artwork: String,
}
fn supported(source: &str) -> bool { matches!(source, "com.apple.Music" | "com.spotify.client") }
static VOLUME: std::sync::OnceLock<Mutex<HashMap<String, (u32, u32)>>> = std::sync::OnceLock::new();
fn volumes() -> &'static Mutex<HashMap<String, (u32, u32)>> { VOLUME.get_or_init(Mutex::default) }
fn remember(source: &str, value: u32) {
    let mut map = volumes().lock().unwrap_or_else(|e| e.into_inner());
    let old = map.get(source).copied().unwrap_or((50, 50));
    let value = value.min(100);
    map.insert(source.into(), (value, if value > 0 { value } else { old.1 }));
}
pub(crate) fn volume(source: &str) -> Option<(u32, bool)> {
    if source == crate::qq_music::SOURCE { return crate::qq_music::volume(); }
    if !supported(source) { return None; }
    volumes().lock().unwrap_or_else(|e| e.into_inner()).get(source).map(|v| (v.0, v.0 == 0))
}

#[derive(Default)]
pub(crate) struct Reader {
    automation: Option<Automation>,
    retry_at: Option<Instant>,
    selection: Selection,
}
impl Reader {
    pub(crate) fn query(&mut self) -> Option<NowPlaying> {
        let qq = crate::qq_music::query();
        let now = Instant::now();
        if self.automation.is_none() && self.retry_at.is_none_or(|at| now >= at) {
            self.automation = Automation::start();
            self.retry_at = Some(now + Duration::from_secs(1));
        }
        let external = self.automation.as_mut().and_then(|worker| worker.poll(now));
        self.selection.choose(qq,external)
    }
}

type Signature = (String,String,String,String,bool);
fn signature(media: &NowPlaying) -> Signature {
    (media.source.clone(),media.title.clone(),media.artist.clone(),media.album.clone(),media.playing)
}
#[derive(Default)]
struct Selection { previous_qq: Option<Signature>, previous_external: Option<Signature>, preferred: String }
impl Selection {
    fn choose(&mut self, qq: Option<NowPlaying>, external: Option<NowPlaying>) -> Option<NowPlaying> {
        let qq_signature = qq.as_ref().map(signature);
        let external_signature = external.as_ref().map(signature);
        let qq_started = qq.as_ref().is_some_and(|media| media.playing) && qq_signature != self.previous_qq;
        // An asynchronous initial sample is not evidence that an already
        // playing external app just became active. Only an observed transition
        // can take selection from an active QQ session.
        let external_started = external.as_ref().is_some_and(|media| media.playing) && self.previous_external.as_ref()
            .is_some_and(|previous| Some(previous) != external_signature.as_ref());
        self.previous_qq = qq_signature; self.previous_external = external_signature;
        let selected = match (qq,external) {
            (Some(qq),Some(external)) if external.playing && qq.playing => {
                if qq_started || (!external_started && self.preferred != external.source) { qq } else { external }
            }
            (Some(qq),Some(external)) => if external.playing { external } else { qq },
            (qq,external) => return {
                let selected = qq.or(external);
                self.preferred = selected.as_ref().map_or_else(String::new,|media|media.source.clone());
                selected
            },
        };
        self.preferred = selected.source.clone(); Some(selected)
    }
}

enum AutomationResult { Sample(Option<NowPlaying>), Failed }
struct Automation {
    requests: std::sync::mpsc::SyncSender<()>,
    results: std::sync::mpsc::Receiver<AutomationResult>,
    pending: bool,
    next: Option<Instant>,
    cached: Option<(NowPlaying,Instant)>,
}
impl Automation {
    fn start() -> Option<Self> {
        static LANE: crate::resources::Lane = crate::resources::Lane::new("qingyu-mac-automation",1);
        let (requests,receiver) = std::sync::mpsc::sync_channel(1);
        let (sender,results) = std::sync::mpsc::sync_channel(1);
        LANE.spawn(move || {
            let mut reader = AutomationReader::default();
            while receiver.recv().is_ok() { if sender.send(reader.query()).is_err() { break; } }
        })?;
        Some(Self { requests,results,pending:false,next:None,cached:None })
    }
    fn poll(&mut self, now: Instant) -> Option<NowPlaying> {
        if self.pending {
            match self.results.try_recv() {
                Ok(AutomationResult::Sample(media)) => {
                    self.pending = false; self.cached = media.map(|media|(media,now));
                    self.next = Some(now + Duration::from_millis(750));
                }
                Ok(AutomationResult::Failed) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending = false; self.cached = None; self.next = Some(now + Duration::from_secs(30));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.pending && self.next.is_none_or(|next|now >= next) {
            self.pending = self.requests.try_send(()).is_ok();
            if !self.pending { self.next = Some(now + Duration::from_secs(30)); }
        }
        self.cached.as_ref().filter(|(media,at)|now.saturating_duration_since(*at) <= Duration::from_secs(8)
            && media.sampled_at.is_some_and(|at|now.saturating_duration_since(at) <= Duration::from_secs(8))).map(|(media,_)|media.clone())
    }
}

#[derive(Default)]
struct AutomationReader {
    artwork_key: String,
    artwork: Option<Arc<eframe::egui::ColorImage>>,
}
impl AutomationReader {
    fn query(&mut self) -> AutomationResult {
        let Some(raw) = script(READ, &[]) else { return AutomationResult::Failed; };
        let track = match serde_json::from_slice::<Option<Track>>(&raw) {
            Ok(Some(track)) => track,
            Ok(None) => return AutomationResult::Sample(None),
            Err(_) => return AutomationResult::Failed,
        };
        let sampled_at = Some(Instant::now());
        if !supported(&track.source) || !track.duration.is_finite() || !track.position.is_finite() { return AutomationResult::Sample(None); }
        remember(&track.source, track.volume);
        if self.artwork_key != track.artwork {
            self.artwork_key = track.artwork.clone();
            self.artwork = cover(&track.artwork).map(Arc::new);
        }
        AutomationResult::Sample(Some(NowPlaying { source: track.source, title: track.title, artist: track.artist, album: track.album,
            playing: track.playing, duration: track.duration.max(0.0), position: track.position.max(0.0), sampled_at,
            artwork: self.artwork.clone(), playback_type: MediaKind::Music, volume: Some((track.volume.min(100), track.volume == 0)),
            ..Default::default() }))
    }
}

fn cover(url: &str) -> Option<eframe::egui::ColorImage> {
    let parsed = url::Url::parse(url).ok()?;
    // Spotify supplies its public image CDN URL; avoid following arbitrary artwork targets.
    if parsed.scheme() != "https" || parsed.host_str()? != "i.scdn.co" || !parsed.username().is_empty() || parsed.password().is_some() { return None; }
    let response = ureq::AgentBuilder::new().timeout(Duration::from_secs(3)).redirects(0).build().get(url).call().ok()?;
    let mut bytes = Vec::new(); response.into_reader().take(4 * 1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 4 * 1024 * 1024 { return None; }
    let reader = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().ok()?;
    let dimensions = reader.into_dimensions().ok()?;
    if dimensions.0 > 4096 || dimensions.1 > 4096 { return None; }
    let image = image::load_from_memory(&bytes).ok()?.thumbnail(256, 256).into_rgba8();
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], &image))
}

pub(crate) fn control(command: Transport, source: &str) -> bool {
    if source == crate::qq_music::SOURCE { return crate::qq_music::control(command); }
    if !supported(source) { return false; }
    let (operation, value) = match command {
        Transport::Toggle => ("toggle", String::new()), Transport::Next => ("next", String::new()), Transport::Prev => ("prev", String::new()),
        Transport::Seek(seconds) if seconds.is_finite() && seconds >= 0.0 => ("seek", seconds.to_string()),
        Transport::SetVolume(value) => ("volume", value.min(100).to_string()),
        Transport::SetMute(muted) => {
            let value = if muted { 0 } else { volumes().lock().unwrap_or_else(|e|e.into_inner()).get(source).map_or(50, |v|v.1) };
            ("volume", value.to_string())
        }
        _ => return false,
    };
    let success = script(CONTROL, &[source, operation, &value]).is_some_and(|v| v.as_slice().trim_ascii() == b"true");
    if success && operation == "volume" { remember(source, value.parse().unwrap_or(50)); }
    success
}

#[cfg(test)]
mod tests {
    fn media(source: &str, playing: bool) -> crate::smtc::NowPlaying {
        crate::smtc::NowPlaying { source:source.into(), title:"歌曲".into(), artist:"歌手".into(), playing, ..Default::default() }
    }
    #[test]
    fn qq_and_external_sources_follow_playback_transitions_without_polling_stealing_selection() {
        let mut selection=super::Selection::default();
        let qq=crate::qq_music::SOURCE; let external="com.spotify.client";
        assert_eq!(selection.choose(Some(media(qq,true)),Some(media(external,false))).unwrap().source,qq);
        assert_eq!(selection.choose(Some(media(qq,false)),Some(media(external,false))).unwrap().source,qq);
        assert_eq!(selection.choose(Some(media(qq,false)),Some(media(external,true))).unwrap().source,external);
        assert_eq!(selection.choose(Some(media(qq,true)),Some(media(external,true))).unwrap().source,qq);
        assert_eq!(selection.choose(Some(media(qq,true)),Some(media(external,false))).unwrap().source,qq);
        assert_eq!(selection.choose(Some(media(qq,true)),Some(media(external,true))).unwrap().source,external);
        assert_eq!(selection.choose(Some(media(qq,true)),Some(media(external,true))).unwrap().source,external);
        assert_eq!(selection.choose(None,Some(media(external,false))).unwrap().source,external);
        assert!(selection.choose(None,None).is_none());
        let mut selection=super::Selection::default();
        assert_eq!(selection.choose(Some(media(qq,true)),None).unwrap().source,qq);
        assert_eq!(selection.choose(Some(media(qq,true)),Some(media(external,true))).unwrap().source,qq);
    }
    #[test]
    fn waiting_automation_returns_qq_immediately_and_expired_external_state_is_not_retained() {
        let (requests,_receiver)=std::sync::mpsc::sync_channel(1);
        let (_sender,results)=std::sync::mpsc::sync_channel(1);
        let now=std::time::Instant::now();
        let mut stale=media("com.spotify.client",true); stale.sampled_at=Some(now-std::time::Duration::from_secs(10));
        let mut worker=super::Automation {requests,results,pending:true,next:None,cached:Some((stale,now))};
        let started=std::time::Instant::now();
        let external=worker.poll(now); assert!(external.is_none());
        let selected=super::Selection::default().choose(Some(media(crate::qq_music::SOURCE,true)),external).unwrap();
        assert_eq!(selected.source,crate::qq_music::SOURCE);
        assert!(started.elapsed()<std::time::Duration::from_millis(100));
    }
    #[test]
    fn controls_reject_unknown_players_and_invalid_positions_without_automation() {
        use crate::smtc::Transport;
        assert!(!super::control(Transport::Next, "untrusted.bundle"));
        assert!(!super::control(Transport::Seek(f64::NAN), "com.apple.Music"));
        assert!(!super::control(Transport::Seek(-1.0), "com.spotify.client"));
        assert!(super::volume("untrusted.bundle").is_none());
    }
}
