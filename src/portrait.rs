//! Narrow side panels. All content is laid out vertically in AppKit points;
//! the top dock continues to use the original horizontal pages.
use crate::app::{App,Cmd,Env,MUSIC_PAGE,SHELF_PAGE,CLIPBOARD_PAGE,TODO_PAGE,MAIL_PAGE,WEATHER_PAGE,AGENT_PAGE,MAC_STATS_PAGE,NOTIFICATIONS_PAGE,PAGE_NAMES,PAGE_SETTINGS_TABS,PAGE_GLYPHS};
use crate::paint::{self,Card,H};

use crate::smtc::Transport;
use crate::theme::{self,W};
use eframe::egui::{self,Color32,Pos2,Rect};

pub(crate) const INSET:f32=12.0;

const SURFACE:Color32=Color32::from_rgb(25,53,58);
const SELECTED:Color32=Color32::from_rgb(37,79,85);

fn label(response:&egui::Response,name:&str){
    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,response.enabled(),name));
}

pub(crate) fn button(c:&Card,x:f32,y:f32,w:f32,name:&str,id:impl std::hash::Hash)->bool{
    let response=c.hit(x,y,w,30.0,id).on_hover_text(name);
    label(&response,name);
    let (hover,press)=c.hover_t(&response);
    c.fill(x,y,w,30.0,8.0,paint::mix(SURFACE,SELECTED,hover.max(press)));
    if response.has_focus(){c.stroke(x,y,w,30.0,8.0,1.4,theme::CYAN);}
    c.centered_text(x+w/2.0,y+15.0,name,11.5,W::Semibold,theme::WHITE,H::Center);
    response.clicked()
}

fn icon_button(c:&Card,x:f32,y:f32,sz:f32,glyph:&str,name:&str,id:impl std::hash::Hash)->egui::Response{
    let response=c.hit(x-sz/2.0,y-sz/2.0,sz,sz,id).on_hover_text(name);
    label(&response,name);
    let (hover,press)=c.hover_t(&response);
    if hover>0.001 || response.has_focus(){c.fill(x-sz/2.0,y-sz/2.0,sz,sz,8.0,paint::mix(SURFACE,SELECTED,hover));}
    if response.has_focus(){c.stroke(x-sz/2.0,y-sz/2.0,sz,sz,8.0,1.4,theme::CYAN);}
    paint::glyph(&c.scaled(x,y,1.0-0.06*press),glyph,x,y,if sz>40.0{24.0}else{17.0},theme::WHITE);
    response
}

pub(crate) fn scroll_ui(c:&Card,x:f32,y:f32,w:f32,h:f32,id:impl std::hash::Hash)->egui::Ui{
    let rect=c.rect(x,y,w,h);
    let mut ui=egui::Ui::new(c.ui.ctx().clone(),egui::Id::new(("portrait-scroll",c.live,id)),egui::UiBuilder::new().max_rect(rect).layer_id(c.painter.layer_id()).style(c.ui.style().clone()));
    ui.set_clip_rect(rect.intersect(c.painter.clip_rect()));
    ui.set_opacity(c.painter.opacity());
    if !c.live{ui.disable();}
    ui.spacing_mut().item_spacing.y=0.0;
    ui.spacing_mut().scroll.floating=false;
    ui.spacing_mut().scroll.bar_width=4.0*c.s;
    ui
}

fn row_card<'a>(ui:&'a mut egui::Ui,c:&Card,h:f32,_width:f32)->(Card<'a>,f32){
    let width=ui.available_width()/c.s;
    let (rect,_)=ui.allocate_exact_size(egui::vec2(width*c.s,h*c.s),egui::Sense::hover());
    let mut row=Card::new(ui,rect.min,ui.clip_rect());
    row.s=c.s;row.live=c.live;(row,width)
}

fn mmss(value:f64)->String{let seconds=value.max(0.0) as u64;format!("{:02}:{:02}",seconds/60,seconds%60)}

pub(crate) fn empty(c:&Card,bounds:Rect,glyph:&str,title:&str,hint:&str){
    let center=bounds.center();
    paint::glyph(c,glyph,center.x,center.y-28.0,30.0,theme::CYAN);
    c.centered_text(center.x,center.y+5.0,&c.fit(title,13.0,W::Semibold,bounds.width()-20.0),13.0,W::Semibold,theme::WHITE,H::Center);
    for (row,line) in c.wrap(hint,11.0,W::Regular,bounds.width()-24.0,2).iter().enumerate(){c.centered_text(center.x,center.y+29.0+row as f32*17.0,line,11.0,W::Regular,theme::GRAY,H::Center);}
}

impl App{
    pub(crate) fn portrait_panel(&mut self,c:&Card,env:&Env,cmds:&mut Vec<Cmd>){
        let panel_w=self.expanded_width();
        if self.overview{self.control_center_symbol(c,33.0,23.0,22.0,true,theme::CYAN);}
        else{c.centered_text(20.0,23.0,"轻屿",13.0,W::Semibold,theme::WHITE,H::Left);}
        self.portrait_size_control(c,env);
        c.centered_text(panel_w/2.0,24.0,&self.settings.clock_text(&env.clock),11.0,W::Regular,theme::GRAY,H::Center);
        self.portrait_position_drag(c,env);
        self.panel_pin_control(c,panel_w-64.0,24.0);
        let settings=icon_button(c,panel_w-30.0,24.0,28.0,"settings","打开设置","portrait-settings");
        if Self::settings_gear_clicked(&settings){self.settings_tab=if self.overview{0}else{PAGE_SETTINGS_TABS[self.page]};self.open_settings();}
        self.portrait_navigation(c);
        if self.overview{self.control_center(c,env,cmds);return;}
        if self.task_dialog.is_some(){self.draw_portrait_task_editor(c);return;}
        match self.page{
            MUSIC_PAGE=>self.portrait_music(c,cmds),
            SHELF_PAGE=>self.portrait_shelf(c,cmds),
            CLIPBOARD_PAGE=>self.portrait_clipboard(c,cmds),
            TODO_PAGE=>self.page_library(c,env,cmds,true),
            MAIL_PAGE=>self.portrait_mail(c,cmds),
            WEATHER_PAGE=>self.portrait_weather(c,env,cmds),
            AGENT_PAGE=>self.page_agents(c,env,true),
            MAC_STATS_PAGE=>self.page_mac_stats(c,true),
            NOTIFICATIONS_PAGE=>self.page_notifications(c,true,cmds),
            _=>{}
        }
    }

    fn portrait_navigation(&mut self,c:&Card){
        let pages:Vec<_>=self.settings.page_order.iter().copied().filter(|page|self.module_on(*page)).collect();
        let width=self.expanded_width()-INSET*2.0;
        let fluid=self.settings.skin==crate::model::Skin::Fluid;
        if fluid {
            c.fill(INSET-4.0,44.0,width+8.0,56.0,12.0,crate::fluid::NAV_SURFACE);
            c.stroke(INSET-4.0,44.0,width+8.0,56.0,12.0,0.7,crate::fluid::NAV_EDGE);
        }
        self.control_center_tab(c,INSET,48.0,44.0,48.0);
        let mut content=scroll_ui(c,INSET+50.0,48.0,width-50.0,54.0,"module-navigation");
        content.spacing_mut().item_spacing=egui::Vec2::ZERO;
        content.spacing_mut().scroll.bar_width=3.0*c.s;
        let state=(self.overview,self.page);
        let key=egui::Id::new("portrait-navigation-selection");
        let changed=c.live&&c.ui.ctx().data(|data|data.get_temp::<(bool,usize)>(key))!=Some(state);
        if c.live{c.ui.ctx().data_mut(|data|data.insert_temp(key,state));}
        let focus_key=egui::Id::new("portrait-navigation-focus");
        let focused=c.ui.ctx().memory(|memory|memory.focused());
        // Programmatic/accessibility focus can arrive between egui passes, so
        // gained_focus alone misses a newly focused tab outside the viewport.
        let focus_changed=c.live&&c.ui.ctx().data(|data|data.get_temp::<Option<egui::Id>>(focus_key)).flatten()!=focused;
        if c.live{c.ui.ctx().data_mut(|data|data.insert_temp(focus_key,focused));}
        egui::ScrollArea::horizontal().id_salt("portrait-module-navigation").auto_shrink([false,false]).show(&mut content,|ui|{
            let cell=46.0;
            let (rect,_)=ui.allocate_exact_size(egui::vec2(pages.len() as f32*cell,48.0)*c.s,egui::Sense::hover());
            let mut row=Card::new(ui,rect.min,ui.clip_rect());row.s=c.s;row.live=c.live;
            for (position,page) in pages.iter().copied().enumerate(){
                let x=position as f32*cell;
                let selected=!self.overview&&self.page==page;
                let response=row.hit(x,0.0,44.0,48.0,("portrait-tab",page)).on_hover_text(PAGE_NAMES[page]).on_hover_cursor(egui::CursorIcon::PointingHand);
                response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel,c.live,selected,PAGE_NAMES[page]));
                if selected||response.hovered(){row.fill(x,0.0,44.0,48.0,9.0,if selected{SELECTED}else{SURFACE});}
                if response.has_focus(){row.stroke(x,0.0,44.0,48.0,9.0,1.4,theme::CYAN);if focus_changed||response.gained_focus(){ui.scroll_to_rect(row.rect(x,0.0,44.0,48.0),Some(egui::Align::Center));}}
                if changed&&selected{ui.scroll_to_rect(row.rect(x,0.0,44.0,48.0),Some(egui::Align::Center));}
                let ink=if selected{theme::CYAN}else if fluid{crate::fluid::NAV_INK}else{theme::GRAY};
                paint::glyph(&row,PAGE_GLYPHS[page],x+22.0,16.0,18.0,ink);
                let name=match page{SHELF_PAGE=>"文件",CLIPBOARD_PAGE=>"粘贴",MAC_STATS_PAGE=>"状态",_=>PAGE_NAMES[page]};
                row.centered_text(x+22.0,36.0,name,10.5,W::Semibold,ink,H::Center);
                if fluid&&selected{row.fill(x+16.0,44.0,12.0,2.0,1.0,theme::CYAN);}
                if self.navigation_activated(&row,&response,selected){
                    self.go(page,if page>=self.page{1.0}else{-1.0});
                    // Hover keeps the strip still. Explicit and keyboard page
                    // changes continue to reveal their selected tab.
                    if !response.clicked(){c.ui.ctx().data_mut(|data|data.insert_temp(key,(self.overview,self.page)));}
                }
            }
        });
    }

    fn portrait_music(&mut self,c:&Card,cmds:&mut Vec<Cmd>){
        if self.qq_library_open{self.draw_qq_library(c,cmds,true);return;}
        let panel_w=self.expanded_width();let width=panel_w-INSET*2.0;let panel_h=self.expanded_height();
        let np=self.music_display();let has=self.has_live_music();let has_display=!np.title.is_empty();let playable=self.can_play_music();
        #[cfg(target_os="macos")]
        {
            if button(c,INSET,109.0,(width-12.0)/2.0,"我的歌单","portrait-qq-library"){self.open_qq_library(cmds);}
            if button(c,INSET+(width+12.0)/2.0,109.0,(width-12.0)/2.0,"播放器…","portrait-qq-player"){cmds.push(Cmd::QqMusic(crate::qq_music::Page::Player));}
        }
        let growth=((panel_h-532.0)*0.25).min((panel_w-292.0)*0.5).clamp(0.0,92.0);
        let spread=((panel_h-532.0)*0.26).max(0.0);
        let art_size=if cfg!(target_os="macos"){112.0}else{144.0}+growth;
        let art=Rect::from_min_size(c.pos((panel_w-art_size)/2.0,if cfg!(target_os="macos"){151.0}else{111.0}),egui::vec2(art_size,art_size)*c.s);
        if let Some((image,texture))=&self.music_artwork{
            let [w,h]=image.size;
            crate::skin_paint::mark_colored(texture.id());
            let uv=Rect::from_center_size(egui::pos2(0.5,0.5),if w>h{egui::vec2(h as f32/w as f32,1.0)}else{egui::vec2(1.0,w as f32/h as f32)});
            c.painter.add(egui::epaint::RectShape::filled(art,16.0*c.s,Color32::WHITE).with_texture(texture.id(),uv));
        }else{
            let texture=theme::music_placeholder(c.ui.ctx());
            c.painter.add(egui::epaint::RectShape::filled(art,16.0*c.s,Color32::WHITE).with_texture(texture.id(),Rect::from_min_max(Pos2::ZERO,egui::pos2(1.0,1.0))));
        }
        let shifted=c.shifted(0.0,growth+spread);let c=&shifted;
        let (empty_title,empty_hint)=if cfg!(target_os="macos"){self.qq_music_empty_state()}else{("暂无音乐","播放音乐后，在这里控制")};
        c.text(panel_w/2.0,282.0,&c.fit(if has_display{&np.title}else{empty_title},17.0,W::Semibold,width),17.0,W::Semibold,theme::WHITE,H::Center);
        c.text(panel_w/2.0,303.0,&c.fit(if has_display{&np.artist}else{empty_hint},12.0,W::Regular,width),12.0,W::Regular,theme::GRAY,H::Center);
        let seek=c.hit_drag(INSET-4.0,316.0,width+8.0,28.0,"portrait-seek").on_hover_text("播放进度");
        seek.widget_info(||egui::WidgetInfo::slider(c.live&&has,np.position,"播放进度"));
        if seek.has_focus(){c.stroke(INSET-4.0,316.0,width+8.0,28.0,7.0,1.3,theme::CYAN);}
        if has&&seek.has_focus(){for (key,step) in [(egui::Key::ArrowLeft,-5.0),(egui::Key::ArrowRight,5.0)]{if c.ui.input(|i|i.key_pressed(key)){cmds.push(Cmd::Media(Transport::Seek((np.position+step).clamp(0.0,np.duration.max(0.0)))));}}}
        let position=if has&&np.duration>0.0&&(seek.is_pointer_button_down_on()||seek.clicked()||seek.drag_stopped()){
            seek.interact_pointer_pos().map(|p|((c.to_local(p).x-INSET)/width).clamp(0.0,1.0) as f64*np.duration)
        }else{None};
        let current=position.unwrap_or(np.position);
        let fraction=if np.duration>0.0{(current/np.duration).clamp(0.0,1.0) as f32}else{0.0};
        c.fill(INSET,328.0,width,4.0,2.0,theme::TRACK);
        if fraction>0.0{c.fill(INSET,328.0,width*fraction,4.0,2.0,theme::CYAN);}
        if seek.hovered()||seek.dragged(){c.circle(INSET+width*fraction,330.0,5.0,theme::WHITE);}
        c.text(INSET,351.0,&mmss(current),10.5,W::Regular,theme::GRAY,H::Left);
        c.text(panel_w-INSET,351.0,&if has&&np.duration<=0.0{"--:--".into()}else{format!("-{}",mmss(np.duration-current))},10.5,W::Regular,theme::GRAY,H::Right);
        if seek.clicked()||seek.drag_stopped(){if let Some(position)=position{cmds.push(Cmd::Media(Transport::Seek(position)));}}
        let previous=icon_button(c,panel_w/2.0-63.0,383.0,36.0,"previous","上一首","portrait-previous");
        c.circle(panel_w/2.0,383.0,26.0,SELECTED);
        let (play_glyph,play_label)=self.music_play_button();
        let play=icon_button(c,panel_w/2.0,383.0,52.0,play_glyph,play_label,"portrait-play");
        let next=icon_button(c,panel_w/2.0+63.0,383.0,36.0,"next","下一首","portrait-next");
        if has{if previous.clicked(){cmds.push(Cmd::Media(Transport::Prev));}if next.clicked(){cmds.push(Cmd::Media(Transport::Next));}}
        if playable&&play.clicked(){self.toggle_music_playback(cmds);c.ui.ctx().request_repaint();}
        let (volume,muted)=self.music_volume_ui.unwrap_or((0,false));
        let available=has&&self.music_volume_ui.is_some();
        let mute=icon_button(c,34.0,435.0,28.0,if muted{"mute"}else{"volume"},if muted{"恢复音乐音量"}else{"静音音乐"},"portrait-mute");
        if available&&mute.clicked(){self.change_music_volume(Transport::SetMute(!muted),cmds);}
        let track=c.hit_drag(56.0,421.0,panel_w-124.0,28.0,"portrait-volume").on_hover_text("音乐应用音量 · 拖动或滚轮调整");
        track.widget_info(||egui::WidgetInfo::slider(c.live,volume as f64,"音乐音量"));
        if track.has_focus(){c.stroke(56.0,421.0,panel_w-124.0,28.0,7.0,1.3,theme::CYAN);}
        let mut next_volume=None;
        if available&&(track.clicked()||track.dragged()||track.is_pointer_button_down_on()){
            if let Some(pointer)=track.interact_pointer_pos(){next_volume=Some(((c.to_local(pointer).x-60.0)/(panel_w-132.0)*100.0).round().clamp(0.0,100.0) as u32);}
        }else if available&&(track.hovered()||track.has_focus()){
            let delta=if track.hovered(){c.ui.input(|i|i.raw_scroll_delta.y)}else{0.0};
            if delta.abs()>0.5{next_volume=Some((volume as f32+delta/120.0*4.0).round().clamp(0.0,100.0) as u32);}
            for (key,step) in [(egui::Key::ArrowLeft,-5),(egui::Key::ArrowRight,5)]{if track.has_focus()&&c.ui.input(|i|i.key_pressed(key)){next_volume=Some((volume as i32+step).clamp(0,100) as u32);}}
        }
        if let Some(volume)=next_volume{self.change_music_volume(Transport::SetVolume(volume),cmds);}
        let (volume,muted)=self.music_volume_ui.unwrap_or((0,false));
        c.fill(60.0,433.0,panel_w-132.0,4.0,2.0,theme::TRACK);
        c.fill(60.0,433.0,(panel_w-132.0)*volume as f32/100.0,4.0,2.0,if muted{theme::GRAY}else{theme::CYAN});
        c.circle(60.0+(panel_w-132.0)*volume as f32/100.0,435.0,4.5,if available{theme::WHITE}else{theme::DIM});
        c.centered_text(panel_w-22.0,435.0,&if available{format!("{volume}%")}else{"—".into()},10.5,W::Regular,theme::GRAY,H::Right);
        let qq_playing=has&&np.source==crate::qq_music::SOURCE||self.qq_library.resuming;
        if button(c,panel_w/2.0-if qq_playing{104.0}else{30.0},463.0,if qq_playing{98.0}else{60.0},if self.show_lyrics{"隐藏歌词"}else{"显示歌词"},"portrait-lyrics")&&has{self.show_lyrics=!self.show_lyrics;self.settings.lyrics_enabled=self.show_lyrics;self.settings.save();}
        if qq_playing && button(c,panel_w/2.0+6.0,463.0,98.0,"停止播放","portrait-qq-stop"){cmds.push(Cmd::QqMusicStop);}
        if self.show_lyrics&&has{
            if let Some(native)=self.native_qq_lyrics(){
                let current=native.current.unwrap_or(0).min(native.lines.len()-1);
                c.text(panel_w/2.0,515.0,&c.fit(&native.lines[current],11.0,W::Semibold,width),11.0,W::Semibold,theme::WHITE,H::Center);
                let rows=((panel_h-growth-spread-535.0)/25.0).max(0.0) as usize;
                for (index,line) in native.lines.iter().skip(current+1).take(rows).enumerate(){c.text(panel_w/2.0,540.0+index as f32*25.0,&c.fit(line,11.0,W::Regular,width),11.0,W::Regular,theme::GRAY,H::Center);}
            }else{
            let line=if let Some(lyrics)=&self.ly.lyrics{lyrics.index_at(self.lyric_position()).and_then(|i|lyrics.lines.get(i)).or_else(||lyrics.lines.first()).map(|l|l.text.as_str()).unwrap_or("暂无歌词")}
                else if self.ly.loading{"正在查找歌词…"}else if self.ly.error.is_some(){"歌词暂不可用 · 点击重试"}else{"暂无歌词"};
            c.text(panel_w/2.0,515.0,&c.fit(line,11.0,W::Regular,width),11.0,W::Regular,theme::GRAY,H::Center);
            if let Some(lyrics)=&self.ly.lyrics{
                let current=lyrics.index_at(self.lyric_position()).unwrap_or(0);
                let rows=((panel_h-growth-spread-535.0)/25.0).max(0.0) as usize;
                for (index,line) in lyrics.lines.iter().skip(current+1).take(rows).enumerate(){c.text(panel_w/2.0,540.0+index as f32*25.0,&c.fit(&line.text,11.0,W::Regular,width),11.0,W::Regular,theme::GRAY,H::Center);}
            }
            if c.hit(INSET,498.0,width,25.0,"portrait-lyrics-retry").clicked()&&self.ly.error.is_some(){self.ly.key.clear();self.ly.manual_retry=true;}
            }
        }
    }

    fn portrait_shelf(&mut self,c:&Card,cmds:&mut Vec<Cmd>){
        let panel_w=self.expanded_width();let width=panel_w-INSET*2.0;let panel_h=self.expanded_height();
        c.centered_text(INSET,128.0,"置物架",15.0,W::Semibold,theme::WHITE,H::Left);
        if icon_button(c,panel_w-36.0,128.0,30.0,"plus","添加文件","portrait-shelf-add").clicked(){cmds.push(Cmd::AddFiles);}
        let items=self.shelf.items.clone();self.icons.retain(&items);
        if items.is_empty(){
            empty(c,Rect::from_min_max(egui::pos2(INSET,156.0),egui::pos2(panel_w-INSET,panel_h-40.0)),"shelf","把文件拖到这里","只记位置，保留原文件");
        }else{
            let mut content=scroll_ui(c,INSET,156.0,width,(panel_h-194.0),"shelf");
            egui::ScrollArea::vertical().id_salt("portrait-shelf-list").max_height((panel_h-194.0)*c.s).auto_shrink([false,false]).show(&mut content,|ui|{
                for path in &items{
                    let (row,w)=row_card(ui,c,62.0,width);
                    let response=row.hit_drag(0.0,0.0,w-30.0,62.0,("portrait-file",path));
                    if response.hovered(){row.fill(0.0,0.0,w,62.0,10.0,SURFACE);}
                    self.file_icon_at(&row,path,24.0,30.0,34.0);
                    let name=path.file_name().map(|v|v.to_string_lossy()).unwrap_or_default();
                    label(&response,&format!("打开 {name}"));
                    if response.has_focus(){row.stroke(0.0,0.0,w-30.0,62.0,10.0,1.3,theme::CYAN);}
                    row.text(49.0,25.0,&row.fit(&name,12.0,W::Semibold,w-87.0),12.0,W::Semibold,theme::WHITE,H::Left);
                    let meta=self.icons.get(c.ui.ctx(),path).map(|p|p.metadata).unwrap_or_default();
                    row.text(49.0,44.0,&row.fit(&meta,10.5,W::Regular,w-87.0),10.5,W::Regular,theme::GRAY,H::Left);
                    let remove=icon_button(&row,w-14.0,30.0,28.0,"close","从置物架移除，保留原文件",("portrait-file-remove",path));
                    let mut delete=remove.clicked();
                    response.clone().on_hover_text(path.display().to_string());
                    if response.drag_started()&&path.exists(){cmds.push(Cmd::DragFile(path.clone()));}
                    row.seg((0.0,61.5),(w,61.5),0.5,Color32::from_white_alpha(22));
                    response.context_menu(|ui|{
                        if ui.button("打开").clicked(){cmds.push(Cmd::Open(path.clone()));ui.close_menu();}
                        if ui.button("在文件夹中显示").clicked(){cmds.push(Cmd::Reveal(path.clone()));ui.close_menu();}
                        if ui.button("复制完整路径").clicked(){cmds.push(Cmd::Copy(path.display().to_string()));ui.close_menu();}
                        if ui.button("从置物架移除").clicked(){delete=true;ui.close_menu();}
                        self.shelf_menu_rect=Some(egui::Frame::menu(ui.style()).widget_rect(ui.min_rect()));
                    });
                    if delete{self.shelf.remove(path);self.icons.remove(path);self.shelf_x.remove(path);self.shelf_born.remove(path);}
                    else if response.clicked(){if path.exists(){cmds.push(Cmd::Open(path.clone()));}else{self.say("文件已移动或删除，可从置物架移除");}}
                }
            });
            c.text(INSET,panel_h-14.0,&format!("{} 个文件",items.len()),10.5,W::Regular,theme::GRAY,H::Left);
            let confirming=self.confirm_clear.is_some();
            if button(c,panel_w-88.0,panel_h-33.0,68.0,if confirming{"确认清空"}else{"清空"},"portrait-shelf-clear"){
                if confirming{self.shelf.clear();self.icons.clear();self.shelf_born.clear();self.shelf_x.clear();self.shelf_first=0;self.confirm_clear=None;self.say("置物架已清空，原文件保留");}else{self.confirm_clear=Some(self.t);}
            }
        }
        if self.drag_a>0.002{
            let overlay=c.faded(self.drag_a);
            overlay.fill(INSET,156.0,width,(panel_h-194.0),14.0,Color32::from_black_alpha(150));
            overlay.dashed(INSET,156.0,width,(panel_h-194.0),14.0,1.4,theme::CYAN,self.t as f32*14.0);
            paint::glyph(&overlay,"download",panel_w/2.0,292.0,40.0,theme::CYAN);
            overlay.centered_text(panel_w/2.0,340.0,"松手放进置物架",14.0,W::Semibold,theme::WHITE,H::Center);
        }
    }

    fn portrait_clipboard(&mut self,c:&Card,cmds:&mut Vec<Cmd>){
        let panel_w=self.expanded_width();let width=panel_w-INSET*2.0;let panel_h=self.expanded_height();
        c.centered_text(INSET,128.0,"粘贴板",15.0,W::Semibold,theme::WHITE,H::Left);
        if !self.settings.clipboard_history{
            let bounds=Rect::from_min_max(egui::pos2(INSET,156.0),egui::pos2(panel_w-INSET,panel_h-40.0));
            empty(c,bounds,"clipboard","粘贴板记录已关闭","开启后保留文字、图片与视频记录");
            if button(c,panel_w/2.0-50.0,bounds.center().y+54.0,100.0,"开启记录","portrait-clipboard-enable"){self.settings.clipboard_history=true;self.clipboard.set_enabled(true);self.settings.save();}
            return;
        }
        let items=self.clipboard.lock().items.clone();
        self.clip_icons.retain(&items.iter().filter_map(|clip|clip.media.as_ref().map(|media|media.path.clone())).collect::<Vec<_>>());
        c.centered_text(panel_w-INSET,128.0,&format!("{} 条记录",items.len()),11.0,W::Regular,theme::GRAY,H::Right);
        if items.is_empty(){
            empty(c,Rect::from_min_max(egui::pos2(INSET,156.0),egui::pos2(panel_w-INSET,panel_h-40.0)),"clipboard","复制的内容会出现在这里","文字、图片与视频记录保存在本机");
        }
        let mut action=None;
        let mut content=scroll_ui(c,INSET,156.0,width,panel_h-194.0,"clipboard");
        egui::ScrollArea::vertical().id_salt("portrait-clipboard-list").max_height((panel_h-194.0)*c.s).auto_shrink([false,false]).show(&mut content,|ui|{
            for (index,clip) in items.iter().enumerate(){
                let media=clip.media.as_ref();let height=if media.is_some(){90.0}else{70.0};
                let (row,w)=row_card(ui,c,height,width);
                let response=row.hit(0.0,0.0,w-32.0,height,("portrait-clip",index)).on_hover_text(format!("{}\n{}\n点击复制",clip.preview().chars().take(400).collect::<String>(),clip.description()));
                label(&response,&format!("复制{}",clip.preview()));
                if response.hovered(){row.fill(0.0,0.0,w,height,8.0,SURFACE);}
                if response.has_focus(){row.stroke(0.0,0.0,w-32.0,height,10.0,1.3,theme::CYAN);}
                let text_x=if media.is_some(){self.clipboard_preview(&row,clip,39.0,42.0,54.0);76.0}else{12.0};
                let text=if let Some(media)=media{media.name.clone()}else{clip.preview()};
                for (line,text) in row.wrap(&text,12.0,W::Regular,w-text_x-40.0,2).iter().enumerate(){row.text(text_x,24.0+line as f32*16.0,text,12.0,W::Regular,theme::WHITE,H::Left);}
                let mut open_requested=false;
                if let Some(media)=media{
                    row.text(text_x,58.0,&format!("{} · {}",if media.kind==crate::clipboard::MediaKind::Video{"视频"}else{"图片"},crate::model::human_size(media.bytes)),10.5,W::Regular,theme::GRAY,H::Left);
                    if icon_button(&row,w-54.0,74.0,24.0,if media.kind==crate::clipboard::MediaKind::Video{"play"}else{"expand"},if media.kind==crate::clipboard::MediaKind::Video{"播放视频"}else{"打开图片"},("portrait-clip-open",index)).clicked(){open_requested=true;cmds.push(Cmd::Open(media.path.clone()));}
                }
                row.text(text_x,height-9.0,&clip.at,10.0,W::Regular,theme::GRAY,H::Left);
                let pin=icon_button(&row,w-16.0,18.0,28.0,"pin",if clip.pinned{"取消固定"}else{"固定记录"},("portrait-clip-pin",index));
                if clip.pinned{row.circle(w-6.0,9.0,2.5,theme::CYAN);}
                let delete=icon_button(&row,w-16.0,height-19.0,28.0,"close","删除记录",("portrait-clip-delete",index));
                row.seg((0.0,height-0.5),(w,height-0.5),0.5,Color32::from_white_alpha(22));
                if delete.clicked(){action=Some((index,2));}else if pin.clicked(){action=Some((index,1));}else if !open_requested&&response.clicked(){action=Some((index,0));}
            }
        });
        match action{
            Some((index,0))=>self.copy_clip_record(&items[index],cmds),
            Some((index,1))=>{if !self.clipboard.lock().toggle_pin_clip(&items[index]){self.say(format!("最多固定 {} 条",crate::clipboard::MAX_PINNED));}},
            Some((index,_))=>self.clipboard.lock().remove_clip(&items[index]),
            None=>{}
        }
        let confirming=self.clip_confirm_clear.is_some_and(|at|self.t-at<3.0);
        if items.iter().any(|clip|!clip.pinned)&&button(c,panel_w-120.0,panel_h-33.0,100.0,if confirming{"确认清空"}else{"清空未固定"},"portrait-clip-clear"){
            if confirming{self.clipboard.lock().clear_recent();self.clip_confirm_clear=None;}else{self.clip_confirm_clear=Some(self.t);}
        }
    }

    pub(crate) fn portrait_todo(&mut self,c:&Card,env:&Env){
        let panel_w=self.expanded_width();let width=panel_w-INSET*2.0;let panel_h=self.expanded_height();
        let (done,total)=self.todos.progress(&env.today);
        c.text(INSET,144.0,&format!("{done}/{total}"),28.0,W::Semibold,theme::WHITE,H::Left);
        c.text(INSET+83.0,143.0,"已完成",11.0,W::Regular,theme::GRAY,H::Left);
        if button(c,panel_w-88.0,116.0,68.0,"添加待办","portrait-todo-add"){
            self.todo_choosing=false;self.todo_first=0;self.adding=true;self.editing=None;self.buf.clear();self.todo_input_focused=false;
        }
        let editing=self.adding||self.editing.is_some();
        if editing{
            c.fill(INSET,170.0,width,38.0,10.0,SURFACE);c.stroke(INSET,170.0,width,38.0,10.0,1.2,theme::CYAN);
        }
        let top=if editing{220.0}else{169.0};let height=if editing{panel_h-250.0}else{panel_h-199.0};
        let order=self.todos.display_order(&env.today);
        if order.is_empty()&&!editing{
            empty(c,Rect::from_min_size(egui::pos2(INSET,top),egui::vec2(width,height)),"todo","今天还没有安排","添加事项后，在这里完成与设置提醒");
        }
        let mut content=scroll_ui(c,INSET,top,width,height,"todo");
        egui::ScrollArea::vertical().id_salt("portrait-todo-list").max_height(height*c.s).auto_shrink([false,false]).show(&mut content,|ui|{
            for index in order{
                if self.editing==Some(index){continue;}
                let item=self.todos.items[index].clone();let (row,w)=row_card(ui,c,66.0,width);
                let checked=item.done_for(&env.today);let scheduled=item.scheduled_on(&env.today);
                let response=row.hit(0.0,0.0,w-32.0,66.0,("portrait-todo",index));
                response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::Checkbox,row.live,checked,item.text.clone()));
                if response.hovered(){row.fill(0.0,0.0,w,66.0,8.0,SURFACE);}
                if response.has_focus(){row.stroke(0.0,0.0,w-32.0,66.0,10.0,1.3,theme::CYAN);}
                row.ring(17.0,26.0,7.0,1.4,if checked{theme::CYAN}else{theme::GRAY});
                if checked{row.circle(17.0,26.0,7.0,theme::CYAN);paint::check(&row,17.0,26.0,7.0,1.0,1.5,SURFACE);}
                let ink=if checked||!scheduled{theme::GRAY}else{theme::WHITE};
                for (line,text) in row.wrap(&item.text,12.0,W::Regular,w-77.0,2).iter().enumerate(){row.text(33.0,24.0+line as f32*16.0,text,12.0,W::Regular,ink,H::Left);}
                let hint=item.reminder_text().unwrap_or_else(||{let tag=crate::model::repeat_tag(item.repeat);if tag.is_empty(){"点选完成".into()}else{tag}});
                row.text(33.0,58.0,&row.fit(&hint,10.0,W::Regular,w-74.0),10.0,W::Regular,theme::GRAY,H::Left);
                let edit=icon_button(&row,w-15.0,18.0,28.0,"pencil","编辑与提醒",("portrait-todo-edit",index));
                let delete=icon_button(&row,w-15.0,49.0,28.0,"close","删除待办",("portrait-todo-delete",index));
                row.seg((0.0,65.5),(w,65.5),0.5,Color32::from_white_alpha(22));
                if delete.clicked(){self.todos.items.remove(index);self.todos.save();break;}
                if edit.clicked(){self.open_task_dialog(index);break;}
                if response.clicked(){if scheduled{self.todos.items[index].toggle(&env.today);self.todos.save();self.todo_pop.vel+=5.0;}else{self.say(format!("{}的待办，今天不用做",crate::model::repeat_label(item.repeat)));}}
            }
        });
        c.text(INSET+88.0,panel_h-15.0,&c.fit(if editing{"回车保存 · Esc 取消"}else{"点击完成 · 右侧编辑"},10.5,W::Regular,width-88.0),10.5,W::Regular,theme::GRAY,H::Left);
    }

    fn portrait_mail(&mut self,c:&Card,cmds:&mut Vec<Cmd>){
        let panel_w=self.expanded_width();let width=panel_w-INSET*2.0;let panel_h=self.expanded_height();
        use crate::mail_manage::{Action,Folder};
        if self.mail.accounts.is_empty(){
            let bounds=self.content_rect();empty(c,bounds,"mail","还没有邮箱","点击右上角齿轮添加邮箱");return;
        }
        self.mail_manage_tick();
        if let Some((account,uid))=self.mail.open_uid.clone(){
            if let Some(item)=self.mail_rows().iter().find(|m|m.uid==uid&&m.account.eq_ignore_ascii_case(&account)).cloned(){self.portrait_mail_detail(c,&item,cmds);return;}self.mail.open_uid=None;
        }
        let code=self.visible_mail_code().cloned();
        if let Some(raw)=&code{
            let item=self.display_mail(raw);let hidden=self.mail_code_hidden(raw);
            c.fill(INSET,113.0,width,91.0,12.0,SURFACE);
            c.text(33.0,134.0,"验证码",11.0,W::Regular,theme::GRAY,H::Left);
            c.text(panel_w-33.0,134.0,&c.fit(&item.from,11.0,W::Semibold,width-96.0),11.0,W::Semibold,theme::GRAY,H::Right);
            c.text(33.0,168.0,item.code.as_deref().unwrap_or("••••••"),27.0,W::Semibold,theme::CYAN,H::Left);
            c.text(33.0,190.0,if hidden{"已隐藏 · 点击查看"}else{"点击复制"},10.5,W::Regular,theme::GRAY,H::Left);
            let response=c.hit(INSET,113.0,width,91.0,"portrait-mail-code").on_hover_text("查看或复制验证码");label(&response,if hidden{"查看验证码"}else{"复制验证码"});
            if response.has_focus(){c.stroke(INSET,113.0,width,91.0,12.0,1.3,theme::CYAN);}
            if response.clicked(){if hidden{self.reveal_mail_code(raw);}else if let Some(code)=&item.code{cmds.push(Cmd::Copy(code.clone()));self.code_copied(code);}}
        }
        let folder=self.mail.manage.folder();let trash=folder==Folder::Trash;
        let toolbar=if code.is_some(){213.0}else{113.0};
        if trash{if button(c,INSET,toolbar,94.0,"返回收件箱","portrait-mail-trash-back"){self.mail_toggle_trash();}}
        else{
            for (x,unread,name) in [(INSET,false,"全部"),(98.0,true,"未读")]{
                if self.mail.manage.unread_only==unread{c.fill(x,toolbar,70.0,30.0,8.0,SELECTED);}
                let response=c.hit(x,toolbar,70.0,30.0,("portrait-mail-filter",unread));label(&response,name);
                if response.has_focus(){c.stroke(x,toolbar,70.0,30.0,8.0,1.3,theme::CYAN);}
                c.centered_text(x+35.0,toolbar+15.0,name,11.5,W::Semibold,theme::WHITE,H::Center);
                if response.clicked(){self.mail.manage.unread_only=unread;}
            }
            if icon_button(c,panel_w-36.0,toolbar+15.0,30.0,"trash","回收站","portrait-mail-trash").clicked(){self.mail_toggle_trash();return;}
            if icon_button(c,panel_w-80.0,toolbar+15.0,30.0,"check","全部标为已读","portrait-mail-read-all").clicked(){self.mail_manage_request(None,Folder::Inbox,Action::ReadAll);}
        }
        let rows:Vec<_>=self.mail_rows().iter().filter(|item|trash||!self.mail.manage.unread_only||self.mail_seen(item,folder)==Some(false)).cloned().collect();
        let top=toolbar+40.0;let height=panel_h-40.0-top;
        if rows.is_empty(){empty(c,Rect::from_min_size(egui::pos2(INSET,top),egui::vec2(width,height)),"mail",if trash{"回收站为空"}else if self.mail.manage.unread_only{"没有未读邮件"}else{"还没有收到邮件"},"邮件状态会自动更新");}
        let mut content=scroll_ui(c,INSET,top,width,height,"mail");
        egui::ScrollArea::vertical().id_salt(("portrait-mail-list",trash,self.mail.manage.unread_only)).max_height(height*c.s).auto_shrink([false,false]).show(&mut content,|ui|{
            for raw in &rows{
                let item=self.display_mail(raw);let (row,w)=row_card(ui,c,74.0,width);
                let response=row.hit(0.0,0.0,w,74.0,("portrait-mail-row",&item.account,item.uid));label(&response,&format!("{}，{}",item.from,item.subject));
                if response.has_focus(){row.stroke(0.0,0.0,w,74.0,10.0,1.3,theme::CYAN);}
                if response.hovered(){row.fill(0.0,0.0,w,74.0,10.0,SURFACE);}
                let seen=self.mail_seen(raw,folder);if seen==Some(false){row.circle(5.0,20.0,2.5,theme::CYAN);}
                row.text(15.0,22.0,&row.fit(&item.from,12.0,W::Semibold,w-70.0),12.0,W::Semibold,if seen==Some(true){theme::GRAY}else{theme::WHITE},H::Left);
                row.text(w-4.0,22.0,item.time.rsplit(' ').next().unwrap_or(&item.time),10.0,W::Regular,theme::GRAY,H::Right);
                row.text(15.0,44.0,&row.fit(&item.subject,12.0,W::Regular,w-21.0),12.0,W::Regular,theme::WHITE,H::Left);
                let summary=crate::clipboard::preview(&item.summary);
                row.text(15.0,63.0,&row.fit(&summary,10.5,W::Regular,w-21.0),10.5,W::Regular,theme::GRAY,H::Left);
                row.seg((15.0,73.0),(w-4.0,73.0),0.5,Color32::from_white_alpha(24));
                if response.clicked(){self.mail_open(raw);}
            }
        });
        c.text(INSET,panel_h-14.0,&c.fit(&if self.mail.manage.message.is_empty(){format!("{} 封邮件 · {}",rows.len(),if self.settings.mail_sync_operations{"同步操作"}else{"仅本机操作"})}else{self.mail.manage.message.clone()},10.0,W::Regular,width-34.0),10.0,W::Regular,theme::GRAY,H::Left);
        if icon_button(c,panel_w-36.0,panel_h-19.0,26.0,"refresh","刷新邮件状态","portrait-mail-refresh").clicked(){self.mail_manage_request(None,folder,Action::Load);}
    }

    fn portrait_mail_detail(&mut self,c:&Card,raw:&crate::mail::Item,cmds:&mut Vec<Cmd>){
        let panel_w=self.expanded_width();let width=panel_w-INSET*2.0;let panel_h=self.expanded_height();
        use crate::mail_manage::{Action,Folder};
        let item=self.display_mail(raw);let hidden=self.mail_code_hidden(raw);let folder=self.mail.manage.folder();let ready=self.mail_ready(&item.account,folder);let seen=self.mail_seen(raw,folder)==Some(true);
        if button(c,INSET,112.0,64.0,"返回","portrait-mail-back"){self.mail.open_uid=None;return;}
        if button(c,panel_w-120.0,112.0,100.0,"阅读全文","portrait-mail-reader"){self.mail_open_reader(raw);}
        c.text(INSET,164.0,&c.fit(&item.from,12.0,W::Semibold,width),12.0,W::Semibold,theme::GRAY,H::Left);
        for (index,line) in c.wrap(&item.subject,15.0,W::Semibold,width,2).iter().enumerate(){c.text(INSET,188.0+index as f32*20.0,line,15.0,W::Semibold,theme::WHITE,H::Left);}
        c.text(INSET,231.0,&c.fit(&format!("{} · {}",item.account,item.time),10.0,W::Regular,width),10.0,W::Regular,theme::GRAY,H::Left);
        let body=self.mail_body_lines(&item);
        let mut content=scroll_ui(c,INSET,245.0,width,(panel_h-340.0),("mail-body",&item.account,item.uid));
        egui::ScrollArea::vertical().id_salt(("portrait-mail-body",&item.account,item.uid)).max_height((panel_h-340.0)*c.s).auto_shrink([false,false]).show(&mut content,|ui|{
            if !body.is_empty(){crate::mail_reader::body_ui(ui,&body,12.5*c.s);}else if item.links.is_empty(){ui.label(egui::RichText::new("这封邮件没有正文").font(c.font(12.0,W::Regular)).color(theme::GRAY));}
            for link in &item.links{
                if ui.add(egui::Link::new(egui::RichText::new(format!("{} ↗",link.label)).font(c.font(12.0,W::Regular)).color(theme::CYAN))).on_hover_text(&link.url).clicked(){cmds.push(Cmd::OpenUrl(link.url.clone()));}
            }
        });
        let action_name=if folder==Folder::Trash{"恢复邮件"}else if seen{"标为未读"}else{"标为已读"};
        if button(c,INSET,panel_h-82.0,(width-28.0)/2.0,action_name,"portrait-mail-seen")&&ready{
            self.mail_manage_request(Some(&item.account),folder,if folder==Folder::Trash{Action::Move{uid:item.uid}}else{Action::Read{uid:item.uid,seen:!seen}});
        }
        if folder==Folder::Inbox&&button(c,INSET+(width+28.0)/2.0,panel_h-82.0,(width-28.0)/2.0,"移到回收站","portrait-mail-delete")&&ready{self.mail_manage_request(Some(&item.account),folder,Action::Move{uid:item.uid});}
        if hidden{
            if button(c,INSET,panel_h-40.0,width,"验证码已隐藏 · 查看 15 秒","portrait-mail-reveal"){self.reveal_mail_code(raw);}
        }else if let Some(code)=&item.code{
            if button(c,INSET,panel_h-40.0,width,&format!("复制验证码 {code}"),"portrait-mail-detail-code"){cmds.push(Cmd::Copy(code.clone()));self.code_copied(code);}
        }
    }
}
