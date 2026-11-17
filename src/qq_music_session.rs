//! Profile-bound display cache only. WebKit owns all authentication material.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::Path};

pub(super) const FILE: &str = "qq-music-session.json";
const LIMIT: usize = 128 * 1024;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Playlist {pub id:String,pub title:String,pub url:String}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Playback {
    pub track_url:String,
    pub title:String,
    pub artist:String,
    pub album:String,
    pub position:f64,
    pub duration:f64,
    #[serde(default)] pub playlist:Option<Playlist>,
}
impl Playback {
    pub fn valid(&self)->bool {
        track_url(&self.track_url).is_some() && !self.title.trim().is_empty() &&
            [&self.title,&self.artist,&self.album].iter().all(|text|text.len()<=4096) &&
            self.position.is_finite() && self.duration.is_finite() && (0.0..=7.0*86400.0).contains(&self.position) &&
            (0.0..=7.0*86400.0).contains(&self.duration) && (self.duration==0.0 || self.position<=self.duration) &&
            self.playlist.as_ref().is_none_or(valid_playlist)
    }
}
fn valid_playlist(playlist:&Playlist)->bool {
    !playlist.title.trim().is_empty() && playlist.title.len()<=512 && playlist_url(&playlist.id,&playlist.url).is_some()
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Cache {
    pub version:u8,
    pub source:String,
    pub profile:String,
    pub connected_at_ms:u64,
    pub expired:bool,
    pub sections:BTreeMap<String,Vec<Playlist>>,
    #[serde(default)] pub last_playlist:Option<Playlist>,
    #[serde(default)] pub playback:Option<Playback>,
}
impl Cache {
    pub fn empty(profile:&str)->Self {
        Self{version:2,source:super::SOURCE.into(),profile:profile.into(),connected_at_ms:0,expired:false,sections:BTreeMap::new(),last_playlist:None,playback:None}
    }
    fn valid(&self,profile:&str,now:u64)->bool {
        matches!(self.version,1|2) && self.source==super::SOURCE && self.profile==profile && self.profile.len()==36
            && self.connected_at_ms<=now.saturating_add(5*60*1000)
            && self.sections.len()<=3 && (!self.expired || (self.sections.is_empty() && self.playback.is_none() && self.last_playlist.is_none()))
            && self.sections.iter().all(|(section,playlists)|matches!(section.as_str(),"favorites"|"created"|"collected")
                && playlists.len()<=120 && playlists.iter().all(valid_playlist))
            && self.last_playlist.as_ref().is_none_or(valid_playlist) && self.playback.as_ref().is_none_or(Playback::valid)
            && (self.connected_at_ms>0 || self.sections.is_empty())
    }
    pub fn load(root:&Path,profile:&str,now:u64)->Option<Self> {
        let mut bytes=Vec::new();
        std::fs::File::open(root.join(FILE)).ok()?.take(LIMIT as u64+1).read_to_end(&mut bytes).ok()?;
        if bytes.len()>LIMIT{return None;}
        let mut cache:Self=serde_json::from_slice(&bytes).ok()?;
        if !cache.valid(profile,now){return None;}
        cache.version=2;Some(cache)
    }
    pub fn save(&self,root:&Path,now:u64)->std::io::Result<()> {
        if !self.valid(&self.profile,now){return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"invalid QQ Music display cache"));}
        let bytes=serde_json::to_vec(self)?;
        if bytes.len()>LIMIT{return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,"QQ Music display cache too large"));}
        crate::model::atomic_write(&root.join(FILE),&bytes)
    }
}
pub(super) fn track_url(value:&str)->Option<String> {
    if value.len()>2048{return None;}
    let url=url::Url::parse(value).ok()?;
    if url.scheme()!="https" || url.host_str()!=Some("y.qq.com") || url.port_or_known_default()!=Some(443) ||
        !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some(){return None;}
    let mut parts=url.path().split('/');
    if parts.next()!=Some("") || parts.next()!=Some("n") || !matches!(parts.next(),Some("ryqq"|"ryqq_v2")) || parts.next()!=Some("songDetail"){return None;}
    let id=parts.next()?;
    (!id.is_empty() && id.len()<=64 && id.bytes().all(|b|b.is_ascii_alphanumeric()) && parts.next().is_none()).then(||url.into())
}
pub(super) fn now_ms()->u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis().min(u128::from(u64::MAX)) as u64
}
pub(super) fn playlist_url(id:&str,value:&str)->Option<String> {
    if value.len()>2048{return None;}
    let url=url::Url::parse(value).ok()?;
    if url.scheme()!="https" || url.host_str()!=Some("y.qq.com") || url.port_or_known_default()!=Some(443)
        || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {return None;}
    let path=url.path();
    let valid=if id=="favorites" {matches!(path,"/n/ryqq/profile/like/song"|"/n/ryqq_v2/profile/like/song")}
        else {!id.is_empty() && id.len()<=20 && id.bytes().all(|b|b.is_ascii_digit())
            && [format!("/n/ryqq/playlist/{id}"),format!("/n/ryqq_v2/playlist/{id}")].iter().any(|p|p==path)};
    valid.then(||url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cache(profile:&str)->Cache {
        let mut cache=Cache::empty(profile);cache.connected_at_ms=1000;
        cache.sections.insert("collected".into(),vec![Playlist{id:"123".into(),title:"已验证歌单".into(),url:"https://y.qq.com/n/ryqq_v2/playlist/123".into()}]);cache
    }
    #[test]
    fn cache_is_bound_to_source_and_profile_and_cannot_import_credentials_or_arbitrary_urls(){
        let profile="9c6f5408-a539-5c83-989e-159b401bac85";let base=cache(profile);assert!(base.valid(profile,2000));
        assert!(!base.valid("different-profile",2000));
        let mut value=base.clone();value.source="another-source".into();assert!(!value.valid(profile,2000));
        let mut value=base.clone();value.sections.get_mut("collected").unwrap()[0].url="https://evil.test/n/ryqq_v2/playlist/123".into();assert!(!value.valid(profile,2000));
        let mut value=serde_json::to_value(&base).unwrap();value["cookies"]=serde_json::json!(["must never be imported"]);
        assert!(serde_json::from_value::<Cache>(value).is_err());
        let mut value=base.clone();value.expired=true;assert!(!value.valid(profile,2000));
    }
    #[test]
    fn bounded_atomic_cache_survives_reload_and_explicit_expiry_tombstone(){
        static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
        let directory=std::env::temp_dir().join(format!("qingyu-qq-session-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir_all(&directory).unwrap();let profile="9c6f5408-a539-5c83-989e-159b401bac85";let display_cache=cache(profile);
        display_cache.save(&directory,2000).unwrap();assert_eq!(Cache::load(&directory,profile,2000),Some(display_cache));
        let mut legacy=serde_json::to_value(cache(profile)).unwrap();legacy["version"]=1.into();legacy.as_object_mut().unwrap().remove("playback");legacy.as_object_mut().unwrap().remove("last_playlist");
        std::fs::write(directory.join(FILE),serde_json::to_vec(&legacy).unwrap()).unwrap();
        let migrated=Cache::load(&directory,profile,2000).unwrap();assert_eq!(migrated.version,2);assert_eq!(migrated.sections["collected"].len(),1);assert!(migrated.playback.is_none());
        let mut expired=Cache::empty(profile);expired.expired=true;expired.save(&directory,2000).unwrap();
        assert!(Cache::load(&directory,profile,2000).unwrap().expired);
        std::fs::write(directory.join(FILE),vec![b' ';LIMIT+1]).unwrap();assert!(Cache::load(&directory,profile,2000).is_none());
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn legacy_cache_migrates_without_auth_material_or_autoplay_state_and_rejects_invalid_resume(){
        let profile="9c6f5408-a539-5c83-989e-159b401bac85";
        let mut value=serde_json::to_value(cache(profile)).unwrap();value["version"]=1.into();value.as_object_mut().unwrap().remove("last_playlist");value.as_object_mut().unwrap().remove("playback");
        let legacy:Cache=serde_json::from_value(value).unwrap();assert!(legacy.valid(profile,2000));assert!(legacy.playback.is_none());assert!(legacy.last_playlist.is_none());
        let good=Playback{track_url:"https://y.qq.com/n/ryqq_v2/songDetail/001Abc123".into(),title:"上一首歌".into(),artist:"歌手".into(),position:42.0,duration:210.0,..Default::default()};
        assert!(good.valid());
        for url in ["https://evil.test/n/ryqq_v2/songDetail/001Abc123","https://y.qq.com/n/ryqq_v2/songDetail/001Abc123?token=x","https://user@y.qq.com/n/ryqq/songDetail/001Abc123","https://y.qq.com/n/ryqq_v2/playlist/123"]{
            let mut invalid=good.clone();invalid.track_url=url.into();assert!(!invalid.valid());
        }
        let mut invalid=good.clone();invalid.position=f64::NAN;assert!(!invalid.valid());invalid.position=211.0;assert!(!invalid.valid());
        let mut remembered=cache(profile);remembered.playback=Some(good);remembered.last_playlist=remembered.sections["collected"].first().cloned();
        assert!(remembered.valid(profile,2000));remembered.expired=true;assert!(!remembered.valid(profile,2000));
    }
}
