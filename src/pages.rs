//! 音乐、置物架、粘贴板和待办页面。
//! 坐标全部是面板局部逻辑点（419×140），文字 y 是基线。

use crate::anim::{self, lerp, smoothstep, Spring};
use crate::app::{App, Cmd, Env};
use crate::mail;
use crate::paint::{self, capsule, mix, Card, H};
use crate::smtc::Transport;
use crate::theme::{self, size, W};
use eframe::egui::{self, Color32, Pos2, Rect};

/// 横向粘贴板一屏显示的条数。
pub(crate) const CLIP_ROWS: usize = if cfg!(target_os="macos"){8}else{6};
pub(crate) const MUSIC_VOLUME_RECT: Rect = Rect::from_min_max(Pos2::new(345.0,6.0),Pos2::new(389.0,104.0));

fn mmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// 由标题生成稳定的封面渐变色。
fn cover_colors(title: &str) -> (Color32, Color32) {
    let mut h: u64 = 1469598103934665603;
    for b in title.bytes() {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    let palette = [
        (Color32::from_rgb(255, 94, 98), Color32::from_rgb(255, 153, 102)),
        (Color32::from_rgb(86, 204, 242), Color32::from_rgb(47, 128, 237)),
        (Color32::from_rgb(168, 85, 247), Color32::from_rgb(236, 72, 153)),
        (Color32::from_rgb(52, 211, 153), Color32::from_rgb(16, 120, 200)),
        (Color32::from_rgb(251, 191, 36), Color32::from_rgb(239, 68, 68)),
    ];
    palette[(h % palette.len() as u64) as usize]
}

impl App {
    // ================= 音乐 =================
    pub(crate) fn music_control_y(&self)->f32 {self.content_rect().bottom()-if cfg!(target_os="macos"){52.0}else{16.0}}
    pub(crate) fn music_volume_bounds(&self)->Rect {MUSIC_VOLUME_RECT.translate(egui::vec2(0.0,self.music_control_y()-118.0))}

    pub(crate) fn page_music(&mut self,c:&Card,_env:&Env,cmds:&mut Vec<Cmd>){
        if self.qq_library_open{self.draw_qq_library(c,cmds,false);return;}
        let bounds=c.layout_bounds(self.content_rect());let controls=self.music_control_y();let seek_y=controls-38.0;
        let has=self.has_live_music();let playable=self.can_play_music();let with_lyrics=has&&self.show_lyrics;
        let art_size=if with_lyrics{74.0}else{90.0};let art_y=bounds.top();
        let np=self.music_display();let has_display=!np.title.is_empty();
        if let Some((image,texture))=&self.music_artwork{
            let [w,h]=image.size;let uv=Rect::from_center_size(Pos2::new(0.5,0.5),if w>h{egui::vec2(h as f32/w as f32,1.0)}else{egui::vec2(1.0,w as f32/h as f32)});
            c.painter.add(egui::epaint::RectShape::filled(Rect::from_min_size(c.pos(44.0,art_y),egui::Vec2::splat(art_size*c.s)),12.0*c.s,Color32::WHITE).with_texture(texture.id(),uv));
        }else{
            let (a,b)=if has_display{cover_colors(&np.title)}else{(theme::CARD,theme::BTN)};
            c.vgrad(44.0,art_y,art_size,art_size,12.0,a,b);paint::note(c,44.0+art_size/2.0,art_y+art_size/2.0,art_size*0.4,theme::WHITE);
        }
        let text_x=if with_lyrics{130.0}else{150.0};let text_width=if with_lyrics{92.0}else{225.0};
        let (empty,hint)=self.qq_music_empty_state();
        c.centered_text(text_x,art_y+28.0,&c.fit(if has_display{&np.title}else{empty},14.0,W::Semibold,text_width),14.0,W::Semibold,theme::WHITE,H::Left);
        c.centered_text(text_x,art_y+51.0,&c.fit(if has_display{&np.artist}else{hint},11.5,W::Regular,text_width),11.5,W::Regular,theme::GRAY,H::Left);
        if with_lyrics{self.draw_music_lyrics(&c.shifted(0.0,20.0));}
        let position=np.position;let duration=np.duration;
        let seek=c.hit_drag(44.0,seek_y-9.0,331.0,22.0,"m-seek").on_hover_text("拖动播放进度");
        seek.widget_info(||egui::WidgetInfo::slider(c.live&&has,position,"播放进度"));
        let target=if has&&duration>0.0&&(seek.is_pointer_button_down_on()||seek.clicked()||seek.drag_stopped()){
            seek.interact_pointer_pos().map(|point|((c.to_local(point).x-48.0)/323.0).clamp(0.0,1.0) as f64*duration)
        }else{None};
        let current=target.unwrap_or(position);let fraction=if duration>0.0{(current/duration).clamp(0.0,1.0)as f32}else{0.0};
        c.fill(48.0,seek_y-2.0,323.0,4.0,2.0,theme::TRACK);
        if self.settings.skin==crate::model::Skin::Fluid{
            let head=48.0+323.0*fraction;let hot=if seek.is_pointer_button_down_on(){1.0}else if self.music_playing(){0.62}else{0.45};
            if has&&fraction>0.0{c.fill(48.0,seek_y-2.0,323.0*fraction,4.0,2.0,self.fluid.tint_color(0.78));paint::chase(c,48.0,head,seek_y,4.0,self.fluid.tint_color(0.30),hot);}
            if c.live{let point=c.pos(head,seek_y)-self.fluid_origin;self.fluid.set_seek((has&&fraction>0.0).then_some((point.x,point.y)),hot);}
        }else if fraction>0.0{c.fill(48.0,seek_y-2.0,323.0*fraction,4.0,2.0,theme::CYAN);}
        if has&&(seek.hovered()||seek.dragged()){c.circle(48.0+323.0*fraction,seek_y,4.5,theme::WHITE);}
        if seek.has_focus(){c.stroke(44.0,seek_y-9.0,331.0,22.0,7.0,1.3,theme::CYAN);}
        c.centered_text(48.0,seek_y+17.0,&mmss(current),10.5,W::Regular,theme::GRAY,H::Left);
        c.centered_text(371.0,seek_y+17.0,&if has&&duration<=0.0{"--:--".into()}else{format!("-{}",mmss((duration-current).max(0.0)))},10.5,W::Regular,theme::GRAY,H::Right);
        if seek.clicked()||seek.drag_stopped(){if let Some(target)=target{cmds.push(Cmd::Media(Transport::Seek(target)));}}
        let (play_glyph,play_label)=self.music_play_button();
        for (x,glyph,name,id,command) in [(162.0,"previous","上一首","m-prev",Some(Transport::Prev)),(210.5,play_glyph,play_label,"m-play",None),(258.0,"next","下一首","m-next",Some(Transport::Next))]{
            let enabled=if command.is_none(){playable}else{has};
            let response=c.hit(x-16.0,controls-14.0,32.0,28.0,id).on_hover_text(name);
            response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live&&enabled,name));
            let (hover,press)=c.hover_t(&response);
            if hover>0.001||press>0.001{c.fill(x-16.0,controls-14.0,32.0,28.0,8.0,mix(theme::BTN,theme::BTN_HOVER,hover.max(press)));}
            if response.has_focus(){c.stroke(x-16.0,controls-14.0,32.0,28.0,8.0,1.3,theme::CYAN);}
            paint::glyph(c,glyph,x,controls,if id=="m-play"{22.0}else{18.0},if enabled{theme::WHITE}else{theme::DIM});
            if enabled&&response.clicked(){if let Some(command)=command{cmds.push(Cmd::Media(command));}else{self.toggle_music_playback(cmds);}}
        }
        let lyric=c.hit(60.0,controls-14.0,28.0,28.0,"m-lyr").on_hover_text(if self.show_lyrics{"隐藏歌词"}else{"显示歌词"});
        c.centered_text(74.0,controls,"词",13.0,W::Semibold,if self.show_lyrics{theme::WHITE}else{theme::GRAY},H::Center);
        if lyric.clicked()&&has{self.show_lyrics=!self.show_lyrics;self.settings.lyrics_enabled=self.show_lyrics;self.settings.save();}
        if has{
            let (button,card)=capsule(c,279.0,controls-10.0,68.0,20.0,"m-island-lyrics",theme::BTN,theme::BTN_HOVER);
            card.centered_text(313.0,controls,"岛上歌词",10.0,W::Semibold,if self.settings.island_lyrics{theme::WHITE}else{theme::GRAY},H::Center);
            if button.clicked(){self.settings.island_lyrics=!self.settings.island_lyrics;self.settings.save();}
        }
        let volume=c.hit(354.0,controls-14.0,26.0,28.0,"m-volume").on_hover_text("音乐应用音量 · 点击展开，滚轮调整");
        volume.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live&&has,"音乐音量"));
        paint::speaker(c,367.0,controls,self.music_volume_ui.is_some_and(|(_,muted)|muted),if self.music_volume_ui.is_some(){theme::WHITE}else{theme::GRAY});
        if has&&self.music_volume_ui.is_some(){
            if volume.clicked(){self.music_volume_open=!self.music_volume_open;}
            if volume.hovered(){let delta=c.ui.input(|i|i.raw_scroll_delta.y);if delta.abs()>0.5{let value=self.music_volume_ui.unwrap().0;self.change_music_volume(Transport::SetVolume((value as f32+delta/120.0*4.0).round().clamp(0.0,100.0)as u32),cmds);}}
        }
        #[cfg(target_os="macos")]
        for (x,name,player,id) in [(44.0,"我的歌单",false,"m-qq-library"),(216.0,"播放器…",true,"m-qq-player")]{
            if crate::portrait::button(c,x,bounds.bottom()-28.0,159.0,name,id){if player{cmds.push(Cmd::QqMusic(crate::qq_music::Page::Player));}else{self.open_qq_library(cmds);}}
        }
    }

    pub(crate) fn change_music_volume(&mut self,command:Transport,cmds:&mut Vec<Cmd>){
        let Some((volume,_))=self.music_volume_ui else{return;};
        let next=match command{Transport::SetVolume(v)=>(v,false),Transport::SetMute(muted)=>(volume,muted),_=>return};
        if self.music_volume_ui==Some(next){return;}
        self.music_volume_ui=Some(next);
        self.music_volume_quiet_until=self.t+0.9;
        cmds.push(Cmd::MusicVolume(command,self.media.source.clone()));
    }

    pub(crate) fn draw_music_volume(&mut self,c:&Card,cmds:&mut Vec<Cmd>){
        let shifted=c.shifted(0.0,self.music_control_y()-118.0);let c=&shifted;
        let ctx=c.ui.ctx();
        let bounds=MUSIC_VOLUME_RECT;
        let panel=c.rect(bounds.min.x,bounds.min.y,bounds.width(),bounds.height());
        let trigger=c.rect(354.0,106.0,26.0,24.0);
        if self.music_volume_open && ctx.input(|i|i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p|!panel.contains(p) && !trigger.contains(p))){
            self.music_volume_open=false;
        }
        let a=ctx.animate_bool_with_time(egui::Id::new("music-volume-open"),self.music_volume_open,0.12);
        let Some((volume,muted))=self.music_volume_ui else{return;};
        if a<0.01{return;}
        let popup=c.faded(a);
        let popup=if self.music_volume_open{popup}else{popup.ghost()};
        // 浮条最后绘制并占据整块命中区域，防止点击穿到后面的翻页按钮。
        popup.hit(bounds.min.x,bounds.min.y,bounds.width(),bounds.height(),"m-volume-surface");
        popup.fill(bounds.min.x,bounds.min.y,bounds.width(),bounds.height(),12.0,Color32::from_gray(36));
        popup.stroke(bounds.min.x,bounds.min.y,bounds.width(),bounds.height(),12.0,1.0,Color32::from_gray(60));
        let track=popup.hit_drag(351.0,27.0,32.0,54.0,"m-volume-track");
        let mut command=None;
        if popup.live && (track.clicked() || track.dragged() || track.is_pointer_button_down_on()) {
            if let Some(pointer)=track.interact_pointer_pos(){
                command=Some(Transport::SetVolume(((75.0-popup.to_local(pointer).y)/43.0*100.0).round().clamp(0.0,100.0) as u32));
            }
        }else if popup.live && panel.contains(ctx.pointer_hover_pos().unwrap_or(Pos2::ZERO)) {
            let delta=ctx.input(|i|i.raw_scroll_delta.y);
            if delta.abs()>0.5{command=Some(Transport::SetVolume((volume as f32+delta/120.0*4.0).round().clamp(0.0,100.0) as u32));}
        }
        let mute=popup.hit(353.0,82.0,28.0,20.0,"m-volume-mute").on_hover_text(if muted{"恢复音乐音量"}else{"静音音乐，保留当前音量"});
        let (hover,press)=popup.hover_t(&mute);
        if popup.live && mute.clicked(){command=Some(Transport::SetMute(!muted));}
        if let Some(command)=command {self.change_music_volume(command,cmds);}
        let (volume,muted)=self.music_volume_ui.unwrap_or((volume,muted));
        let ink=if muted{theme::GRAY}else{theme::WHITE};
        popup.centered_text(367.0,20.0,&format!("{volume}%"),10.5,W::Semibold,ink,H::Center);
        popup.fill(364.5,32.0,5.0,43.0,2.5,Color32::from_gray(68));
        let head=75.0-43.0*volume as f32/100.0;
        if volume>0 {popup.fill(364.5,head,5.0,(75.0-head).max(2.5),2.5,ink);}
        popup.circle(367.0,head,if track.dragged(){5.5}else{4.5},ink);
        popup.fill(353.0,82.0,28.0,20.0,6.0,paint::mix(theme::BTN,theme::BTN_HOVER,if muted{1.0}else{hover}));
        popup.scaled(367.0,92.0,1.0-0.06*press).centered_text(367.0,92.0,if muted{"恢复"}else{"静音"},9.5,W::Semibold,theme::WHITE,H::Center);
    }

    fn draw_music_lyrics(&mut self,c:&Card) {
        if self.draw_qq_lyrics(c){return;}
        let col=c.clipped(243.0,28.0,136.0,74.0);
        c.seg((233.0,32.0),(233.0,98.0),1.0,Color32::from_gray(35));
        if let Some(lyrics)=&self.ly.lyrics {
            let current=if lyrics.synced {lyrics.index_at(self.lyric_position())}else{Some(0)};
            let target=current.unwrap_or(0);
            let id=egui::Id::new("music-lyrics-scroll");
            if !self.ly.scroll_animation_started {
                c.ui.ctx().animate_value_with_time(id,target as f32,0.0);
                self.ly.scroll_animation_started=true;
            }
            let position=c.ui.ctx().animate_value_with_time(id,target as f32,0.3);
            let first=(position.floor() as usize).saturating_sub(2);
            for (index,line) in lyrics.lines.iter().enumerate().skip(first).take(6) {
                let distance=index as f32-position;
                if distance.abs()>2.2 {continue;}
                let active=current==Some(index);
                let ink=if active {theme::WHITE}else{theme::GRAY};
                let row=col.faded((1.0-distance.abs()*0.35).clamp(0.0,1.0));
                let mut job=egui::text::LayoutJob::simple(line.text.clone(),row.font(if active {12.0}else{10.0},if active {W::Semibold}else{W::Regular}),ink,132.0*c.s);
                job.wrap.max_rows=if active {3}else{1};
                job.wrap.break_anywhere=true;
                let galley=row.painter.layout_job(job);
                let y=64.0+distance*29.0-galley.size().y/(2.0*c.s);
                row.painter.galley(row.pos(246.0,y),galley,ink);
            }
            if !lyrics.synced {col.text(246.0,39.0,"文本歌词",8.0,W::Regular,theme::GRAY,H::Left);}
        } else {
            let status=if self.ly.loading {"正在查找歌词…"}else if let Some(error)=&self.ly.error {error.label()}else{"暂无歌词"};
            let galley=col.painter.layout(status.into(),col.font(10.0,W::Regular),theme::GRAY,130.0*c.s);
            col.painter.galley(col.pos(246.0,61.0-galley.size().y/(2.0*c.s)),galley,theme::GRAY);
            if self.ly.error.is_some() {
                col.text(246.0,87.0,"点击重试",9.0,W::Regular,theme::GRAY,H::Left);
                if c.hit(243.0,28.0,136.0,74.0,"lyrics-retry").on_hover_text("重新查询当前歌曲的歌词").clicked(){self.ly.key.clear();self.ly.manual_retry=true;}
            }
        }
    }

    // ================= 置物架 =================
    pub(crate) fn shelf_grid_overflows(&self)->bool{
        let rows=self.shelf.items.len().div_ceil(5);
        let height=rows as f32*78.0+rows.saturating_sub(1) as f32*4.0;
        height>self.content_rect().height()-25.0
    }

    pub(crate) fn page_shelf(&mut self,c:&Card,_env:&Env,cmds:&mut Vec<Cmd>){
        let bounds=c.layout_bounds(self.content_rect());let items=self.shelf.items.clone();self.icons.retain(&items);
        let add=c.hit(311.0,4.0,38.0,28.0,"shelf-add").on_hover_text("添加文件");
        add.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,"添加文件"));
        c.centered_text(330.0,18.0,"添加",11.0,W::Semibold,theme::GRAY,H::Center);
        if add.clicked(){cmds.push(Cmd::AddFiles);}
        let grid_h=bounds.height()-25.0;
        if items.is_empty(){crate::portrait::empty(c,bounds,"shelf","把文件拖到这里","只记位置，保留原文件");}
        else{
            let mut content=crate::portrait::scroll_ui(c,bounds.left(),bounds.top(),bounds.width(),grid_h,"shelf-grid");
            content.spacing_mut().item_spacing.y=4.0*c.s;
            egui::ScrollArea::vertical().id_salt("shelf-grid").auto_shrink([false,false]).show(&mut content,|ui|{
                let gap=4.0;let cell=(ui.available_width()/c.s-gap*4.0)/5.0;
                for chunk in items.chunks(5){
                    let (rect,_)=ui.allocate_exact_size(egui::vec2((cell*5.0+gap*4.0)*c.s,78.0*c.s),egui::Sense::hover());
                    for (column,path) in chunk.iter().enumerate(){
                        let mut row=Card::new(ui,rect.min+egui::vec2(column as f32*(cell+gap)*c.s,0.0),ui.clip_rect());row.s=c.s;row.live=c.live;
                        let action=row.hit_drag(0.0,0.0,cell,78.0,("shelf",path)).on_hover_text(path.display().to_string());
                        let name=path.file_name().map(|name|name.to_string_lossy().into_owned()).unwrap_or_default();
                        action.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,format!("打开 {name}")));
                        if action.hovered()||action.has_focus(){row.fill(0.0,0.0,cell,78.0,8.0,theme::BTN_HOVER);}
                        if action.has_focus(){row.stroke(0.5,0.5,cell-1.0,77.0,8.0,1.3,theme::CYAN);}
                        self.file_icon_at(&row,path,cell/2.0,25.0,34.0);
                        row.centered_text(cell/2.0,53.0,&row.fit(&name,10.5,W::Semibold,cell-4.0),10.5,W::Semibold,theme::WHITE,H::Center);
                        let meta=self.icons.get(c.ui.ctx(),path).map(|preview|preview.metadata).unwrap_or_default();
                        row.centered_text(cell/2.0,69.0,&row.fit(&meta,10.0,W::Regular,cell-4.0),10.0,W::Regular,theme::GRAY,H::Center);
                        let remove=row.hit(cell-21.0,0.0,21.0,21.0,("shelf-remove",path)).on_hover_text("移除记录，保留原文件");
                        if action.hovered()||remove.hovered()||remove.has_focus(){row.circle(cell-10.5,10.5,8.0,theme::BTN);paint::close(&row,cell-10.5,10.5,2.5,theme::WHITE);}
                        let mut delete=remove.clicked();
                        if action.drag_started()&&path.exists(){cmds.push(Cmd::DragFile(path.clone()));}
                        action.context_menu(|ui|{
                            if ui.button("打开").clicked(){cmds.push(Cmd::Open(path.clone()));ui.close_menu();}
                            if ui.button("在文件夹中显示").clicked(){cmds.push(Cmd::Reveal(path.clone()));ui.close_menu();}
                            if ui.button("复制完整路径").clicked(){cmds.push(Cmd::Copy(path.display().to_string()));ui.close_menu();}
                            if ui.button("从置物架移除").clicked(){delete=true;ui.close_menu();}
                            self.shelf_menu_rect=Some(egui::Frame::menu(ui.style()).widget_rect(ui.min_rect()));
                        });
                        if delete{self.shelf.remove(path);self.icons.remove(path);self.shelf_x.remove(path);self.shelf_born.remove(path);}
                        else if action.clicked(){if path.exists(){cmds.push(Cmd::Open(path.clone()));}else{self.say("文件已移动或删除，可移除此条记录");}}
                    }
                }
            });
            c.centered_text(bounds.left(),bounds.bottom()-10.0,&format!("{} 个文件",items.len()),10.5,W::Regular,theme::GRAY,H::Left);
            let confirming=self.confirm_clear.is_some();
            let (clear,card)=capsule(c,bounds.right()-62.0,bounds.bottom()-22.0,62.0,22.0,"shelf-clear",theme::BTN,theme::BTN_HOVER);
            card.centered_text(bounds.right()-31.0,bounds.bottom()-11.0,if confirming{"确认清空"}else{"清空"},10.5,W::Semibold,theme::WHITE,H::Center);
            if clear.clicked(){if confirming{self.shelf.clear();self.icons.clear();self.shelf_x.clear();self.shelf_born.clear();self.confirm_clear=None;}else{self.confirm_clear=Some(self.t);}}
        }
        if self.drag_a>0.002{self.drag_overlay(&c.faded(self.drag_a));}
    }

    // ================= 独立粘贴板 =================
    pub(crate) fn page_clipboard(&mut self, c: &Card, _env: &Env, _cmds: &mut Vec<Cmd>) {
        let bounds=c.layout_bounds(self.content_rect());
        if !self.settings.clipboard_history {
            crate::portrait::empty(c,bounds,"clipboard","粘贴板记录已关闭","开启后保留复制的文字、图片和视频");
            let (r,bc)=capsule(c,179.5,bounds.center().y+54.0,60.0,24.0,"clip-enable",theme::BTN,theme::BTN_HOVER);
            bc.centered_text(209.5,bounds.center().y+66.0,"开启",11.0,W::Semibold,theme::WHITE,H::Center);
            if r.clicked() { self.settings.clipboard_history = true; self.clipboard.set_enabled(true); self.settings.save(); }
            return;
        }
        let items = self.clipboard.lock().items.clone();
        let n = items.len();
        self.clip_icons.retain(&items.iter().filter_map(|clip|clip.media.as_ref().map(|media|media.path.clone())).collect::<Vec<_>>());
        let recent = items.iter().filter(|clip| !clip.pinned).count();
        self.clip_first = self.clip_first.min(n.saturating_sub(CLIP_ROWS));

        // 右上：条数与「清空」（只清未固定的）。
        if recent > 0 {
            let confirming = self.clip_confirm_clear.is_some_and(|at| self.t - at < 3.0);
            if !confirming { self.clip_confirm_clear = None; }
            let label = if confirming { "确认" } else { "清空" };
            let lw = c.text_w("清空", size::SMALL, W::Regular) + 16.0;
            let base = if confirming { Color32::from_rgb(120, 28, 36) } else { theme::BTN };
            let hover = if confirming { Color32::from_rgb(150, 34, 44) } else { theme::BTN_HOVER };
            let x = 351.0 - lw;
            let (r, bc) = capsule(c, x, 11.5, lw, 17.0, "clip-clear", base, hover);
            bc.text(x + lw / 2.0, 23.5, label, size::SMALL, W::Regular, theme::WHITE, H::Center);
            let r = r.on_hover_text(if confirming { "再点一次清空未固定的记录" } else { "清空未固定的记录" });
            if r.clicked() {
                if confirming {
                    self.clipboard.lock().clear_recent();
                    self.clip_first = 0;
                    self.clip_confirm_clear = None;
                    self.say("粘贴板记录已清空（固定的保留）");
                } else {
                    self.clip_confirm_clear = Some(self.t);
                }
            }
        }

        if n==0{crate::portrait::empty(c,bounds,"clipboard","复制的内容会出现在这里","文字、图片与视频记录保存在本机");return;}
        c.fill(42.0, bounds.top(), 335.0, bounds.height(), 12.0, theme::CARD);
        // 超过一屏时右侧细滚动条，滚轮查看。
        if n > CLIP_ROWS {
            let (top,track)=(bounds.top()+7.0,bounds.height()-14.0);
            let thumb = (track * CLIP_ROWS as f32 / n as f32).max(12.0);
            let y = top + (track - thumb) * self.clip_first as f32 / (n - CLIP_ROWS) as f32;
            c.fill(371.0, top, 2.5, track, 1.25, Color32::from_white_alpha(14));
            c.fill(371.0, y, 2.5, thumb, 1.25, Color32::from_white_alpha(70));
        }
        let mut action: Option<(usize, u8)> = None; // 0 复制 1 固定 2 删除
        for (row, (index, clip)) in items.iter().enumerate().skip(self.clip_first).take(CLIP_ROWS).enumerate() {
            let y = bounds.top()+13.0 + row as f32 * 22.5;
            let r = c.hit(48.0, y - 10.0, 323.0, 20.0, ("clip", index, clip.text.len(), clip.at.as_str()));
            let (h, _) = c.hover_t(&r);
            if h > 0.01 {
                c.fill(48.0, y - 10.0, 323.0, 20.0, 7.0, Color32::from_white_alpha((12.0 * h) as u8));
            }
            self.clipboard_preview(c, clip, 60.0, y, 18.0);
            if clip.pinned { paint::pin(c, 73.0, y - 5.0, 6.5, theme::BLUE); }
            let text = c.fit(&clip.preview(), size::BODY, W::Regular, 198.0);
            c.text(78.0, y + 3.8, &text, size::BODY, W::Regular, Color32::from_gray(235), H::Left);
            c.text(318.0, y + 3.5, &clip.at, size::TINY, W::Regular, theme::DIM, H::Right);
            r.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,format!("复制{}",clip.preview())));
            if r.has_focus(){c.stroke(48.0,y-10.0,273.0,20.0,7.0,1.2,theme::CYAN);}
            let tip=clip.preview().chars().take(400).collect::<String>();
            let r = r.on_hover_text(format!("{tip}\n{}\n点击复制 · 右键查看更多",clip.description()));
            r.context_menu(|ui|{
                if ui.button("复制").clicked(){action=Some((index,0));ui.close_menu();}
                if let Some(media)=&clip.media{
                    if ui.button(if media.kind==crate::clipboard::MediaKind::Video{"播放视频"}else{"打开图片"}).clicked(){_cmds.push(Cmd::Open(media.path.clone()));ui.close_menu();}
                    if ui.button("在 Finder 中显示").clicked(){_cmds.push(Cmd::Reveal(media.path.clone()));ui.close_menu();}
                }
                if ui.button(if clip.pinned{"取消固定"}else{"固定记录"}).clicked(){action=Some((index,1));ui.close_menu();}
                if ui.button("删除记录").clicked(){action=Some((index,2));ui.close_menu();}
                self.shelf_menu_rect=Some(egui::Frame::menu(ui.style()).widget_rect(ui.min_rect()));
            });
            let pin = c.hit(322.0, y - 10.0, 20.0, 20.0, ("clip-pin", index)).on_hover_text(if clip.pinned { "取消固定" } else { "固定（清空时保留）" });
            paint::pin(c, 332.0, y, 9.0, if clip.pinned || pin.hovered() { theme::BLUE } else { theme::GRAY });
            let delete = c.hit(344.0, y - 10.0, 20.0, 20.0, ("clip-delete", index)).on_hover_text("删除这条记录");
            paint::close(c, 353.5, y, 2.4, theme::GRAY);
            if delete.clicked() { action = Some((index, 2)); }
            else if pin.clicked() { action = Some((index, 1)); }
            else if r.clicked() { action = Some((index, 0)); }
        }
        match action {
            Some((index, 0)) => self.copy_clip_record(&items[index], _cmds),
            Some((index, 1)) => {
                if !self.clipboard.lock().toggle_pin_clip(&items[index]) { self.say(format!("最多固定 {} 条", crate::clipboard::MAX_PINNED)); }
            }
            Some((index, _)) => { self.clipboard.lock().remove_clip(&items[index]); }
            None => {}
        }
    }

    pub(crate) fn copy_clip_record(&mut self, clip: &crate::clipboard::Clip, cmds: &mut Vec<Cmd>) {
        #[cfg(any(windows, target_os = "macos"))]
        { let _=cmds; self.say(if crate::clipboard::copy_clip(clip) { "已复制" } else if clip.media.is_some() { "复制失败，记录文件可能已被移除" } else { "复制失败，请重试" }); }
        #[cfg(not(any(windows, target_os = "macos")))]
        if clip.media.is_none(){cmds.push(Cmd::Copy(clip.text.clone()));self.say("已复制");}else{self.say("此平台暂不支持复制媒体");}
    }

    pub(crate) fn clipboard_preview(&mut self, c: &Card, clip: &crate::clipboard::Clip, cx:f32, cy:f32, size:f32) {
        if let Some(media)=&clip.media {
            c.fill(cx-size/2.0,cy-size/2.0,size,size,4.0,theme::BTN);
            if let Some(texture)=self.clip_icons.get(c.ui.ctx(),&media.path).and_then(|preview|preview.image){
                let dimensions=texture.size_vec2();let scale=size/dimensions.x.max(dimensions.y);
                c.painter.image(texture.id(),Rect::from_center_size(c.pos(cx,cy),dimensions*scale*c.s),Rect::from_min_max(Pos2::ZERO,Pos2::new(1.0,1.0)),Color32::WHITE);
            }else{paint::file_glyph(c,paint::file_kind(&media.path),cx,cy,size*0.8);}
            if media.kind==crate::clipboard::MediaKind::Video {
                c.circle(cx,cy,size*0.20,Color32::from_black_alpha(180));paint::glyph(c,"play",cx,cy,size*0.24,Color32::WHITE);
            }
        } else {paint::glyph(c,"clipboard",cx,cy,size*0.75,theme::GRAY);}
    }

    pub(crate) fn file_icon_at(&mut self, c: &Card, p: &std::path::Path, cx: f32, cy: f32, sz: f32) {
        let tex=self.icons.get(c.ui.ctx(),p).and_then(|preview|preview.image);
        if let Some(tex) = tex {
            let s = tex.size_vec2();
            let k = sz / s.x.max(s.y);
            let r = Rect::from_center_size(c.pos(cx, cy), s * k * c.s);
            c.painter.image(tex.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        } else {
            paint::file_glyph(c, paint::file_kind(p), cx, cy, sz);
        }
    }

    /// 拖入遮罩：流动虚线框 + 跟随光标的柔光 + 上下轻跳的托盘图标。
    fn drag_overlay(&self, c: &Card) {
        let bounds=c.layout_bounds(self.content_rect());let (x,y,w,h)=(bounds.left(),bounds.top(),bounds.width(),bounds.height());
        c.fill(x, y, w, h, 12.0, Color32::from_black_alpha(110));
        if let Some(p) = self.cursor_local {
            let px = p.x.clamp(x + 20.0, x + w - 20.0);
            let py = p.y.clamp(y + 10.0, y + h - 10.0);
            c.clipped(x, y, w, h).glow(px, py, 90.0, Color32::from_rgba_unmultiplied(120, 170, 255, 34));
        }
        c.dashed(x, y, w, h, 12.0, 1.4, Color32::from_white_alpha(230), self.t as f32 * 14.0);
        let bob = ((self.t * 5.0).sin() as f32 * 0.5 + 0.5) * 3.0;
        paint::tray(c,209.5,y+h/2.0-16.0,30.0,bob, theme::WHITE, Color32::from_gray(90));
        c.text(209.5,y+h/2.0+28.0,"松手放进置物架", 12.0, W::Semibold, theme::WHITE, H::Center);
    }

    // ================= 待办 =================
    pub(crate) fn page_todo(&mut self, c: &Card, env: &Env, _cmds: &mut Vec<Cmd>) {
        // 今天未完成优先，已完成沉底；不在今天的重复待办变暗显示、不能勾选。
        let (done, total) = self.todos.progress(&env.today);
        let order = self.todos.display_order(&env.today);
        let count = order.len();
        let pop = self.todo_pop.pos.abs().min(0.4);
        let hc = c.scaled(60.0, 50.0, 1.0 + pop * 0.25);
        let w = hc.text(42.0, 61.0, &format!("{done}"), size::HERO, W::Bold, if done > 0 && done == total { theme::GREEN } else { theme::WHITE }, H::Left);
        hc.text(42.0 + w, 61.0, &format!("/{total}"), size::HERO, W::Bold, theme::WHITE, H::Left);
        c.text(42.0, 78.0, "已完成", size::SMALL, W::Regular, theme::GRAY, H::Left);

        let (add, ac) = capsule(c, 41.0, 96.0, 74.0, 26.0, "t-add", theme::BTN, theme::BTN_HOVER);
        paint::plus(&ac, 56.0, 109.0, 6.2, theme::WHITE, Color32::BLACK);
        ac.text(67.0, 112.5, "加一个", size::BODY, W::Regular, theme::WHITE, H::Left);
        let can_edit = !self.todos.items.is_empty() && !self.adding && self.editing.is_none();
        let ec = if can_edit { c.clone() } else { c.faded(0.45) };
        let (edit, edc) = capsule(&ec, 121.0, 96.0, 68.0, 26.0, "t-edit", theme::BTN, if can_edit { theme::BTN_HOVER } else { theme::BTN });
        if self.todo_choosing{paint::close(&edc,135.0,109.0,3.5,theme::WHITE);}else{paint::pencil(&edc,135.0,109.0,11.0,theme::WHITE);}
        edc.text(145.0, 112.5, if self.todo_choosing{"取消"}else{"改一下"}, size::BODY, W::Regular, theme::WHITE, H::Left);

        c.fill(196.0,31.0,181.0,self.content_rect().bottom()-31.0, 12.0, theme::CARD);
        let editing = self.adding || self.editing.is_some();
        if count == 0 && !editing {
            paint::checklist(c, 286.5, 64.0, 32.0, Color32::from_gray(150), Color32::from_rgb(52, 54, 58));
            c.text(286.5, 99.0, "今天还没有安排", size::SMALL, W::Regular, theme::GRAY, H::Center);
        } else {
            let mut row = 0;
            let row_y = |r: usize| 46.0 + r as f32 * 22.0;
            if editing {
                // 输入行（文本框本身由 app::input_field 放置）
                let y = row_y(0);
                c.fill(203.0, y - 10.0, 167.0, 20.0, 7.0, Color32::from_rgb(34, 34, 38));
                c.stroke(203.0, y - 10.0, 167.0, 20.0, 7.0, 1.0, paint::alpha(theme::BLUE, 0.7));
                if self.editing.is_some() {
                    paint::pencil(c, 212.0, y, 9.0, theme::BLUE);
                } else {
                    paint::plus(c, 212.0, y, 5.0, theme::BLUE, Color32::BLACK);
                }
                row = 1;
            }
            let ctx = c.ui.ctx().clone();
            let capacity=((self.content_rect().bottom()-51.0)/22.0).floor()as usize;let cap=capacity.saturating_sub(usize::from(editing));
            let mut shown = 0;
            for &i in order.iter().skip(self.todo_first.min(count.saturating_sub(1))) {
                if shown >= cap {
                    break;
                }
                if Some(i) == self.editing {
                    continue;
                }
                let item = self.todos.items[i].clone();
                let today_item = item.scheduled_on(&env.today);
                let y = row_y(row);
                let r = c.hit(202.0, y - 10.0, 169.0, 20.0, ("todo", i, item.text.as_str()));
                let (h, _) = c.hover_t(&r);
                if h > 0.01 {
                    c.fill(202.0, y - 10.0, 169.0, 20.0, 7.0, Color32::from_white_alpha((12.0 * h) as u8));
                }
                let checked = item.done_for(&env.today);
                let ct = ctx.animate_bool_with_time(egui::Id::new(("todo-check", item.text.as_str())), checked, 0.28);
                let rc = if today_item { c.clone() } else { c.faded(0.45) };
                if self.todo_choosing{paint::pencil(c,212.0,y,10.0,theme::BLUE);}else{
                    rc.ring(212.0, y, 5.2, 1.3, paint::alpha(Color32::from_gray(120), 1.0 - ct));
                    if ct > 0.0 {
                        c.circle(212.0, y, 6.0 * anim::ease_out_cubic(ct.min(1.0) / 0.6).min(1.0), theme::GREEN);
                        paint::check(c, 212.0, y, 6.0, smoothstep(0.3, 1.0, ct), 1.4, Color32::BLACK);
                    }
                }
                // 每周重复：文字右侧小标签（周一三五 / 工作日 / 每天）。
                let tag = crate::model::repeat_tag(item.repeat);
                let tag_w = if tag.is_empty() { 0.0 } else { c.text_w(&tag, size::TINY, W::Regular) + 6.0 };
                let text = c.fit(&item.text, size::BODY, W::Regular, 104.0 - tag_w);
                let repeat_hint = if item.is_repeating() { format!("\n{}{}", crate::model::repeat_label(item.repeat), if today_item { "" } else { " · 今天不用做" }) } else { String::new() };
                r.clone().on_hover_text(format!("{}{}{}",item.text,repeat_hint,item.reminder_text().map(|d|format!("\n提醒：{d}")).unwrap_or_default()));
                let tcol = mix(Color32::from_gray(235), Color32::from_gray(110), ct);
                let tw = rc.text(223.0, y + 3.8, &text, size::BODY, W::Regular, tcol, H::Left);
                if !tag.is_empty() {
                    rc.text(223.0 + tw + 5.0, y + 3.2, &tag, size::TINY, W::Regular, if today_item { theme::BLUE } else { theme::GRAY }, H::Left);
                }
                if ct > 0.0 {
                    c.fill(223.0, y + 0.2, tw * smoothstep(0.2, 1.0, ct), 1.0, 0.5, Color32::from_gray(110));
                }
                if self.todo_choosing{
                    c.text(366.0,y+3.8,"修改",size::TINY,W::Regular,theme::BLUE,H::Right);
                    if r.clicked(){self.todo_choosing=false;self.editing=Some(i);self.adding=false;self.buf=item.text.clone();self.todo_input_focused=false;break;}
                    row+=1;shown+=1;continue;
                }
                let delete=c.hit(353.0,y-9.0,17.0,18.0,("todo-delete",i)).on_hover_text("删除待办").clicked();
                paint::close(c,361.5,y,2.4,theme::GRAY);
                let reminder=c.hit(333.0,y-9.0,18.0,18.0,("todo-reminder",i)).on_hover_text(item.reminder_text().map(|due|format!("提醒：{due} · 点击修改")).unwrap_or_else(||"设置提醒与每周重复（可选）".into()));
                let reminder_clicked=reminder.clicked();
                let ink=if item.due.is_some()||reminder.hovered(){theme::BLUE}else{theme::GRAY};
                c.ring(342.0,y,4.7,1.1,ink);
                c.path(&[(342.0,y-2.5),(342.0,y),(344.3,y+1.0)],1.1,ink);
                if reminder_clicked {self.open_task_reminder(i);}
                let mut edit_this=false;
                r.context_menu(|ui|{
                    if ui.button("编辑与提醒…").clicked(){edit_this=true;ui.close_menu();}
                    if ui.button("取消提醒").clicked(){self.todos.items[i].due=None;self.todos.save();ui.close_menu();}
                });
                if edit_this {self.open_task_dialog(i);}
                if delete {
                    self.todos.items.remove(i);
                    if let Some(index)=self.editing.as_mut(){if i<*index{*index-=1;}}
                    self.todos.save();self.todo_first=self.todo_first.min(self.todos.items.len().saturating_sub(3));break;
                }
                if r.clicked() && !edit_this && !reminder_clicked && !today_item {
                    self.say(format!("{}的待办，今天不用做", crate::model::repeat_label(item.repeat)));
                } else if r.clicked() && !r.double_clicked() && !edit_this && !reminder_clicked {
                    let now = self.todos.items[i].done_for(&env.today);
                    self.todos.items[i].toggle(&env.today);
                    self.todos.save();
                    if !now {
                        self.todo_pop.vel += 5.0;
                    }
                }
                row += 1;
                shown += 1;
            }
            let rest = count.saturating_sub(shown + self.editing.map(|_|1).unwrap_or(0));
            if self.todo_choosing{
                c.text(370.0,120.0,if rest>0{"点选修改 · 滚轮查看"}else{"点击要修改的事项"},size::TINY,W::Regular,theme::GRAY,H::Right);
            }else if rest > 0 {
                c.text(370.0, 120.0, &format!("共 {count} 项 · 滚轮查看"), size::TINY, W::Regular, theme::DIM, H::Right);
            }
        }

        if add.clicked() {
            self.todo_choosing=false;
            self.todo_first=0;
            self.adding = true;
            self.editing = None;
            self.buf.clear();
            self.todo_input_focused=false;
        }
        if edit.clicked() && can_edit {
            if count==1 {
                let i=order[0];
                self.editing = Some(i);
                self.adding = false;
                self.buf = self.todos.items[i].text.clone();
                self.todo_input_focused=false;
            }else{self.todo_choosing=!self.todo_choosing;if self.todo_choosing{self.pending.push(Cmd::Focus);}}
        }
    }

    // ---------------------------------------------------------------
    // 邮件：有验证码大卡时分栏，否则显示紧凑单行列表。
    // ---------------------------------------------------------------
    pub(crate) fn visible_mail_code(&self)->Option<&mail::Item> {
        self.mail.code_item().filter(|item|!self.mail.manage.trash_open && self.settings.mail_code && !self.mail_code_hidden(item))
    }
    pub(crate) fn mail_list_bounds(&self)->Rect {
        let left=if self.visible_mail_code().is_some(){182.0}else{36.0};
        Rect::from_min_max(Pos2::new(left,54.0),Pos2::new(380.0,self.content_rect().bottom()-20.0))
    }
    pub(crate) fn page_mail(&mut self, c: &Card, _env: &Env, cmds: &mut Vec<Cmd>) {
        let bounds=c.layout_bounds(self.content_rect());let footer=bounds.bottom()-14.0;
        if self.mail.accounts.is_empty() {
            crate::portrait::empty(c,bounds,"mail","还没有邮箱","点击右上角齿轮添加邮箱");
            return;
        }

        self.mail_manage_tick();
        // v0.5.5：点开某封邮件 → 整块换成正文摘要，左上「返回」回到列表。
        if let Some((account, uid)) = self.mail.open_uid.clone() {
            match self.mail_rows()
                .iter()
                .find(|m| m.uid == uid && m.account.eq_ignore_ascii_case(&account))
                .cloned()
            {
                Some(item) => {
                    self.mail_detail(c, &item, cmds);
                    return;
                }
                None => self.mail.open_uid = None,
            }
        }
        let code=self.visible_mail_code().cloned();
        if let Some(raw)=&code {
            c.fill(36.0,36.0,140.0,self.content_rect().bottom()-56.0,11.0,theme::CARD);
            let hidden=self.mail_code_hidden(raw);
            let item=self.display_mail(raw);
            let value=item.code.as_deref().unwrap_or("••••••");
            c.text(46.0,51.0,"验证码",10.5,W::Regular,theme::GRAY,H::Left);
            c.text(166.0,51.0,&c.fit(&item.from,10.5,W::Semibold,78.0),10.5,W::Semibold,theme::WHITE,H::Right);
            let code_size=23.0*(120.0/c.text_w(value,23.0,W::Bold)).min(1.0);
            c.text(46.0,75.0,value,code_size,W::Bold,theme::GREEN,H::Left);
            c.text(46.0,91.0,&c.fit(&item.subject,10.5,W::Regular,120.0),10.5,W::Regular,Color32::from_gray(190),H::Left);
            c.text(46.0,108.0,if hidden{"已隐藏 · 点击查看"}else{"点击复制"},10.5,W::Regular,theme::DIM,H::Left);
            c.text(166.0,108.0,item.time.rsplit(' ').next().unwrap_or(&item.time),10.0,W::Regular,theme::GRAY,H::Right);
            c.hit(42.0,38.0,128.0,16.0,"mail-code-source").on_hover_text(format!("来自：{}\n{}\n收件邮箱：{}\n{}",item.from,item.subject,item.account,item.time));
            if c.hit(40.0,56.0,132.0,38.0,("mail-code",item.uid)).on_hover_text(format!("复制 {} 的验证码",item.from)).clicked(){if hidden{self.reveal_mail_code(raw);}else{cmds.push(Cmd::Copy(value.to_string()));self.code_copied(value);}}
        }
        use crate::mail_manage::{Action,Folder};
        let folder=self.mail.manage.folder();let trash=folder==Folder::Trash;
        let list_bounds=self.mail_list_bounds();let list_x=list_bounds.left();let row_w=list_bounds.width()-12.0;
        let compact=code.is_none();let row_h=if compact{21.5}else{34.0};
        let all_count=self.mail.recent.len();
        let unread=self.mail.recent.iter().filter(|m|self.mail_seen(m,Folder::Inbox)==Some(false)).count();
        if trash{
            c.text(list_x+6.0,47.0,&format!("{} {}",if self.settings.mail_sync_operations{"回收站"}else{"本机回收站"},self.mail.manage.trash.len()),10.5,W::Semibold,theme::WHITE,H::Left);
            c.text(375.0,47.0,"返回邮件",10.0,W::Regular,theme::BLUE,H::Right);
            if c.hit(322.0,33.0,56.0,19.0,"mail-trash-back").clicked(){self.mail_toggle_trash();return;}
        }else{
            for (x,width,label,selected,id) in [
                (list_x,52.0,format!("全部 {all_count}"),!self.mail.manage.unread_only,"mail-all"),
                (list_x+56.0,52.0,format!("未读 {unread}"),self.mail.manage.unread_only,"mail-unread"),
            ]{
                if selected{c.fill(x,33.0,width,19.0,6.0,theme::BTN);}
                c.text(x+width/2.0,47.0,&label,10.0,W::Semibold,if selected{theme::WHITE}else{theme::GRAY},H::Center);
                if c.hit(x,33.0,width,19.0,id).on_hover_text("当前加载的最近邮件").clicked(){self.mail.manage.unread_only=id=="mail-unread";}
            }
            c.text(307.0,46.0,if self.settings.mail_sync_operations{"同步"}else{"仅本机"},8.0,W::Regular,theme::DIM,H::Center);
            let ready=self.mail.manage.pending.is_empty()&&self.mail.accounts.iter().any(|a|a.enabled&&self.mail.manage.data.contains_key(&(crate::mail::key(a),Folder::Inbox))&&self.mail.manage.identities.get(&crate::mail::key(a))==Some(a));
            c.text(375.0,47.0,"全部已读",10.0,W::Regular,if ready{theme::BLUE}else{theme::DIM},H::Right);
            if c.hit(325.0,33.0,53.0,19.0,"mail-read-all").on_hover_text(if self.settings.mail_sync_operations{"将已连接邮箱的未读邮件全部标为已读"}else{"仅将轻屿当前加载的邮件标为已读，不更改邮箱"}).clicked()&&ready{
                self.mail_manage_request(None,Folder::Inbox,Action::ReadAll);
            }
        }
        if !trash{
            let hit=c.hit(359.0,footer-8.0,22.0,20.0,"mail-trash").on_hover_text("回收站");
            crate::mail_manage_ui::trash_icon(c,370.0,footer+2.0,if hit.hovered(){theme::WHITE}else{theme::GRAY});
            if hit.clicked(){self.mail_toggle_trash();return;}
        }
        if !self.mail.manage.message.is_empty(){
            c.text(42.0,footer+4.0,&c.fit(&self.mail.manage.message,8.0,W::Regular,125.0),8.0,W::Regular,theme::GRAY,H::Left);
            if c.hit(40.0,footer-8.0,130.0,20.0,"mail-manage-status").on_hover_text(format!("{}\n点击刷新邮件状态",self.mail.manage.message)).clicked(){self.mail_manage_request(None,folder,Action::Load);}
        }
        let indices:Vec<_>=self.mail_rows().iter().enumerate().filter(|(_,m)|trash||!self.mail.manage.unread_only||self.mail_seen(m,folder)==Some(false)).map(|(i,_)|i).collect();
        if indices.is_empty(){
            let message=if !self.mail.manage.pending.is_empty(){"正在同步邮件…"}else if trash{"回收站为空"}else if self.mail.manage.unread_only{"没有未读邮件"}else{"还没有收到邮件"};
            crate::portrait::empty(c,list_bounds,"mail",message,"邮件状态会自动更新");return;
        }
        let mut content=egui::Ui::new(c.ui.ctx().clone(),egui::Id::new(("mail-list",c.live)),
            egui::UiBuilder::new().max_rect(c.rect(list_bounds.left(),list_bounds.top(),list_bounds.width(),list_bounds.height())).layer_id(c.painter.layer_id()).style(c.ui.style().clone()));
        content.set_clip_rect(c.rect(list_bounds.left(),list_bounds.top(),list_bounds.width(),list_bounds.height()).intersect(c.painter.clip_rect()));
        content.set_opacity(c.painter.opacity());if !c.live{content.disable();}
        content.spacing_mut().item_spacing.y=0.0;content.spacing_mut().scroll.floating=false;content.spacing_mut().scroll.bar_width=4.0*c.s;
        content.visuals_mut().widgets.inactive.bg_fill=Color32::from_gray(80);
        egui::ScrollArea::vertical().id_salt(("mail-list-scroll",trash,self.mail.manage.unread_only)).max_height(list_bounds.height()*c.s).auto_shrink([false,false]).scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible).show_rows(&mut content,row_h*c.s,indices.len(),|ui,range|{
            let rows:Vec<_>=range.map(|i|self.mail_rows()[indices[i]].clone()).collect();
            for raw in rows{
                let item=self.display_mail(&raw);let hidden=self.mail_code_hidden(&raw);let seen=self.mail_seen(&raw,folder);
                let row_w=(ui.available_width()/c.s).max(1.0);
                let(rect,_)=ui.allocate_exact_size(egui::vec2(row_w*c.s,row_h*c.s),egui::Sense::hover());
                let mut row=Card::new(ui,rect.min,ui.clip_rect());row.s=c.s;row.live=c.live;
                let hit=row.hit(0.0,0.0,row_w,row_h-1.0,("mail-row",item.uid,item.account.as_str()));
                let hovered=ui.ctx().pointer_hover_pos().is_some_and(|p|rect.contains(p)&&ui.clip_rect().contains(p));
                let ready=self.mail_ready(&item.account,folder);let actions=trash||hovered&&ready;
                if hovered{row.fill(0.0,0.0,row_w,row_h-1.0,5.0,Color32::from_gray(29));}
                if seen==Some(false)&&!trash{row.circle(5.0,if compact{10.5}else{8.0},2.1,theme::BLUE);}
                let ink=if seen==Some(true){theme::GRAY}else{theme::WHITE};
                if compact{
                    row.text(12.0,14.0,&row.fit(&item.from,11.0,W::Semibold,57.0),11.0,W::Semibold,ink,H::Left);
                    let subject_w=(row_w-128.0-if hidden{32.0}else{0.0}).max(10.0);
                    let width=row.text(76.0,14.0,&row.fit(&item.subject,10.5,W::Regular,subject_w),10.5,W::Regular,if seen==Some(true){theme::GRAY}else{Color32::from_gray(210)},H::Left);
                    if hidden{row.fill(80.0+width,5.0,29.0,12.0,3.0,theme::BTN);row.text(94.5+width,14.0,"已隐藏",8.0,W::Regular,theme::GRAY,H::Center);}
                }else{
                    row.text(12.0,12.0,&row.fit(&item.from,11.0,W::Semibold,row_w-65.0),11.0,W::Semibold,ink,H::Left);
                    row.text(12.0,27.0,&row.fit(&item.subject,10.0,W::Regular,row_w-20.0),10.0,W::Regular,theme::GRAY,H::Left);
                }
                let cy=if compact{10.0}else{8.0};let mut acted=false;
                if actions{
                    if trash{
                        row.text(row_w-5.0,cy+4.0,"恢复",10.0,W::Regular,if ready{theme::BLUE}else{theme::DIM},H::Right);
                        if row.hit(row_w-39.0,0.0,36.0,20.0,("mail-restore",item.uid,item.account.as_str())).clicked()&&ready{self.mail_manage_request(Some(&item.account),folder,Action::Move{uid:item.uid});acted=true;}
                    }else{
                        row.ring(row_w-35.0,cy,4.7,1.1,theme::GRAY);paint::check(&row,row_w-35.0,cy,6.0,if seen==Some(true){1.0}else{0.0},1.1,theme::GRAY);
                        if row.hit(row_w-45.0,0.0,20.0,20.0,("mail-seen",item.uid,item.account.as_str())).on_hover_text(if seen==Some(true){"标为未读"}else{"标为已读"}).clicked(){self.mail_manage_request(Some(&item.account),folder,Action::Read{uid:item.uid,seen:seen!=Some(true)});acted=true;}
                        let remove=row.hit(row_w-24.0,0.0,21.0,20.0,("mail-delete",item.uid,item.account.as_str())).on_hover_text(if self.settings.mail_sync_operations{"移到邮箱回收站"}else{"移到本机回收站，不更改邮箱"});
                        crate::mail_manage_ui::trash_icon(&row,row_w-14.0,cy,if remove.hovered(){theme::RED}else{theme::GRAY});
                        if remove.clicked(){self.mail_manage_request(Some(&item.account),folder,Action::Move{uid:item.uid});acted=true;}
                    }
                }else{row.text(row_w-6.0,cy+4.0,item.time.rsplit(' ').next().unwrap_or(&item.time),10.0,W::Regular,theme::GRAY,H::Right);}
                row.seg((6.0,row_h-0.5),(row_w-6.0,row_h-0.5),0.5,Color32::from_gray(40));
                hit.clone().on_hover_text(format!("{}\n{}\n收件邮箱：{}\n{}",item.from,item.subject,item.account,item.time));
                if hit.clicked()&&!acted{self.mail_open(&raw);}
            }
        });
    }

    /// 岛内邮件详情可滚动，也可以进入独立窗口阅读全文。
    pub(crate) fn mail_detail(&mut self, c: &Card, raw: &mail::Item, cmds: &mut Vec<Cmd>) {
        let bottom=self.content_rect().bottom();let body_height=(bottom-140.0).max(54.0);
        let hidden=self.mail_code_hidden(raw);
        let display=self.display_mail(raw);let item=&display;
        let body=self.mail_body_lines(item);
        c.fill(36.0, 36.0, 347.0, bottom-36.0, 11.0, theme::CARD);
        let back = c.hit(330.0, 42.0, 44.0, 14.0, ("mail-back", item.uid));
        let bh = c.hover_t(&back).0;
        c.text(374.0, 50.0, "返回", 10.5, W::Regular, if bh > 0.5 { theme::WHITE } else { theme::GRAY }, H::Right);
        if back.clicked() {
            self.mail.open_uid = None;
            return;
        }
        let folder=self.mail.manage.folder();let ready=self.mail_ready(&item.account,folder);
        let seen=self.mail_seen(raw,folder)==Some(true);
        let status=if folder==crate::mail_manage::Folder::Trash{"恢复"}else if seen{"标为未读"}else{"标为已读"};
        c.text(215.0,50.0,status,9.0,W::Regular,if ready{theme::BLUE}else{theme::DIM},H::Right);
        if c.hit(166.0,39.0,50.0,18.0,("mail-detail-seen",item.uid)).clicked()&&ready{
            let action=if folder==crate::mail_manage::Folder::Trash{crate::mail_manage::Action::Move{uid:item.uid}}else{crate::mail_manage::Action::Read{uid:item.uid,seen:!seen}};
            self.mail_manage_request(Some(&item.account),folder,action);
        }
        if folder==crate::mail_manage::Folder::Inbox{
            c.text(252.0,50.0,"删除",9.0,W::Regular,if ready{theme::RED}else{theme::DIM},H::Right);
            if c.hit(220.0,39.0,35.0,18.0,("mail-detail-delete",item.uid)).on_hover_text(if self.settings.mail_sync_operations{"移到邮箱回收站"}else{"移到本机回收站，不更改邮箱"}).clicked()&&ready{self.mail_manage_request(Some(&item.account),folder,crate::mail_manage::Action::Move{uid:item.uid});}
        }
        let from = c.fit(&item.from, 11.5, W::Semibold, 112.0);
        c.text(46.0, 50.0, &from, 11.5, W::Semibold, theme::WHITE, H::Left);
        let read=c.hit(266.0,42.0,60.0,14.0,("mail-read",&item.account,item.uid)).on_hover_text("在独立窗口阅读完整正文");
        c.text(324.0,50.0,"阅读全文",10.5,W::Regular,if read.hovered(){theme::BLUE}else{theme::GRAY},H::Right);
        if read.clicked(){self.mail_open_reader(raw);}
        let subject = c.fit(&item.subject, 13.5, W::Semibold, 328.0);
        c.text(46.0, 73.0, &subject, 13.5, W::Semibold, theme::WHITE, H::Left);
        c.hit(46.0,58.0,328.0,20.0,("mail-subject",&item.account,item.uid)).on_hover_text(&item.subject);
        c.text(374.0, 92.0, &item.time, 10.0, W::Regular, Color32::from_gray(170), H::Right);
        let account=format!("收件邮箱：{}",item.account);
        c.text(46.0,92.0,&c.fit(&account,10.0,W::Regular,230.0),10.0,W::Regular,Color32::from_gray(175),H::Left);
        c.hit(46.0,80.0,230.0,16.0,("mail-account",&item.account,item.uid)).on_hover_text(&account);

        // 正文和链接共用滚动区域，会议与验证码操作保留在底部。
        let mut content=egui::Ui::new(c.ui.ctx().clone(),egui::Id::new(("mail-content",&item.account,item.uid,c.live)),
            egui::UiBuilder::new().max_rect(c.rect(46.0,104.0,328.0,body_height)).layer_id(c.painter.layer_id()).style(c.ui.style().clone()));
        content.set_clip_rect(c.rect(46.0,104.0,328.0,body_height).intersect(c.painter.clip_rect()));
        content.set_opacity(c.painter.opacity());
        if !c.live {content.disable();}
        content.spacing_mut().item_spacing.y=6.0*c.s;
        content.style_mut().spacing.scroll.floating=false;
        content.spacing_mut().scroll.bar_width=4.0*c.s;
        egui::ScrollArea::vertical().id_salt(("mail-body",&item.account,item.uid)).max_height(body_height*c.s).auto_shrink([false,false]).show(&mut content,|ui|{
            if !body.is_empty() {
                crate::mail_reader::body_ui(ui,body.as_ref(),11.5*c.s);
            }else if item.links.is_empty(){
                ui.label(egui::RichText::new("（这封邮件没有正文）").font(c.font(12.0,W::Regular)).color(theme::DIM));
            }
            for link in &item.links {
                let text=format!("{} ↗",c.fit(&link.label,12.0,W::Regular,295.0));
                if ui.add(egui::Link::new(egui::RichText::new(text).font(c.font(12.0,W::Regular)).color(theme::BLUE))).on_hover_text(&link.url).clicked(){
                    cmds.push(Cmd::OpenUrl(link.url.clone()));
                }
            }
        });

        if !self.mail.manage.message.is_empty(){
            c.text(46.0,bottom-2.0,&c.fit(&self.mail.manage.message,8.0,W::Regular,130.0),8.0,W::Regular,theme::GRAY,H::Left);
            c.hit(44.0,bottom-12.0,132.0,15.0,"mail-detail-status").on_hover_text(&self.mail.manage.message);
        }
        // 底部一行：验证码，点击复制。
        c.seg((46.0,bottom-35.0),(374.0,bottom-35.0),0.6,Color32::from_gray(48));
        let y=bottom-16.0;
        if hidden {
            c.text(46.0,y,"验证码已隐藏（不代表失效）",11.0,W::Regular,theme::GRAY,H::Left);
            c.text(374.0,y,"查看 15 秒",10.5,W::Regular,theme::BLUE,H::Right);
            if c.hit(290.0,y-14.0,86.0,24.0,("mail-reveal",item.uid)).clicked(){self.reveal_mail_code(raw);}
        } else if let Some(code) = &item.code {
            let label = format!("验证码 {code}");
            let w = c.text(46.0, y, &label, 13.0, W::Semibold, theme::GREEN, H::Left);
            c.text(46.0 + w + 6.0, y, "点击复制", 10.5, W::Regular, theme::DIM, H::Left);
            let hit = c.hit(42.0, y - 14.0, w + 60.0, 24.0, ("mail-detail-code", item.uid));
            if hit.clicked() {
                cmds.push(Cmd::Copy(code.clone()));
                self.code_copied(code);
            }
        }
    }
}
