//! Immediate playback feedback, reconciled with source-bound confirmed state.
use crate::{app::{App,Cmd},smtc::{NowPlaying,Transport}};
use std::time::Instant;

#[derive(Default)]
pub(crate) struct Feedback {sequence:u64,pending:Option<Pending>,qq_stopped_at:Option<Instant>}
struct Pending {id:u64,source:String,title:String,artist:String,album:String,playing:bool,position:f64,at:f64,until:f64,created:Instant,confirmed:bool}
impl Pending {
    fn matches(&self,media:&NowPlaying)->bool {
        self.source==media.source && self.title==media.title && self.artist==media.artist && self.album==media.album
    }
}
impl Feedback {
    fn current(&self,media:&NowPlaying,t:f64)->Option<&Pending>{self.pending.as_ref().filter(|p|p.matches(media)&&t<p.until)}
    fn playing(&self,media:&NowPlaying,t:f64)->bool{self.current(media,t).map_or(media.playing,|p|p.playing)}
    fn position(&self,media:&NowPlaying,t:f64)->f64{
        self.current(media,t).map_or(media.position,|p|{
            let position=p.position+if p.playing{(t-p.at).max(0.0)}else{0.0};
            if media.duration>0.0{position.min(media.duration)}else{position}
        })
    }
    fn toggle(&mut self,media:&NowPlaying,t:f64)->(u64,bool){
        let playing=!self.playing(media,t);let position=self.position(media,t);
        self.sequence=self.sequence.wrapping_add(1).max(1);let id=self.sequence;
        self.pending=Some(Pending{id,source:media.source.clone(),title:media.title.clone(),artist:media.artist.clone(),album:media.album.clone(),playing,position,at:t,until:t+2.0,created:Instant::now(),confirmed:false});
        (id,playing)
    }
    fn complete(&mut self,id:u64,success:bool)->bool{
        let Some(pending)=self.pending.as_mut().filter(|p|p.id==id)else{return false;};
        if success{pending.confirmed=true;false}else{self.pending=None;true}
    }
    fn observe(&mut self,media:&NowPlaying,t:f64)->bool{
        let Some(pending)=&self.pending else{return false;};
        if !pending.matches(media){self.pending=None;return false;}
        let expired=t>=pending.until;
        let confirmed=pending.confirmed && media.playing==pending.playing && media.sampled_at.is_some_and(|at|at>=pending.created);
        let failed=expired && media.playing!=pending.playing;
        if expired||confirmed{self.pending=None;}
        failed
    }
    pub(crate) fn clear(&mut self){self.pending=None;}
}

impl App {
    pub(crate) fn music_playing(&self)->bool{!self.can_resume_qq_music()&&self.playback_feedback.playing(&self.media,self.t)}
    pub(crate) fn music_position(&self)->f64{self.playback_feedback.position(&self.media,self.t)}
    pub(crate) fn can_resume_qq_music(&self)->bool{
        cfg!(target_os="macos") && (self.media.title.is_empty()||self.media.source==crate::qq_music::SOURCE&&!self.qq_library.player_ready&&!self.fresh_qq_sample(&self.media))
            && self.qq_library.session!=crate::qq_music::SessionStatus::Expired
            && self.qq_library.resume.is_some()
    }
    fn fresh_qq_sample(&self,media:&NowPlaying)->bool{
        media.source==crate::qq_music::SOURCE && media.sampled_at.is_some_and(|at|at.elapsed()<=std::time::Duration::from_secs(8)
            && self.playback_feedback.qq_stopped_at.is_none_or(|stopped|at>stopped))
    }
    #[cfg(target_os="macos")]
    pub(crate) fn reconcile_qq_media(&self,polled:&NowPlaying)->NowPlaying{
        let live=self.qq_library.live.as_ref().filter(|media|self.fresh_qq_sample(media)&&media.is_music());
        // External playback selection remains with the system media reader.
        // QQ's GUI-thread snapshot updates its own title and lyrics together,
        // even when the background media poll still holds the previous song.
        if polled.source!=crate::qq_music::SOURCE && !polled.title.is_empty() && polled.playing{return polled.clone();}
        if let Some(live)=live.filter(|live|live.playing||polled.source==crate::qq_music::SOURCE||polled.title.is_empty()){
            let mut media=live.clone();
            if (media.title.as_str(),media.artist.as_str(),media.album.as_str())==(polled.title.as_str(),polled.artist.as_str(),polled.album.as_str())
                && polled.source==crate::qq_music::SOURCE{media.artwork=polled.artwork.clone();}
            return media;
        }
        if polled.source==crate::qq_music::SOURCE && polled.sampled_at.is_some() && !self.fresh_qq_sample(polled){return NowPlaying::default();}
        if polled.source==crate::qq_music::SOURCE && self.playback_feedback.qq_stopped_at.is_some()
            && polled.sampled_at.is_none(){return NowPlaying::default();}
        polled.clone()
    }
    pub(crate) fn has_live_music(&self)->bool{!self.media.title.is_empty()&&!self.can_resume_qq_music()}
    pub(crate) fn can_play_music(&self)->bool{!self.media.title.is_empty()||self.can_resume_qq_music()||self.qq_library.resuming}
    pub(crate) fn music_display(&self)->NowPlaying{
        if self.has_live_music(){let mut media=self.media.clone();media.playing=self.music_playing();media.position=self.music_position();return media;}
        let Some(saved)=self.qq_library.resume.as_ref().filter(|_|self.can_resume_qq_music()) else{return self.media.clone();};
        let title=if saved.title.is_empty(){saved.playlist_title.clone()}else{saved.title.clone()};
        let artist=if self.qq_library.resuming{"正在继续播放… · 再点可取消".into()}
            else if !self.qq_library.playback_error.is_empty(){self.qq_library.playback_error.clone()}
            else if saved.title.is_empty(){"点击播放歌单".into()}
            else if saved.artist.is_empty(){"上次播放 · 点击继续".into()}
            else{format!("{} · 上次播放",saved.artist)};
        NowPlaying{source:crate::qq_music::SOURCE.into(),title,artist,album:saved.album.clone(),position:saved.position,duration:saved.duration,..Default::default()}
    }
    pub(crate) fn music_play_button(&self)->(&'static str,&'static str){
        if self.qq_library.resuming{("close","取消继续播放")}
        else if self.music_playing(){("pause","暂停")}
        else if self.can_resume_qq_music(){("play",if self.qq_library.resume.as_ref().is_some_and(|resume|resume.title.is_empty()){"播放歌单"}else{"继续上次播放"})}
        else{("play","播放")}
    }
    pub(crate) fn toggle_music_playback(&mut self,cmds:&mut Vec<Cmd>){
        if self.qq_library.resuming{
            self.cancel_qq_playback();cmds.push(Cmd::QqMusicStop);
        }else if self.can_resume_qq_music(){
            self.qq_library.resuming=true;self.qq_library.playback_error.clear();cmds.push(Cmd::QqMusicResume);
        }else if self.media.title.is_empty(){
            self.open_qq_library(cmds);
        }else if self.media.source==crate::qq_music::SOURCE{
            let(request,playing)=self.playback_feedback.toggle(&self.media,self.t);
            cmds.push(Cmd::QqMusicSetPlaying{request,playing});
        }else{cmds.push(Cmd::Media(Transport::Toggle));}
    }
    pub fn complete_qq_playback(&mut self,request:u64,success:bool){
        if self.playback_feedback.complete(request,success){self.say("QQ 音乐暂未响应，请重试");}
    }
    pub fn cancel_qq_playback(&mut self){self.playback_feedback.clear();self.qq_library.resuming=false;}
    pub fn stop_qq_playback(&mut self){
        self.cancel_qq_playback();self.playback_feedback.qq_stopped_at=Some(Instant::now());
        if self.media.source==crate::qq_music::SOURCE{self.media=NowPlaying::default();self.music_artwork=None;self.qq_lyrics=None;}
    }
    pub(crate) fn observe_music_playback(&mut self,media:&NowPlaying){
        if self.playback_feedback.observe(media,self.t){self.say("QQ 音乐暂未响应，请重试");}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track()->NowPlaying{NowPlaying{source:crate::qq_music::SOURCE.into(),title:"歌曲".into(),artist:"歌手".into(),playing:true,position:30.0,duration:180.0,..Default::default()}}
    #[test]
    fn clicks_respond_immediately_and_latest_intent_survives_stale_receipts(){
        let media=track();let mut feedback=Feedback::default();
        let (first,playing)=feedback.toggle(&media,0.0);assert!(!playing);assert!(!feedback.playing(&media,0.0));assert_eq!(feedback.position(&media,0.5),30.0);
        let (second,playing)=feedback.toggle(&media,0.5);assert!(playing);assert!(feedback.playing(&media,0.5));assert!((feedback.position(&media,0.8)-30.3).abs()<0.001);
        assert!(!feedback.complete(first,false));assert!(feedback.playing(&media,0.9));
        let (third,playing)=feedback.toggle(&media,0.9);assert!(!playing);assert!(third>second);
        assert!(!feedback.complete(second,true));assert!(!feedback.playing(&media,1.0));
        assert!(feedback.complete(third,false));assert!(feedback.playing(&media,1.0));
    }
    #[test]
    fn confirmed_feedback_ignores_old_samples_and_never_leaks_to_another_track(){
        let mut media=track();let mut feedback=Feedback::default();
        media.sampled_at=Some(Instant::now());let(id,_)=feedback.toggle(&media,0.0);
        feedback.complete(id,true);media.playing=false;
        assert!(!feedback.observe(&media,0.1));assert!(feedback.pending.is_some(),"old sample must not settle a new command");
        media.sampled_at=Some(Instant::now());feedback.observe(&media,0.2);assert!(feedback.pending.is_none());
        feedback.toggle(&media,0.3);media.title="另一首".into();feedback.observe(&media,0.4);assert!(feedback.pending.is_none());assert!(!feedback.playing(&media,0.4));
    }
    #[test]
    fn unconfirmed_playback_falls_back_to_real_state_after_deadline(){
        let media=track();let mut feedback=Feedback::default();feedback.toggle(&media,0.0);
        assert!(feedback.observe(&media,2.1));assert!(feedback.playing(&media,2.1));assert_eq!(feedback.position(&media,2.1),media.position);
    }
}
