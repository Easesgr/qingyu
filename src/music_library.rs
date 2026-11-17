//! Island views of the official QQ Music page's bounded public content.
use crate::{app::{App,Cmd},paint::{self,Card,H},theme::{self,W}};
use eframe::egui::{self,Color32};

impl App {
    pub fn set_qq_music(&mut self,library:crate::qq_music::Library,lyrics:Option<crate::qq_music::NativeLyrics>){
        if !library.playback_error.is_empty() && library.playback_error!=self.qq_library.playback_error{self.say(&library.playback_error);}
        else if self.qq_library.resuming && !library.resuming && !library.message.is_empty() && library.message!=self.qq_library.message{self.say(&library.message);}
        self.qq_lyrics=if library.live.is_some(){library.live_lyrics.clone()}else{lyrics};
        self.qq_library=library;
    }

    pub(crate) fn open_qq_library(&mut self,cmds:&mut Vec<Cmd>){
        self.qq_library_open=true;
        if self.qq_library.playlists.is_empty() && !self.qq_library.loading{self.refresh_qq_library(cmds);}
    }

    fn refresh_qq_library(&mut self,cmds:&mut Vec<Cmd>){
        self.qq_library.loading=true;
        if self.qq_library.session!=crate::qq_music::SessionStatus::Connected{
            self.qq_library.session=crate::qq_music::SessionStatus::Restoring;
        }
        self.qq_library.message.clear();
        cmds.push(Cmd::QqMusicRefresh);
    }

    pub(crate) fn qq_music_empty_state(&self)->(&'static str,&'static str){
        use crate::qq_music::SessionStatus;
        match self.qq_library.session{
            SessionStatus::Connected=>("QQ 音乐已连接","从「我的歌单」选择音乐"),
            SessionStatus::Restoring=>("正在恢复 QQ 音乐…","正在读取已有登录和歌单"),
            SessionStatus::Expired=>("登录 QQ 音乐","在「我的歌单」中连接账号"),
            SessionStatus::Unknown=>("QQ 音乐","从「我的歌单」选择音乐"),
        }
    }

    pub(crate) fn native_qq_lyrics(&self)->Option<&crate::qq_music::NativeLyrics>{
        if self.media.source!=crate::qq_music::SOURCE{return None;}
        self.qq_lyrics.as_ref().filter(|lyrics|lyrics.title.trim()==self.media.title.trim() && !lyrics.lines.is_empty())
    }

    pub(crate) fn draw_qq_library(&mut self,c:&Card,cmds:&mut Vec<Cmd>,portrait:bool){
        let inset=crate::portrait::INSET;
        let (x,width,top,height)=if portrait{(inset,self.expanded_width()-inset*2.0,109.0,self.expanded_height()-129.0)}
            else{(44.0,331.0,38.0,self.content_rect().bottom()-38.0)};
        let gap=12.0;let half=(width-gap)/2.0;
        if crate::portrait::button(c,x,top,half,"返回播放","qq-library-back"){self.qq_library_open=false;}
        if crate::portrait::button(c,x+half+gap,top,half,if self.qq_library.loading{"正在读取…"}else{"刷新歌单"},"qq-library-refresh") && !self.qq_library.loading{
            self.refresh_qq_library(cmds);
        }
        let library=self.qq_library.clone();
        let list_top=top+43.0;let list_height=(height-43.0).max(40.0);
        if library.playlists.is_empty(){
            use crate::qq_music::SessionStatus;
            let center=x+width/2.0;let y=list_top+(list_height/2.0-40.0).max(0.0);
            let (title,hint,action)=match library.session{
                SessionStatus::Expired=>("登录 QQ 音乐","登录后，在这里选择收藏与歌单","登录 QQ 音乐…"),
                SessionStatus::Connected if library.loading=>("正在读取你的歌单…","正在同步收藏和个人歌单",""),
                SessionStatus::Connected=>("暂未读取到歌单","打开「我的音乐」后刷新歌单","打开我的音乐…"),
                _ if library.loading=>("正在恢复 QQ 音乐…","正在读取已有登录和歌单",""),
                _=>("暂时无法读取 QQ 音乐","重试以恢复已有登录和歌单","重试读取"),
            };
            c.centered_text(center,y,title,14.0,W::Semibold,theme::WHITE,H::Center);
            let hint=if library.message.trim().is_empty(){hint}else{library.message.as_str()};
            c.centered_text(center,y+25.0,&c.fit(hint,11.5,W::Regular,width),11.5,W::Regular,theme::GRAY,H::Center);
            if !library.loading{
                if matches!(library.session,SessionStatus::Unknown|SessionStatus::Restoring){
                    if crate::portrait::button(c,center-76.0,y+45.0,152.0,action,"qq-library-retry"){self.refresh_qq_library(cmds);}
                }else if crate::portrait::button(c,center-76.0,y+45.0,152.0,action,"qq-library-login"){
                    cmds.push(Cmd::QqMusic(crate::qq_music::Page::Library));
                }
            }
            return;
        }
        let mut content=crate::portrait::scroll_ui(c,x,list_top,width,list_height,"qq-library");
        let mut play=None;
        egui::ScrollArea::vertical().id_salt(("qq-library-list",portrait)).max_height(list_height*c.s).auto_shrink([false,false]).show(&mut content,|ui|{
            for playlist in &library.playlists{
                let row_width=ui.available_width()/c.s;
                let (rect,_)=ui.allocate_exact_size(egui::vec2(row_width*c.s,54.0*c.s),egui::Sense::hover());
                let mut row=Card::new(ui,rect.min,ui.clip_rect());row.s=c.s;row.live=c.live;
                let label=format!("播放歌单：{}",playlist.title);
                let response=row.hit(0.0,0.0,row_width,54.0,("qq-playlist",&playlist.id)).on_hover_text(&label);
                response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,&label));
                let (hover,press)=row.hover_t(&response);
                if hover.max(press)>0.001{row.fill(0.0,0.0,row_width,54.0,8.0,paint::mix(theme::BTN,theme::BTN_HOVER,hover.max(press)));}
                if response.has_focus(){row.stroke(0.0,0.0,row_width,54.0,9.0,1.3,theme::CYAN);}
                paint::glyph(&row,"music",20.0,27.0,19.0,theme::CYAN);
                row.text(40.0,24.0,&row.fit(&playlist.title,12.5,W::Semibold,row_width-76.0),12.5,W::Semibold,theme::WHITE,H::Left);
                row.text(40.0,42.0,"点击播放",10.5,W::Regular,theme::GRAY,H::Left);
                paint::glyph(&row,"play",row_width-18.0,27.0,14.0,Color32::WHITE);
                row.seg((0.0,53.5),(row_width,53.5),0.5,Color32::from_white_alpha(22));
                if response.clicked(){play=Some(playlist.id.clone());}
            }
        });
        if let Some(id)=play{cmds.push(Cmd::QqMusicPlayPlaylist(id));self.qq_library_open=false;}
    }

    pub(crate) fn draw_qq_lyrics(&self,c:&Card)->bool{
        let Some(lyrics)=self.native_qq_lyrics() else{return false;};
        let current=lyrics.current.unwrap_or(0).min(lyrics.lines.len()-1);
        let col=c.clipped(243.0,28.0,136.0,74.0);
        c.seg((233.0,32.0),(233.0,98.0),1.0,Color32::from_gray(35));
        let position=c.ui.ctx().animate_value_with_time(egui::Id::new("qq-music-lyric-row"),current as f32,0.2);
        for (index,line) in lyrics.lines.iter().enumerate().skip(current.saturating_sub(2)).take(5){
            let distance=index as f32-position;let active=lyrics.current==Some(index);
            let row=col.faded((1.0-distance.abs()*0.35).clamp(0.0,1.0));let ink=if active{theme::WHITE}else{theme::GRAY};
            let mut job=egui::text::LayoutJob::simple(line.clone(),row.font(if active{12.0}else{10.5},if active{W::Semibold}else{W::Regular}),ink,132.0*c.s);
            job.wrap.max_rows=if active{3}else{1};job.wrap.break_anywhere=true;
            let galley=row.painter.layout_job(job);
            row.painter.galley(row.pos(246.0,64.0+distance*29.0-galley.size().y/(2.0*c.s)),galley,ink);
        }
        true
    }
}
