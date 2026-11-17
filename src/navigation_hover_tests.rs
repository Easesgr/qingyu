//! Navigation intent, exercised with real egui pointer and keyboard input.
use super::*;
use super::control_center_tests::{click, draw, key, response};
use crate::{app::*, model::Dock};

fn panel(dock:Dock,width:u32,reduced:bool)->H {
    let mut h=H::new();
    h.app.settings.dock=dock;
    h.app.settings.side_panel_width=width;
    h.app.settings.page_order=[MUSIC_PAGE,SHELF_PAGE,TODO_PAGE,MAIL_PAGE,WEATHER_PAGE,AGENT_PAGE,CLIPBOARD_PAGE,MAC_STATS_PAGE,NOTIFICATIONS_PAGE];
    h.app.settings.clipboard_history=false;
    h.app.settings.agent_activity=false;
    h.app.settings.lyrics_enabled=false;
    h.ctx.data_mut(|data|data.insert_temp(egui::Id::new("macos-accessibility-preferences"),
        (std::time::Instant::now(),if reduced{2u32}else{0u32})));
    if reduced{h.ctx.style_mut(|style|style.animation_time=0.0);}
    h.app.show_control_center();h.app.panel_pinned=true;
    idle(&mut h,75);
    h
}

fn idle(h:&mut H,frames:usize){for _ in 0..frames{draw(h,vec![]);}}

fn tab(h:&H,page:usize)->egui::Response {
    response(h,(if h.app.settings.dock.is_side(){"portrait-tab"}else if cfg!(target_os="macos"){"page-tab"}else{"dot"},page))
}

fn move_to(h:&mut H,point:Pos2){h.cursor=Some(point);draw(h,vec![egui::Event::PointerMoved(point)]);}

fn hover(h:&mut H,point:Pos2){move_to(h,point);idle(h,22);}

fn content_point(h:&H)->Pos2 {
    h.app.content_origin(Rect::from_min_size(Pos2::ZERO,h.app.canvas_size()))
        +egui::vec2(h.app.expanded_width()/2.0,120.0)
}

#[test]
fn navigation_hover_selects_modules_and_main_in_every_dock_with_or_without_motion(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for reduced in [false,true]{for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut h=panel(dock,520,reduced);
        for page in [MUSIC_PAGE,SHELF_PAGE,TODO_PAGE,MAIL_PAGE,WEATHER_PAGE,AGENT_PAGE,CLIPBOARD_PAGE,MAC_STATS_PAGE,NOTIFICATIONS_PAGE]{
            let point=tab(&h,page).rect.center();
            let started=h.t;
            hover(&mut h,point);
            assert!(!h.app.is_control_center()&&h.app.page()==page,"{dock:?}/{reduced}: hover page {page}");
            assert!(h.t-started<=0.42,"navigation intent should finish after a brief deliberate dwell");
            let away=content_point(&h);hover(&mut h,away);
            assert_eq!(h.app.page(),page,"leaving a tab keeps its selected page");
            assert!(!h.app.is_control_center());
            let main=response(&h,"control-center-tab").rect.center();hover(&mut h,main);
            assert!(h.app.is_control_center(),"{dock:?}/{reduced}: hover main");
            hover(&mut h,away);assert!(h.app.is_control_center());
        }
    }}
}

#[test]
fn navigation_hover_ignores_brief_passes_and_press_drag_release_without_fresh_motion(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut h=panel(dock,520,false);
        let target=tab(&h,WEATHER_PAGE).rect.center();
        move_to(&mut h,target);idle(&mut h,3);
        let away=content_point(&h);move_to(&mut h,away);idle(&mut h,16);
        assert!(h.app.is_control_center(),"{dock:?}: passing a tab must not switch");
        let source=tab(&h,MUSIC_PAGE).rect.center();
        move_to(&mut h,source);
        draw(&mut h,vec![egui::Event::PointerButton{pos:source,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        move_to(&mut h,target);idle(&mut h,20);
        assert!(h.ctx.input(|input|input.pointer.is_decidedly_dragging()));
        assert!(h.app.is_control_center(),"{dock:?}: held/dragging pointer must not switch");
        draw(&mut h,vec![egui::Event::PointerButton{pos:target,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        idle(&mut h,20);
        assert!(h.app.is_control_center(),"{dock:?}: releasing a drag must not arm stationary hover");
        hover(&mut h,target+egui::vec2(1.0,0.0));
        assert!(!h.app.is_control_center());assert_eq!(h.app.page(),WEATHER_PAGE);
    }
}

#[test]
fn navigation_hover_keeps_side_strip_still_and_does_not_follow_scrolling_under_a_stationary_pointer(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for reduced in [false,true]{for dock in [Dock::Left,Dock::Right]{
        let mut h=panel(dock,292,reduced);
        let near_end=tab(&h,MAIL_PAGE).rect.center();
        assert!(tab(&h,MAIL_PAGE).rect.width()>40.0);
        hover(&mut h,near_end);assert_eq!(h.app.page(),MAIL_PAGE);assert!(!h.app.is_control_center());
        let before=tab(&h,MAIL_PAGE).rect;idle(&mut h,75);
        assert_eq!(tab(&h,MAIL_PAGE).rect,before,"hover selection must not recenter the strip");
        assert_eq!(h.app.page(),MAIL_PAGE);
        let stationary=tab(&h,SHELF_PAGE).rect.center();hover(&mut h,stationary);
        assert_eq!(h.app.page(),SHELF_PAGE);
        h.app.show_page(NOTIFICATIONS_PAGE);idle(&mut h,80);
        assert_eq!(h.app.page(),NOTIFICATIONS_PAGE,"{dock:?}/{reduced}: programmatic centering must not cause a hover cascade");
        let revealed=(0..PAGE_COUNT).find(|page|*page!=NOTIFICATIONS_PAGE&&tab(&h,*page).rect.contains(stationary)).expect("scroll moves a different tab under the pointer");
        hover(&mut h,stationary+egui::vec2(1.0,0.0));
        assert_eq!(h.app.page(),revealed,"fresh pointer motion can select the revealed tab");
        let scroll_target=(0..PAGE_COUNT).find(|page|*page!=revealed&&tab(&h,*page).rect.width()>40.0).unwrap();
        let point=tab(&h,scroll_target).rect.center();move_to(&mut h,point);idle(&mut h,3);
        draw(&mut h,vec![egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(90.0,0.0),modifiers:Default::default()}]);
        idle(&mut h,80);assert_eq!(h.app.page(),revealed,"scrolling cancels a pending hover");
        let id=tab(&h,MUSIC_PAGE).id;h.ctx.memory_mut(|memory|memory.request_focus(id));idle(&mut h,80);
        assert_eq!(h.app.page(),revealed,"keyboard focus scrolling alone must not select another page");
        key(&mut h,egui::Key::Enter,egui::Modifiers::NONE);assert_eq!(h.app.page(),MUSIC_PAGE);
    }}
}

#[test]
fn navigation_hover_preserves_editing_and_ime_until_explicit_navigation(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut h=panel(dock,520,false);h.app.show_page(TODO_PAGE);
        idle(&mut h,25);h.app.adding=true;h.app.buf="保留草稿".into();
        draw(&mut h,vec![egui::Event::Ime(egui::ImeEvent::Enabled),egui::Event::Ime(egui::ImeEvent::Preedit("zhong".into()))]);
        let main=response(&h,"control-center-tab").rect.center();hover(&mut h,main);idle(&mut h,20);
        assert_eq!(h.app.page(),TODO_PAGE);assert!(!h.app.is_control_center());assert!(h.app.is_editing());
        assert!(h.app.buf.starts_with("保留草稿"));
        let target=tab(&h,SHELF_PAGE).rect.center();hover(&mut h,target);idle(&mut h,20);
        assert_eq!(h.app.page(),TODO_PAGE);assert!(h.app.is_editing());
        draw(&mut h,vec![egui::Event::Ime(egui::ImeEvent::Commit("中文".into())),egui::Event::Ime(egui::ImeEvent::Disabled)]);
        idle(&mut h,16);assert_eq!(h.app.page(),TODO_PAGE);assert!(h.app.is_editing());
        click(&mut h,main);assert!(h.app.is_control_center());assert!(!h.app.is_editing());
        hover(&mut h,target);assert_eq!(h.app.page(),SHELF_PAGE);assert!(!h.app.is_control_center());
    }
}

#[test]
fn navigation_hover_pending_intent_is_canceled_by_other_navigation(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut h=panel(dock,520,false);
        let target=tab(&h,SHELF_PAGE).rect.center();move_to(&mut h,target);idle(&mut h,3);
        h.app.show_page(WEATHER_PAGE);idle(&mut h,20);
        assert_eq!(h.app.page(),WEATHER_PAGE,"external navigation cancels a pending hover");
        hover(&mut h,target+egui::vec2(1.0,0.0));assert_eq!(h.app.page(),SHELF_PAGE);
        h.app.show_control_center();idle(&mut h,2);
        let target=tab(&h,TODO_PAGE).rect.center();move_to(&mut h,target);idle(&mut h,3);
        key(&mut h,egui::Key::ArrowRight,egui::Modifiers::NONE);idle(&mut h,20);
        assert_eq!(h.app.page(),MUSIC_PAGE,"keyboard navigation cancels a pending hover");
        hover(&mut h,target+egui::vec2(1.0,0.0));assert_eq!(h.app.page(),TODO_PAGE);
    }
}

#[test]
fn navigation_hover_preserves_a_focused_text_editor_without_an_app_editing_flag(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let mut h=H::new();h.app.show_page(MUSIC_PAGE);
    let mut draft="尚未提交的文字".to_owned();
    let mut probe=|h:&mut H,events:Vec<egui::Event>,focus:bool|{
        h.t+=1.0/60.0;
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(360.0,240.0))),time:Some(h.t),focused:true,events,..Default::default()};
        let _=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
            let editor=ui.put(Rect::from_min_size(egui::pos2(10.0,120.0),egui::vec2(320.0,30.0)),
                egui::TextEdit::singleline(&mut draft).id(egui::Id::new("navigation-editor-probe")));
            if focus{editor.request_focus();}
            let card=crate::paint::Card::new(ui,Pos2::ZERO,ui.max_rect());
            h.app.control_center_tab(&card,10.0,10.0,28.0,28.0);
        });});
    };
    probe(&mut h,vec![],true);probe(&mut h,vec![],false);
    assert!(!h.app.is_editing());
    let point=egui::pos2(24.0,24.0);
    probe(&mut h,vec![egui::Event::PointerMoved(point)],false);
    for _ in 0..20{probe(&mut h,vec![],false);}
    assert!(!h.app.is_control_center());assert_eq!(h.app.page(),MUSIC_PAGE);
    assert_eq!(h.ctx.memory(|memory|memory.focused()),Some(egui::Id::new("navigation-editor-probe")));
    for pressed in [true,false]{probe(&mut h,vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}],false);}
    assert!(h.app.is_control_center(),"an explicit click can leave the editor");
    assert_eq!(draft,"尚未提交的文字");
}

#[test]
fn navigation_hover_retains_immediate_click_and_keyboard_activation(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut h=panel(dock,292,false);
        let point=tab(&h,SHELF_PAGE).rect.center();click(&mut h,point);
        assert_eq!(h.app.page(),SHELF_PAGE);assert!(!h.app.is_control_center());
        idle(&mut h,35);
        let main=response(&h,"control-center-tab");h.ctx.memory_mut(|memory|memory.request_focus(main.id));
        key(&mut h,egui::Key::Space,egui::Modifiers::NONE);assert!(h.app.is_control_center());
        let module=tab(&h,NOTIFICATIONS_PAGE);h.ctx.memory_mut(|memory|memory.request_focus(module.id));idle(&mut h,35);
        key(&mut h,egui::Key::Enter,egui::Modifiers::NONE);
        assert_eq!(h.app.page(),NOTIFICATIONS_PAGE);assert!(!h.app.is_control_center());
        // A focused navigation tab must not be mistaken for a text editor.
        let main=response(&h,"control-center-tab").rect.center();hover(&mut h,main);
        assert!(h.app.is_control_center());
    }
}

#[test]
fn navigation_hover_does_not_activate_ghost_tabs_or_interrupt_file_drag(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let mut h=H::new();h.app.show_page(MUSIC_PAGE);
    let point=egui::pos2(34.0,72.0);
    for frame in 0..24{
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(520.0,700.0))),time:Some(frame as f64/60.0),focused:true,
            events:vec![egui::Event::PointerMoved(point)],..Default::default()};
        let _=h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
            let card=crate::paint::Card::new(ui,Pos2::ZERO,ui.max_rect()).ghost();
            h.app.control_center_tab(&card,20.0,48.0,44.0,48.0);
            h.app.portrait_panel(&card,&Env::default(),&mut Vec::new());
        });});
    }
    assert_eq!(h.app.page(),MUSIC_PAGE);assert!(!h.app.is_control_center());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut h=panel(dock,520,false);
        h.hovered_files=vec![egui::HoveredFile{path:Some("/synthetic/drag.txt".into()),mime:String::new()}];draw(&mut h,vec![]);
        assert_eq!(h.app.page(),SHELF_PAGE);assert!(!h.app.is_control_center());
        let main=response(&h,"control-center-tab").rect.center();hover(&mut h,main);idle(&mut h,16);
        assert_eq!(h.app.page(),SHELF_PAGE);assert!(!h.app.is_control_center(),"file drag keeps its destination");
        h.hovered_files.clear();idle(&mut h,12);assert!(h.app.is_control_center(),"canceled file drag restores the main view");
    }
}
