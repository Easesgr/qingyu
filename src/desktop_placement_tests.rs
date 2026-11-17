//! Positioning contract tests: egui drawing, input and native blur agree on the
//! same safe inset. A camera bridge is visual only and cannot steal menu clicks.
use super::*;

struct Fixture {ctx:egui::Context,app:App,t:f64,directory:std::path::PathBuf,previous:Option<std::ffi::OsString>}
impl Fixture {
    fn new()->Self {
        let directory=std::env::temp_dir().join(format!("qingyu-placement-test-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let previous=std::env::var_os("QINGYU_DATA_DIR");std::env::set_var("QINGYU_DATA_DIR",&directory);
        let ctx=egui::Context::default();let mut app=App::new(&ctx,Options { fetch_lyrics: false, monitor_agents: false });
        app.settings.pet=false;app.settings.skin=model::Skin::GlassDark;app.settings.glass_blur=true;
        Self{ctx,app,t:0.,directory,previous}
    }
    fn frame(&mut self,cursor:Option<Pos2>)->(FrameOut,Vec<egui::epaint::ClippedShape>) {
        self.t+=1./60.;let size=self.app.canvas_size();
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(CANVAS_W,size.y))),time:Some(self.t),events:cursor.map(egui::Event::PointerMoved).into_iter().collect(),..Default::default()};
        let env=Env{t:self.t,cursor,clock:"09:41:00".into(),today:"2026-11-15".into(),date:"11月15日 周日".into(),glass_blur:true,..Default::default()};
        let mut frame=None;
        let output=self.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{frame=Some(self.app.frame(ui,Rect::from_min_size(Pos2::ZERO,size),&env));});});
        (frame.unwrap(),output.shapes)
    }
}
impl Drop for Fixture {fn drop(&mut self){if let Some(previous)=&self.previous{std::env::set_var("QINGYU_DATA_DIR",previous)}else{std::env::remove_var("QINGYU_DATA_DIR")};let _=std::fs::remove_dir_all(&self.directory);}}

#[test]
fn camera_bridge_is_decorative_and_expanded_text_hits_and_blur_stay_below_it() {
    let _lock=crate::ui_tests::ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for scale in [0.8,1.,1.4] {
        let mut fixture=Fixture::new();let inset=38./scale;
        let bridge=Rect::from_min_size(egui::pos2(WIN_W/2.-90./scale,0.),egui::vec2(180./scale,inset));
        fixture.app.set_desktop_placement(DesktopPlacement::Camera{content_top:inset,bridge});
        assert_eq!(fixture.app.canvas_size().y,WIN_H+inset);
        fixture.app.settings.minimal=true;
        assert!(fixture.app.base_size().0>=bridge.width(),"minimal body still joins the camera without a black shelf");
        fixture.app.settings.minimal=false;
        let camera_cursor=Some(egui::pos2(WIN_W/2.,inset/2.));
        for _ in 0..90 {let (frame,_)=fixture.frame(camera_cursor);assert!(!frame.expanded);assert!(!frame.hit.contains(camera_cursor.unwrap()));}
        let cursor=Some(egui::pos2(WIN_W/2.,inset+16.));
        for _ in 0..90 {fixture.frame(cursor);}
        let (frame,shapes)=fixture.frame(cursor);assert!(frame.expanded);assert!(frame.hit.top()>=inset);assert!(frame.file_drop_zone.unwrap().top()>=inset);
        assert!(frame.region.top()<=0.0,"visual bridge remains in the visible region");
        for shape in frame.glass.unwrap(){assert!(shape.outline().iter().all(|p|p.y>=inset-0.01),"blur cannot cover the camera or menu bar");}
        let mut text_count=0;
        for clipped in shapes {if let egui::Shape::Text(text)=clipped.shape {
            let visible=text.visual_bounding_rect().intersect(clipped.clip_rect);
            if visible.is_positive(){text_count+=1;assert!(visible.top()>=inset-0.01,"{} overlaps camera: {:?}",text.galley.text(),visible);assert!(visible.bottom()<=WIN_H+inset+0.01);}
        }}
        assert!(text_count>3,"rendered real expanded content");
    }
}

#[test]
fn floating_mode_removes_legacy_gap_without_overwriting_preferences() {
    let _lock=crate::ui_tests::ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut fixture=Fixture::new();fixture.app.settings.island_shape=model::IslandShape::Notch;
    fixture.app.set_desktop_placement(DesktopPlacement::Floating{content_top:8.});
    assert_eq!(fixture.app.island_shape(),model::IslandShape::Pill);assert_eq!(fixture.app.island_gap(),8.);
    let (frame,_)=fixture.frame(None);assert_eq!(frame.hit.top(),8.);assert_eq!(fixture.app.canvas_size(),egui::vec2(WIN_W,WIN_H+8.));
    assert!(matches!(&frame.glass.unwrap()[0],GlassShape::Round(..)));
    fixture.app.set_desktop_placement(DesktopPlacement::Legacy);assert_eq!(fixture.app.island_shape(),model::IslandShape::Notch);
    fixture.app.set_desktop_placement(DesktopPlacement::Camera{content_top:38.,bridge:Rect::from_min_size(Pos2::ZERO,egui::vec2(180.,38.))});
    fixture.app.settings.dock=model::Dock::Left;
    assert_eq!(fixture.app.canvas_size(),egui::vec2(crate::side_dock::SIDE_W,crate::side_dock::SIDE_H));assert_eq!(fixture.app.island_gap(),0.);
    assert_eq!(fixture.app.settings.island_shape,model::IslandShape::Notch);
}

#[test]
fn positioning_modes_persist_and_unknown_values_use_auto() {
    let _lock=crate::ui_tests::ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let fixture=Fixture::new();
    for mode in model::MacosPlacement::ALL {
        let mut settings=model::Settings::default();settings.macos_placement=mode;settings.save();assert_eq!(model::Settings::load().macos_placement,mode);
    }
    assert_eq!(model::MacosPlacement::parse("future-mode"),model::MacosPlacement::Auto);
    std::fs::write(fixture.directory.join("settings.txt"),"macos_placement=future-mode\n").unwrap();
    assert_eq!(model::Settings::load().macos_placement,model::MacosPlacement::Auto);
}

#[cfg(target_os="macos")]
#[test]
fn mac_edge_handle_keeps_adjacent_desktop_clickable_and_waits_for_hover_intent(){
    let _lock=crate::ui_tests::ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut fixture=Fixture::new();fixture.app.settings.dock=model::Dock::Right;fixture.app.settings.macos_side_handle=true;
    for _ in 0..90{fixture.frame(None);}
    let (frame,_)=fixture.frame(None);
    assert!(!frame.expanded);assert!(frame.hit.width()<=8.1);assert!(frame.hit.height()<=57.0);
    let center=frame.hit.center();
    assert!(!frame.hit.contains(center-egui::vec2(12.0,0.0)),"desktop beside the handle stays clickable");
    for _ in 0..12{let (frame,_)=fixture.frame(Some(center));assert!(!frame.expanded,"brief pointer crossings cannot expand the island");}
    for _ in 0..35{fixture.frame(Some(center));}
    assert!(fixture.frame(Some(center)).0.expanded);
    fixture.app.settings.save();assert!(model::Settings::load().macos_side_handle);
}
