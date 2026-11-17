//! Real egui frames and pointer events; all messages/tasks are synthetic fixtures.
use super::*;
use crate::{agent_monitor::{AgentKind,Completion,Notice,NoticeKind,Session,Snapshot,Status as AgentStatus},app::{AGENT_PAGE,NOTIFICATIONS_PAGE},extras::{Alert,AlertSource}};

fn island(dock:Dock)->Side{
    let mut h=H::new();h.app.settings.dock=dock;h.app.settings.macos_side_handle=false;
    h.app.settings.island_shape=crate::model::IslandShape::Pill;
    h.app.settings.notification_preview=true;
    h.app.settings.read_notifications=true;h.app.settings.macos_dock_badges=true;
    h.app.settings.skin=crate::model::Skin::GlassDark;
    Side{h,out:None}
}

fn message(app:&mut App,index:usize){
    app.enqueue_alert(Alert{source:AlertSource::LocalTest,title:format!("设计群 · 新消息 {index}"),body:"周四下午三点评审，资料已经准备好了".into(),page:NOTIFICATIONS_PAGE,at:app.t,duration:8.0},false);
}

fn task(turn:u64,status:AgentStatus)->Session{
    Session{id:"codex:demo-task".into(),agent:AgentKind::Codex,status,turn_key:turn,
        project:"/demo/qingyu".into(),title:"完善岛内通知与任务提醒".into(),started_at:1_794_699_940,
        finished_at:(status==AgentStatus::Completed).then_some(1_794_700_002),..Default::default()}
}

fn lifecycle(s:&mut Side,session:Session,kind:NoticeKind){
    let at_ms=1_794_700_000_000+(s.h.t*1000.0)as u64;
    let sequence=s.h.app.agent_snapshot.notices.iter().map(|event|event.sequence).max().unwrap_or(0)+1;
    s.h.app.agent_snapshot.notices.push(Notice{sequence,at_ms,kind,session:session.clone()});
    if kind==NoticeKind::Completed{
        let sequence=s.h.app.agent_snapshot.completions.iter().map(|event|event.sequence).max().unwrap_or(0)+1;
        s.h.app.agent_snapshot.completions.push(Completion{sequence,at_ms,session:session.clone()});
    }
    if let Some(current)=s.h.app.agent_snapshot.sessions.iter_mut().find(|current|current.id==session.id){*current=session;}
    else{s.h.app.agent_snapshot.sessions.push(session);}
}

fn next_completion(s:&mut Side,turn:u64){lifecycle(s,task(turn,AgentStatus::Completed),NoticeKind::Completed);}

fn has_kind(alert:&Alert,kind:NoticeKind)->bool{alert.source.agent_notice().is_some_and(|(_,_,current)|current==kind)}

fn opened_agent(s:&Side,id:&str){
    assert!(s.h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenAgent{session_id,..}if session_id==id)),"the notification must target the external conversation app");
    assert!(s.h.app.agent_expanded.is_none());
}

#[test]
fn island_notice_uses_each_agents_identity_icon_and_completion_without_history(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (kind,prefix) in [(AgentKind::Codex,"codex"),(AgentKind::Claude,"claude"),(AgentKind::Hermes,"hermes")]{
        let mut s=island(Dock::Right);s.h.app.agent_snapshot.generation=1;s.frame(vec![]);
        let mut session=task(1,AgentStatus::Working);session.agent=kind;session.id=format!("{prefix}:live-conversation");
        session.title="真实对话名称".into();
        lifecycle(&mut s,session.clone(),NoticeKind::Started);s.settle();
        let tip=s.h.app.notice.as_ref().unwrap();
        assert!(tip.body.starts_with(kind.label()));assert!(s.h.app.notice_source(tip).starts_with(kind.label()));
        let icon=crate::agent_icons::texture(&s.h.ctx,kind).unwrap().id();
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Mesh(mesh)if mesh.texture_id==icon)),"{kind:?} must show its own icon");
        s.click("notice-dismiss");
        session.status=AgentStatus::Completed;session.finished_at=Some(1_794_700_002);
        lifecycle(&mut s,session,NoticeKind::Completed);
        s.frame(vec![]);
        let alert=s.h.app.notice.as_ref().unwrap();assert!(alert.body.starts_with(kind.label()));
        assert_eq!(alert.title,"真实对话名称");assert!(s.h.app.history.is_empty());
        assert_eq!(s.h.app.agent_tasks.sessions()[0].agent,kind);
        s.h.t+=61.0;s.frame(vec![]);assert!(s.h.app.agent_tasks.sessions().is_empty());
    }
}

#[test]
fn island_notice_log_fixture_runs_through_feedback_queue_privacy_and_target(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let snapshots=crate::agent_monitor::notification_fixture();let mut s=island(Dock::Right);
    s.h.app.agent_snapshot=snapshots[0].clone();s.frame(vec![]);assert!(s.h.app.history.is_empty());
    s.h.t=1.0;s.h.app.agent_snapshot=snapshots[1].clone();s.frame(vec![]);
    assert!(s.h.app.notice.as_ref().is_some_and(|alert|has_kind(alert,NoticeKind::Started)));assert!(s.h.app.history.is_empty());
    assert!(!s.h.cmds.iter().any(|command|matches!(command,crate::app::Cmd::Notify(..))));
    message(&mut s.h.app,1);s.frame(vec![]);
    assert!(s.h.app.activity_notice.is_none());assert!(matches!(s.h.app.notice.as_ref().unwrap().source,AlertSource::LocalTest));
    s.h.t=2.0;s.h.app.agent_snapshot=snapshots[2].clone();s.frame(vec![]);
    assert_eq!(s.h.app.history.len(),1,"only the message is retained in notification history");
    assert_eq!(s.h.app.alerts.iter().filter(|alert|has_kind(alert,NoticeKind::Completed)).count(),2,"both short rounds remain transient completion alerts");
    for _ in 0..60{s.frame(vec![]);}assert_eq!(s.h.app.history.len(),1);
    let completed=s.h.app.alerts.iter().find(|alert|has_kind(alert,NoticeKind::Completed)).unwrap().clone();
    assert_eq!(completed.title,"完善岛内通知");assert!(completed.body.contains("用时"));
    let previous_page=s.h.app.page();let mut commands=Vec::new();s.h.app.open_notification(&completed,&mut commands);
    assert_eq!(s.h.app.page(),previous_page);
    assert!(commands.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenAgent{agent:AgentKind::Codex,session_id,..}if session_id=="codex:11111111-1111-1111-1111-111111111111")));
    s.h.app.settings.notification_preview=false;s.h.app.reconcile_notification_privacy();
    assert!(s.h.app.history.iter().all(|alert|!has_kind(alert,NoticeKind::Completed)));
    assert!(s.h.app.alerts.iter().filter(|alert|has_kind(alert,NoticeKind::Completed)).all(|alert|alert.title=="对话 11111111"));
    s.h.app.set_notification_session_locked(true);assert!(s.h.app.history.is_empty());
    s.h.app.set_notification_session_locked(false);s.frame(vec![]);assert!(s.h.app.history.is_empty());
    s.h.app.settings.agent_completion=false;next_completion(&mut s,3);s.frame(vec![]);
    s.h.app.settings.agent_completion=true;s.frame(vec![]);assert!(s.h.app.history.is_empty());
}

#[test]
fn island_notices_fit_each_dock_pause_on_hover_dismiss_advance_and_open(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=island(dock);s.frame(vec![]);message(&mut s.h.app,1);message(&mut s.h.app,2);s.settle();
        assert!(!s.h.app.open);
        let panel=s.h.app.notice_surface(Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size())).unwrap().rect;
        for id in ["notice-summary","notice-dismiss"]{
            let response=s.h.ctx.read_response(egui::Id::new(("card",true,id))).unwrap();
            assert!(panel.contains_rect(response.rect),"{dock:?} {id}");assert!(response.rect.height()>=28.0);
        }
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().contains("+1"))));
        if dock.is_side(){assert_eq!(s.out.as_ref().unwrap().hit_outline.as_ref().unwrap(),&s.out.as_ref().unwrap().glass.as_ref().unwrap()[0].outline());}
        let action=s.h.ctx.read_response(egui::Id::new(("card",true,"notice-summary"))).unwrap().rect.center();
        s.h.cursor=Some(action);s.frame(vec![egui::Event::PointerMoved(action)]);
        s.h.t+=12.0;s.frame(vec![]);assert!(s.h.app.notice.as_ref().unwrap().title.ends_with('1'),"hover must preserve the actionable notice");
        assert!(!s.h.app.open,"hovering the independent strip must not open the island");
        s.h.cursor=None;s.frame(vec![egui::Event::PointerGone]);s.settle();assert!(!s.h.app.open);
        s.click("notice-dismiss");s.frame(vec![]);assert!(s.h.app.notice.as_ref().unwrap().title.ends_with('2'),"{dock:?}: open={}, notice={:?}",s.h.app.open,s.h.app.notice.as_ref().map(|a|&a.title));
        s.h.cursor=None;s.frame(vec![egui::Event::PointerGone]);s.settle();
        s.h.t+=9.0;s.frame(vec![]);assert!(s.h.app.notice.is_none());
        message(&mut s.h.app,3);s.settle();s.click("notice-summary");assert_eq!(s.h.app.page(),NOTIFICATIONS_PAGE);assert!(s.h.app.open);
    }
}

#[test]
fn island_completion_float_preserves_the_page_and_opens_the_exact_session(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=island(dock);s.h.app.show_page(TODO_PAGE);s.settle();s.frame(vec![]);
        next_completion(&mut s,1);for _ in 0..20{s.frame(vec![]);}assert_eq!(s.h.app.page(),TODO_PAGE);
        assert!(s.h.ctx.read_response(egui::Id::new(("card",true,"notice-summary"))).is_some());
        let page_before=s.h.app.page();s.click("notice-dismiss");assert_eq!(s.h.app.page(),page_before);assert!(s.h.app.notice.is_none());
        next_completion(&mut s,2);for _ in 0..20{s.frame(vec![]);}s.click("notice-summary");
        assert_eq!(s.h.app.page(),TODO_PAGE);opened_agent(&s,"codex:demo-task");
    }
}

#[test]
fn island_notice_settings_migrate_safely_and_bound_the_display_interval(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let _h=H::new();let dir=crate::model::config_dir();
    std::fs::write(dir.join("settings.txt"),"notifications=0\nnotification_preview=0\nfocus_minutes=45\nunknown_setting=keep\n").unwrap();
    std::fs::write(dir.join("mail-accounts.json"),b"preserve-account-fixture").unwrap();
    let mut settings=crate::model::Settings::load();assert!(settings.agent_activity&&settings.agent_completion);assert_eq!(settings.agent_activity_interval,45);assert!(!settings.notifications);
    settings.agent_activity=false;settings.agent_completion=false;settings.agent_activity_interval=999;settings.save();
    let loaded=crate::model::Settings::load();assert!(!loaded.agent_activity&&!loaded.agent_completion);assert_eq!(loaded.agent_activity_interval,120);
    assert_eq!(std::fs::read(dir.join("mail-accounts.json")).unwrap(),b"preserve-account-fixture");
    let saved=std::fs::read_to_string(dir.join("settings.txt")).unwrap();assert!(saved.contains("unknown_setting=keep")&&saved.contains("focus_minutes=45"));
}

#[test]
fn island_notice_body_can_drag_with_native_coordinates_and_buttons_still_open(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let bounds=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    for dock in [Dock::Left,Dock::Right]{
        let mut s=island(dock);s.h.app.settings.island_offset=-40;
        let mut screen=Rect::NOTHING;
        for _ in 0..90{screen=island_drag_frame(&mut s.h,bounds,1.0,None,None);}
        message(&mut s.h.app,1);
        for _ in 0..90{screen=island_drag_frame(&mut s.h,bounds,1.0,None,None);}
        let win=Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size());let body=s.h.app.displayed_island_rect(win);
        let start=body.center()-screen.min.to_vec2();let end=start+Vec2::new(0.0,90.0);
        island_drag_frame(&mut s.h,bounds,1.0,Some(start),Some(true));
        island_drag_frame(&mut s.h,bounds,1.0,Some(end),None);
        assert!(s.h.app.settings.island_offset>-40,"notice background must retain body dragging");
        screen=island_drag_frame(&mut s.h,bounds,1.0,Some(end),Some(false));
        for _ in 0..60{screen=island_drag_frame(&mut s.h,bounds,1.0,None,None);}
        let action=s.h.ctx.read_response(egui::Id::new(("card",true,"notice-summary"))).unwrap().rect.center()-screen.min.to_vec2();
        for pressed in [true,false]{island_drag_frame(&mut s.h,bounds,1.0,Some(action),Some(pressed));}
        assert_eq!(s.h.app.page(),NOTIFICATIONS_PAGE,"the notice action must remain clickable with the native drag region present");
    }
}

#[test]
fn island_notice_geometry_covers_panel_sizes_lyrics_and_reduced_motion(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{for reduced in [false,true]{for (width,height) in [(292,532),(520,900)]{
        let mut s=island(dock);s.h.app.settings.side_panel_width=width;s.h.app.settings.side_panel_height=height;
        s.h.app.settings.island_lyrics=true;s.h.media=crate::smtc::NowPlaying{title:"演示音乐".into(),artist:"合成数据".into(),..Default::default()};
        #[cfg(target_os="macos")]
        s.h.ctx.data_mut(|data|data.insert_temp(egui::Id::new("macos-accessibility-preferences"),(std::time::Instant::now(),if reduced{2u32}else{0u32})));
        for _ in 0..60{s.frame(vec![]);}message(&mut s.h.app,1);
        let mut previous_radius:Option<f32>=None;
        for _ in 0..50{
            s.frame(vec![]);let out=s.out.as_ref().unwrap();let panel=s.h.app.displayed_island_rect(Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size()));
            assert!(panel.is_finite());
            if dock.is_side(){
                assert_eq!(out.hit_outline.as_ref().unwrap(),&out.glass.as_ref().unwrap()[0].outline());
                let radius=crate::side_dock::shape_parameters(panel.width(),panel.height(),s.h.app.progress()).2;
                if let Some(previous)=previous_radius{if !reduced{assert!((radius-previous).abs()<5.0,"summary radius should follow the body spring");}}
                previous_radius=Some(radius);
            }
        }
        assert!(!s.h.app.open);assert!(s.h.ctx.read_response(egui::Id::new(("card",true,"notice-summary"))).is_some(),"messages must be readable during island lyrics");
        if dock.is_side(){
            let panel=s.h.app.displayed_island_rect(Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size()));
            assert!((panel.width()-crate::side_dock::SIDE_BAR_W).abs()<0.1,"notifications must not resize the resident island: {panel:?}");
            let expected=crate::side_dock::shape_parameters(panel.width(),panel.height(),0.0).2;
            assert!((previous_radius.unwrap()-expected).abs()<0.1);
            for id in ["notice-summary","notice-dismiss"]{
                let response=s.h.ctx.read_response(egui::Id::new(("card",true,id))).unwrap();
                let notice=s.h.app.notice_surface(Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size())).unwrap().rect;
                assert!(!notice.intersects(panel));assert!(notice.contains_rect(response.rect));assert!(response.rect.height()>=28.0);
            }
        }
    }}}
}

#[test]
fn floating_notice_keeps_resident_and_expanded_controls_and_masks_through_animation(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{for expanded in [false,true]{for minimal in [false,true]{
        let mut s=island(dock);s.h.app.settings.minimal=minimal;
        s.h.app.settings.macos_side_handle=minimal;
        if expanded{s.h.app.show_page(WEATHER_PAGE);}
        s.settle();let win=Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size());
        let original=s.h.app.displayed_island_rect(win);let original_page=s.h.app.page();
        message(&mut s.h.app,1);s.h.app.notification_bell_until=s.h.t+6.0;let mut saw_entrance=false;
        for frame in 0..40{
            s.frame(vec![]);let out=s.out.as_ref().unwrap();let body=s.h.app.displayed_island_rect(win);
            assert!((body.min-original.min).length()<0.1&&(body.size()-original.size()).length()<0.1,"notification cannot morph the original island");
            assert_eq!(s.h.app.page(),original_page);
            if let Some(strip)=s.h.app.notice_surface(win){
                assert!(!strip.rect.intersects(body));assert!(win.contains_rect(strip.rect));
                assert_eq!(out.notice_shapes.len(),1);
                let native=&out.notice_shapes[0];assert_eq!(*native,crate::app::GlassShape::Round(strip.rect,strip.radius));
                assert!(out.glass.as_ref().unwrap().contains(native));
                assert_eq!(out.notice_hit,Some(strip.rect));
                assert!(!crate::shape::contains_outline(&native.outline(),strip.rect.min+Vec2::splat(strip.radius*0.1)));
                if frame<8&&strip.alpha<0.98{saw_entrance=true;}
            }
        }
        assert!(saw_entrance);assert!(!s.h.app.notice_motion.animating());
        let action=s.h.ctx.read_response(egui::Id::new(("card",true,"notice-summary"))).unwrap();
        assert!(action.rect.width()>=28.0&&action.rect.height()>=28.0);
        if expanded{
            assert!(s.h.ctx.read_response(egui::Id::new(("card",true,"control-center-tab"))).is_some());
        }
        s.click("notice-dismiss");assert!(s.h.app.notice.is_none());
        let previous=s.h.app.notice_motion.progress();
        message(&mut s.h.app,2);s.frame(vec![]);
        assert!(s.h.app.notice_motion.progress()>0.5&&previous>0.5,"new content must reverse the exit without a zero-opacity flash");
        s.h.app.set_notification_session_locked(true);assert!(s.h.app.hud_notice.is_none());
    }}}
}

#[test]
fn floating_notice_material_paint_native_input_and_blur_follow_the_same_morph(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{for scale in [1.0,2.0]{
        let mut s=island(dock);s.h.ctx.set_pixels_per_point(scale);s.settle();
        let win=Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size());
        let body=s.h.app.displayed_island_rect(win);message(&mut s.h.app,1);
        let mut first=None;let mut last=None;
        for frame in 0..40{
            s.frame(vec![]);let surface=s.h.app.notice_surface(win).unwrap();
            let out=s.out.as_ref().unwrap();let native=crate::app::GlassShape::Round(surface.rect,surface.radius);
            assert_eq!(out.notice_shapes,vec![native.clone()]);assert_eq!(out.notice_hit,Some(surface.rect));
            assert!(out.glass.as_ref().unwrap().contains(&native));
            let outline=native.outline();
            assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Path(path)if path.closed&&path.fill!=egui::Color32::TRANSPARENT&&path.points==outline)),"the material must paint the exact native outline at {dock:?}/{scale}/{frame}");
            assert!(!surface.rect.intersects(body));assert!(win.contains_rect(surface.rect));
            if let Some(previous)=last{
                let previous:crate::island_notice::Surface=previous;
                assert!((surface.rect.min-previous.rect.min).length()<75.0&&(surface.rect.max-previous.rect.max).length()<75.0);
            }
            if first.is_none(){first=Some(surface);}
            last=Some(surface);
        }
        let first=first.unwrap();let last=last.unwrap();
        assert!(first.rect.width()<last.rect.width()*0.6&&first.rect.height()<4.0,"the row must grow from the island boundary");
        assert!(first.content_alpha==0.0&&last.content_alpha==1.0);
        assert_eq!(last.content_rect,last.rect);
    }}
}

#[test]
fn island_notice_compacts_to_its_content_and_opens_from_the_whole_row_except_close(){
    use crate::notifications::{Batch,Status};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{
        let mut widths=Vec::new();
        for long in [false,true]{
            let mut s=island(dock);
            let (sender,body)=if long{("项目讨论群里的很长很长的联系人名称","资料已经全部准备好了，请在今天下午三点前查收并提出反馈")}
                else{("同事","收到")};
            s.h.app.notification_batch(Batch{status:Status::Watching{apps:1},events:vec![super::super::notification_tests::preview("compact","com.tencent.qq","QQ",sender,body)]});
            s.settle();let win=Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size());let panel=s.h.app.notice_surface(win).unwrap().rect;
            assert_eq!(panel.height(),crate::island_notice::NOTICE_HEIGHT);assert!(panel.width()>=180.0&&panel.width()<=360.0);
            let summary=s.h.ctx.read_response(egui::Id::new(("card",true,"notice-summary"))).unwrap().rect;
            let close=s.h.ctx.read_response(egui::Id::new(("card",true,"notice-dismiss"))).unwrap().rect;
            assert_eq!(summary,panel);assert_eq!(close.size(),Vec2::splat(28.0));
            assert!(s.h.ctx.read_response(egui::Id::new(("card",true,"notice-action"))).is_none(),"the separate view action must be removed");
            assert!(!s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if panel.contains(text.visual_bounding_rect().center())&&text.galley.text()=="查看")));
            widths.push(panel.width());
        }
        assert!(widths[0]<230.0&&widths[1]>widths[0]+80.0,"short messages must not occupy the long-message lane: {widths:?}");
        for target in ["icon","text","space","close"]{
            let mut s=island(dock);
            s.h.app.notification_batch(Batch{status:Status::Watching{apps:1},events:vec![super::super::notification_tests::preview("whole-row","com.tencent.qq","QQ","同事","收到")]});
            s.settle();let panel=s.h.ctx.read_response(egui::Id::new(("card",true,"notice-summary"))).unwrap().rect;
            let close=s.h.ctx.read_response(egui::Id::new(("card",true,"notice-dismiss"))).unwrap().rect;
            let point=match target{"icon"=>Pos2::new(panel.left()+16.0,panel.center().y),"space"=>Pos2::new(close.left()-2.0,panel.center().y),"close"=>close.center(),_=>panel.center()};
            s.click_at(point);assert!(s.h.app.notice.is_none(),"{dock:?}/{target}");
            let opened:Vec<_>=s.h.cmds.iter().filter(|cmd|matches!(cmd,crate::app::Cmd::OpenNotification{..})).collect();
            if target=="close"{assert!(opened.is_empty(),"the close control cannot activate the row underneath it");}
            else{assert_eq!(opened.len(),1);assert!(matches!(opened[0],crate::app::Cmd::OpenNotification{activation,..}if activation=="action-whole-row"));}
        }
    }
}

#[test]
fn floating_notice_compact_width_stays_with_the_cached_delivery_and_morphs_during_handoff(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{
        let mut s=island(dock);
        for (title,body) in [("项目讨论群里的很长很长的联系人名称","资料已经全部准备好了，请在今天下午三点前查收并提出反馈"),("消息","收到")]{
            s.h.app.enqueue_alert(Alert{source:AlertSource::LocalTest,title:title.into(),body:body.into(),page:NOTIFICATIONS_PAGE,at:s.h.t,duration:8.0},false);
        }
        s.settle();let win=Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size());let old=s.h.app.notice.clone().unwrap();
        let wide=s.h.app.notice_surface(win).unwrap();assert_eq!(wide.rect.width(),360.0);
        s.h.t=old.at+old.duration+0.1;s.frame(vec![]);
        assert_eq!(s.h.app.hud_notice.as_ref().unwrap().title,old.title);
        assert_eq!(s.h.app.notice_surface(win).unwrap().content_rect.width(),wide.content_rect.width());
        // The cached row is already exiting. Clicking it cannot open either
        // the old delivery or the queued one before that delivery is readable.
        s.click_at(s.h.app.notice_surface(win).unwrap().rect.center());
        assert!(!s.h.app.open);assert!(s.h.cmds.iter().all(|cmd|!matches!(cmd,crate::app::Cmd::OpenNotification{..})));
        let mut previous=s.h.app.notice_surface(win).unwrap();let mut switched=false;
        for _ in 0..65{
            s.frame(vec![]);let next=s.h.app.notice_surface(win).unwrap();
            assert!((next.rect.min-previous.rect.min).length()<75.0&&(next.rect.max-previous.rect.max).length()<75.0);
            let native=crate::app::GlassShape::Round(next.rect,next.radius);let out=s.out.as_ref().unwrap();
            assert_eq!(out.notice_hit,Some(next.rect));assert_eq!(out.notice_shapes,vec![native.clone()]);assert!(out.glass.as_ref().unwrap().contains(&native));
            if s.h.app.hud_notice.as_ref().is_some_and(|a|a.title=="消息"){switched=true;}
            else{assert_eq!(next.content_rect.width(),wide.content_rect.width(),"an outgoing delivery keeps its measured width");}
            previous=next;
        }
        assert!(switched);assert_eq!(previous.rect.width(),180.0);assert!(!s.h.app.notice_motion.animating());
    }
}

#[test]
fn floating_notice_queue_waits_for_invisible_text_and_preserves_the_next_reading_interval(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in Dock::ALL{
        let mut s=island(dock);s.settle();
        for (title,body) in [("旧消息","正在阅读的内容"),("新消息","下一条消息的内容")]{
            s.h.app.enqueue_alert(Alert{source:AlertSource::LocalTest,title:title.into(),body:body.into(),page:NOTIFICATIONS_PAGE,at:s.h.t,duration:8.0},false);
        }
        s.settle();let old=s.h.app.notice.clone().unwrap();
        s.h.t=old.at+old.duration+0.1;s.frame(vec![]);
        assert_eq!(s.h.app.notice.as_ref().unwrap().title,"新消息");
        assert_eq!(s.h.app.hud_notice.as_ref().unwrap().title,"旧消息","queue selection cannot replace visible text");
        let win=Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size());
        assert!(s.h.app.notice_surface(win).unwrap().content_alpha>0.9);
        assert!(!s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.opacity_factor>0.01&&text.galley.text().contains("新消息"))));
        // A stalled UI frame during the handoff must not silently consume the
        // pending delivery's display interval before its text is visible.
        s.h.t+=30.0;s.frame(vec![]);assert_eq!(s.h.app.notice.as_ref().unwrap().title,"新消息");
        let mut switched=s.h.app.hud_notice.as_ref().is_some_and(|a|a.title=="新消息");
        if switched{assert_eq!(s.h.app.notice_surface(win).unwrap().content_alpha,0.0);}
        for _ in 0..60{
            let before=s.h.app.notice_motion.progress();s.frame(vec![]);
            assert!((s.h.app.notice_motion.progress()-before).abs()<0.2);
            if !switched&&s.h.app.hud_notice.as_ref().is_some_and(|a|a.title=="新消息"){
                assert_eq!(s.h.app.notice_surface(win).unwrap().content_alpha,0.0,"only invisible content may change delivery");
                switched=true;
            }
        }
        assert!(switched);let next=s.h.app.notice.clone().unwrap();
        assert!(s.h.app.notice_is_presented(&next));
        s.h.t=next.at+next.duration-0.1;s.frame(vec![]);assert!(s.h.app.notice.is_some());
        s.h.t+=0.2;s.frame(vec![]);assert!(s.h.app.notice.is_none());
        assert!(s.h.app.notice_surface(win).is_some(),"expiry must start a retracting surface");
        s.settle();assert!(s.h.app.notice_surface(win).is_none());assert!(s.h.app.hud_notice.is_none());
    }
}

#[test]
fn floating_notice_open_and_privacy_clear_content_while_the_empty_material_retracts(){
    use crate::notifications::{Batch,Status};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for action in ["dismiss","open","lock","off","previews"]{
        let mut s=island(Dock::Right);
        s.h.app.notification_batch(Batch{status:Status::Watching{apps:1},events:vec![super::super::notification_tests::preview("motion-private","com.tencent.qq","QQ","私密发送者","这是一条私密正文")]});
        s.settle();let win=Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size());
        let before=s.h.app.notice_surface(win).unwrap();let alert=s.h.app.notice.clone().unwrap();
        match action{
            "dismiss"=>s.h.app.dismiss_notice(&alert),
            "open"=>{let mut commands=vec![];s.h.app.open_notification(&alert,&mut commands);assert_eq!(commands.len(),1);},
            "lock"=>s.h.app.set_notification_session_locked(true),
            "off"=>{s.h.app.settings.notifications=false;s.h.app.reconcile_notification_privacy();},
            _=>{s.h.app.settings.notification_preview=false;s.h.app.reconcile_notification_privacy();},
        }
        if action!="dismiss"{
            assert!(s.h.app.hud_notice.as_ref().is_none_or(|a|!a.title.contains("私密")&&!a.body.contains("私密")),"opened or private previews must leave memory immediately");
        }
        s.frame(vec![]);let after=s.h.app.notice_surface(win).unwrap();
        if action=="previews"{
            assert_eq!(after.rect.height(),before.rect.height());
            assert!((after.rect.min-before.rect.min).length()<8.0&&(after.rect.max-before.rect.max).length()<8.0,"redaction changes the measured width continuously");
        }else{
            assert!(after.rect.height()<before.rect.height()&&after.rect.width()<before.rect.width());
        }
        if action!="dismiss"{
            assert!(!s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.opacity_factor>0.01&&text.galley.text().contains("私密"))));
        }
        s.settle();if action!="previews"{assert!(s.h.app.notice_surface(win).is_none());assert!(!s.h.app.notice_motion.animating());}
    }
}

#[test]
fn island_notice_redacted_preview_keeps_the_original_dismiss_and_open_delivery(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for preview in [true,false]{for open in [false,true]{
        let mut s=island(Dock::Top);s.h.app.settings.notification_preview=preview;
        s.h.app.enqueue_alert(Alert{source:AlertSource::LocalTest,title:"本地私密预览".into(),body:"登录验证码 123456".into(),page:NOTIFICATIONS_PAGE,at:s.h.t,duration:8.0},false);
        s.settle();
        assert!(!s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().contains("123456"))));
        s.click(if open{"notice-summary"}else{"notice-dismiss"});
        assert!(s.h.app.notice.is_none(),"redaction cannot break the original delivery's identity");
        if open{assert!(s.h.app.history.is_empty()&&s.h.app.hud_notice.is_none());}
    }}
}

#[test]
fn island_execution_tip_dismissal_and_privacy_are_per_turn_and_otp_safe(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut s=island(Dock::Left);
    s.h.app.agent_snapshot.generation=1;s.frame(vec![]);
    lifecycle(&mut s,task(1,AgentStatus::Working),NoticeKind::Started);
    s.settle();assert!(s.h.app.notice.is_some());s.click("notice-dismiss");
    s.h.t+=70.0;s.frame(vec![]);assert!(s.h.app.notice.is_none(),"a working turn cannot periodically reappear");
    lifecycle(&mut s,task(2,AgentStatus::Working),NoticeKind::Started);s.frame(vec![]);
    assert!(s.h.app.notice.is_some());
    s.h.app.agent_snapshot.sessions[0].title="邮件验证码 123456 的修复".into();s.frame(vec![]);
    assert!(!s.h.app.notice.as_ref().unwrap().title.contains("123456"));
    s.h.app.settings.agent_activity=false;s.frame(vec![]);assert!(s.h.app.notice.is_none());
    next_completion(&mut s,2);s.frame(vec![]);assert!(s.h.app.history.is_empty());assert!(has_kind(s.h.app.notice.as_ref().unwrap(),NoticeKind::Completed));
    s.h.app.set_page_visible(AGENT_PAGE,false);assert!(s.h.app.history.is_empty());assert!(s.h.app.notice.is_none());
    s.h.app.set_page_visible(AGENT_PAGE,true);s.frame(vec![]);assert!(s.h.app.history.is_empty());
}

#[test]
fn island_running_conversations_keep_titles_renames_and_event_targets(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=island(dock);s.h.app.agent_snapshot.generation=1;s.frame(vec![]);
        let first=Session{id:"codex:11111111-1111-1111-1111-111111111111".into(),title:"修复 QQ 通知".into(),project:"Window".into(),..task(1,AgentStatus::Working)};
        let second=Session{id:"codex:22222222-2222-2222-2222-222222222222".into(),title:"接入微信预览".into(),project:"Window".into(),..task(2,AgentStatus::Working)};
        lifecycle(&mut s,first,NoticeKind::Started);lifecycle(&mut s,second,NoticeKind::Started);s.settle();
        let notice=s.h.app.notice.as_ref().unwrap();
        assert_eq!(notice.title,"修复 QQ 通知");
        assert!(notice.source.agent_notice().is_some_and(|(id,_,kind)|id.ends_with("111111111111")&&kind==NoticeKind::Started));
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="修复 QQ 通知")));
        s.h.app.agent_snapshot.sessions[0].title="QQ 通知接入".into();s.frame(vec![]);
        assert_eq!(s.h.app.notice.as_ref().unwrap().title,"QQ 通知接入");
        s.h.app.settings.notification_preview=false;s.frame(vec![]);
        assert_eq!(s.h.app.notice.as_ref().unwrap().title,"对话 11111111");
        s.h.app.settings.notification_preview=true;s.frame(vec![]);
        assert_eq!(s.h.app.notice.as_ref().unwrap().title,"QQ 通知接入");
        s.click("notice-dismiss");s.frame(vec![]);s.settle();
        assert_eq!(s.h.app.notice.as_ref().unwrap().title,"接入微信预览");
        s.click("notice-summary");
        opened_agent(&s,"codex:22222222-2222-2222-2222-222222222222");
    }
}

#[test]
fn island_subagents_keep_distinct_names_timers_and_click_targets_for_each_notice(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        for (kind,status) in [(NoticeKind::Started,AgentStatus::Working),(NoticeKind::Completed,AgentStatus::Completed),(NoticeKind::Approval,AgentStatus::Approval)]{
            let mut s=island(dock);s.h.app.agent_snapshot.generation=1;s.frame(vec![]);
            let ids=["codex:019b0000-0000-7000-8000-aaaaaaaaaaaa","codex:019b0000-0000-7000-8000-bbbbbbbbbbbb","codex:019b0000-0000-7000-8000-cccccccccccc"];
            let names=["优化任务通知","子 Agent · 检查界面 · bbbbbbbb","子 Agent · 检查事件 · cccccccc"];
            for index in 0..3{
                let session=Session{id:ids[index].into(),title:names[index].into(),project:"Window".into(),pid:Some(8123),
                    started_at:1_794_699_940+index as u64*10,approval_key:1,..task(7,status)};
                lifecycle(&mut s,session,kind);
            }
            s.settle();
            let rows=s.h.app.agent_tasks.sessions();assert_eq!(rows.len(),3,"a shared process/project/turn cannot merge parent and child tasks");
            for index in 0..3{
                let row=rows.iter().find(|row|row.id==ids[index]).unwrap();
                assert_eq!(row.title,names[index]);assert_eq!(row.turn_key,7);
                assert_eq!(row.started_at,1_794_699_940+index as u64*10);
            }
            assert_eq!(s.h.app.notice.as_ref().unwrap().title,names[0]);
            assert_eq!(s.h.app.alerts.iter().filter(|alert|has_kind(alert,kind)).count(),2);

            // Both the visible parent and the queued child refresh by their
            // own session ID; changing a name cannot restart either task.
            s.h.app.agent_snapshot.sessions[0].title="优化主对话".into();
            let renamed="子 Agent · 复核界面 · bbbbbbbb";
            s.h.app.agent_snapshot.sessions[1].title=renamed.into();s.frame(vec![]);
            assert_eq!(s.h.app.notice.as_ref().unwrap().title,"优化主对话");
            for alert in s.h.app.alerts.iter().chain(s.h.app.history.iter()){
                if let Some((id,turn,current_kind))=alert.source.agent_notice(){
                    assert_eq!(turn,7);assert_eq!(current_kind,kind);
                    if id==ids[1]{assert_eq!(alert.title,renamed);}
                    if id==ids[2]{assert_eq!(alert.title,names[2]);}
                }
            }
            let child=s.h.app.agent_tasks.sessions().iter().find(|row|row.id==ids[1]).unwrap();
            assert_eq!(child.title,renamed);assert_eq!(child.started_at,1_794_699_950);
            s.click("notice-dismiss");s.settle();
            for (index,name) in [(1,renamed),(2,names[2])]{
                let alert=s.h.app.notice.as_ref().unwrap();
                assert_eq!(alert.title,name);assert_eq!(alert.source.agent_notice(),Some((ids[index],7,kind)));
                assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().starts_with("子 Agent ·"))));
                let previous_commands=s.h.cmds.len();s.click("notice-summary");
                opened_agent(&s,ids[index]);
                let targets:Vec<_>=s.h.cmds[previous_commands..].iter().filter_map(|cmd|match cmd{crate::app::Cmd::OpenAgent{session_id,..}=>Some(session_id.as_str()),_=>None}).collect();
                assert_eq!(targets,vec![ids[index]],"clicking a child notice must keep its exact conversation identity");
                s.settle();
            }
            assert!(s.h.app.notice.is_none());assert!(s.h.app.alerts.is_empty());
        }
    }
}

#[test]
fn island_lifecycle_events_replace_periodic_tips_and_freeze_the_final_total_time(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=island(dock);s.h.app.agent_snapshot.generation=1;
        s.h.app.agent_snapshot.sessions=vec![task(9,AgentStatus::Working)];
        s.frame(vec![]);assert!(s.h.app.display_notice().is_none(),"starting the monitor cannot announce an old running task");
        let mut current=task(10,AgentStatus::Working);current.title="接入通知事件".into();
        current.started_at=1_794_700_000+s.h.t as u64;
        lifecycle(&mut s,current.clone(),NoticeKind::Started);s.settle();
        assert!(has_kind(s.h.app.notice.as_ref().unwrap(),NoticeKind::Started));s.click("notice-dismiss");
        current.status=AgentStatus::Waiting;lifecycle(&mut s,current.clone(),NoticeKind::Waiting);s.settle();
        assert!(s.h.app.notice.as_ref().unwrap().body.contains("等待确认"));s.click("notice-dismiss");
        current.status=AgentStatus::Working;s.h.app.agent_snapshot.sessions=vec![current.clone()];
        s.h.t+=120.0;s.frame(vec![]);assert!(s.h.app.display_notice().is_none(),"ordinary work and elapsed intervals cannot emit notifications");
        current.status=AgentStatus::Idle;current.finished_at=Some(1_794_700_000+s.h.t as u64);
        lifecycle(&mut s,current,NoticeKind::Interrupted);s.settle();
        assert!(s.h.app.notice.as_ref().unwrap().body.contains("已中断"));s.click("notice-dismiss");

        let mut next=task(11,AgentStatus::Working);next.started_at=1_794_700_000+s.h.t as u64;
        lifecycle(&mut s,next.clone(),NoticeKind::Started);s.frame(vec![]);
        s.h.t+=846.0;next.status=AgentStatus::Completed;next.finished_at=Some(next.started_at+846);
        lifecycle(&mut s,next,NoticeKind::Completed);s.settle();
        let result=s.h.app.notice.as_ref().unwrap();assert!(has_kind(result,NoticeKind::Completed));
        assert!(result.body.contains("总用时 14 分 6 秒"));let body=result.body.clone();
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().contains("14分6秒"))),"the completion must visibly include its total time");
        let mut later=task(12,AgentStatus::Working);later.started_at=1_794_700_000+s.h.t as u64;
        lifecycle(&mut s,later,NoticeKind::Started);s.frame(vec![]);
        assert_eq!(s.h.app.notice.as_ref().unwrap().body,body,"a newer turn cannot change the completed turn's total time");
        s.click("notice-summary");opened_agent(&s,"codex:demo-task");
    }
}

#[test]
fn island_interruption_view_targets_the_external_app_with_observed_process_identity(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=island(dock);s.h.app.agent_snapshot.generation=1;s.frame(vec![]);
        let mut session=task(1,AgentStatus::Idle);session.title="处理被中断的对话".into();
        session.finished_at=Some(1_794_700_000);session.detail="本轮执行已中断".into();
        session.pid=Some(8123);session.updated_at=1_794_700_000;
        lifecycle(&mut s,session,NoticeKind::Interrupted);s.settle();let page=s.h.app.page();s.click("notice-summary");
        assert_eq!(s.h.app.page(),page);opened_agent(&s,"codex:demo-task");
        assert!(s.h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenAgent{pid:Some(8123),observed_at_ms:Some(1_794_700_000_000),..})));
    }
}

#[test]
fn island_message_body_opens_codex_claude_and_original_chat_while_close_only_dismisses(){
    use crate::notifications::{Batch,Status};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{for target in 0..3{for dismiss in [false,true]{
        let mut s=island(dock);s.h.app.agent_snapshot.generation=1;s.frame(vec![]);
        let id=if target==0{"codex:11111111-1111-1111-1111-111111111111"}else{"claude:22222222-2222-2222-2222-222222222222"};
        if target<2{
            let mut session=task(1,AgentStatus::Working);session.id=id.into();
            session.agent=if target==0{AgentKind::Codex}else{AgentKind::Claude};session.pid=Some(8123);session.updated_at=1_794_700_000;
            lifecycle(&mut s,session,NoticeKind::Started);
        }else{
            s.h.app.notification_batch(Batch{status:Status::Watching{apps:1},events:vec![super::super::notification_tests::preview("body-click","com.tencent.qq","QQ","设计群","资料已准备好")]});
        }
        s.settle();let previous_page=s.h.app.page();
        assert_eq!(s.h.app.notice.as_ref().unwrap().duration,15.0);
        s.click(if dismiss{"notice-dismiss"}else{"notice-summary"});
        let opened:Vec<_>=s.h.cmds.iter().filter(|cmd|matches!(cmd,crate::app::Cmd::OpenAgent{..}|crate::app::Cmd::OpenNotification{..})).collect();
        if dismiss{assert!(opened.is_empty(),"closing must never activate the source application");}
        else if target<2{
            assert_eq!(opened.len(),1);opened_agent(&s,id);
            assert!(matches!(opened[0],crate::app::Cmd::OpenAgent{pid:Some(8123),observed_at_ms:Some(1_794_700_000_000),..}));
        }else{
            assert_eq!(opened.len(),1);
            assert!(matches!(opened[0],crate::app::Cmd::OpenNotification{bundle_id,activation}if bundle_id=="com.tencent.qq"&&!activation.is_empty()));
        }
        assert_eq!(s.h.app.page(),previous_page);assert!(s.h.app.notice.is_none());
        if !dismiss{
            assert!(s.h.app.hud_notice.is_none(),"the opened message must not remain painted during a cached exit");
            if target==2{assert!(s.h.app.history.is_empty(),"the opened QQ message is consumed from the inbox too");}
            s.frame(vec![]);assert!(s.out.as_ref().unwrap().notice_hit.is_some(),"the empty material must retract smoothly after opening");
            s.settle();assert!(s.out.as_ref().unwrap().notice_hit.is_none());
        }
    }}}
}

#[test]
fn island_wechat_notice_shows_sender_and_body_even_with_a_long_contact_name(){
    use crate::notifications::{Batch,Status};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let folder=std::env::var_os("QINGYU_WECHAT_NOTICE_GALLERY").map(std::path::PathBuf::from);
    if let Some(folder)=&folder{std::fs::create_dir_all(folder).unwrap();}
    for dock in [Dock::Top,Dock::Left,Dock::Right]{for sender in ["张三","项目讨论群里的很长很长的联系人名称"]{
        let mut s=island(dock);s.h.app.settings.wechat_notifications=true;
        s.h.app.notification_batch(Batch{status:Status::Watching{apps:1},events:vec![super::super::notification_tests::preview("wechat-details","com.tencent.xinWeChat","微信",sender,"你好，资料已经发给你了，请查收")]});
        let size=s.h.app.canvas_size();s.h.ctx.set_pixels_per_point(2.0);
        let mut raster=folder.as_ref().map(|_|crate::preview_raster::Raster::new((size.x*2.0)as usize,(size.y*2.0)as usize));
        for frame in 0..70{
            let output=s.frame(vec![]);
            if let Some(raster)=&mut raster{
                raster.update_textures(&output.textures_delta);
                if frame==69{
                    raster.buf.fill([0.13,0.18,0.20,1.0]);raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                    image::save_buffer(folder.as_ref().unwrap().join(format!("wechat-{}-{}.png",dock.key(),if sender=="张三"{"short"}else{"long"})),&rgba,(size.x*2.0)as u32,(size.y*2.0)as u32,image::ColorType::Rgba8).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
        let alert=s.h.app.notice.as_ref().unwrap();
        assert_eq!(alert.title,format!("微信 · {sender}"));assert_eq!(alert.body,"你好，资料已经发给你了，请查收");
        let surface=s.h.app.notice_surface(Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size())).unwrap().rect;
        let text:Vec<_>=s.h.shapes.iter().filter_map(|shape|match &shape.shape{egui::Shape::Text(text)if shape.clip_rect.intersects(surface)&&text.visual_bounding_rect().intersects(surface)&&text.opacity_factor>0.99=>Some(text),_=>None}).collect();
        assert!(text.iter().any(|text|text.galley.text().starts_with("微信 ·")),"the sender's label must remain in the single line");
        assert!(text.iter().any(|text|text.galley.text().contains("你好")),"a long sender label cannot hide the message preview");
        s.click("notice-summary");
        assert!(s.h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenNotification{bundle_id,activation}if bundle_id=="com.tencent.xinWeChat"&&activation=="action-wechat-details")),"{dock:?}/{sender}: commands={:?}, notice={:?}",s.h.cmds,s.h.app.notice);
        assert!(s.h.app.history.is_empty()&&s.h.app.hud_notice.is_none());
    }}
}

#[test]
fn island_message_bursts_accumulate_in_fifo_order_and_each_click_consumes_only_one(){
    use crate::notifications::{Batch,Status};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=island(dock);s.h.app.settings.wechat_notifications=true;s.frame(vec![]);
        s.h.app.notification_batch(Batch{status:Status::Watching{apps:1},events:(0..12).map(|index|super::super::notification_tests::preview(&format!("burst-{index}"),"com.tencent.xinWeChat","微信","同事",&format!("第 {index} 条消息"))).collect()});
        for index in 0..12{
            s.settle();let expected=format!("burst-{index}");
            assert!(matches!(&s.h.app.notice.as_ref().unwrap().source,AlertSource::Desktop{key,..}if key==&expected),"all messages must be shown in arrival order at {dock:?}");
            assert_eq!(s.h.app.alerts.len(),11-index);assert_eq!(s.h.app.history.len(),12-index);
            if index==0{assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().contains("+11"))),"the row must show the accumulated queue count");}
            let before=s.h.cmds.len();s.click("notice-summary");
            assert_eq!(s.h.cmds[before..].iter().filter(|cmd|matches!(cmd,crate::app::Cmd::OpenNotification{..})).count(),1);
            assert!(s.h.app.hud_notice.is_none());assert_eq!(s.h.app.history.len(),11-index);
            s.frame(vec![]);
        }
        assert!(s.h.app.alerts.is_empty()&&s.h.app.notice.is_none()&&s.h.app.history.is_empty());
    }
}

#[test]
fn island_notice_timeout_leaves_reading_time_and_pauses_for_keyboard_focus(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        for (kind,status,seconds) in [(NoticeKind::Started,AgentStatus::Working,15.0),(NoticeKind::Completed,AgentStatus::Completed,15.0),(NoticeKind::Waiting,AgentStatus::Waiting,20.0),(NoticeKind::Interrupted,AgentStatus::Error,20.0)]{
            let mut s=island(dock);s.h.app.agent_snapshot.generation=1;s.frame(vec![]);
            lifecycle(&mut s,task(1,status),kind);s.settle();
            let notice=s.h.app.notice.as_ref().unwrap();assert_eq!(notice.duration,seconds);
            s.h.t=notice.at+seconds-0.25;s.frame(vec![]);assert!(s.h.app.notice.is_some());
            let id=egui::Id::new(("card",true,"notice-summary"));
            s.h.ctx.memory_mut(|memory|memory.request_focus(id));
            s.h.t+=30.0;s.frame(vec![]);assert!(s.h.app.notice.is_some(),"a focused notification must remain available for reading and keyboard activation");
            s.h.ctx.memory_mut(|memory|memory.surrender_focus(id));
            s.h.t+=0.5;s.frame(vec![]);assert!(s.h.app.notice.is_none(),"timeout resumes after focus leaves");
        }
    }
}

#[test]
fn island_agent_notices_keep_one_text_row_at_every_dock_and_scale(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{for scale in [1.0,2.0]{
        for (kind,body) in [(NoticeKind::Completed,"Codex 已完成 · 总用时 14 分 6 秒"),(NoticeKind::Started,"Codex 已开始"),(NoticeKind::Waiting,"Codex 等待确认 · 需要你的操作"),(NoticeKind::Interrupted,"Codex 已中断 · 发生错误")]{
            let mut s=island(dock);s.h.ctx.set_pixels_per_point(scale);s.frame(vec![]);
            let source=AlertSource::AgentEvent{session_id:"codex:demo-task".into(),turn_key:1,kind};
            s.h.app.enqueue_alert(Alert{source,title:"整理通知中心".into(),body:body.into(),page:AGENT_PAGE,at:s.h.t,duration:8.0},false);
            s.settle();
            let panel=s.h.app.notice_surface(Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size())).unwrap().rect;
            assert!((panel.height()-crate::island_notice::NOTICE_HEIGHT).abs()<0.1,"agent notices should be a shallow row: {dock:?} {panel:?}");
            let action=s.h.ctx.read_response(egui::Id::new(("card",true,"notice-summary"))).unwrap().rect;
            let close=s.h.ctx.read_response(egui::Id::new(("card",true,"notice-dismiss"))).unwrap().rect;
            for rect in [action,close]{assert!(panel.contains_rect(rect));assert!(rect.width()>=28.0&&rect.height()>=28.0);}
            assert_eq!(action,panel,"the whole row must open its original target");
            let mut text_count=0;
            for shape in &s.h.shapes{if let egui::Shape::Text(text)=&shape.shape{
                if shape.clip_rect.intersects(panel)&&text.opacity_factor>0.99{
                    let bounds=text.visual_bounding_rect();
                    assert!((bounds.center().y-panel.center().y).abs()<1.0,"{dock:?}/{body}: {} must be on the single row",text.galley.text());
                    assert!(bounds.left()>=panel.left()-1.0&&bounds.right()<=panel.right()+1.0);
                    assert!(bounds.right()<=close.left()-5.0,"summary cannot overlap the close button");text_count+=1;
                    assert!(!text.galley.text().contains("Codex"),"the agent must not be repeated on a separate line");
                }
            }}
            assert_eq!(text_count,2,"conversation and status should form one row");
            if dock.is_side(){assert_eq!(s.out.as_ref().unwrap().hit_outline.as_ref().unwrap(),&s.out.as_ref().unwrap().glass.as_ref().unwrap()[0].outline());}
            s.click("notice-summary");opened_agent(&s,"codex:demo-task");
        }
    }}
}

#[test]
fn island_notice_gallery_uses_actual_egui_geometry_and_synthetic_data(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let folder=std::env::var_os("QINGYU_ISLAND_NOTICE_GALLERY").map(std::path::PathBuf::from);
    if let Some(folder)=&folder{std::fs::create_dir_all(folder).unwrap();}
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        for stage in ["message","short","working","waiting","interrupted","completed","expanded","light"]{
            if stage=="light"&&dock!=Dock::Top{continue;}
            let mut s=island(dock);s.h.ctx.set_pixels_per_point(2.0);
            if stage=="light"{s.h.app.settings.skin=crate::model::Skin::GlassLight;}
            let size=s.h.app.canvas_size();let mut raster=folder.as_ref().map(|_|crate::preview_raster::Raster::new((size.x*2.0)as usize,(size.y*2.0)as usize));
            s.h.app.agent_snapshot.generation=1;
            let output=s.frame(vec![]);if let Some(raster)=&mut raster{raster.update_textures(&output.textures_delta);raster.free_textures(&output.textures_delta);}
            if stage=="expanded"{
                s.h.app.show_page(TODO_PAGE);
                for _ in 0..65{let output=s.frame(vec![]);if let Some(raster)=&mut raster{raster.update_textures(&output.textures_delta);raster.free_textures(&output.textures_delta);}}
            }
            match stage {
                "message"|"light"=>{message(&mut s.h.app,1);message(&mut s.h.app,2);},
                "short"=>s.h.app.enqueue_alert(Alert{source:AlertSource::AgentEvent{session_id:"codex:compact-demo".into(),turn_key:1,kind:NoticeKind::Completed},title:"修复完成".into(),body:"Codex 已完成 · 总用时 12 秒".into(),page:AGENT_PAGE,at:s.h.t,duration:15.0},false),
                "working"=>lifecycle(&mut s,task(1,AgentStatus::Working),NoticeKind::Started),
                "waiting"=>lifecycle(&mut s,task(1,AgentStatus::Waiting),NoticeKind::Waiting),
                "interrupted"=>lifecycle(&mut s,task(1,AgentStatus::Error),NoticeKind::Interrupted),
                _=>{
                    next_completion(&mut s,1);
                    for session in &mut s.h.app.agent_snapshot.sessions{session.title="整理通知中心".into();session.started_at=1_794_699_156;}
                    for event in &mut s.h.app.agent_snapshot.completions{event.session.title="整理通知中心".into();event.session.started_at=1_794_699_156;}
                    for event in &mut s.h.app.agent_snapshot.notices{event.session.title="整理通知中心".into();event.session.started_at=1_794_699_156;}
                },
            }
            for frame in 0..65 {
                let output=s.frame(vec![]);
                if let Some(raster)=&mut raster{
                    raster.update_textures(&output.textures_delta);
                    if frame==64 || stage=="working"&&[0,1,3,7,14].contains(&frame){
                        raster.buf.fill([0.13,0.18,0.20,1.0]);raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                        let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                        let name=if frame==64{format!("demo-{}-{stage}.png",dock.key())}else{format!("demo-{}-enter-{:03}ms.png",dock.key(),(frame+1)*1000/60)};
                        image::save_buffer(folder.as_ref().unwrap().join(name),&rgba,(size.x*2.0)as u32,(size.y*2.0)as u32,image::ColorType::Rgba8).unwrap();
                    }
                    raster.free_textures(&output.textures_delta);
                }
            }
            assert!(s.h.app.display_notice().is_some());
            assert_eq!(s.h.app.open,stage=="expanded");
            let panel=s.h.app.displayed_island_rect(Rect::from_min_size(Pos2::ZERO,size));
            let notice=s.h.app.notice_surface(Rect::from_min_size(Pos2::ZERO,size)).unwrap().rect;
            assert!(!notice.intersects(panel));
            for shape in &s.h.shapes{if let egui::Shape::Text(text)=&shape.shape{
                if shape.clip_rect.intersects(notice)&&text.visual_bounding_rect().intersects(notice)&&text.opacity_factor>0.9{
                    assert!(text.visual_bounding_rect().left()>=notice.left()-1.0&&text.visual_bounding_rect().right()<=notice.right()+1.0,"{dock:?}/{stage}: {}",text.galley.text());
                }
            }}
            if stage=="working"{
                let alert=s.h.app.notice.clone().unwrap();s.h.app.dismiss_notice(&alert);
                for frame in 0..30{
                    let output=s.frame(vec![]);
                    if let Some(raster)=&mut raster{
                        raster.update_textures(&output.textures_delta);
                        if [0,1,3,7,14].contains(&frame){
                            raster.buf.fill([0.13,0.18,0.20,1.0]);raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
                            image::save_buffer(folder.as_ref().unwrap().join(format!("demo-{}-exit-{:03}ms.png",dock.key(),(frame+1)*1000/60)),&rgba,(size.x*2.0)as u32,(size.y*2.0)as u32,image::ColorType::Rgba8).unwrap();
                        }
                        raster.free_textures(&output.textures_delta);
                    }
                }
                assert!(s.h.app.hud_notice.is_none());assert!(!s.h.app.notice_motion.animating());
            }
        }
    }
    if let Some(folder)=folder{std::fs::write(folder.join("README.txt"),"演示数据 / Synthetic fixtures. PNGs use actual egui drawing at 2× scale. No real conversation or messages are displayed. message=new message with queue; working=task metadata fixture; completed=completion journal; expanded=independent row below the unchanged content page. enter/exit=actual spring frames at 16,33,66,133,250ms for every dock.\n").unwrap();}
}
