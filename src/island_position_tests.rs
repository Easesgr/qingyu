//! Real egui interactions on the same movable-window plan as AppKit.
use super::*;
use crate::{app::{DesktopPlacement,FrameOut,GlassShape},extras::{Alert,AlertSource},model::{Dock,Skin}};

struct Island{h:H,room:Vec2,screen:Rect,out:Option<FrameOut>,drag_geometry:Option<crate::app::DragGeometry>,backing:f32}
impl Island{
    fn new(dock:Dock,position:[f32;2],length:u32)->Self{
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.island_position=Some(position);h.app.settings.resident_length=length;h.app.settings.skin=Skin::GlassDark;
        h.media=crate::smtc::NowPlaying{title:"云间漫步".into(),artist:"轻屿电台".into(),duration:224.0,position:24.0,playing:false,..Default::default()};
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,code:0,day:true,at:chrono::Utc::now().timestamp(),..Default::default()});
        Self{h,room:Vec2::new(1440.0,900.0),screen:Rect::NOTHING,out:None,drag_geometry:None,backing:1.0}
    }
    fn draw(&mut self,pointer:Option<Pos2>,pressed:Option<bool>,extra:Vec<egui::Event>)->egui::FullOutput{
        let dock=self.h.app.settings.dock;
        let global_room=self.drag_geometry.as_ref().and_then(|geometry|{
            let display=crate::display_layout::selected(&geometry.displays,&self.h.app.settings.display_id)?;
            let bounds=geometry.rooms.iter().find(|(id,_)|id==&display.id).map_or(display.bounds,|(_,bounds)|*bounds);
            let (_,point)=crate::island_position::display_local_geometry(geometry.cursor,bounds,geometry.scale)?;
            Some((Vec2::new((bounds.right-bounds.left) as f32,(bounds.bottom-bounds.top) as f32)/geometry.scale,point,geometry.scale))
        });
        if let Some((room,_,_))=global_room{self.room=room;}
        let scale=global_room.map_or(1.0,|(_,_,scale)|scale);
        let panel=if dock.is_side(){self.h.app.settings.side_panel_size()}else{Vec2::new(PW,crate::control_center::TOP_H)};
        let (_,plan)=crate::island_position::plan(self.room,self.h.app.placement_position().unwrap(),dock,self.h.app.resident_size(),panel,scale);
        self.h.app.set_desktop_placement(DesktopPlacement::Positioned(plan));self.screen=plan.room;
        let cursor=pointer.map(|point|global_room.map_or(point,|(_,point,_)|point)+self.screen.min.to_vec2());
        let mut events=cursor.map(|point|vec![egui::Event::PointerMoved(point)]).unwrap_or_else(||vec![egui::Event::PointerGone]);
        if let (Some(pos),Some(pressed))=(cursor,pressed){events.push(egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()});}
        events.extend(extra);self.h.t+=1.0/60.0;
        let env=Env{t:self.h.t,unix_time:1_794_703_260,clock:"09:41:00".into(),today:"2026-11-15".into(),date:"11月15日 周日".into(),cursor,drag_geometry:self.drag_geometry.clone(),screen:Some(self.screen),media:self.h.media.clone(),screen_locked:self.h.screen_locked,..Default::default()};
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,plan.canvas)),time:Some(self.h.t),events,focused:true,
            viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(self.backing*scale),..Default::default()})).collect(),..Default::default()};
        let output=self.h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
            self.out=Some(self.h.app.frame(ui,Rect::from_min_size(Pos2::ZERO,plan.canvas),&env));
        });});
        let out=self.out.as_ref().unwrap();
        if let Some(glass)=&out.glass{assert_eq!(out.hit_outline.as_ref().unwrap(),&glass[0].outline(),"native input and blur must use the painted body");}
        self.h.shapes=output.shapes.clone();output
    }
    fn settle(&mut self,pointer:Option<Pos2>){for _ in 0..100{self.draw(pointer,None,vec![]);}}
    fn body(&self)->Rect{
        let rect=self.h.app.displayed_island_rect(Rect::from_min_size(Pos2::ZERO,self.h.app.canvas_size()));
        rect.translate(-self.screen.min.to_vec2())
    }
    fn response(&self,id:impl std::hash::Hash,drag:bool)->egui::Response{
        self.h.ctx.read_response(egui::Id::new((if drag{"card-drag"}else{"card"},true,id))).expect("visible control")
    }
    fn world(&self,point:Pos2)->Pos2{point-self.screen.min.to_vec2()}
    fn resident(&self)->Rect{self.h.app.resident_rect(Rect::from_min_size(Pos2::ZERO,self.h.app.canvas_size())).translate(-self.screen.min.to_vec2())}
    fn press(&mut self,point:Pos2){self.draw(Some(point),None,vec![]);self.draw(Some(point),Some(true),vec![]);}
    fn release(&mut self,point:Pos2){self.draw(Some(point),Some(false),vec![]);}
    fn draw_global(&mut self,pointer:Option<Pos2>,pressed:Option<bool>,global:Pos2,displays:&[crate::display_layout::Display],scale:f32,extra:Vec<egui::Event>){
        let rooms=self.drag_geometry.as_ref().map_or_else(Vec::new,|geometry|geometry.rooms.clone());
        self.drag_geometry=Some(crate::app::DragGeometry{cursor:global,displays:displays.to_vec(),rooms,scale});
        self.draw(pointer,pressed,extra);
    }
}

#[test]
fn movable_island_preserves_legacy_configuration_and_roundtrips_new_options(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let h=H::new();
    let path=crate::model::config_dir().join("settings.txt");
    std::fs::write(&path,"dock=right\nisland_offset=-40\nmacos_side_handle=1\nside_panel_width=480\nside_panel_height=820\nfuture_option=keep\n").unwrap();
    let mut settings=crate::model::Settings::load();
    assert_eq!((settings.dock,settings.island_offset,settings.island_position,settings.resident_length),(Dock::Right,-40,None,0));
    assert_eq!((settings.side_panel_width,settings.side_panel_height,settings.macos_side_handle),(480,820,true));
    settings.island_position=Some([0.23,0.81]);settings.resident_length=520;settings.save();
    let saved=crate::model::Settings::load();assert_eq!((saved.island_position,saved.resident_length),(Some([0.23,0.81]),520));
    assert_eq!((saved.side_panel_width,saved.side_panel_height),(480,820));assert!(std::fs::read_to_string(path).unwrap().contains("future_option=keep"));
    drop(h);
}

#[test]
fn movable_island_collapsed_drag_moves_both_axes_without_middle_snap_or_drift(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{
        let mut island=Island::new(dock,[if dock==Dock::Right{1.0}else if dock==Dock::Left{0.0}else{0.35},0.3],220);island.settle(None);island.h.app.settings.save();
        let start=island.body().center();island.press(start);
        for _ in 0..12{island.draw(Some(start),None,vec![]);assert!(!island.h.app.open);}
        let end=Pos2::new(750.0,600.0);island.settle(Some(end));
        assert_eq!(island.h.app.settings.dock,Dock::Top,"the screen middle is a floating capsule");
        assert!(!island.h.app.open);let held=island.h.app.settings.island_position.unwrap();
        let body=island.body();assert!((body.center()-end).length()<3.0,"{dock:?}: {body:?}");
        island.settle(Some(end));assert_eq!(island.h.app.settings.island_position,Some(held),"holding must not accumulate native-frame movement");
        assert_ne!(crate::model::Settings::load().island_position,Some(held),"position is persisted on release");
        island.release(end);assert_eq!(crate::model::Settings::load().island_position,Some(held));
        island.settle(None);let saved=island.h.app.settings.island_position;island.settle(Some(Pos2::new(1100.0,300.0)));assert_eq!(island.h.app.settings.island_position,saved);
    }
}

#[test]
fn movable_island_drags_to_top_and_keeps_the_expanded_header_reachable(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{for length in [80,520]{
        let mut island=Island::new(dock,[0.5,0.5],length);island.settle(None);
        let start=island.body().center();island.press(start);
        let end=Pos2::new(370.0,0.0);island.settle(Some(end));island.release(end);island.settle(None);
        assert_eq!(island.h.app.settings.dock,Dock::Top);
        assert!(island.body().top().abs()<0.1,"{dock:?}, {length}: the resident reaches the screen top: {:?}, {:?}",island.body(),island.h.app.settings.island_position);
        assert_eq!(crate::model::Settings::load().island_position,island.h.app.settings.island_position);
        island.h.app.show_control_center();island.settle(None);
        assert!(island.resident().top().abs()<0.1);
        assert!((island.body().top()-island.resident().bottom()-crate::island_position::PANEL_GAP).abs()<0.1);
        let gear=island.response("page-settings",false);
        assert!(island.screen.contains_rect(gear.rect));
        assert!(island.out.as_ref().unwrap().hit.contains_rect(gear.rect));
        assert!(!island.h.app.settings_open);
    }}
}

#[test]
fn downward_island_retains_resident_and_crosses_a_fused_neck_at_every_edge(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for position in [[0.3,0.0],[0.5,0.5],[0.8,1.0]]{for length in [80,520]{
        let mut island=Island::new(Dock::Top,position,length);island.settle(None);
        let resident=island.resident();let clock_height=|island:&Island|island.h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text()=="09:41"=>Some(text.galley.rows[0].height()),_=>None}).unwrap();
        let before_font=clock_height(&island);let point=resident.center();island.settle(Some(point));
        assert!(island.h.app.open&&island.h.app.is_control_center());
        assert!((island.resident().min-resident.min).length()<0.001&&(island.resident().size()-resident.size()).length()<0.001);
        assert_eq!(clock_height(&island),before_font,"resident text must keep its original size");
        let panel=island.body();assert!((panel.size()-Vec2::new(PW,crate::control_center::TOP_H)).length()<0.01);
        assert!(!panel.intersects(resident));assert!(Rect::from_min_size(Pos2::ZERO,island.room).contains_rect(panel));
        if position[1]<0.7{assert!((panel.top()-resident.bottom()-8.0).abs()<0.01);}else{assert!((resident.top()-panel.bottom()-8.0).abs()<0.01);}
        let gap=Pos2::new(resident.center().x,if panel.top()>resident.bottom(){(resident.bottom()+panel.top())/2.0}else{(panel.bottom()+resident.top())/2.0});
        let local=gap+island.screen.min.to_vec2();let out=island.out.as_ref().unwrap();
        assert_eq!(out.hit_shapes.len(),1);assert!(crate::shape::contains_outline(&out.hit_shapes[0].outline(),local),"the painted neck must own native clicks and blur");
        let contour=out.hit_shapes[0].outline();assert_eq!(out.glass.as_ref().unwrap()[0].outline(),contour,"native blur and input must follow the complete fused silhouette");
        for point in [Pos2::new(out.hit.left()+0.5,local.y),Pos2::new(out.hit.right()-0.5,local.y)]{
            assert!(out.hit.contains(point)&&!crate::shape::contains_outline(&contour,point),"unpainted space beside the neck still belongs to the desktop");
        }
        island.settle(Some(gap));assert!(island.h.app.open,"crossing the fused neck must retain hover");
        island.h.app.enqueue_alert(Alert{source:AlertSource::LocalTest,title:"连接区域检查".into(),body:"布局验证已完成".into(),page:crate::app::AGENT_PAGE,at:island.h.t,duration:30.0},false);
        island.settle(Some(gap));let out=island.out.as_ref().unwrap();let notice=out.notice_hit.expect("an independent notification lane remains available");
        assert!(!notice.intersects(out.hit)&&island.screen.contains_rect(notice),"a notification must clear the complete fused material");
        assert_eq!(out.notice_shapes[0].outline(),out.glass.as_ref().unwrap()[1].outline(),"notice blur and native input must use the same separate contour");
        let gear=island.world(island.response("page-settings",false).rect.center());island.settle(Some(gear));assert!(island.h.app.open);
        let music=island.world(island.response("header-music",false).rect.center());island.press(music);island.release(music);assert_eq!(island.h.app.page(),crate::app::MUSIC_PAGE);assert!(!island.h.app.settings_open);
        island.h.t+=5.0;island.settle(None);assert!(!island.h.app.open);assert!((island.resident().min-resident.min).length()<0.001);
    }}
}

#[test]
fn fluid_join_reverses_the_live_spring_without_detaching_input_or_blur(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for position in [[0.02,0.0],[0.98,0.98]]{for length in [80,520]{
        let mut island=Island::new(Dock::Top,position,length);island.settle(None);
        let resident=island.resident();island.h.app.show_control_center();
        let verify=|island:&Island|{
            let out=island.out.as_ref().unwrap();assert_eq!(out.hit_shapes.len(),1);
            let outline=out.hit_shapes[0].outline();assert_eq!(out.glass.as_ref().unwrap()[0].outline(),outline);
            assert!(outline.iter().all(|p|p.is_finite()&&island.screen.expand(0.001).contains(*p)));
            assert!(crate::shape::contains_outline(&outline,island.resident().center()+island.screen.min.to_vec2()));
            if island.h.app.progress()>0.003{
                assert!(crate::shape::contains_outline(&outline,island.body().center()+island.screen.min.to_vec2()));
            }
        };
        for _ in 0..60{island.draw(None,None,vec![]);verify(&island);}
        island.h.t+=5.0;island.draw(None,None,vec![]);
        let mut closing=false;
        for _ in 0..60{
            island.draw(None,None,vec![]);verify(&island);
            if !island.h.app.open&&island.h.app.progress()<0.8&&island.h.app.progress()>0.15{closing=true;break;}
        }
        assert!(closing,"exercise a live collapse");
        let height=island.h.app.h.pos;let velocity=island.h.app.h.vel;
        assert!(velocity<0.0);island.h.app.show_control_center();
        assert_eq!((island.h.app.h.pos,island.h.app.h.vel),(height,velocity),"reversal changes the spring target, preserving position and velocity");
        for _ in 0..60{island.draw(None,None,vec![]);verify(&island);}
        assert!(island.h.app.open&&(island.h.app.progress()-1.0).abs()<0.001);
        assert_eq!(island.resident(),resident);
        let out=island.out.as_ref().unwrap();let outline=out.hit_shapes[0].outline();
        let panel=island.body();let gap=Pos2::new(resident.center().x,if panel.top()>resident.bottom(){(resident.bottom()+panel.top())*0.5}else{(panel.bottom()+resident.top())*0.5});
        let gap=gap+island.screen.min.to_vec2();
        assert!(crate::shape::contains_outline(&outline,gap));
        for ppp in [1.0,1.4,2.0]{
            let pieces=crate::app::blur_pieces(&out.hit_shapes,ppp);
            assert!(pieces.iter().any(|piece|gap.x*ppp>=piece.x&&gap.x*ppp<=piece.x+piece.w&&gap.y*ppp>=piece.y&&gap.y*ppp<=piece.y+piece.h));
            // The new general-outline path must retain the concave corner,
            // instead of restoring the old bounding-rectangle blur fallback.
            let bounds=Rect::from_points(&outline);let corner=bounds.min+Vec2::splat(1.0);
            if !crate::shape::contains_outline(&outline,corner){assert!(!pieces.iter().any(|piece|corner.x*ppp>=piece.x&&corner.x*ppp<piece.x+piece.w&&corner.y*ppp>=piece.y&&corner.y*ppp<piece.y+piece.h));}
        }
    }}
}

#[test]
fn downward_island_leave_grace_reentry_pin_keyboard_and_privacy_share_one_lifecycle(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{
        let mut island=Island::new(dock,[0.5,0.4],220);island.settle(None);let point=island.body().center();island.settle(Some(point));assert!(island.h.app.open);
        for _ in 0..29{let output=island.draw(None,None,vec![]);assert!(island.h.app.open,"less than 500ms outside retains content");assert!(output.viewport_output[&egui::ViewportId::ROOT].repaint_delay<=std::time::Duration::from_millis(500),"the leave deadline must wake without another pointer event");}
        island.draw(Some(point),None,vec![]);for _ in 0..29{island.draw(None,None,vec![]);assert!(island.h.app.open,"reentry resets the deadline");}
        for _ in 0..5{island.draw(None,None,vec![]);}assert!(!island.h.app.open);
        island.h.app.show_control_center();island.settle(None);
        let pin=island.world(island.response("panel-pin",false).rect.center());island.press(pin);island.release(pin);assert!(island.h.app.panel_pinned);
        assert!(!island.h.app.settings_open);island.h.app.foreground_changed();island.h.t+=12.0;island.settle(None);assert!(island.h.app.open&&island.h.app.panel_pinned);
        let response=island.response("panel-pin",false);island.h.ctx.memory_mut(|memory|memory.request_focus(response.id));
        island.draw(None,None,vec![]);island.draw(None,None,vec![egui::Event::Key{key:egui::Key::Space,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
        island.draw(None,None,vec![egui::Event::Key{key:egui::Key::Space,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);
        assert!(!island.h.app.panel_pinned,"keyboard toggles the same pin");
        for _ in 0..28{island.draw(None,None,vec![]);assert!(island.h.app.open);}island.settle(None);assert!(!island.h.app.open);
        island.h.app.show_control_center();island.settle(None);let pin=island.world(island.response("panel-pin",false).rect.center());island.press(pin);island.release(pin);
        island.draw(None,None,vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);assert!(!island.h.app.panel_pinned);
        island.h.app.panel_pinned=true;island.h.screen_locked=true;island.draw(Some(point),None,vec![]);assert!(!island.h.app.panel_pinned&&!island.h.app.open,"lock-screen privacy overrides both the pin and pointer hover");
    }
}

#[test]
fn movable_island_snaps_only_at_left_and_right_and_can_cancel_a_drag(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut island=Island::new(Dock::Top,[0.5,0.5],0);island.settle(None);island.h.app.settings.save();let original=island.h.app.settings.island_position;
    let start=island.body().center();island.press(start);
    for (point,dock) in [(Pos2::new(10.0,400.0),Dock::Left),(Pos2::new(32.0,400.0),Dock::Left),(Pos2::new(700.0,500.0),Dock::Top),(Pos2::new(1435.0,450.0),Dock::Right),(Pos2::new(1408.0,450.0),Dock::Right),(Pos2::new(900.0,30.0),Dock::Top)]{
        island.settle(Some(point));assert_eq!(island.h.app.settings.dock,dock,"{point:?}");
        let body=island.body();assert!(Rect::from_min_size(Pos2::ZERO,island.room).contains_rect(body));
        assert!(!island.h.app.open);
    }
    island.draw(Some(Pos2::new(900.0,30.0)),None,vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert_eq!((island.h.app.settings.dock,island.h.app.settings.island_position),(Dock::Top,original));
    assert_eq!(crate::model::Settings::load().island_position,original);
}

#[test]
fn cross_display_drag_tracks_the_target_monitor_and_persists_it_on_release(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let primary=crate::display_layout::Display{id:"primary".into(),label:"内屏".into(),bounds:crate::display_layout::Bounds{left:0,top:0,right:1440,bottom:900},work_top:0,primary:true};
    let external=crate::display_layout::Display{id:"external".into(),label:"外屏".into(),bounds:crate::display_layout::Bounds{left:-1512,top:98,right:0,bottom:1080},work_top:98,primary:false};
    let displays=[primary.clone(),external.clone()];
    let mut island=Island::new(Dock::Top,[0.55,0.4],0);island.h.app.settings.display_id=primary.id.clone();island.settle(None);
    let start=island.body().center();
    let source=Pos2::new(start.x,start.y);
    island.draw_global(Some(start),None,source,&displays,1.0,vec![]);
    island.draw_global(Some(start),Some(true),source,&displays,1.0,vec![]);
    for _ in 0..12{island.draw_global(Some(start),None,source,&displays,1.0,vec![]);}

    let edge=Pos2::new(external.bounds.left as f32+10.0,external.bounds.top as f32+500.0);
    island.backing=2.0;
    for _ in 0..12{island.draw_global(Some(start),None,edge,&displays,1.0,vec![]);}
    assert_eq!(island.h.app.settings.display_id,external.id);
    assert_eq!(island.h.app.settings.dock,Dock::Left,"目标屏边缘吸附沿目标屏坐标计算");
    let position=island.h.app.settings.island_position.unwrap();
    assert!(position[0]<0.1&&position[1]>0.3&&position[1]<0.8,"{position:?}");

    island.draw_global(Some(start),Some(false),edge,&displays,1.0,vec![]);
    let released=island.h.app.settings.island_position;
    assert_eq!(crate::model::Settings::load().display_id,external.id,"松手后目标屏偏好应落盘");
    assert_eq!(crate::model::Settings::load().island_position,released);
}

#[test]
fn cross_display_escape_restores_the_source_display_position_and_dock(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let primary=crate::display_layout::Display{id:"primary".into(),label:"内屏".into(),bounds:crate::display_layout::Bounds{left:0,top:0,right:1440,bottom:900},work_top:0,primary:true};
    let external=crate::display_layout::Display{id:"external".into(),label:"外屏".into(),bounds:crate::display_layout::Bounds{left:-1512,top:98,right:0,bottom:1080},work_top:98,primary:false};
    let displays=[primary.clone(),external.clone()];
    let mut island=Island::new(Dock::Right,[0.86,0.68],0);island.h.app.settings.display_id=primary.id.clone();island.settle(None);
    island.h.app.settings.save();
    let original=(island.h.app.settings.display_id.clone(),island.h.app.settings.dock,island.h.app.settings.island_position);
    let start=island.body().center();let source=Pos2::new(start.x,start.y);
    island.draw_global(Some(start),None,source,&displays,1.0,vec![]);
    island.draw_global(Some(start),Some(true),source,&displays,1.0,vec![]);
    for _ in 0..12{island.draw_global(Some(start),None,source,&displays,1.0,vec![]);}
    let target=Pos2::new(external.bounds.left as f32+700.0,external.bounds.top as f32+300.0);
    island.backing=2.0;
    for _ in 0..12{island.draw_global(Some(start),None,target,&displays,1.0,vec![]);}
    assert_eq!(island.h.app.settings.display_id,external.id);
    assert_ne!(island.h.app.settings.island_position,original.2);
    assert!(island.out.as_ref().unwrap().editing,"native shell must grant keyboard input while dragging so Escape reaches the island");

    island.draw_global(Some(start),None,target,&displays,1.0,vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert_eq!((island.h.app.settings.display_id.as_str(),island.h.app.settings.dock,island.h.app.settings.island_position),
        (original.0.as_str(),original.1,original.2));
    assert!(!island.out.as_ref().unwrap().editing,"cancelling releases the drag's keyboard capture");
    assert_eq!(crate::model::Settings::load().display_id,original.0,"取消不应保存临时目标屏");
    for _ in 0..12{
        island.draw_global(Some(start),None,target,&displays,1.0,vec![]);
        assert_eq!((island.h.app.settings.display_id.as_str(),island.h.app.settings.dock,island.h.app.settings.island_position),(original.0.as_str(),original.1,original.2),"Escape cancels the rest of the same mouse press");
        assert!(!island.out.as_ref().unwrap().editing);
    }
    island.draw_global(Some(start),Some(false),target,&displays,1.0,vec![]);
    assert_eq!(crate::model::Settings::load().display_id,original.0);
}

#[test]
fn expanded_page_remains_open_while_dragging_to_another_display(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let primary=crate::display_layout::Display{id:"primary".into(),label:"内屏".into(),bounds:crate::display_layout::Bounds{left:0,top:0,right:1440,bottom:900},work_top:0,primary:true};
    let external=crate::display_layout::Display{id:"external".into(),label:"错列外屏".into(),bounds:crate::display_layout::Bounds{left:-1512,top:98,right:0,bottom:1080},work_top:98,primary:false};
    let displays=[primary.clone(),external.clone()];
    let mut island=Island::new(Dock::Top,[0.4,0.36],0);island.h.app.settings.display_id=primary.id.clone();
    island.h.app.show_page(crate::app::WEATHER_PAGE);island.settle(None);assert!(island.h.app.open);
    let start=island.world(island.response("island-position",true).rect.center());let source=start;
    island.draw_global(Some(start),None,source,&displays,1.0,vec![]);
    island.draw_global(Some(start),Some(true),source,&displays,1.0,vec![]);
    for _ in 0..12{island.draw_global(Some(start),None,source,&displays,1.0,vec![]);}
    let target=Pos2::new(external.bounds.left as f32+700.0,external.bounds.top as f32+380.0);
    island.backing=2.0;
    for _ in 0..12{island.draw_global(Some(start),None,target,&displays,1.0,vec![]);}
    assert_eq!(island.h.app.settings.display_id,external.id);
    assert_eq!(island.h.app.page(),crate::app::WEATHER_PAGE);
    assert!(island.h.app.open,"拖动换屏不能关闭已展开页");
    island.draw_global(Some(start),Some(false),target,&displays,1.0,vec![]);
    assert_eq!(crate::model::Settings::load().display_id,external.id);
}

#[test]
fn cross_display_drag_uses_dock_work_areas_without_native_pointer_drift(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let primary=crate::display_layout::Display{id:"primary".into(),label:"HDMI".into(),bounds:crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080},work_top:25,primary:true};
    let external=crate::display_layout::Display{id:"external".into(),label:"内屏".into(),bounds:crate::display_layout::Bounds{left:-1512,top:98,right:0,bottom:1080},work_top:130,primary:false};
    let displays=[primary.clone(),external.clone()];
    // The physical frame owns monitor selection; normalization excludes the
    // bottom Dock on the source and the left Dock on the destination.
    let rooms=[(primary.id.clone(),crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1022}),
        (external.id.clone(),crate::display_layout::Bounds{left:-1454,top:98,right:0,bottom:1080})];
    let mut island=Island::new(Dock::Top,[0.6,0.63],0);island.h.app.settings.display_id=primary.id.clone();
    island.drag_geometry=Some(crate::app::DragGeometry{cursor:Pos2::ZERO,displays:displays.to_vec(),rooms:rooms.to_vec(),scale:1.0});
    for _ in 0..100{island.draw_global(None,None,Pos2::ZERO,&displays,1.0,vec![]);}
    let start=island.body().center();let source=start;
    island.draw_global(Some(start),Some(true),source,&displays,1.0,vec![]);
    for _ in 0..12{island.draw_global(Some(start),None,source,&displays,1.0,vec![]);}
    let position=island.h.app.settings.island_position.unwrap();
    assert!((position[0]-0.6).abs()<0.0001&&(position[1]-0.63).abs()<0.0001,"holding the island must not normalize against the 1080pt frame: {position:?}");
    let shifted=source+Vec2::new(30.0,24.0);
    for _ in 0..100{island.draw_global(Some(start),None,shifted,&displays,1.0,vec![]);}
    assert!((island.body().center()-shifted).length()<1.0,"source drag must follow the native cursor despite the Dock");

    island.backing=2.0;
    let native_target=Pos2::new(-1454.0+1454.0*0.55,98.0+982.0*0.62);
    for _ in 0..100{island.draw_global(Some(start),None,native_target,&displays,1.0,vec![]);}
    assert_eq!(island.h.app.settings.display_id,external.id);
    let target_room_point=Pos2::new(native_target.x+1454.0,native_target.y-98.0);
    assert!((island.body().center()-target_room_point).length()<1.0,"target drag must remove the side Dock origin before scaling: {:?}",island.body());
    island.draw_global(Some(start),Some(false),native_target,&displays,1.0,vec![]);
    let saved=crate::model::Settings::load();assert_eq!(saved.display_id,external.id);assert_eq!(saved.island_position,island.h.app.settings.island_position);

    island.h.app.enqueue_alert(Alert{source:AlertSource::LocalTest,title:"换屏后通知".into(),body:"独立通知布局".into(),page:crate::app::AGENT_PAGE,at:island.h.t,duration:30.0},false);
    for _ in 0..100{island.draw_global(None,None,native_target,&displays,1.0,vec![]);}
    let out=island.out.as_ref().unwrap();let notice=out.notice_hit.expect("the destination keeps an independent notification lane");
    assert!(island.screen.contains_rect(notice)&&!notice.intersects(out.hit));
    assert_eq!(out.notice_shapes[0].outline(),out.glass.as_ref().unwrap()[1].outline());
}

#[test]
fn cross_display_first_movement_can_enter_another_monitor_and_escape_still_restores_source(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let primary=crate::display_layout::Display{id:"primary".into(),label:"主屏".into(),bounds:crate::display_layout::Bounds{left:0,top:0,right:1440,bottom:900},work_top:0,primary:true};
    let external=crate::display_layout::Display{id:"external".into(),label:"上方错列屏".into(),bounds:crate::display_layout::Bounds{left:1600,top:-500,right:3040,bottom:400},work_top:-476,primary:false};
    let displays=[primary.clone(),external.clone()];
    let mut island=Island::new(Dock::Top,[0.5,0.5],0);island.settle(None);island.h.app.settings.save();
    let original=island.h.app.settings.island_position;let start=island.body().center();
    island.draw_global(Some(start),None,start,&displays,1.0,vec![]);
    island.draw_global(Some(start),Some(true),start,&displays,1.0,vec![]);
    let target=Pos2::new(2320.0,-100.0);
    island.draw_global(Some(start),None,target,&displays,1.0,vec![]);
    assert_eq!(island.h.app.settings.display_id,external.id,"the first drag event resolves its source from the window, even after the pointer crossed a gap");
    let saved_target=island.h.app.settings.island_position.unwrap();
    let rendered=island.h.app.placement_position().unwrap();
    assert!((Vec2::from(rendered)-Vec2::from(saved_target)).length()<0.001,"normalized glide cannot carry source-screen coordinates onto another screen");
    island.draw_global(Some(start),None,target,&displays,1.0,vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);
    assert!(island.h.app.settings.display_id.is_empty(),"Escape restores following the primary display as well as explicit ids");
    assert_eq!(island.h.app.settings.island_position,original);
    assert!(crate::model::Settings::load().display_id.is_empty());
}

#[test]
fn cross_display_saved_target_falls_back_visibly_and_reconnects_without_rewriting_preference(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let primary=crate::display_layout::Display{id:"primary".into(),label:"HDMI".into(),bounds:crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080},work_top:25,primary:true};
    let external=crate::display_layout::Display{id:"external".into(),label:"内屏".into(),bounds:crate::display_layout::Bounds{left:-1512,top:98,right:0,bottom:1080},work_top:130,primary:false};
    let displays=[primary.clone(),external.clone()];
    let mut island=Island::new(Dock::Top,[0.67,0.72],0);island.h.app.settings.display_id=external.id.clone();island.h.app.settings.save();
    let saved=crate::model::Settings::load();assert_eq!(saved.display_id,external.id);assert_eq!(saved.island_position,Some([0.67,0.72]));
    island.h.app.settings=saved;
    for _ in 0..100{island.draw_global(None,None,Pos2::ZERO,&displays,1.0,vec![]);}
    let before=island.body();let preferred=(island.h.app.settings.display_id.clone(),island.h.app.settings.island_position);
    for _ in 0..100{island.draw_global(None,None,Pos2::ZERO,&[primary.clone()],1.0,vec![]);}
    assert_eq!(crate::display_layout::selected(&island.drag_geometry.as_ref().unwrap().displays,&island.h.app.settings.display_id).unwrap().id,primary.id);
    assert!(Rect::from_min_size(Pos2::ZERO,island.room).contains_rect(island.body()),"断开的显示器回退到可见主屏计划");
    assert_eq!((island.h.app.settings.display_id.clone(),island.h.app.settings.island_position),preferred);

    for _ in 0..100{island.draw_global(None,None,Pos2::ZERO,&displays,1.0,vec![]);}
    assert_eq!(crate::display_layout::selected(&island.drag_geometry.as_ref().unwrap().displays,&island.h.app.settings.display_id).unwrap().id,external.id);
    assert_eq!((island.h.app.settings.display_id.clone(),island.h.app.settings.island_position),preferred);
    assert!((island.body().center()-before.center()).length()<0.01,"重连恢复所保存屏幕的相对位置");
    assert_eq!(crate::model::Settings::load().display_id,external.id);
}

#[test]
fn movable_island_expanded_drag_keeps_header_controls_and_clamps_at_the_bottom(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{
        let mut island=Island::new(dock,[0.5,0.7],520);island.h.app.show_page(crate::app::WEATHER_PAGE);island.settle(None);
        assert!(island.h.app.open);let before=island.body().size();
        let start=island.world(island.response("island-position",true).rect.center());island.press(start);
        let end=start+Vec2::new(if dock==Dock::Left{300.0}else if dock==Dock::Right{-300.0}else{110.0},200.0);
        island.settle(Some(end));assert!(island.h.app.open,"a header drag preserves the expanded state");
        assert_eq!(island.h.app.settings.dock,Dock::Top);island.release(end);island.settle(None);
        // Open explicitly after leaving; the body moves upward to fit, while
        // the stored collapsed anchor remains close to the bottom.
        island.h.app.show_page(crate::app::WEATHER_PAGE);island.settle(None);
        let body=island.body();assert!((body.width()-PW).abs()<0.1&&(body.height()-crate::control_center::TOP_H).abs()<0.1);
        assert!(body.bottom()<=island.room.y-7.9);assert_eq!((island.h.app.settings.side_panel_width,island.h.app.settings.side_panel_height),(292,532));
        if dock==Dock::Top{assert!((body.size()-before).length()<0.01);}
        let gear=island.response("page-settings",false);assert!(island.out.as_ref().unwrap().hit.contains_rect(gear.rect));
    }
}

#[test]
fn movable_island_resident_lengths_keep_fonts_controls_expansion_and_notifications(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{for length in [0,80,419,520]{
        let mut island=Island::new(dock,[0.5,0.94],length);island.settle(None);let collapsed=island.body();
        let min=if dock.is_side(){island.h.app.side_bar_len()}else{let mut layout=crate::bar_layout::Layout::new(true,false,true);layout.widen(crate::bar_layout::WEATHER,island.h.app.weather_pill_extra(&island.h.ctx).unwrap_or(0.0));layout.companions(island.h.app.settings.pet,island.h.app.module_on(crate::app::AGENT_PAGE));layout.width};
        let actual=if dock.is_side(){collapsed.height()}else{collapsed.width()};assert!((actual-min.max(length as f32)).abs()<0.2,"{dock:?}/{length}: {actual}/{min}");
        let clock_size=island.h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text()==if dock.is_side(){"09"}else{"09:41"}=>Some(text.galley.rows[0].height()),_=>None});
        assert!(clock_size.is_some());
        let weather=island.response(if dock.is_side(){"weather-side"}else{"weather-pill"},false);assert!(island.out.as_ref().unwrap().hit.contains_rect(weather.rect));
        island.h.app.enqueue_alert(Alert{source:AlertSource::Agent{session_id:"synthetic".into(),turn_key:1},title:"图形检查已完成".into(),body:"Codex · 等待确认".into(),page:crate::app::AGENT_PAGE,at:island.h.t,duration:30.0},false);island.settle(None);
        let out=island.out.as_ref().unwrap();let strip=out.notice_hit.expect("a separate notice stays visible near the display bottom");
        assert!(!strip.intersects(Rect::from_points(out.hit_outline.as_ref().unwrap())));assert!(island.screen.contains_rect(strip));
        island.h.app.show_page(crate::app::WEATHER_PAGE);island.settle(None);assert!(island.h.app.progress().is_finite());
        let expanded=island.body();assert!((expanded.width()-if dock.is_side(){292.0}else{PW}).abs()<0.1);assert!((expanded.height()-if dock.is_side(){532.0}else{crate::control_center::TOP_H}).abs()<0.1);
        assert_eq!((island.h.app.settings.side_panel_width,island.h.app.settings.side_panel_height),(292,532));
    }}
}

#[test]
fn movable_island_short_clicks_open_weather_and_music_without_starting_a_drag(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{for length in [80,520]{for (target,id) in [(crate::app::WEATHER_PAGE,if dock.is_side(){"weather-side"}else{"weather-pill"}),(crate::app::MUSIC_PAGE,if dock.is_side(){"side-music"}else{"header-music"})]{
        let mut island=Island::new(dock,[0.5,0.5],length);island.settle(None);let saved=island.h.app.settings.island_position;
        let point=island.world(island.response(id,false).rect.center());island.press(point);island.release(point);
        assert!(island.h.app.open);assert_eq!(island.h.app.page(),target);assert_eq!(island.h.app.settings.island_position,saved);
    }}}
}

#[cfg(target_os="macos")]
#[test]
fn movable_island_handle_and_lyrics_lengths_are_independent_in_reduced_motion(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for handle in [false,true]{for length in [0,80,520]{
        let mut island=Island::new(if handle{Dock::Right}else{Dock::Top},[0.5,0.5],length);island.h.app.settings.macos_side_handle=handle;
        island.h.ctx.data_mut(|data|data.insert_temp(egui::Id::new("macos-accessibility-preferences"),(std::time::Instant::now(),2u32)));
        if !handle{island.h.app.settings.island_lyrics=true;island.h.app.set_lyrics("云间漫步","轻屿电台",224.0,crate::lyrics::parse_lrc("[00:00]风带来了窗外的消息\n[00:20]把日子过成一首轻柔的歌"));}
        island.settle(None);let body=island.body();
        assert!((if handle{body.height()-52.0f32.max(length as f32)}else{body.width()-300.0f32.max(length as f32)}).abs()<0.1);
        assert!((if handle{body.width()-6.0}else{body.height()-30.0}).abs()<0.1);
        island.h.ctx.data_mut(|data|data.insert_temp(egui::Id::new("macos-accessibility-preferences"),(std::time::Instant::now(),2u32)));
        island.h.app.show_page(crate::app::WEATHER_PAGE);island.draw(None,None,vec![]);assert_eq!(island.h.app.progress(),1.0);
        assert_eq!((island.h.app.settings.side_panel_width,island.h.app.settings.side_panel_height),(292,532));
    }}
}

#[test]
#[ignore="Actual synthetic egui renders: QINGYU_POSITION_GALLERY=directory"]
fn movable_island_gallery(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_POSITION_GALLERY").expect("gallery directory"));std::fs::create_dir_all(&dest).unwrap();
    for (name,dock,position,length,expanded,notice,lyrics,handle) in [
        ("top-expanded-short",Dock::Top,[0.35,0.0],80,true,true,false,false),("top-expanded-long",Dock::Top,[0.65,0.0],520,true,false,false,false),
        ("middle-pinned",Dock::Top,[0.5,0.4],520,true,true,false,false),
        ("free-auto",Dock::Top,[0.5,0.5],0,false,false,false,false),("free-short",Dock::Top,[0.3,0.68],80,false,false,false,false),("free-long",Dock::Top,[0.7,0.25],520,false,false,false,false),
        ("bottom-expanded-short",Dock::Top,[0.75,0.98],80,true,true,false,false),("bottom-expanded-long",Dock::Top,[0.75,0.98],520,true,true,false,false),
        ("left-short",Dock::Left,[0.0,0.33],80,false,false,false,false),("right-long",Dock::Right,[1.0,0.6],520,false,true,false,false),
        ("left-expanded",Dock::Left,[0.0,0.98],520,true,true,false,false),("right-expanded",Dock::Right,[1.0,0.02],80,true,true,false,false),
        ("lyrics-short",Dock::Top,[0.45,0.5],80,false,false,true,false),("lyrics-long",Dock::Top,[0.55,0.5],520,false,false,true,false),
        ("handle-long",Dock::Right,[1.0,0.5],320,false,false,false,true),
    ]{
        if std::env::var("QINGYU_POSITION_CASE").ok().is_some_and(|only|only!=name){continue;}
        let mut island=Island::new(dock,position,length);island.h.ctx.set_pixels_per_point(2.0);island.h.app.settings.macos_side_handle=handle;
        if lyrics{island.h.app.settings.island_lyrics=true;island.h.app.set_lyrics("云间漫步","轻屿电台",224.0,crate::lyrics::parse_lrc("[00:00]风带来了窗外的消息\n[00:20]把日子过成一首轻柔的歌\n[00:40]一起漫步在云端"));}
        if expanded{island.h.app.show_page(crate::app::WEATHER_PAGE);}
        if name=="middle-pinned"{island.h.app.show_control_center();island.h.app.panel_pinned=true;}
        if notice{island.h.app.enqueue_alert(Alert{source:AlertSource::Agent{session_id:"synthetic".into(),turn_key:1},title:"窗口布局调整".into(),body:"Codex 已完成 · 点击查看".into(),page:crate::app::AGENT_PAGE,at:island.h.t,duration:30.0},false);}
        let first=island.draw(None,None,vec![]);let second=island.draw(None,None,vec![]);let size=island.h.app.canvas_size();let mut raster=crate::preview_raster::Raster::new((size.x*2.0).ceil() as usize,(size.y*2.0).ceil() as usize);raster.update_textures(&first.textures_delta);raster.update_textures(&second.textures_delta);
        for index in 0..100{
            let output=island.draw(None,None,vec![]);raster.update_textures(&output.textures_delta);
            if index==99{
                raster.buf.fill([0.15,0.21,0.25,1.0]);raster.draw(&island.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw((size.x*2.0).ceil() as u32,(size.y*2.0).ceil() as u32,rgba).unwrap();
                let out=island.out.as_ref().unwrap();let body=out.hit;let bounds=out.notice_hit.map_or(body,|notice|notice.union(body)).expand(16.0).intersect(Rect::from_min_size(Pos2::ZERO,size));
                image::imageops::crop_imm(&image,(bounds.left()*2.0).floor() as u32,(bounds.top()*2.0).floor() as u32,(bounds.width()*2.0).floor() as u32,(bounds.height()*2.0).floor() as u32).to_image().save(dest.join(format!("{name}.png"))).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
#[ignore="Actual egui entrance/steady/exit renders: QINGYU_FLUID_JOIN_GALLERY=directory"]
fn fluid_join_motion_gallery(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_FLUID_JOIN_GALLERY").expect("gallery directory"));std::fs::create_dir_all(&dest).unwrap();
    let selected=std::env::var("QINGYU_FLUID_JOIN_CASE").ok();
    let mut timings=String::new();
    for (skin,label) in [(Skin::GlassDark,"dark"),(Skin::GlassLight,"light"),(Skin::Classic,"classic"),(Skin::Fluid,"fluid")]{
        for (position,lane) in [([0.5,0.0],"top"),([0.9,0.98],"above")]{for length in [80,520]{
            if selected.as_ref().is_some_and(|selected|selected!=&format!("{label}-{lane}-{length}")){continue;}
            let mut island=Island::new(Dock::Top,position,length);island.h.app.settings.skin=skin;island.h.ctx.set_pixels_per_point(2.0);
            // The first frame receives synthetic media and length settings.
            // Let their resident target reach the placement plan before
            // fixing the buffer/crop, exactly as the native shell does.
            let first=island.draw(None,None,vec![]);let second=island.draw(None,None,vec![]);let size=island.h.app.canvas_size();
            let mut raster=crate::preview_raster::Raster::new((size.x*2.0).ceil() as usize,(size.y*2.0).ceil() as usize);raster.update_textures(&first.textures_delta);raster.update_textures(&second.textures_delta);
            let win=Rect::from_min_size(Pos2::ZERO,size);
            let panel=island.h.app.horizontal_panel_rect(win,Vec2::new(PW,crate::control_center::TOP_H));
            let bounds=panel.union(island.h.app.resident_rect(win)).expand(18.0).intersect(win);
            let save=|island:&Island,output:egui::FullOutput,raster:&mut crate::preview_raster::Raster,phase:&str|{
                raster.buf.fill(if skin==Skin::GlassLight{[0.69,0.76,0.78,1.0]}else{[0.15,0.21,0.25,1.0]});
                raster.draw(&island.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                let image=image::RgbaImage::from_raw(raster.w as u32,raster.h as u32,rgba).unwrap();
                image::imageops::crop_imm(&image,(bounds.left()*2.0).floor() as u32,(bounds.top()*2.0).floor() as u32,(bounds.width()*2.0).floor() as u32,(bounds.height()*2.0).floor() as u32)
                    .to_image().save(dest.join(format!("{label}-{lane}-{length}-{phase}.png"))).unwrap();
                raster.free_textures(&output.textures_delta);
            };
            for _ in 0..80{let output=island.draw(None,None,vec![]);raster.update_textures(&output.textures_delta);raster.free_textures(&output.textures_delta);}
            island.h.app.show_control_center();
            let mut entered=false;
            let mut draw_time=std::time::Duration::ZERO;
            for frame in 0..70{
                let started=std::time::Instant::now();let output=island.draw(None,None,vec![]);draw_time+=started.elapsed();raster.update_textures(&output.textures_delta);
                if !entered&&island.h.app.progress()>=0.32{save(&island,output,&mut raster,"entrance");entered=true;}
                else if frame==69{save(&island,output,&mut raster,"steady");}
                else{raster.free_textures(&output.textures_delta);}
            }
            let mut exited=false;
            island.h.app.foreground_changed();island.h.t+=5.0;
            for frame in 0..100{
                let output=island.draw(None,None,vec![]);raster.update_textures(&output.textures_delta);
                if !island.h.app.open&&!exited&&island.h.app.progress()<=0.48{save(&island,output,&mut raster,"exit");exited=true;}
                else if frame==99{save(&island,output,&mut raster,"closed");}
                else{raster.free_textures(&output.textures_delta);}
            }
            assert!(entered&&exited);
            timings.push_str(&format!("{label}-{lane}-{length}: actual egui entrance+steady 70 frames {:.3}ms/frame (debug, excludes raster/export)\n",draw_time.as_secs_f64()*1000.0/70.0));
        }}
    }
    std::fs::write(dest.join(selected.map_or_else(||"timings.txt".into(),|selected|format!("timings-{selected}.txt"))),timings).unwrap();
}
