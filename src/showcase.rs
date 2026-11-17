//! Native README capture using the production App, not a recreated UI.
//! Everything lives in a newly created temporary directory. No shell workers,
//! clipboard watcher, account connection, agent discovery or external commands run.
//! Pointer events are removed before egui sees them: only scene keys are accepted.
use crate::{app::{self, App, Env, Options}, agent_monitor::{AgentKind, Integration, Session, Status},
    extras::{Alert, AlertSource}, library::{self, EntryState}, model::{self, Dock, Skin}, smtc::NowPlaying};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use std::{path::PathBuf, sync::Arc, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

const SCENES: &[&str] = &[
    "overview", "music", "playlists", "shelf", "clipboard", "library", "library-detail",
    "todos", "mail", "mail-reader", "weather", "sunlight", "agents", "mac-stats",
    "notifications", "notice", "collapsed", "top-overview", "settings", "modules", "pets", "completion",
    "side-overview", "style-classic", "style-cream", "style-fluid", "style-glass-light", "style-glass-dark",
];

struct Capture {
    app: App,
    media: NowPlaying,
    started: Instant,
    scene: usize,
    root: PathBuf,
    fixed_unix: u64,
    wallpaper: egui::TextureHandle,
}

/// Each invocation owns its own fixtures; the caller's data environment is ignored.
pub fn run() -> eframe::Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    let root = std::env::temp_dir().join(format!("qingyu-readme-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&root).expect("create isolated capture data");
    std::env::set_var("QINGYU_DATA_DIR", &root);
    let scene = std::env::args().nth(1).and_then(|name| SCENES.iter().position(|item| *item == name)).unwrap_or(0);
    let viewport = egui::ViewportBuilder::default().with_title("轻屿 README · overview")
        .with_icon(crate::theme::app_icon()).with_inner_size([1000.0, 820.0])
        .with_resizable(false).with_position([100.0, 50.0]);
    let options = eframe::NativeOptions {viewport, ..Default::default()};
    eframe::run_native("Qingyu README", options, Box::new(move |cc| {
        let mut capture = Capture::new(cc, root, scene);
        capture.select(cc.egui_ctx.clone(), scene);
        Ok(Box::new(capture))
    }))
}

impl Capture {
    fn new(cc: &eframe::CreationContext<'_>, root: PathBuf, scene: usize) -> Self {
        let fixed_unix = chrono::DateTime::parse_from_rfc3339("2026-11-17T09:41:00+08:00").unwrap().timestamp() as u64;
        let position = crate::weather::Position {lat:30.27, lon:120.15, label:"杭州".into(), region:"浙江 · 中国".into()};
        let mut settings = model::Settings::default();
        settings.skin=Skin::Fluid; settings.dock=Dock::Top;settings.island_shape=model::IslandShape::Pill;
        settings.side_panel_width=380; settings.side_panel_height=600;
        settings.notification_preview=true; settings.notifications=true;
        settings.read_notifications=true; settings.macos_dock_badges=true;
        settings.wechat_notifications=true; settings.wecom_notifications=true;
        settings.weather_city=position.label.clone(); settings.weather_position=Some(position.clone());
        settings.weather_saved=vec![position.clone()];
        settings.island_lyrics=false; settings.resident_length=390;
        settings.save();
        let mut weather = crate::weather::Snapshot {
            place:"杭州".into(), position:Some(position.clone()), temp:19.0, feels:Some(18.0),
            humidity:Some(62), wind:Some(7.2), code:2, day:true, high:Some(23.0), low:Some(14.0),
            sunrise:Some("06:26".into()), sunset:Some("17:02".into()), at:chrono::Utc::now().timestamp(),
            ..Default::default()
        };
        weather.days=(0..7).map(|i| crate::weather::Day {
            label:["今天","明天","周四","周五","周六","周日","周一"][i].into(),
            date:format!("2026-11-{:02}",17+i), high:[23.,22.,21.,19.,20.,22.,24.][i],
            low:[14.,15.,14.,12.,13.,14.,16.][i], code:[2,0,3,61,2,0,0][i], rain:[10,0,20,75,15,0,0][i],
        }).collect();
        weather.sun_times=Some(crate::weather::SunTimes {
            day_start:fixed_unix as i64-9*3600-41*60, day_end:fixed_unix as i64+14*3600+19*60,
            sunrise:fixed_unix as i64-3*3600-15*60, sunset:fixed_unix as i64+7*3600+21*60,
            timezone:"Asia/Shanghai".into(),
        });
        std::fs::write(root.join("weather-cities.json"),serde_json::to_vec(&vec![weather]).unwrap()).unwrap();
        let mut app=App::new(&cc.egui_ctx,Options {fetch_lyrics:false,monitor_agents:false});
        app.weather.switch("杭州",Some(position),chrono::Utc::now().timestamp());
        app.set_side_panel_size(Vec2::new(380.0,600.0));
        app.todos.items=["完成轻屿的开源使用指南","整理本周设计灵感","下午散步，买一束花","检查项目的公开截图"].iter().enumerate().map(|(i,text)|model::TodoItem {
            text:(*text).into(), due:(i==0).then(||"2026-11-18".into()),
            done_on:(i==3).then(||"2026-11-17".into()), ..Default::default()
        }).collect();
        let files=root.join("files");std::fs::create_dir(&files).unwrap();
        for (name,bytes) in [("产品手册.md","# 轻屿\n把重要的事放在眼前。\n".as_bytes()),
            ("配色灵感.png",include_bytes!("../assets/qingyu/preview-wallpaper.png").as_slice()),
            ("品牌标志.png",include_bytes!("../assets/qingyu/brand-logo.png").as_slice()),
            ("本周清单.txt","把想法收好，把眼前的事做好。\n".as_bytes())] {
            let path=files.join(name);std::fs::write(&path,bytes).unwrap();app.shelf.items.push(path);
        }
        std::fs::create_dir(files.join("项目素材")).unwrap();app.shelf.items.push(files.join("项目素材"));
        {
            let mut history=app.clipboard.lock();
            for (text,at) in [("把重要的事放在眼前，把其余的交给轻屿。","09:21"),("https://developer.apple.com/design/","09:28"),("cargo check --locked --all-targets --features record","09:35")] {history.push(text,at);}
            history.items[1].pinned=true;
            history.items.insert(0,crate::clipboard::Clip {text:"配色灵感.png".into(),at:"09:38".into(),pinned:false,media:Some(crate::clipboard::Media {
                path:files.join("配色灵感.png"),name:"配色灵感.png".into(),kind:crate::clipboard::MediaKind::Image,
                bytes:std::fs::metadata(files.join("配色灵感.png")).unwrap().len(),
            })});
        }
        for (kind,text) in [(library::Kind::Idea,"给工作留一处安静的角落"),(library::Kind::Link,"https://developer.apple.com/design/"),
            (library::Kind::Action,"把今天的阅读笔记整理成一页"),(library::Kind::Idea,"周末去海边，记录傍晚的光"),(library::Kind::Link,"https://www.rust-lang.org/")] {
            let id=app.library.capture(kind,text).unwrap();
            if kind==library::Kind::Idea {app.library.set_state(&id,EntryState {pinned:true,..Default::default()}).unwrap();}
        }
        // Add rich metadata through the same persistent record format used by Hermes.
        let records=app.library.root().join("records");
        for entry in std::fs::read_dir(records).unwrap().flatten() {
            let mut record:serde_json::Value=serde_json::from_slice(&std::fs::read(entry.path()).unwrap()).unwrap();
            let link=record["kind"]=="link";
            if link {record["title"]=if record["url"].as_str().unwrap_or_default().contains("apple.com"){"Apple 设计资源与灵感".into()}else{"Rust：可靠而高效的工具".into()};}
            record["summary"]=if link {"留给下次创作的参考，保留原始链接，随时回来继续阅读。".into()}else{"给想法一个位置，留住原话，再把它变成可以推进的小事。".into()};
            record["tags"]=serde_json::json!(if link {vec!["设计","待读"]}else{vec!["灵感","日常"]});
            record["topic"]="日常创作".into();record["source"]="手动记录 · 示例".into();
            std::fs::write(entry.path(),serde_json::to_vec_pretty(&record).unwrap()).unwrap();
        }
        app.library.refresh();
        app.mail.accounts=vec![crate::mail::Account {email:"hello@example.com".into(),provider:crate::mail::Provider::Qq,enabled:true,custom:None}];
        app.mail.status.insert("hello@example.com".into(),crate::mail::Status::Ready);
        app.mail.recent=[("轻屿团队","欢迎来到轻屿","让常用工具回到手边。音乐、文件、想法和消息，都可以在这个小岛里找到。\n\n这是一封用于公开截图的示例邮件，没有接入真实邮箱。"),
            ("设计小组","本周设计评审 · 资料已更新","配色与图标已经整理完毕，请在评审前阅读新的设计说明。"),
            ("阅读清单","给周末留一点好内容","三篇设计文章、一本正在读的书，以及一个值得试试的点子。"),
            ("GitHub","Your verification code: 482913","Your example verification code is 482913. This is demonstration content.")]
            .iter().enumerate().map(|(i,(from,subject,summary))|crate::mail::Item {
                received_unix:None,account:"hello@example.com".into(),from:(*from).into(),subject:(*subject).into(),
                time:["09:40","09:32","09:18","09:08"][i].into(),code:(i==3).then(||"482913".into()),
                summary:(*summary).into(),html:String::new().into(),links:vec![],uid:100+i as u32,
            }).collect();
        app.mail.manage.identities.insert("hello@example.com".into(),app.mail.accounts[0].clone());
        app.mail.manage.data.insert(("hello@example.com".into(),crate::mail_manage::Folder::Inbox),crate::mail_manage::Snapshot {
            validity:1,entries:app.mail.recent.iter().map(|item|(item.uid,item.uid==100)).collect(),items:vec![],
        });
        app.mail.manage.next_refresh=f64::INFINITY;
        let sessions=[(AgentKind::Codex,Status::Working,"完善开源文档","正在整理 README 与功能截图",244),
            (AgentKind::Claude,Status::Approval,"检查交互细节","等待确认下一步操作",87),
            (AgentKind::Hermes,Status::Working,"整理阅读收藏","正在给收藏归类并补充标签",122)];
        app.agent_snapshot.sessions=sessions.iter().enumerate().map(|(i,(agent,status,title,detail,elapsed))|Session {
            id:format!("{}:readme-example",["codex","claude","hermes"][i]),agent:*agent,status:*status,title:(*title).into(),
            detail:(*detail).into(),project:"~/Projects/Qingyu".into(),updated_at:fixed_unix,started_at:fixed_unix-elapsed,
            turn_key:i as u64+1, ..Default::default()
        }).collect();
        app.agent_snapshot.integrations_installed=true;
        app.agent_snapshot.integrations=[AgentKind::Codex,AgentKind::Claude,AgentKind::Hermes].iter().map(|agent|Integration {
            agent:*agent,detected:true,installed:true,ready:true,message:"只读截图中的示例状态".into(),
        }).collect();
        app.notification_status=crate::notifications::Status::Watching {apps:3};
        app.history=[("微信","设计小组：新的配色参考已发到群里。","com.tencent.xinWeChat"),
            ("QQ","小伙伴：周末一起去散步吧。","com.tencent.qq"),
            ("企业微信","项目组：资料已更新，方便时看一下。","com.tencent.WeWorkMac")].iter().enumerate().map(|(i,(name,body,id))|Alert {
                title:(*name).into(),body:(*body).into(),source:AlertSource::Desktop {bundle_id:(*id).into(),name:(*name).into(),activation:String::new(),key:format!("readme-{i}")},
                page:app::NOTIFICATIONS_PAGE,at:20.0,duration:15.0,
            }).collect();
        let mut stats=crate::mac_stats::Snapshot {supported:true,paused:false,cpu_usage:Some(18.0),cpu_name:"Apple Silicon".into(),logical_cores:10,
            memory_total:32*1024u64.pow(3),memory_used:18*1024u64.pow(3),memory_compressed:2*1024u64.pow(3),memory_valid:true,
            swap_used:0,swap_valid:true,disk_total:1024u64.pow(4),disk_available:622*1024u64.pow(3),disk_valid:true,
            disk_read_bytes_per_sec:Some(3_200_000.),disk_write_bytes_per_sec:Some(940_000.),
            gpus:vec![crate::mac_stats::Gpu {name:"Apple GPU".into(),usage:Some(12.0)}],
            network:Some(crate::mac_stats::Network {interface:"Wi-Fi · en0".into(),received_bytes_per_sec:Some(2_420_000.),sent_bytes_per_sec:Some(168_000.),..Default::default()}),
            network_status:crate::mac_stats::NetworkStatus::Online,..Default::default()};
        let sampled=SystemTime::now();
        for i in 0..31 {stats.sampled_at=Some(sampled-Duration::from_secs(60-i*2));stats.cpu_usage=Some(18.0+(i as f64*0.7).sin()*7.0);app.stats_history.observe(&stats);}
        app.mac_snapshot=stats;
        let image=image::load_from_memory(include_bytes!("../assets/qingyu/music-placeholder.png")).unwrap().to_rgba8();
        let artwork=Arc::new(egui::ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize],image.as_raw()));
        let media=NowPlaying {title:"沿海慢行".into(),artist:"轻屿电台".into(),album:"把片刻留给自己".into(),source:crate::qq_music::SOURCE.into(),
            playing:true,position:86.0,duration:228.0,artwork:Some(artwork),volume:Some((48,false)),playback_type:crate::smtc::MediaKind::Music,..Default::default()};
        app.qq_library=crate::qq_music::Library {logged_in:true,session:crate::qq_music::SessionStatus::Connected,player_ready:true,
            playlists:["我喜欢的音乐","写代码时的背景音","周末慢下来","沿途的风景"].iter().enumerate().map(|(i,title)|crate::qq_music::Playlist {id:format!("demo-{i}"),title:(*title).into()}).collect(),..Default::default()};
        app.qq_lyrics=Some(crate::qq_music::NativeLyrics {title:media.title.clone(),current:Some(2),
            lines:["沿着海岸，慢慢向前","把忙碌交给风","把片刻留给自己","每一个平凡的今天","都有值得收藏的光"].iter().map(|line|(*line).into()).collect()});
        let image=image::load_from_memory(include_bytes!("../assets/qingyu/preview-wallpaper.png")).unwrap().to_rgba8();
        let wallpaper=cc.egui_ctx.load_texture("readme-desktop",egui::ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize],image.as_raw()),egui::TextureOptions::LINEAR);
        Self {app,media,started:Instant::now(),scene,root,fixed_unix,wallpaper}
    }

    fn select(&mut self,ctx:egui::Context,index:usize) {
        self.scene=index;self.app.settings_open=false;self.app.settings.dock=Dock::Top;self.app.settings.skin=Skin::Fluid;
        self.app.settings.island_shape=model::IslandShape::Pill;
        self.app.set_desktop_placement(app::DesktopPlacement::Floating {content_top:18.0});
        self.app.set_side_panel_size(Vec2::new(380.0,600.0));
        self.app.panel_pinned=true;self.app.open=true;
        self.app.library_view.legacy_tasks=false;self.app.library_view.selected=None;
        self.app.weather.solar_open=false;self.app.qq_library_open=false;self.app.mail.open_uid=None;
        self.app.notice=None;self.app.hud_notice=None;self.app.activity_notice=None;self.app.alerts.clear();
        let name=SCENES[index];
        match name {
            "overview"=>self.app.show_control_center(),
            "side-overview"=>{
                self.app.settings.dock=Dock::Left;self.app.settings.island_shape=model::IslandShape::Notch;
                self.app.set_desktop_placement(app::DesktopPlacement::Legacy);self.app.show_control_center();
            },
            "style-classic"|"style-cream"|"style-fluid"|"style-glass-light"|"style-glass-dark"=>{
                self.app.settings.skin=match name {"style-classic"=>Skin::Classic,"style-cream"=>Skin::Cream,"style-glass-light"=>Skin::GlassLight,"style-glass-dark"=>Skin::GlassDark,_=>Skin::Fluid};
                self.app.show_control_center();
            },
            "music"|"playlists"=>{self.app.show_page(app::MUSIC_PAGE);self.app.qq_library_open=name=="playlists";},
            "shelf"=>self.app.show_page(app::SHELF_PAGE),
            "clipboard"=>self.app.show_page(app::CLIPBOARD_PAGE),
            "library"|"library-detail"|"todos"=>{self.app.show_page(app::LIBRARY_PAGE);self.app.library_view.legacy_tasks=name=="todos";
                if name=="library-detail"{self.app.library_view.selected=self.app.library.entries.iter().find(|entry|entry.kind==library::Kind::Idea).map(|entry|entry.id.clone());}},
            "mail"|"mail-reader"=>{self.app.show_page(app::MAIL_PAGE);if name=="mail-reader"{self.app.mail.open_uid=Some(("hello@example.com".into(),100));}},
            "weather"|"sunlight"=>{self.app.show_page(app::WEATHER_PAGE);self.app.weather.solar_open=name=="sunlight";},
            "agents"=>self.app.show_page(app::AGENT_PAGE),
            "mac-stats"=>self.app.show_page(app::MAC_STATS_PAGE),
            "notifications"=>self.app.show_page(app::NOTIFICATIONS_PAGE),
            "notice"|"collapsed"|"top-overview"|"completion"=>{
                self.app.settings.dock=Dock::Top;self.app.settings.island_shape=model::IslandShape::Pill;
                if matches!(name,"notice"|"completion"){self.app.settings.skin=Skin::Classic;}
                self.app.set_desktop_placement(app::DesktopPlacement::Floating {content_top:18.0});
                self.app.show_control_center();
                if name!="top-overview"{self.app.panel_pinned=false;self.app.suspend_island();}
            },
            "settings"|"modules"|"pets"=>{
                self.app.settings.dock=Dock::Left;self.app.settings.island_shape=model::IslandShape::Notch;
                self.app.set_desktop_placement(app::DesktopPlacement::Legacy);
                self.app.show_control_center();self.app.settings_tab=if name=="modules"{10}else if name=="pets"{13}else{0};
                self.app.open_settings();
            },
            _=>{},
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("轻屿 README · {:02} {name}",index+1)));
        eprintln!("README scene {}/{}: {name}",index+1,SCENES.len());
    }
}

impl eframe::App for Capture {
    fn raw_input_hook(&mut self,_ctx:&egui::Context,input:&mut egui::RawInput) {
        // A read-only capture must never activate install, Keychain, browser or clipboard actions.
        input.events.retain(|event|matches!(event,egui::Event::Key {key:egui::Key::ArrowLeft|egui::Key::ArrowRight|egui::Key::Escape|egui::Key::Home|egui::Key::End|egui::Key::N,..}));
        input.hovered_files.clear();input.dropped_files.clear();
    }
    fn update(&mut self,ctx:&egui::Context,_frame:&mut eframe::Frame) {
        if ctx.input(|input|input.key_pressed(egui::Key::ArrowRight)){self.select(ctx.clone(),(self.scene+1)%SCENES.len());}
        if ctx.input(|input|input.key_pressed(egui::Key::ArrowLeft)){self.select(ctx.clone(),(self.scene+SCENES.len()-1)%SCENES.len());}
        if ctx.input(|input|input.key_pressed(egui::Key::Home)){self.select(ctx.clone(),0);}
        if ctx.input(|input|input.key_pressed(egui::Key::End)){self.select(ctx.clone(),SCENES.iter().position(|name|*name=="completion").unwrap());}
        if ctx.input(|input|input.key_pressed(egui::Key::N)){self.select(ctx.clone(),SCENES.iter().position(|name|*name=="notice").unwrap());}
        if ctx.input(|input|input.key_pressed(egui::Key::Escape)){ctx.send_viewport_cmd(egui::ViewportCommand::Close);}
        ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE,egui::Key::ArrowRight);
            input.consume_key(egui::Modifiers::NONE,egui::Key::ArrowLeft);
            input.consume_key(egui::Modifiers::NONE,egui::Key::Home);
            input.consume_key(egui::Modifiers::NONE,egui::Key::End);
            input.consume_key(egui::Modifiers::NONE,egui::Key::N);
        });
        let name=SCENES[self.scene];let side=self.app.settings.dock.is_side();
        let size=self.app.canvas_size();let origin=if side {Pos2::new(250.0,10.0)}else{Pos2::new(180.0,170.0)};
        let win=Rect::from_min_size(origin,size);
        let env=Env {t:self.started.elapsed().as_secs_f64()+30.0,unix_time:self.fixed_unix,unix_ms:self.fixed_unix*1000,
            clock:"09:41:00".into(),today:"2026-11-17".into(),date:"11月17日 周二".into(),battery:Some(86),network:Some(true),
            cursor:(!matches!(name,"collapsed"|"notice"|"completion")).then_some(origin+Vec2::new(200.0,120.0)),
            media:self.media.clone(),volume:Some((36,false)),screen:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(1000.0,820.0))),..Default::default()};
        if let Some(snapshot)=self.app.weather.snapshot.as_mut(){snapshot.at=chrono::Utc::now().timestamp();}
        if matches!(name,"notice"|"completion") && self.app.notice.is_none(){
            let mut alert=self.app.history.front().unwrap().clone();alert.at=env.t;alert.duration=3600.0;
            if let AlertSource::Desktop {key,..}=&mut alert.source{*key="readme-new-notification".into();}
            if name=="completion" {
                alert.source=AlertSource::AgentEvent {session_id:"codex:readme-example".into(),turn_key:99,kind:crate::agent_activity::NoticeKind::Completed};
                alert.title="完善开源文档".into();alert.body="已完成 · 用时 4 分 04 秒".into();alert.page=app::AGENT_PAGE;
            }
            self.app.enqueue_alert(alert,false);
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::from_rgb(35,43,54))).show(ctx,|ui| {
            ui.painter().image(self.wallpaper.id(),ui.max_rect(),Rect::from_min_max(Pos2::ZERO,Pos2::new(1.0,1.0)),Color32::WHITE);
            let output=self.app.frame(ui,win,&env);
            // Deliberately discard all commands: this window cannot play audio,
            // open apps, request permissions, connect mail or alter user services.
            drop(output);
        });
        self.app.settings_ui(ctx,false);
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

impl Drop for Capture {
    fn drop(&mut self){let _=std::fs::remove_dir_all(&self.root);}
}
