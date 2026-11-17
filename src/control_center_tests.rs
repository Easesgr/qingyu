//! Deterministic egui inputs; all gallery content is synthetic.
use super::*;
use crate::{app::*, agent_monitor::{AgentKind, Session, Status}, extras::{Alert, AlertSource}, model::{Dock, Skin}};

pub(super) fn draw(h: &mut H, events: Vec<egui::Event>) -> (egui::FullOutput, FrameOut) {
    h.t += 1.0 / 60.0;
    let size = h.app.canvas_size();
    let canvas = Rect::from_min_size(Pos2::ZERO, if h.app.settings.dock.is_side() { size } else { Vec2::new(CANVAS_W, size.y) });
    let win = Rect::from_min_size(Pos2::ZERO, size);
    let env = Env { t: h.t, unix_time: 1_794_703_260, clock: "09:41:00".into(), today: "2026-11-15".into(),
        date: "11月15日 周日".into(), media: h.media.clone(), cursor: h.cursor,
        // Negative display origin exercises the same coordinate space as AppKit.
        screen: Some(Rect::from_min_max(Pos2::new(-800.0, -200.0), Pos2::new(1440.0, 1400.0))),
        screen_locked: h.screen_locked, ..Default::default() };
    let mut raw = egui::RawInput { screen_rect: Some(canvas), time: Some(h.t), events, focused: true,
        hovered_files: h.hovered_files.clone(), dropped_files: std::mem::take(&mut h.dropped_files), ..Default::default() };
    h.app.filter_todo_input(&mut raw);
    let mut frame = None;
    let output = h.ctx.run(raw, |ctx| {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| { frame = Some(h.app.frame(ui, win, &env)); });
    });
    let frame = frame.unwrap();
    h.shapes = output.shapes.clone(); h.hit = frame.hit; h.region = frame.region; h.cmds.extend(frame.cmds.clone());
    (output, frame)
}

fn hover(h: &mut H, cursor: Option<Pos2>, seconds: f64) {
    h.cursor = cursor;
    for _ in 0..(seconds * 60.0) as usize {
        draw(h, cursor.map(|pos| vec![egui::Event::PointerMoved(pos)]).unwrap_or_else(|| vec![egui::Event::PointerGone]));
    }
}

fn edge(h: &H) -> Pos2 {
    let size = h.app.canvas_size();
    match h.app.settings.dock { Dock::Top => Pos2::new(WIN_W / 2.0, 15.0),
        Dock::Left => Pos2::new(1.0, size.y / 2.0), Dock::Right => Pos2::new(size.x - 1.0, size.y / 2.0) }
}

#[track_caller]
pub(super) fn response(h: &H, id: impl std::hash::Hash) -> egui::Response {
    h.ctx.read_response(egui::Id::new(("card", true, id))).expect("visible interactive control")
}

fn focus_control_tab(h: &mut H) -> egui::Response {
    let tab = response(h, "control-center-tab");
    h.ctx.memory_mut(|memory| memory.request_focus(tab.id));
    // Side navigation scrolls horizontally to the selected or focused tab.
    for _ in 0..35 { draw(h, vec![]); }
    let tab = response(h, "control-center-tab");
    assert!(tab.rect.width() >= 28.0 - 0.01);
    tab
}

pub(super) fn click(h: &mut H, point: Pos2) {
    h.cursor = Some(point); draw(h, vec![egui::Event::PointerMoved(point)]);
    for pressed in [true, false] { draw(h, vec![egui::Event::PointerButton { pos: point, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }]); }
}

pub(super) fn key(h: &mut H, key: egui::Key, modifiers: egui::Modifiers) {
    draw(h, vec![egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }]);
    draw(h, vec![egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers }]);
}

fn fixture(h: &mut H) {
    h.app.settings.notification_preview = true;
    h.app.settings.agent_activity = false;
    h.media = crate::smtc::NowPlaying { source: "cloudmusic.exe".into(), title: "漫步云端".into(), artist: "轻屿电台".into(), duration: 224.0, position: 42.0, playing: true,volume:Some((66,false)), ..Default::default() };
    h.app.todos.items = vec![crate::model::TodoItem::new("整理下周的设计素材"), crate::model::TodoItem::new("给窗边的植物浇水"), crate::model::TodoItem::new("阅读 20 分钟")];
    h.app.shelf.items = vec!["/synthetic/周报草稿.pdf".into(), "/synthetic/周末清单.md".into()];
    h.app.clipboard.lock().items = vec![crate::clipboard::Clip { text: "此为合成的粘贴板示例内容".into(), at: "09:38".into(), pinned: true, media: None }];
    h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,feels:Some(18.0),humidity:Some(58),wind:Some(12.0),high:Some(23.0),low:Some(15.0),sunrise:Some("06:52".into()),sunset:Some("17:21".into()),code:2,day:true,at:1_794_703_260,
        days:(0..7).map(|index|crate::weather::Day{label:["今天","明天","周二","周三","周四","周五","周六"][index].into(),date:format!("2026-11-{:02}",15+index),code:[2,0,3,61,80,2,0][index],high:[23.0,24.0,22.0,19.0,18.0,20.0,22.0][index],low:[15.0,16.0,14.0,13.0,12.0,12.0,14.0][index],rain:[10,0,20,70,55,10,0][index],..Default::default()}).collect(),..Default::default()});
    h.app.agent_snapshot.sessions = vec![Session { id: "synthetic-task".into(), title: "优化音乐播放体验".into(), agent: AgentKind::Codex, status: Status::Working,
        started_at: 1_794_703_000, turn_key: 1, ..Default::default() }, Session { id: "synthetic-wait".into(), title: "核对页面间距".into(), agent: AgentKind::Claude, status: Status::Waiting, started_at: 1_794_703_100, turn_key: 2, ..Default::default() }];
    h.app.mac_snapshot = crate::mac_stats::Snapshot { supported: true, paused: false, cpu_usage: Some(24.0), memory_total: 16 * 1024u64.pow(3), memory_used: 7 * 1024u64.pow(3), memory_valid: true,
        disk_available: 206 * 1024u64.pow(3), disk_valid: true, ..Default::default() };
    let now=std::time::SystemTime::now();
    for index in 0..24{
        h.app.mac_snapshot.sampled_at=Some(now-std::time::Duration::from_secs((23-index)*2));
        h.app.mac_snapshot.cpu_usage=Some(24.0+(index as f64*0.7).sin()*7.0);
        h.app.mac_snapshot.disk_total=512*1024u64.pow(3);
        h.app.mac_snapshot.memory_compressed=512*1024u64.pow(2);
        h.app.mac_snapshot.gpus=vec![crate::mac_stats::Gpu{name:"Apple M1 Pro".into(),usage:Some(15.0+(index as f64*0.4).sin()*4.0)}];
        h.app.mac_snapshot.network=Some(crate::mac_stats::Network{interface:"en0".into(),received_bytes_per_sec:Some(1024.0*(256.0+64.0*(index as f64*0.3).sin())),sent_bytes_per_sec:Some(12.0*1024.0),..Default::default()});
        h.app.mac_snapshot.network_status=crate::mac_stats::NetworkStatus::Online;
        h.app.stats_history.observe(&h.app.mac_snapshot);
    }
    h.app.mac_snapshot.cpu_usage=Some(24.0);
    let account = crate::mail::Account { email: "synthetic@qq.com".into(), provider: crate::mail::Provider::Qq, custom: None, enabled: true };
    h.app.mail.accounts = vec![account];
    h.app.mail.status.insert("synthetic@qq.com".into(), crate::mail::Status::Ready);
    h.app.mail.recent = vec![crate::mail::Item { account: "synthetic@qq.com".into(), from: "示例团队".into(), subject: "下周的行程安排".into(), time: "09:22".into(), code: None, summary: "合成邮件，无真实内容".into(), html: "".into(), links: vec![], uid: 42, received_unix: None }];
    for index in 0..12{
        h.app.shelf.items.push(format!("/synthetic/设计资料-{}.pdf",index+1).into());
        h.app.clipboard.lock().items.push(crate::clipboard::Clip{text:format!("合成记录 {}：留下一点今天的灵感",index+1),at:format!("09:{:02}",35-index),pinned:false,media:None});
        let mut item=h.app.mail.recent[0].clone();item.uid=43+index;item.subject=format!("合成邮件 {} · 下周的行程安排",index+1);h.app.mail.recent.push(item);
        h.app.history.push_back(Alert{source:AlertSource::Internal,title:format!("合成日常提醒 {}",index+1),body:"设计资料已经准备好，可在详情查看".into(),page:TODO_PAGE,at:h.t,duration:10.0});
    }
    let files=crate::model::config_dir().join("synthetic-files");std::fs::create_dir_all(&files).unwrap();
    for (index,path) in h.app.shelf.items.iter_mut().enumerate(){let file=files.join(path.file_name().unwrap());std::fs::write(&file,format!("Synthetic review file {}. No user content.",index+1)).unwrap();*path=file;}
    h.app.qq_library=crate::qq_music::Library{session:crate::qq_music::SessionStatus::Connected,logged_in:true,
        playlists:(0..8).map(|index|crate::qq_music::Playlist{id:format!("synthetic-playlist-{index}"),title:format!("合成歌单 {} · 午后的风",index+1)}).collect(),..Default::default()};
    let mut art=egui::ColorImage::new([64,64],egui::Color32::BLACK);
    for y in 0..64{for x in 0..64{let sky=y as f32/63.0;let sun=(x as f32-44.0).powi(2)+(y as f32-20.0).powi(2)<90.0;art.pixels[y*64+x]=if sun{egui::Color32::from_rgb(238,196,136)}else{egui::Color32::from_rgb((49.0+sky*14.0)as u8,(102.0-sky*46.0)as u8,(122.0-sky*30.0)as u8)};}}
    h.media.artwork=Some(std::sync::Arc::new(art));
    h.app.library.entries=(0..8).map(|index|crate::library::Entry{id:format!("synthetic-{index}"),kind:[crate::library::Kind::Idea,crate::library::Kind::Link,crate::library::Kind::Action][index%3],title:format!("合成灵感 {} · 为日常留一点空白",index+1),summary:"整理今天的想法，在有空的时候继续探索。".into(),original:"合成的原始记录，无真实用户内容。".into(),url:None,tags:vec!["设计".into()],topic:Some("日常灵感".into()),next_action:None,source:"合成资料".into(),created_at:"2026-11-15T09:00:00+09:00".into(),updated_at:"2026-11-15T09:00:00+09:00".into(),state:Default::default()}).collect();

}

#[test]
fn control_center_peer_navigation_keeps_all_modules_and_shares_header_and_settings() {
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut h=H::new();fixture(&mut h);h.app.settings.dock=dock;
        let point=edge(&h);hover(&mut h,Some(point),1.2);
        let home=response(&h,"control-center-tab").rect;
        let setting=if dock.is_side(){"portrait-settings"}else{"page-settings"};
        let setting_rect=response(&h,setting).rect;
        assert!(!h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="主控")));
        let order=h.app.settings.page_order;
        for page in order{
            h.app.show_page(page);hover(&mut h,Some(point),0.6);
            let tab=response(&h,(if dock.is_side(){"portrait-tab"}else{"page-tab"},page));
            assert!(tab.rect.width()>=28.0&&tab.rect.height()>=28.0,"{dock:?} page {page} selected tab stays reachable");
            assert_eq!(response(&h,"control-center-tab").rect,home,"home is a fixed peer, not a replacement for a module");
            assert_eq!(response(&h,setting).rect,setting_rect);
        }
        h.app.show_page(*order.last().unwrap());key(&mut h,egui::Key::ArrowRight,egui::Modifiers::NONE);
        assert!(h.app.overview,"last module → home");
        key(&mut h,egui::Key::ArrowRight,egui::Modifiers::NONE);assert_eq!(h.app.page(),order[0]);assert!(!h.app.overview);
        key(&mut h,egui::Key::ArrowLeft,egui::Modifiers::NONE);assert!(h.app.overview,"first module → home");
        click(&mut h,setting_rect.center());assert_eq!(h.app.settings_tab,0);
        h.app.settings_open=false;h.app.show_page(WEATHER_PAGE);hover(&mut h,Some(point),0.5);
        click(&mut h,setting_rect.center());assert_eq!(h.app.settings_tab,PAGE_SETTINGS_TABS[WEATHER_PAGE]);
    }
}

#[test]
fn control_center_pet_uses_the_resident_identity_without_another_pet_hit_target(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|error|error.into_inner());
    let mut h=H::new();
    for kind in crate::model::PetKind::ALL{
        h.app.settings.resident_pet=kind;
        let mut render=|time,reduced|{
            h.app.t=time;h.app.reduced_motion=reduced;
            h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(100.0,50.0))),time:Some(time),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                    let c=crate::paint::Card::new(ui,Pos2::ZERO,ui.clip_rect());
                    h.app.control_center_symbol(&c,25.0,25.0,24.0,true,crate::theme::CYAN);
                });
            }).shapes
        };
        assert_eq!(format!("{:?}",render(1.0,true)),format!("{:?}",render(12.7,true)));
        assert_ne!(format!("{:?}",render(1.0,false)),format!("{:?}",render(12.7,false)));
        assert!(h.ctx.read_response(egui::Id::new(("card",true,"island-pet"))).is_none());
    }
}

#[test]
fn control_center_full_content_uses_extra_rows_and_music_controls_commit(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();fixture(&mut h);let point=edge(&h);
    h.app.show_page(SHELF_PAGE);hover(&mut h,Some(point),0.9);
    let path=h.app.shelf.items[9].clone();let file=h.ctx.read_response(egui::Id::new(("card-drag",true,("shelf",&path)))).unwrap();assert!(file.rect.height()>45.0&&file.rect.bottom()>185.0,"the second file row uses the expanded body");
    h.app.show_page(CLIPBOARD_PAGE);hover(&mut h,Some(point),0.6);
    let clip=h.app.clipboard.lock().items[7].clone();let row=response(&h,("clip",7usize,clip.text.len(),clip.at.as_str()));assert!(row.rect.bottom()>200.0);
    h.app.show_page(MAIL_PAGE);hover(&mut h,Some(point),0.6);
    let item=h.app.mail.recent[6].clone();let row=response(&h,("mail-row",item.uid,item.account.as_str()));assert!(row.rect.height()>15.0&&row.rect.bottom()>180.0);
    h.app.show_page(MUSIC_PAGE);hover(&mut h,Some(point),0.6);
    assert!(response(&h,"m-play").rect.center().y>170.0);
    let seek=h.ctx.read_response(egui::Id::new(("card-drag",true,"m-seek"))).unwrap();let pos=Pos2::new(seek.rect.left()+seek.rect.width()/331.0*(4.0+323.0*0.75),seek.rect.center().y);
    click(&mut h,pos);assert!(h.cmds.iter().any(|command|matches!(command,Cmd::Media(crate::smtc::Transport::Seek(value))if (*value-168.0).abs()<0.5)));
    let library=response(&h,"m-qq-library");assert!(library.rect.bottom()>225.0);click(&mut h,library.rect.center());assert!(h.app.qq_library_open);
}

#[test]
fn control_center_hover_opens_overview_with_and_without_notices_on_every_dock() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] { for notice in 0..4 { for fine in [false, true] {
        let mut h = H::new(); h.app.settings.dock = dock; h.app.settings.macos_side_handle = fine;
        match notice {
            1 => h.app.alert("待办提醒", "待处理事项", TODO_PAGE, false),
            2 => { h.app.agent_snapshot.sessions = vec![Session { id: "hover-task".into(), status: Status::Working, turn_key: 1, ..Default::default() }];
                h.app.activity_notice = Some(Alert { source: AlertSource::AgentActivity { session_id: "hover-task".into(), turn_key: 1 }, title: "合成执行任务".into(), body: "执行中".into(), page: AGENT_PAGE, at: 0.0, duration: 10.0 }); }
            3 => h.app.enqueue_alert(Alert { source: AlertSource::Agent { session_id: "hover-task".into(), turn_key: 1 }, title: "合成完成任务".into(), body: "已完成".into(), page: AGENT_PAGE, at: 0.0, duration: 10.0 }, false),
            _ => {}
        }
        let point = edge(&h); hover(&mut h, Some(point), 1.4);
        assert!(h.app.open && h.app.is_control_center(), "{dock:?}, notice={notice}, handle={fine}");
        assert!(response(&h, "control-center-tab").rect.is_positive(), "the main overview has its own navigation icon");
        assert!(response(&h, ("control-module", MUSIC_PAGE)).rect.is_positive());
        if notice > 0 { assert!(response(&h, "notice-summary").rect.is_positive()); }
    } } }
}

#[test]
fn control_center_all_details_keep_one_expanded_frame_and_have_no_side_arrows() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] { for large in [false, true] {
        let mut h = H::new(); fixture(&mut h); h.app.settings.dock = dock;
        if large { h.app.settings.side_panel_width = 520; h.app.settings.side_panel_height = 900; }
        let point = edge(&h); hover(&mut h, Some(point), 1.4);
        let expected = h.app.displayed_island_rect(Rect::from_min_size(Pos2::ZERO, h.app.canvas_size()));
        let target = h.app.h.target;
        for page in 0..PAGE_COUNT {
            h.app.show_page(page);
            assert_eq!(h.app.h.target, target, "{dock:?} {page}: selecting a module must not change its target height");
            hover(&mut h, Some(point), 0.7);
            let rect = h.app.displayed_island_rect(Rect::from_min_size(Pos2::ZERO, h.app.canvas_size()));
            assert!((rect.height() - expected.height()).abs() < 0.1 && (rect.width() - expected.width()).abs() < 0.1);
            for id in ["nav-l", "nav-r"] { assert!(h.ctx.read_response(egui::Id::new(("card", true, id))).is_none()); }
        }
        h.app.show_page(MAIL_PAGE); h.app.mail.open_uid = Some(("synthetic@qq.com".into(), 42)); draw(&mut h, vec![]);
        assert_eq!(h.app.expanded_height(), target); assert_eq!(h.app.h.target, target);
        h.app.show_page(MUSIC_PAGE); h.app.qq_library_open = true; draw(&mut h, vec![]);
        assert_eq!(h.app.expanded_height(), target); assert_eq!(h.app.h.target, target);
        h.app.qq_library_open = false; hover(&mut h, Some(point), 0.5);
        assert_eq!(h.app.h.target, target);
        if !dock.is_side() {
            for id in ["m-prev", "m-next"] { assert!(response(&h, id).rect.is_positive(), "track controls stay available"); }
        }
        h.app.show_control_center(); draw(&mut h, vec![]); assert_eq!(h.app.h.target, target);
        assert_eq!((h.app.settings.side_panel_width, h.app.settings.side_panel_height), if large { (520, 900) } else { (292, 532) });
    } }
}

#[test]
fn control_center_detail_stays_until_collapse_then_hover_returns_to_overview() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let mut h = H::new(); h.app.settings.dock = dock; h.app.show_page(AGENT_PAGE);
        let point = edge(&h); hover(&mut h, Some(point), 1.2);
        assert!(!h.app.is_control_center());
        hover(&mut h, Some(point), 1.0); assert!(!h.app.is_control_center(), "staying inside cannot reset details");
        hover(&mut h, None, 5.5); assert!(!h.app.open);
        hover(&mut h, Some(point), 1.3);
        assert!(h.app.is_control_center()); assert_eq!(h.app.page(), AGENT_PAGE);
    }
}

#[test]
fn control_center_cards_open_same_or_other_module_and_return_button_works() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] { for page in [MUSIC_PAGE, WEATHER_PAGE] {
        let mut h = H::new(); h.app.settings.dock = dock;
        // In a tall one-column side panel, keep the tested cards in view.
        h.app.settings.page_order = [MUSIC_PAGE, WEATHER_PAGE, SHELF_PAGE, TODO_PAGE, MAIL_PAGE, AGENT_PAGE, CLIPBOARD_PAGE, MAC_STATS_PAGE, NOTIFICATIONS_PAGE];
        let point = edge(&h); hover(&mut h, Some(point), 1.4);
        let card = response(&h, ("control-module", page)); click(&mut h, card.rect.center());
        assert!(!h.app.is_control_center()); assert_eq!(h.app.page(), page);
        hover(&mut h, Some(point), 0.8);
        let back = focus_control_tab(&mut h); click(&mut h, back.rect.center());
        assert!(h.app.is_control_center());
    } }
}

#[test]
fn control_center_quick_actions_do_not_navigate_or_duplicate_commands() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new(); fixture(&mut h); let point = edge(&h); hover(&mut h, Some(point), 1.4);
    let play = response(&h, "control-music-play"); click(&mut h, play.rect.center());
    assert!(h.app.is_control_center()); assert_eq!(h.cmds.iter().filter(|cmd| matches!(cmd, Cmd::Media(crate::smtc::Transport::Toggle))).count(), 1);
    let add = response(&h, "control-shelf-add"); click(&mut h, add.rect.center());
    assert!(h.app.is_control_center()); assert_eq!(h.cmds.iter().filter(|cmd| matches!(cmd, Cmd::AddFiles)).count(), 1);
}

#[test]
fn control_center_music_transports_share_one_row_and_keep_keyboard_and_accessibility_actions() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] { for scale in [1.0, 1.25, 2.0] {
        let mut h = H::new(); fixture(&mut h); h.app.settings.dock = dock; h.ctx.set_pixels_per_point(scale);
        let point = edge(&h); hover(&mut h, Some(point), 1.4);
        let card = response(&h, ("control-module", MUSIC_PAGE)).rect;
        let controls = ["control-music-previous", "control-music-play", "control-music-next"].map(|id| response(&h, id));
        for control in &controls {
            assert!(control.rect.width() >= 26.0 - 0.05 && control.rect.height() >= 26.0 - 0.05);
            assert!(card.expand(0.05).contains_rect(control.rect));
            assert!((control.rect.center().y - controls[0].rect.center().y).abs() < 0.05);
        }
        for pair in controls.windows(2) { assert!(pair[0].rect.right() < pair[1].rect.left()); }
        for (control, command) in controls.iter().zip([crate::smtc::Transport::Prev, crate::smtc::Transport::Toggle, crate::smtc::Transport::Next]) {
            h.cmds.clear(); click(&mut h, control.rect.center());
            assert_eq!(h.cmds, vec![Cmd::Media(command)]); assert!(h.app.is_control_center());
        }
        h.cmds.clear(); h.cursor = None; draw(&mut h, vec![egui::Event::PointerGone]);
        h.ctx.memory_mut(|memory| memory.request_focus(controls[1].id));
        key(&mut h, egui::Key::Space, egui::Modifiers::NONE);
        assert_eq!(h.cmds, vec![Cmd::Media(crate::smtc::Transport::Toggle)]); assert!(h.app.is_control_center());
        #[cfg(target_os = "macos")]
        {
            h.ctx.enable_accesskit();
            h.cmds.clear();
            draw(&mut h, vec![egui::Event::AccessKitActionRequest(egui::accesskit::ActionRequest {
                action: egui::accesskit::Action::Click, target: controls[1].id.value().into(), data: None,
            })]);
            assert_eq!(h.cmds, vec![Cmd::Media(crate::smtc::Transport::Toggle)]); assert!(h.app.is_control_center());
        }
    } }
}

#[test]
fn control_center_tiles_cover_all_counts_and_preserve_saved_order_around_featured_agent() {
    for side in [false, true] { for size in [Vec2::new(395.0, 190.0), Vec2::new(272.0, 406.0), Vec2::new(500.0, 774.0)] {
        for count in 0..=PAGE_COUNT { for rotation in 0..PAGE_COUNT {
            let pages: Vec<_> = (0..PAGE_COUNT).map(|index| (index + rotation) % PAGE_COUNT).take(count).collect();
            let rects = crate::control_center::overview_rects(&pages, side, size);
            assert_eq!(rects.len(), pages.len());
            if count == 0 { continue; }
            let bounds = Rect::from_min_size(Pos2::ZERO, size);
            let mut area = 0.0;
            for (index, rect) in rects.iter().enumerate() {
                assert!(rect.is_positive() && bounds.expand(0.05).contains_rect(*rect));
                area += rect.area();
                for other in rects.iter().skip(index + 1) { assert!(!rect.shrink(0.01).intersect(other.shrink(0.01)).is_positive()); }
            }
            assert!((area - bounds.area()).abs() < 0.5, "{side}/{count}/{rotation}: every tile must fill the shared content area");
            if let Some(agent) = pages.iter().position(|page| *page == AGENT_PAGE) {
                let music = pages.iter().position(|page| *page == MUSIC_PAGE);
                if count > 1 { assert!(rects.iter().enumerate().all(|(index, rect)| index == agent || Some(index) == music || rect.area() < rects[agent].area())); }
                if let Some(music) = music {
                    if side { assert_eq!(rects[music].width(), size.x); assert_eq!(rects[agent].width(), size.x); }
                    else { assert!((rects[music].area() - rects[agent].area()).abs() < 0.1); }
                }
                let remaining: Vec<_> = rects.iter().enumerate().filter(|(index, _)| *index != agent && Some(*index) != music).map(|(_, rect)| rect.min).collect();
                assert!(remaining.windows(2).all(|pair| pair[0].y < pair[1].y || pair[0].y == pair[1].y && pair[0].x < pair[1].x), "unfeatured modules retain saved reading order");
            }
        } }
    } }
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_control_center_play_resumes_saved_music_and_cancels_in_place(){
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut h=H::new();h.app.settings.dock=dock;h.media=Default::default();
        h.app.set_qq_music(crate::qq_music::Library{resume:Some(crate::qq_music::Resume{title:"上次听的歌".into(),artist:"歌手".into(),position:42.0,duration:240.0,..Default::default()}),..Default::default()},None);
        let point=edge(&h);hover(&mut h,Some(point),1.4);
        h.cmds.clear();let play=response(&h,"control-music-play");click(&mut h,play.rect.center());
        assert_eq!(h.cmds,vec![Cmd::QqMusicResume]);assert!(h.app.is_control_center());assert!(h.app.qq_library.resuming);
        h.cmds.clear();let cancel=response(&h,"control-music-play");click(&mut h,cancel.rect.center());
        assert_eq!(h.cmds,vec![Cmd::QqMusicStop]);assert!(h.app.is_control_center());assert!(!h.app.qq_library.resuming);
    }
}

#[test]
fn control_center_order_and_disabled_modules_follow_existing_settings() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new(); h.app.settings.page_order = [AGENT_PAGE, WEATHER_PAGE, MUSIC_PAGE, NOTIFICATIONS_PAGE, SHELF_PAGE, MAIL_PAGE, TODO_PAGE, CLIPBOARD_PAGE, MAC_STATS_PAGE];
    h.app.set_page_visible(WEATHER_PAGE, false); h.app.set_page_visible(MUSIC_PAGE, false);
    let expected: Vec<_> = h.app.settings.page_order.into_iter().filter(|page| h.app.module_on(*page)).collect();
    let point = edge(&h); hover(&mut h, Some(point), 1.4);
    assert_eq!(h.app.control_summaries(&Env::default()).iter().map(|summary| summary.page).collect::<Vec<_>>(), expected);
    h.app.set_page_visible(AGENT_PAGE, false); assert!(h.app.is_control_center(), "disabling an underlying module cannot navigate away from main");
    for page in 0..PAGE_COUNT { if page != TODO_PAGE { h.app.set_page_visible(page, false); } }
    h.app.show_page(TODO_PAGE); hover(&mut h, Some(point), 0.8);
    assert!(response(&h, "control-center-tab").rect.is_positive(), "one enabled module still needs a return path");
    assert_eq!(h.app.settings.page_order.len(), 9);
}

#[test]
fn control_center_keyboard_activation_and_back_preserve_editing() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new(); let point = edge(&h); hover(&mut h, Some(point), 1.4);
    let card = response(&h, ("control-module", TODO_PAGE)); h.ctx.memory_mut(|memory| memory.request_focus(card.id));
    key(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    assert_eq!(h.app.page(), TODO_PAGE); assert!(!h.app.is_control_center());
    hover(&mut h, Some(point), 0.8);
    let _back = focus_control_tab(&mut h);
    key(&mut h, egui::Key::Enter, egui::Modifiers::NONE); assert!(h.app.is_control_center());
    key(&mut h, egui::Key::ArrowRight, egui::Modifiers::NONE); assert!(!h.app.is_control_center()); assert_eq!(h.app.page(),MUSIC_PAGE);
    h.app.show_page(TODO_PAGE); h.app.adding = true; h.app.buf = "正在输入".into(); hover(&mut h, Some(point), 1.0);
    key(&mut h, egui::Key::OpenBracket, egui::Modifiers::COMMAND);
    assert!(!h.app.is_control_center()); assert_eq!(h.app.buf, "正在输入");
    h.app.cancel_edit(); key(&mut h, egui::Key::OpenBracket, egui::Modifiers::COMMAND); assert!(h.app.is_control_center());
    h.app.show_page(MUSIC_PAGE); h.app.qq_library_open = true; h.app.show_control_center();
    key(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.app.qq_library_open, "hidden playlist state must not consume the main panel's Escape");
}

#[test]
fn control_center_file_drag_preserves_main_on_cancel_and_detail_on_drop() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new(); h.app.show_page(SHELF_PAGE); h.app.show_control_center();
    let point = edge(&h); hover(&mut h, Some(point), 1.2);
    h.hovered_files = vec![egui::HoveredFile { path: Some("/synthetic/item.txt".into()), mime: String::new() }]; draw(&mut h, vec![]);
    assert!(!h.app.is_control_center()); assert_eq!(h.app.page(), SHELF_PAGE);
    h.hovered_files.clear(); draw(&mut h, vec![]); assert!(h.app.is_control_center());
    h.hovered_files = vec![egui::HoveredFile { path: Some("/synthetic/item.txt".into()), mime: String::new() }]; draw(&mut h, vec![]);
    let path = crate::model::config_dir().join("drop.txt"); std::fs::write(&path, "synthetic").unwrap();
    h.hovered_files.clear(); h.dropped_files = vec![egui::DroppedFile { path: Some(path.clone()), ..Default::default() }]; draw(&mut h, vec![]);
    assert!(!h.app.is_control_center()); assert_eq!(h.app.page(), SHELF_PAGE); assert!(h.app.shelf.items.contains(&path));
}

#[test]
fn control_center_notification_action_keeps_precise_agent_mail_and_desktop_targets() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for target in 0..3 {
        let mut h = H::new(); fixture(&mut h); h.app.settings.agent_completion = true;
        let (source, page) = match target {
            0 => (AlertSource::Agent { session_id: "synthetic-task".into(), turn_key: 1 }, AGENT_PAGE),
            1 => (AlertSource::Mail { account: "SYNTHETIC@qq.com".into(), uid: 42 }, MAIL_PAGE),
            _ => { h.app.settings.read_notifications = true; h.app.settings.macos_dock_badges = true;
                h.app.notification_status = crate::notifications::Status::Watching { apps: 1 };
                (AlertSource::Desktop { bundle_id: "com.tencent.qq".into(), name: "QQ".into(), activation: "synthetic-notice".into(), key: "synthetic-key".into() }, NOTIFICATIONS_PAGE) }
        };
        h.app.enqueue_alert(Alert { source, title: "合成提醒".into(), body: "示例".into(), page, at: h.t, duration: 20.0 }, false);
        h.app.alert("其他待办", "仍须保留", TODO_PAGE, false);
        let point = edge(&h); hover(&mut h, Some(point), 1.4);
        assert!(h.app.is_control_center()); let action = response(&h, "notice-summary"); click(&mut h, action.rect.center());
        if target == 0 { assert!(h.app.is_control_center()); assert!(h.cmds.iter().any(|cmd|matches!(cmd,Cmd::OpenAgent{agent:AgentKind::Codex,session_id,..}if session_id=="synthetic-task"))); }
        if target == 1 { assert!(!h.app.is_control_center()); assert_eq!(h.app.mail.open_uid, Some(("synthetic@qq.com".into(), 42))); }
        if target == 2 { assert!(h.cmds.iter().any(|cmd| matches!(cmd, Cmd::OpenNotification { bundle_id, activation } if bundle_id == "com.tencent.qq" && activation == "synthetic-notice"))); }
        assert!(h.app.history.iter().any(|alert| alert.title == "其他待办"));
    }
}

#[test]
fn control_center_agent_summaries_identify_named_unnamed_and_private_conversations() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new(); h.app.settings.notification_preview = true;
    h.app.agent_snapshot.sessions = vec![
        Session { id: "codex:11111111-1111-1111-1111-111111111111".into(), title: "修复 QQ 通知".into(), project: "Window".into(), agent: AgentKind::Codex, status: Status::Working, turn_key: 1, ..Default::default() },
        Session { id: "codex:22222222-2222-2222-2222-222222222222".into(), title: "接入微信预览".into(), project: "Window".into(), agent: AgentKind::Codex, status: Status::Working, turn_key: 2, ..Default::default() },
    ];
    let env = Env::default();
    h.app.agent_tasks.update(&h.app.agent_snapshot, 0.0, 0);
    let summary = || h.app.control_summaries(&env).into_iter().find(|summary| summary.page == AGENT_PAGE).unwrap();
    assert_eq!(summary().detail, "修复 QQ 通知");
    assert_eq!(summary().more, ["接入微信预览 · 执行中"]);

    h.app.agent_snapshot.sessions[0].title = "QQ 通知接入".into();
    h.app.agent_tasks.update(&h.app.agent_snapshot, 1.0, 0);
    let renamed = h.app.control_summaries(&env).into_iter().find(|summary| summary.page == AGENT_PAGE).unwrap();
    assert_eq!(renamed.detail, "QQ 通知接入");
    for session in &mut h.app.agent_snapshot.sessions { session.title.clear(); }
    h.app.agent_tasks.update(&h.app.agent_snapshot, 2.0, 0);
    let unnamed = h.app.control_summaries(&env).into_iter().find(|summary| summary.page == AGENT_PAGE).unwrap();
    assert_eq!(unnamed.detail, "对话 11111111 · 名称待同步");
    assert_eq!(unnamed.more, ["对话 22222222 · 名称待同步 · 执行中"]);
    assert!(!format!("{unnamed:?}").contains("Window"));

    h.app.agent_snapshot.sessions[0].title = "隐藏的真实对话名称".into();
    h.app.agent_tasks.update(&h.app.agent_snapshot, 3.0, 0);
    h.app.settings.notification_preview = false;
    let private = h.app.control_summaries(&env).into_iter().find(|summary| summary.page == AGENT_PAGE).unwrap();
    assert_eq!(private.detail, "对话 11111111");
    assert_eq!(private.more, ["对话 22222222 · 执行中"]);
    assert!(!format!("{private:?}").contains("隐藏的真实对话名称"));
}

#[test]
fn control_center_featured_agent_uses_real_task_states_and_keeps_titles_private() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] {
        let mut h = H::new(); fixture(&mut h); h.app.settings.dock = dock;
        h.app.agent_snapshot.sessions = [
            ("codex:11111111", "审批测试任务", Status::Approval), ("codex:22222222", "等待测试任务", Status::Waiting),
            ("codex:33333333", "执行测试任务", Status::Working), ("codex:44444444", "思考测试任务", Status::Thinking),
            ("codex:55555555", "不能显示已启动", Status::Running), ("codex:66666666", "不能显示已完成", Status::Completed),
            ("codex:77777777", "不能显示未知", Status::Unknown),
        ].into_iter().enumerate().map(|(index, (id, title, status))| Session {
            id: id.into(), title: title.into(), status, turn_key: index as u64 + 1, ..Default::default()
        }).collect();
        h.app.agent_snapshot.sessions.push(Session { id: "process-only".into(), title: "不能显示进程".into(), status: Status::Working, process_only: true, ..Default::default() });
        let point = edge(&h); hover(&mut h, Some(point), 1.4);
        let card = response(&h, ("control-module", AGENT_PAGE)).rect;
        let summary = h.app.control_summaries(&Env::default()).into_iter().find(|summary| summary.page == AGENT_PAGE).unwrap();
        assert!(summary.active);
        let agent = summary.agent.unwrap();
        assert_eq!((agent.working, agent.waiting, agent.approval), (2, 1, 1));
        assert_eq!(agent.tasks.iter().map(|task| task.status).collect::<Vec<_>>(), [Status::Approval, Status::Waiting, Status::Working, Status::Thinking]);
        let visible = |h: &H| h.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if card.expand(0.1).contains_rect(text.visual_bounding_rect()) => Some(text.galley.text().to_owned()), _ => None,
        }).collect::<Vec<_>>().join("\n");
        let text = visible(&h);
        for expected in ["2 执行 · 1 等待 · 1 审批", "审批测试任务", "等待测试任务", "需要审批", "等待操作", "另有 2 个任务"] { assert!(text.contains(expected), "{dock:?}: missing {expected}: {text}"); }
        assert!(!text.contains("不能显示"));
        for locked in [false, true] {
            h.app.settings.notification_preview = locked;
            h.screen_locked = locked;
            draw(&mut h, vec![]);
            let private = h.app.control_summaries(&Env::default()).into_iter().find(|summary| summary.page == AGENT_PAGE).unwrap();
            let tasks = &private.agent.as_ref().unwrap().tasks;
            if locked { assert!(tasks.is_empty(), "locking suspends and clears the current task projection"); }
            else { assert_eq!(tasks[0].title, "对话 11111111"); assert_eq!(tasks[1].title, "对话 22222222"); }
            let text = visible(&h);
            for title in ["审批测试任务", "等待测试任务", "执行测试任务", "思考测试任务"] { assert!(!text.contains(title)); }
            if !locked {
                assert_eq!(response(&h, ("control-module", AGENT_PAGE)).rect, card);
                for label in ["对话 11111111", "对话 22222222"] { assert!(text.contains(label), "{dock:?}: missing private identity {label}: {text}"); }
            }
        }
        h.screen_locked = false;
        hover(&mut h, None, 0.1); hover(&mut h, Some(point), 1.4);
        h.app.agent_snapshot.sessions.retain(|session| session.status == Status::Approval);
        draw(&mut h, vec![]);
        let summary = h.app.control_summaries(&Env::default()).into_iter().find(|summary| summary.page == AGENT_PAGE).unwrap();
        assert!(summary.active, "approval is real activity even without an executing task");
        assert_eq!(response(&h, ("control-module", AGENT_PAGE)).rect, card);
        h.app.agent_snapshot.sessions.clear(); draw(&mut h, vec![]);
        assert_eq!(response(&h, ("control-module", AGENT_PAGE)).rect, card, "task transitions cannot move the featured area");
        h.app.set_page_visible(AGENT_PAGE, false); hover(&mut h, Some(point), 0.6);
        let pages: Vec<_> = h.app.control_summaries(&Env::default()).iter().map(|summary| summary.page).collect();
        let origin = h.app.content_origin(Rect::from_min_size(Pos2::ZERO, h.app.canvas_size()));
        let bounds = h.app.content_rect().translate(origin.to_vec2());
        for (page, expected) in pages.iter().zip(crate::control_center::overview_rects(&pages, dock.is_side(), bounds.size())) {
            let actual = response(&h, ("control-module", *page)).rect;
            assert!((actual.min - (expected.min + bounds.min.to_vec2())).length() < 0.05 && (actual.max - (expected.max + bounds.min.to_vec2())).length() < 0.05,
                "{dock:?}/{page}: actual {actual:?}, expected {:?}", expected.translate(bounds.min.to_vec2()));
        }
        h.app.settings.visible_pages = [false; PAGE_COUNT];
        draw(&mut h, vec![]);
        assert!(h.app.control_summaries(&Env::default()).is_empty());
        assert!(h.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "没有启用的模块")));
    }
}

#[test]
fn control_center_summaries_respect_privacy_and_never_infer_agent_activity() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = H::new(); fixture(&mut h);
    h.app.mail.recent[0].subject = "登录验证码 651234".into(); h.app.mail.recent[0].code = Some("651234".into());
    h.app.agent_snapshot.sessions[0].title = "检查验证码 651234".into();
    h.app.clipboard.lock().items[0].text = "sk-sensitive-secret-651234".into();
    h.app.agent_snapshot.sessions = vec![Session { title: "Source Loaded".into(), status: Status::Running, ..Default::default() },
        Session { status: Status::Unknown, ..Default::default() }, Session { process_only: true, status: Status::Working, title: "Process only".into(), ..Default::default() }];
    let env = Env { today: "2026-11-15".into(), ..Default::default() };
    let summaries = h.app.control_summaries(&env);
    let agent = summaries.iter().find(|summary| summary.page == AGENT_PAGE).unwrap();
    assert!(!agent.active); assert_eq!(agent.value, "暂无执行中任务");
    let all = format!("{summaries:?}"); assert!(!all.contains("651234") && !all.contains("sk-sensitive"));
    h.app.settings.notification_preview = false;
    let summaries = format!("{:?}", h.app.control_summaries(&env));
    for private in ["漫步云端", "轻屿电台", "下周的行程", "设计素材", "周末清单", "Source Loaded"] { assert!(!summaries.contains(private)); }
    let point = edge(&h); hover(&mut h, Some(point), 1.3);
    assert!(!h.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("651234") || text.galley.text().contains("sk-sensitive"))));
}

#[test]
fn control_center_every_module_fits_without_scrolling_and_stays_still_at_fractional_scales() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] { for scale in [1.0, 1.05, 1.25, 1.4, 2.0] { for large in [false,true] {
        let mut h = H::new(); fixture(&mut h); h.app.settings.dock = dock; h.ctx.set_pixels_per_point(scale);
        if large {h.app.settings.side_panel_width=520;h.app.settings.side_panel_height=900;}
        let point = edge(&h); hover(&mut h, Some(point), 1.4);
        let origin=h.app.content_origin(Rect::from_min_size(Pos2::ZERO,h.app.canvas_size()));
        let bounds=h.app.content_rect().translate(origin.to_vec2());
        let initial:Vec<_>=(0..PAGE_COUNT).map(|page|response(&h,("control-module",page)).rect).collect();
        for (page,rect) in initial.iter().enumerate(){
            assert!(rect.width()>90.0&&rect.height()>40.0,"{dock:?}/{scale}/{page}: the complete card must be visible");
            assert!(bounds.expand(0.05).contains_rect(*rect),"{dock:?}/{scale}/{page}: all modules must fit in the content viewport");
            for other in initial.iter().skip(page+1){assert!(!rect.shrink(0.01).intersect(other.shrink(0.01)).is_positive(),"module interiors cannot overlap");}
        }
        if dock==Dock::Top {
            let widths:Vec<_>=initial.iter().map(|rect|rect.width()).collect();
            let heights:Vec<_>=initial.iter().map(|rect|rect.height()).collect();
            assert!(widths.iter().copied().fold(0.0,f32::max)>widths.iter().copied().fold(f32::INFINITY,f32::min)*1.9,"the main overview gives one module a larger reading area");
            assert!(heights.iter().copied().fold(0.0,f32::max)>heights.iter().copied().fold(f32::INFINITY,f32::min)*1.9,"the mosaic varies both dimensions");
            let coverage=initial.iter().copied().reduce(Rect::union).unwrap();
            assert!((coverage.left()-bounds.left()).abs()<0.1&&(coverage.right()-bounds.right()).abs()<0.1,"the overview uses the full content width");
            let used_area:f32=initial.iter().map(|rect|rect.width()*rect.height()).sum();
            assert!((used_area-bounds.width()*bounds.height()).abs()<0.5,"the varied tiles still cover the overview without gutters");
        }
        h.cursor=Some(initial[NOTIFICATIONS_PAGE].center());
        let last=response(&h,("control-module",NOTIFICATIONS_PAGE)).id;
        h.ctx.memory_mut(|memory|memory.request_focus(last));
        for frame in 0..45{
            let mut events=vec![egui::Event::PointerMoved(h.cursor.unwrap())];
            if frame%10==0{events.push(egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:Vec2::new(0.0,if frame%20==0{-45.0}else{45.0}),modifiers:Default::default()});}
            draw(&mut h,events);
            for (page,initial) in initial.iter().enumerate(){
                let current=response(&h,("control-module",page)).rect;
                assert!((current.min-initial.min).length()<0.05&&(current.max-initial.max).length()<0.05,"{dock:?}/{scale}/{page}: focus, hover and wheel must not move or resize overview cards");
            }
            assert!(h.app.is_control_center());
        }
    } } }
}

#[test]
fn control_center_geometry_matches_hit_and_blur_at_sizes_scales_and_reduced_motion() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for dock in [Dock::Top, Dock::Left, Dock::Right] { for scale in [1.0, 1.25, 2.0] { for large in [false, true] { for reduced in [false, true] {
        let mut h = H::new(); fixture(&mut h); h.app.settings.dock = dock; h.app.settings.skin = Skin::GlassDark; h.ctx.set_pixels_per_point(scale);
        if large { h.app.settings.side_panel_width = 520; h.app.settings.side_panel_height = 900; }
        #[cfg(target_os = "macos")]
        h.ctx.data_mut(|data| data.insert_temp(egui::Id::new("macos-accessibility-preferences"), (std::time::Instant::now(), if reduced { 2u32 } else { 0u32 })));
        h.app.reduced_motion = reduced;
        let point = edge(&h); hover(&mut h, Some(point), 1.3);
        let (_, frame) = draw(&mut h, vec![]); let canvas = Rect::from_min_size(Pos2::ZERO, h.app.canvas_size());
        assert!(canvas.contains_rect(h.app.displayed_island_rect(canvas)), "{dock:?} {scale} body must fit its actual canvas");
        assert!(frame.region.contains_rect(frame.hit.intersect(canvas).shrink(1.0)));
        if dock.is_side() {
            let blur = frame.glass.as_ref().unwrap().first().unwrap().outline();
            assert_eq!(frame.hit_outline.as_ref().unwrap(), &blur, "native blur and input must use the same polygon");
        }
        let panel = h.app.displayed_island_rect(canvas);
        for page in 0..PAGE_COUNT {
            let rect = response(&h, ("control-module", page)).rect;
            if rect.is_positive() { assert!(panel.expand(0.5).contains_rect(rect), "{dock:?}, {scale}, page {page} cannot spill horizontally"); }
        }
    } } } }
}

#[test]
#[ignore = "Exports actual egui renders; all seeded content is synthetic"]
fn control_center_gallery() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dest = std::path::PathBuf::from(std::env::var_os("QINGYU_CONTROL_PREVIEW_DIR").expect("preview directory")); std::fs::create_dir_all(&dest).unwrap();
    for (name, dock, width, height, notice, private) in [
        ("top", Dock::Top, 292, 532, false, false), ("top-notice", Dock::Top, 292, 532, true, false),
        ("left", Dock::Left, 292, 532, false, false), ("right", Dock::Right, 292, 532, false, false),
        ("large", Dock::Right, 520, 900, false, false), ("private", Dock::Top, 292, 532, true, true),
        ("detail-weather-top", Dock::Top, 292, 532, false, false), ("detail-weather-side", Dock::Right, 292, 532, false, false),
        ("detail-music", Dock::Top, 292, 532, false, false), ("detail-agent", Dock::Top, 292, 532, false, false),
        ("detail-mail", Dock::Top, 292, 532, false, false), ("detail-qq-library", Dock::Top, 292, 532, false, false),
        ("detail-files",Dock::Top,292,532,false,false),("detail-clips",Dock::Top,292,532,false,false),("detail-library",Dock::Top,292,532,false,false),
        ("detail-weather-large",Dock::Right,520,900,false,false),("detail-music-large",Dock::Right,520,900,false,false),("detail-files-large",Dock::Right,520,900,false,false),
        ("detail-clips-large",Dock::Right,520,900,false,false),("detail-library-large",Dock::Right,520,900,false,false),
        ("detail-notifications",Dock::Top,292,532,false,false),("detail-stats",Dock::Top,292,532,false,false),
        ("empty-weather",Dock::Right,520,900,false,false),("empty-files",Dock::Top,292,532,false,false),
        ("empty-main",Dock::Top,292,532,false,false),("empty-main-side",Dock::Right,520,900,false,false),
    ] {
        if let Ok(cases)=std::env::var("QINGYU_CONTROL_REVIEW_CASES"){if !cases.split(',').any(|case|case==name){continue;}}
        let mut h = H::new(); fixture(&mut h); h.app.settings.dock = dock; h.app.settings.side_panel_width = width; h.app.settings.side_panel_height = height;
        if notice { h.app.enqueue_alert(Alert { source: AlertSource::Agent { session_id: "synthetic-task".into(), turn_key: 1 }, title: "优化音乐播放体验".into(), body: "Codex 已完成".into(), page: AGENT_PAGE, at: h.t, duration: 20.0 }, false); }
        if name.starts_with("detail-weather") { h.app.show_page(WEATHER_PAGE); }
        if name.starts_with("detail-music") { h.app.show_page(MUSIC_PAGE); }
        if name.starts_with("detail-files"){h.app.show_page(SHELF_PAGE);}
        if name.starts_with("detail-clips"){h.app.show_page(CLIPBOARD_PAGE);}
        if name.starts_with("detail-library"){h.app.library_view.legacy_tasks=false;h.app.show_page(TODO_PAGE);}
        if name == "detail-agent" { h.app.show_page(AGENT_PAGE); }
        if name == "detail-mail" { h.app.show_page(MAIL_PAGE); h.app.mail.open_uid = Some(("synthetic@qq.com".into(), 42)); }
        if name == "detail-qq-library" { h.app.show_page(MUSIC_PAGE); h.app.qq_library_open = true; }
        if name=="detail-notifications"{h.app.show_page(NOTIFICATIONS_PAGE);}
        if name=="detail-stats"{h.app.show_page(MAC_STATS_PAGE);}
        if name.starts_with("empty-main"){h.media=Default::default();h.app.shelf.items.clear();h.app.clipboard.lock().items.clear();h.app.library.entries.clear();h.app.history.clear();h.app.mail.recent.clear();h.app.mail.accounts.clear();h.app.mac_snapshot=Default::default();h.app.agent_snapshot=Default::default();}
        if name=="empty-weather"{h.app.weather.snapshot.as_mut().unwrap().days.clear();h.app.show_page(WEATHER_PAGE);}
        if name=="empty-files"{h.app.shelf.items.clear();h.app.show_page(SHELF_PAGE);}
        h.app.settings.notification_preview = !private; h.ctx.set_pixels_per_point(2.0);
        let point = edge(&h); h.cursor = Some(point);
        let size = h.app.canvas_size(); let canvas = if dock.is_side() { size } else { Vec2::new(CANVAS_W, size.y) };
        let mut raster = crate::preview_raster::Raster::new((canvas.x * 2.0) as usize, (canvas.y * 2.0) as usize);
        for index in 0..95 {
            if index==1&&name.starts_with("detail-music"){h.app.set_lyrics("漫步云端","轻屿电台",224.0,crate::lyrics::parse_lrc("[00:00]把日子过成一首轻柔的歌\n[00:20]风带来了窗外的消息\n[00:40]一起漫步在云端\n[01:00]慢慢走过平凡的时光\n[01:30]留下一点温柔和光"));}
            if index >= 70 { h.cursor = None; }
            let (output, _) = draw(&mut h, if index >= 70 { vec![egui::Event::PointerGone] } else { vec![egui::Event::PointerMoved(point)] }); raster.update_textures(&output.textures_delta);
            if index == 94 {
                raster.buf.fill([0.15, 0.21, 0.25, 1.0]); raster.draw(&h.ctx.tessellate(output.shapes, output.pixels_per_point), output.pixels_per_point);
                let mut rgba = Vec::new(); raster.to_rgba8(&mut rgba);
                let image = image::RgbaImage::from_raw((canvas.x * 2.0) as u32, (canvas.y * 2.0) as u32, rgba).unwrap();
                let win=Rect::from_min_size(Pos2::ZERO,size);
                let body=h.app.displayed_island_rect(win).union(h.app.resident_rect(win)).expand(12.0).intersect(Rect::from_min_size(Pos2::ZERO,canvas));
                image::imageops::crop_imm(&image, (body.left() * 2.0) as u32, (body.top() * 2.0) as u32, (body.width() * 2.0) as u32, (body.height() * 2.0) as u32).to_image().save(dest.join(format!("{name}.png"))).unwrap();
            }
            raster.free_textures(&output.textures_delta);
        }
    }
    std::fs::write(dest.join("README.txt"), "Actual egui renders at 2×. All track, task, file, clipboard, mail, weather and Agent data are synthetic. No live user content is included.\n").unwrap();
}
