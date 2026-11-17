#![cfg_attr(windows, windows_subsystem = "windows")]
//! 桌面外壳：无边框透明窗口、原生菜单与系统数据，界面交给 `qingyu::app::App`。

use eframe::egui::{self, Color32};
#[cfg(not(target_os="macos"))]
use eframe::egui::Pos2;
use qingyu::app::{self, App, Cmd, Env, Options};
use qingyu::smtc;
use qingyu::poll_clock::PollClock;
use std::time::{Duration, Instant};

mod diagnostics;
mod graphics;
#[cfg(all(target_os="macos", debug_assertions))]
mod dev_startup;

struct Shell {
    app: App,
    t0: Instant,
    #[cfg(windows)]
    frame_at: Instant,
    media: smtc::Shared,
    seek_rx: Option<(&'static str,std::sync::mpsc::Receiver<bool>)>,
    media_control:Option<smtc::ControlWorker>,
    music_volume_rx:Option<(String,std::sync::mpsc::Receiver<bool>)>,
    agent_open_rx:Option<std::sync::mpsc::Receiver<Result<(),String>>>,
    qq_playback_rx:Option<(u64,Instant,std::sync::mpsc::Receiver<bool>)>,
    #[cfg(target_os="macos")]
    qq_music_opened:bool,
    _resource_monitor:Option<qingyu::resources::Monitor>,
    resource_at:PollClock,
    guard_notified:bool,
    media_enabled: smtc::Poller,
    /// 刚点过播放控制或音量时，短时间内按最快频率采样，让按钮状态及时回显。
    media_fast_until: Option<Instant>,
    placed: bool,
    placement_at: PollClock,
    placement_scale: f32,
    /// 上次定位时的目标屏幕、水平位置与 OLED 像素偏移；变化后立即重新定位，不等下一次轮询。
    placement_key: (String, i32, qingyu::model::Dock, bool, (i32, i32)),
    /// OLED 像素偏移当前实际使用的位移；展开、拖拽、隐藏时保持不变。
    #[cfg(any(windows,target_os="macos"))]
    oled_shift: (i32, i32),
    #[cfg(not(target_os="macos"))]
    win_x: f32,
    #[cfg(windows)]
    system_state: qingyu::system::Snapshot,
    #[cfg(windows)]
    system_rx: std::sync::mpsc::Receiver<qingyu::system::Observation>,
    fullscreen: Option<qingyu::display_layout::Bounds>,
    /// 岛体是否暂时隐藏。
    #[cfg(any(windows,target_os="macos"))]
    hidden: bool,
    /// 毛玻璃皮肤的背景亮度采样（仅毛玻璃皮肤运行）。
    #[cfg(windows)]
    backdrop: Option<qingyu::backdrop::Sampler>,
    #[cfg(windows)]
    native: Option<qingyu::native::Window>,
    #[cfg(windows)]
    tray: Option<qingyu::tray::Tray>,
    #[cfg(any(windows,target_os="macos"))]
    autostart: bool,
    /// v0.5.5：邮件收信服务（工作线程），以及上次同步给它的账号/开关快照。
    /// 邮箱模块开启时才存在；关闭后收信与整理线程随之退出。
    mail: Option<qingyu::mail::Service>,
    mail_sig: Option<(bool, u32, Vec<qingyu::mail::Account>)>,
    /// 粘贴板模块开启且记录启用时才监听系统剪贴板。
    clipboard_watch: Option<qingyu::clipboard::Watcher>,
    #[cfg(target_os="macos")] notification_monitor:Option<qingyu::notifications::Monitor>,
    #[cfg(target_os="macos")] mac_window: Option<qingyu::macos::Window>,
    #[cfg(target_os="macos")] mac_system: qingyu::macos::System,
    #[cfg(target_os="macos")] mac_state: qingyu::macos::Snapshot,
    #[cfg(target_os="macos")] mac_placement_mode: qingyu::model::MacosPlacement,
    #[cfg(target_os="macos")] mac_panel_size:(u32,u32),
    #[cfg(target_os="macos")] mac_position_key:(Option<(i32,i32)>,(i32,i32)),
}

const MODULE_OFF:&str="该功能已在「功能编排」中关闭";

impl Shell {
    fn open_qq_music(&mut self,page:qingyu::qq_music::Page){
        if !self.app.module_on(app::MUSIC_PAGE){self.app.say(MODULE_OFF);return;}
        match qingyu::qq_music::open(page) {
            Ok(())=>{
                #[cfg(target_os="macos")] {self.qq_music_opened=true;}
                self.media_fast_until=Some(Instant::now()+Duration::from_secs(3));
                self.media_enabled.set_interval(smtc::POLL_FAST);
            }
            Err(error)=>self.app.say(error),
        }
    }

    /// 按模块开关启停后台服务：关闭的模块不保留任何线程，重新开启时从头对齐配置。
    fn reconcile_modules(&mut self,ctx:&egui::Context){
        #[cfg(target_os="macos")]
        if self.qq_music_opened && !self.app.module_on(app::MUSIC_PAGE) {
            self.app.stop_qq_playback();
            qingyu::qq_music::shutdown();self.qq_music_opened=false;
        }
        let mail_on=self.app.module_on(app::MAIL_PAGE);
        if mail_on!=self.mail.is_some() {self.mail=mail_on.then(qingyu::mail::Service::start);self.mail_sig=None;}
        let clipboard_on=self.app.clipboard_active();
        if clipboard_on!=self.clipboard_watch.is_some() {
            self.clipboard_watch=if clipboard_on {let ctx=ctx.clone();qingyu::clipboard::watch(self.app.clipboard.clone(),move||ctx.request_repaint())}else{None};
        }

    }


    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        diagnostics::graphics(cc);
        let running_compat=cc.wgpu_render_state.is_some();
        diagnostics::startup("加载本地配置");
        #[cfg(target_os="macos")]
        if !qingyu::model::config_dir().join("settings.txt").exists(){
            let settings=qingyu::model::Settings{dock:qingyu::model::Dock::Right,island_offset:-20,..Default::default()};
            settings.save();
        }
        let mut app = App::new(&cc.egui_ctx, Options::default());
        qingyu::qq_music::set_repaint_context(cc.egui_ctx.clone());
        app.render_compat_running=running_compat;
        diagnostics::startup("启动剪贴板记录");
        if std::env::args().any(|arg|arg=="--settings"){app.open_settings();}
        if std::env::args().any(|arg|arg=="--library") {
            app.show_page(app::LIBRARY_PAGE);
        }
        #[cfg(feature="record")]
        if std::env::var("QINGYU_MAIL_DEMO").is_ok(){app.mail_demo();}
        diagnostics::startup("初始化媒体和托盘");
        let media = smtc::shared();
        let media_enabled=smtc::spawn_poller(media.clone(),cc.egui_ctx.clone());
        media_enabled.store(app.settings.visible_pages[app::MUSIC_PAGE],std::sync::atomic::Ordering::Relaxed);
        #[cfg(windows)]
        let autostart = qingyu::tray::autostart_enabled();
        #[cfg(target_os="macos")] let autostart=qingyu::macos::autostart_enabled();
        #[cfg(windows)]
        let tray = app.start_tray(qingyu::tray::State {
            minimal: app.settings.minimal,
            autostart,
        });
        // 音频、网络与输入法查询由后台完成，避免首帧等待系统服务。
        diagnostics::startup("创建后台采样与窗口");
        #[cfg(windows)] let system_state=qingyu::system::Snapshot::default();
        #[cfg(windows)] let system_rx=qingyu::system::observe(cc.egui_ctx.clone(),system_state.clone());
        // 岛体大小只作用于主窗口，在首帧之前生效，避免启动时先按 100% 闪一下。
        #[cfg(any(windows,target_os="macos"))] eframe::set_root_viewport_scale(app.settings.island_scale());
        Self {
            app,
            t0: Instant::now(),
            #[cfg(windows)]
            frame_at: Instant::now(),
            media,
            seek_rx: None,
            media_control:None,
            music_volume_rx:None,
            agent_open_rx:None,
            qq_playback_rx:None,
            #[cfg(target_os="macos")]
            qq_music_opened:false,
            _resource_monitor:qingyu::resources::Monitor::start(qingyu::model::config_dir().join("diagnostics")),
            resource_at:PollClock::default(),
            guard_notified:false,
            media_enabled,
            media_fast_until: None,
            placed: false,
            placement_at: PollClock::default(),
            placement_scale: 0.0,
            placement_key: (String::new(), 0, qingyu::model::Dock::Top, true, (0, 0)),
            #[cfg(any(windows,target_os="macos"))]
            oled_shift: (0, 0),
            #[cfg(not(target_os="macos"))]
            win_x: 0.0,
            #[cfg(windows)]
            system_state,
            #[cfg(windows)] system_rx,
            fullscreen:None,
            #[cfg(any(windows,target_os="macos"))] hidden:false,
            #[cfg(windows)] backdrop:None,
            #[cfg(windows)]
            native: qingyu::native::Window::new(cc),
            #[cfg(windows)]
            tray,
            #[cfg(any(windows,target_os="macos"))]
            autostart,
            mail: None,
            mail_sig: None,
            clipboard_watch: None,
            #[cfg(target_os="macos")] notification_monitor:None,
            #[cfg(target_os="macos")] mac_window:qingyu::macos::Window::new(cc),
            #[cfg(target_os="macos")] mac_system:qingyu::macos::System::start(cc.egui_ctx.clone()),
            #[cfg(target_os="macos")] mac_state:Default::default(),
            #[cfg(target_os="macos")] mac_placement_mode:Default::default(),
            #[cfg(target_os="macos")] mac_panel_size:(0,0),
            #[cfg(target_os="macos")] mac_position_key:(None,(0,0)),
        }
    }

    #[cfg(windows)]
    fn poll_tray(&mut self) {
        use qingyu::tray::{self, Event};
        let events = self.tray.as_ref().map(|t| t.poll()).unwrap_or_default();
        for e in events {
            match e {
                Event::OpenSettings=>self.app.open_settings(),

                Event::ToggleMinimal => {
                    self.app.set_minimal(!self.app.settings.minimal);
                    self.app.say(if self.app.settings.minimal { "极简刘海已打开" } else { "极简刘海已关闭" });
                }
                Event::ToggleAutostart => {
                    let want = !self.autostart;
                    match tray::set_autostart(want) {
                        Ok(()) => {
                            self.autostart = want;
                            self.app.say(if want { "已设为开机启动" } else { "已取消开机启动" });
                        }
                        Err(e) => self.app.say(e),
                    }
                }
            }
        }
        if let Some(t) = &self.tray {
            t.sync(tray::State { minimal: self.app.settings.minimal, autostart: self.autostart });
        }
    }

    #[cfg(target_os="macos")]
    fn mac_toggle_autostart(&mut self) {
        let want=!self.autostart;
        match qingyu::macos::set_autostart(want) {
            Ok(())=>{self.autostart=want;self.app.say(if want{"已设为登录时启动"}else{"已取消登录时启动"});},
            Err(error)=>self.app.say(error),
        }
    }

    #[cfg(target_os="macos")]
    fn mac_pick_files(&mut self,kind:u32,title:&str,multiple:bool) {
        match self.mac_window.as_ref().ok_or_else(||"原生窗口尚未准备好，请重试".to_string()).and_then(|window|window.pick_files(kind,title,multiple)) {
            Ok(())=>{},Err(error)=>self.app.say(error),
        }
    }

    #[cfg(target_os="macos")]
    fn update_macos(&mut self,ctx:&egui::Context) {
        use qingyu::macos;
        let events=self.mac_window.as_ref().map(|w|w.events()).unwrap_or(0);
        if events&macos::SETTINGS!=0 {macos::activate();self.app.open_settings();}
        if events&macos::MINIMAL!=0 {self.app.set_minimal(!self.app.settings.minimal);}
        if events&macos::AUTOSTART!=0 {self.mac_toggle_autostart();}
        if events&macos::QQ_MUSIC_LIBRARY!=0 {self.open_qq_music(qingyu::qq_music::Page::Library);}
        if events&macos::QQ_MUSIC_PLAYER!=0 {self.open_qq_music(qingyu::qq_music::Page::Player);}
        if events&macos::FOREGROUND_CHANGED!=0 {self.app.foreground_changed();}
        if events&macos::QUIT!=0 {self.app.save_window_before_exit();ctx.send_viewport_cmd(egui::ViewportCommand::Close);}
        if let Some(window)=&self.mac_window {window.sync_menu(false,self.app.settings.minimal,self.autostart);}
        while let Some(picker)=self.mac_window.as_ref().and_then(|w|w.take_picker()) {
            if picker.paths.is_empty(){continue;}
            match picker.kind {
                1=>{self.app.on_drop(picker.paths);self.app.show_page(app::SHELF_PAGE);},
                _=>{},
            }
        }
        while let Ok(event)=self.mac_system.rx.try_recv() {
            match event {
                macos::SystemEvent::Error(error)=>self.app.say(error),
                macos::SystemEvent::Sample(next)=>{
                    self.fullscreen=next.fullscreen;self.mac_state=next;
                },
            }
        }
        let offset=self.app.placement_offset();
        let scale=self.app.settings.island_scale()*ctx.zoom_factor();
        let key=(self.app.settings.display_id.clone(),(offset*10000.).round() as i32,self.app.settings.dock,self.app.settings.avoid_top_bar,self.oled_shift);
        let refresh=self.placement_at.ready(Duration::from_secs(1));
        let panel_size=(self.app.settings.side_panel_width,self.app.settings.side_panel_height);
        let position=self.app.placement_position();let resident=self.app.resident_size();
        let position_key=(position.map(|point|((point[0]*1_000_000.0).round() as i32,(point[1]*1_000_000.0).round() as i32)),(resident.x.round() as i32,resident.y.round() as i32));
        if !self.placed||refresh||self.placement_scale!=scale||self.placement_key!=key||self.mac_placement_mode!=self.app.settings.macos_placement||self.mac_panel_size!=panel_size||self.mac_position_key!=position_key {
            self.mac_panel_size=panel_size;
            self.mac_position_key=position_key;
            if !self.placed||refresh {self.app.displays=qingyu::display_layout::connected();self.placement_at=PollClock::now();}
            self.placement_scale=scale;self.placement_key=key;self.mac_placement_mode=self.app.settings.macos_placement;
            if let Some(display)=qingyu::display_layout::selected(&self.app.displays,&self.app.settings.display_id) {
                if self.mac_window.as_mut().is_some_and(|w|w.place(display,&self.app.settings,scale,offset,position,resident,self.oled_shift)) {
                    if !self.placed {diagnostics::startup("macOS 原生窗口已定位并显示");}
                    if let Some(window)=&self.mac_window {
                        self.app.set_desktop_placement(window.desktop_placement());
                        self.app.set_side_panel_size(window.side_panel_size());
                        eframe::set_root_viewport_scale(window.scale()/ctx.zoom_factor());
                    }
                    self.placed=true;ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                }
            }
            if !self.placed && self.t0.elapsed()>Duration::from_secs(2) {ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));}
        }
        if let Some(window)=&self.mac_window {eframe::set_root_viewport_scale(window.scale()/ctx.zoom_factor());}
        self.hidden=self.mac_state.locked||qingyu::display_layout::hide_for_fullscreen(self.app.settings.hide_fullscreen,self.fullscreen,qingyu::display_layout::selected(&self.app.displays,&self.app.settings.display_id).map(|d|d.bounds));
        // Menu and input callbacks wake the loop immediately; this also permits
        // monitor changes and clock updates while the pointer is elsewhere.
            self.app.say("检测到资源占用异常，已暂停音乐信息和歌词。重启轻屿可恢复，详情见数据目录的 diagnostics 文件夹。");
        }

    fn env(&mut self, _ctx: &egui::Context) -> Env {
        #[cfg(not(target_os="macos"))] let ctx=_ctx;
        if let Some((source,rx))=&self.music_volume_rx {
            if let Ok(success)=rx.try_recv() {
                if !success && self.media.lock().is_ok_and(|media|media.source==*source) {
                    self.app.say("音乐应用音量暂不可调，请稍后重试");
                }
                self.music_volume_rx=None;
            }
        }
        let (clock, today, date) = local_now();
        #[cfg(not(target_os="macos"))]
        let ppp = ctx.pixels_per_point();
        #[cfg(not(target_os="macos"))]
        let origin = ctx.input(|i| i.viewport().outer_rect).map(|r| r.min).unwrap_or(Pos2::new(self.win_x, 0.0));
        #[cfg(windows)]
        let origin=self.native.as_ref().and_then(|w|w.logical_origin(ppp)).unwrap_or(origin);
        #[cfg(not(target_os="macos"))]
        let cursor = cursor_logical(ppp).map(|(x, y)| Pos2::new(x - origin.x, y - origin.y));
        #[cfg(target_os="macos")] let cursor=self.mac_window.as_ref().and_then(|w|w.cursor()).filter(|_|!self.hidden);
        #[cfg(target_os="macos")]
        let drag_geometry=self.mac_window.as_ref().and_then(|window|window.global_cursor().map(|cursor|app::DragGeometry{
            cursor,displays:self.app.displays.clone(),rooms:qingyu::macos::placement_rooms(&self.app.displays),scale:window.scale(),
        }));
        #[cfg(not(target_os="macos"))]
        let drag_geometry=None;
        // 所在屏幕换算到窗口逻辑坐标，供通知详情等浮层避开屏幕边缘。
        #[cfg(windows)]
        let screen=qingyu::display_layout::selected(&self.app.displays,&self.app.settings.display_id).map(|d|{
            // 与岛体摆放用同一范围：避开顶栏后，拖动换边和浮层都以顶栏下沿为顶边。
            let b=qingyu::display_layout::placement_bounds(d,self.app.settings.avoid_top_bar);
            egui::Rect::from_min_max(Pos2::new(b.left as f32/ppp-origin.x,b.top as f32/ppp-origin.y),Pos2::new(b.right as f32/ppp-origin.x,b.bottom as f32/ppp-origin.y))
        });
        #[cfg(target_os="macos")]
        let screen=self.mac_window.as_ref().and_then(|window|window.screen());
        #[cfg(not(any(windows,target_os="macos")))]
        let screen=None;
        #[cfg(windows)]
        let (backdrop,glass_blur)=(self.backdrop.as_ref().and_then(|b|b.color()),self.native.as_ref().is_some_and(|w|w.blur_active()));
        #[cfg(target_os="macos")]
        let (backdrop,glass_blur)=(None,self.mac_window.as_ref().is_some_and(|w|w.blur_active()));
        #[cfg(not(any(windows,target_os="macos")))]
        let (backdrop,glass_blur)=(None,false);
        Env {
            t: self.t0.elapsed().as_secs_f64(),
            unix_time: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            unix_ms: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64,
            clock,
            today,
            date,
            battery: None,
            charging: false,
            network: None,
            cursor,
            drag_geometry,
            idle_seconds: None,
            screen_locked: {
                #[cfg(windows)] {qingyu::system::screen_locked()}
                #[cfg(target_os="macos")] {self.mac_state.locked}
                #[cfg(not(any(windows,target_os="macos")))] {false}
            },
            browser_at_top: {
                #[cfg(windows)] {if self.app.settings.browser_minimal {self.native.as_mut().and_then(|native|native.browser_at_top(&self.app.settings))}else{None}}
                #[cfg(not(windows))] {None}
            },
            media: self.media.lock().map(|m| m.clone()).unwrap_or_default(),
            volume: {
                #[cfg(windows)] {self.system_state.volume}
                #[cfg(target_os="macos")] {self.mac_state.volume}
                #[cfg(not(any(windows,target_os="macos")))] {None}
            },
            screen,
            backdrop,
            glass_blur,
            opaque_surface: {
                #[cfg(windows)] {self.native.as_ref().is_some_and(|w|w.opaque_surface())}
                #[cfg(not(windows))] {false}
            },
        }
    }
}

impl eframe::App for Shell {
    fn on_exit(&mut self,_gl:Option<&eframe::glow::Context>) {
        #[cfg(target_os="macos")]
        qingyu::qq_music::shutdown();
    }

    fn raw_input_hook(&mut self,_ctx:&egui::Context,input:&mut egui::RawInput) {
        if input.viewport_id==egui::ViewportId::ROOT {self.app.filter_todo_input(input);}
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0; 4]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.resource_at.ready(Duration::from_secs(30)){
            let (count,bytes)={let textures=ctx.tex_manager();let textures=textures.read();(textures.num_allocated(),textures.allocated().map(|(_,info)|info.bytes_used()).sum())};
            qingyu::resources::TEXTURES.store(count,std::sync::atomic::Ordering::Relaxed);
            qingyu::resources::TEXTURE_BYTES.store(bytes,std::sync::atomic::Ordering::Relaxed);
            qingyu::resources::GALLEYS.store(ctx.fonts(|fonts|fonts.num_galleys_in_cache()),std::sync::atomic::Ordering::Relaxed);
            self.resource_at=PollClock::now();
        }
        let guarded=qingyu::resources::GUARD.tripped();
        if guarded && !self.guard_notified {
            // 资源保护熔断：停止音乐会话轮询（含封面、网易云日志）与歌词查询，保住用户电脑，重启后恢复。
            self.guard_notified=true;self.app.opts.fetch_lyrics=false;
            self.app.say("检测到资源占用异常，已暂停音乐信息和歌词。重启轻屿可恢复，详情见数据目录的 diagnostics 文件夹。");
        }
        self.media_enabled.store(self.app.settings.visible_pages[app::MUSIC_PAGE] && !guarded,std::sync::atomic::Ordering::Relaxed);
        if self.media_fast_until.is_some_and(|until|Instant::now()>=until){self.media_fast_until=None;}
        self.media_enabled.set_interval(if self.media_fast_until.is_some(){smtc::POLL_FAST}else{self.app.media_poll_interval()});
        // 设置里改了岛体大小：下一帧起主窗口按新倍率栅格化，定位逻辑随像素比变化同步调整画布。
        #[cfg(any(windows,target_os="macos"))] eframe::set_root_viewport_scale(self.app.settings.island_scale());
        #[cfg(windows)]
        {
            // 用帧间隔限制刷新，绕过部分 Intel 驱动在垂直同步中的忙等待。
            let delay=Duration::from_secs_f64(1.0/60.0).saturating_sub(self.frame_at.elapsed());
            if !delay.is_zero(){std::thread::sleep(delay);}
            self.frame_at=Instant::now();
        }
        #[cfg(windows)]
        self.poll_tray();
        #[cfg(windows)]
        let foreground_changed=qingyu::native::take_foreground_change();
        #[cfg(windows)]
        if foreground_changed {
            // 切到别的程序：收起，并在光标离开前不再自动展开
            self.app.foreground_changed();
        }
        #[cfg(windows)]
        let offset=self.app.placement_offset();
        #[cfg(windows)]
        let shift=self.oled_shift;
        // 拖动时位置不取整，按万分之一比例比较，窗口逐帧平滑跟随。
        #[cfg(windows)]
        let placement_key=(self.app.settings.display_id.clone(),(offset*10000.0).round() as i32,self.app.settings.dock,self.app.settings.avoid_top_bar,shift);
        #[cfg(windows)]
        if !self.placed || self.placement_scale!=ctx.pixels_per_point() || self.placement_key!=placement_key || self.placement_at.ready(Duration::from_secs(1)) {
            // 只有位置变化时沿用已知屏幕，避免拖动中每帧枚举显示器。
            let offset_only=self.placed && self.placement_scale==ctx.pixels_per_point() && !self.placement_at.ready(Duration::from_secs(1))
                && (&self.placement_key.0,self.placement_key.2,self.placement_key.3,self.placement_key.4)==(&placement_key.0,placement_key.2,placement_key.3,placement_key.4);
            if !offset_only {
                self.placement_at=PollClock::now();
                self.app.displays=qingyu::display_layout::connected();
            }
            self.placement_scale=ctx.pixels_per_point();
            self.placement_key=placement_key;
            let display=qingyu::display_layout::selected(&self.app.displays,&self.app.settings.display_id);
            if display.is_some_and(|d|self.native.as_ref().is_some_and(|w|w.place_on_display(self.placement_scale,qingyu::display_layout::placement_bounds(d,self.app.settings.avoid_top_bar),offset,self.app.settings.dock,shift))) {
                if !self.placed {diagnostics::startup("目标屏幕物理坐标定位完成，已请求显示");}
                self.placed=true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            } else if self.t0.elapsed()>=Duration::from_secs(2) {
                // Do not leave a hidden startup window forever when monitor discovery fails.
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }
        #[cfg(target_os="macos")] self.update_macos(ctx);
        #[cfg(not(any(windows,target_os="macos")))]
        if !self.placed {
            if let Some(mon)=ctx.input(|i|i.viewport().monitor_size) {
                self.placed=true;self.win_x=((mon.x-app::WIN_W)/2.0).round();
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(self.win_x,0.0)));
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            }
        }
        #[cfg(windows)]
        {
            while let Ok(observation)=self.system_rx.try_recv(){
                self.system_state=observation.system;
                self.fullscreen=observation.fullscreen;
            }
            self.hidden=qingyu::display_layout::hide_for_fullscreen(self.app.settings.hide_fullscreen,self.fullscreen,qingyu::display_layout::selected(&self.app.displays,&self.app.settings.display_id).map(|d|d.bounds));
            // 毛玻璃：在岛体左右两侧和画布底边（都在窗口区域之外）取样，估计岛背后的明暗。
            if self.app.settings.skin.is_glass() && !self.hidden {
                let sampler=self.backdrop.get_or_insert_with(qingyu::backdrop::Sampler::start);
                let ppp=ctx.pixels_per_point();
                if let Some(origin)=self.native.as_ref().and_then(|w|w.logical_origin(ppp)) {
                    let points=self.app.backdrop_points();
                    sampler.set_points(points.iter().map(|&(x,y)|(((origin.x+x)*ppp).round() as i32,((origin.y+y)*ppp).round() as i32)).collect());
                }
            } else {
                self.backdrop=None;
            }

        }
        if let Some((message,rx))=&self.seek_rx {
            if let Some(success)=match rx.try_recv(){Ok(success)=>Some(success),Err(std::sync::mpsc::TryRecvError::Disconnected)=>Some(false),Err(_)=>None}{
                let message=*message;
                self.seek_rx=None;
                if !success{self.app.say(message);}
            }
        }
        if let Some((request,since,receiver))=&self.qq_playback_rx{
            let result=match receiver.try_recv(){Ok(success)=>Some(success),Err(std::sync::mpsc::TryRecvError::Disconnected)=>Some(false),Err(std::sync::mpsc::TryRecvError::Empty)=>(since.elapsed()>=Duration::from_secs(2)).then_some(false)};
            if let Some(success)=result{let request=*request;self.qq_playback_rx=None;self.app.complete_qq_playback(request,success);}
            else{ctx.request_repaint_after(Duration::from_millis(16));}
        }
        if let Some(result)=self.agent_open_rx.as_ref().and_then(|receiver|match receiver.try_recv(){
            Ok(result)=>Some(result),Err(std::sync::mpsc::TryRecvError::Disconnected)=>Some(Err("无法打开 Agent 应用，请重试".into())),Err(std::sync::mpsc::TryRecvError::Empty)=>None,
        }){self.agent_open_rx=None;if let Err(error)=result{self.app.say(error);}}
        let mut qq_library=qingyu::qq_music::library();
        if guarded || !self.app.module_on(app::MUSIC_PAGE){qq_library.live=None;qq_library.live_lyrics=None;}
        self.app.set_qq_music(qq_library,None);
        let env = self.env(ctx);
        if self.hidden{self.app.suspend_island();}
        self.reconcile_modules(ctx);
        self.app.set_notification_session_locked(env.screen_locked);
        #[cfg(target_os="macos")]
        {
            let config=self.app.notification_config();
            // The worker only exists for an enabled notification module; it
            // never requests consent while starting or polling.
            if self.app.module_on(app::NOTIFICATIONS_PAGE)&&self.app.settings.notifications&&self.app.settings.read_notifications&&self.app.settings.macos_dock_badges {
                let monitor=self.notification_monitor.get_or_insert_with(||qingyu::notifications::Monitor::start(ctx.clone()));
                monitor.configure(config);self.app.notification_batch(monitor.take());
            }else{self.notification_monitor=None;self.app.notification_status=Default::default();}
        }
        #[cfg(not(target_os="macos"))]
        {self.app.notification_status=qingyu::notifications::Status::Unsupported;}
        // v0.5.5 邮件：开关 / 间隔 / 账号清单变了就重新对齐收信线程，再把事件交给界面。
        // 每帧只比较不复制，确有变化时才复制账号清单。
        if let Some(mail)=&self.mail {
            let (mail_enabled,mail_interval)=(self.app.mail_active(),self.app.settings.mail_interval);
            if !self.mail_sig.as_ref().is_some_and(|known|(known.0,known.1)==(mail_enabled,mail_interval) && known.2.as_slice()==self.app.mail_accounts()){
                let sig=(mail_enabled,mail_interval,self.app.mail_accounts().to_vec());
                let accounts:&[qingyu::mail::Account]=if sig.0{ &sig.2 }else{ &[] };
                mail.sync(accounts,sig.1);
                self.mail_sig=Some(sig);
            }
            while let Some(ev)=mail.try_recv(){self.app.mail_event(ev);}
        }
        let mut out = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                // 左右停靠时画布是竖向的 SIDE_W × SIDE_H。
                let size=self.app.canvas_size();
                let win = egui::Rect::from_min_size(ui.max_rect().min,size);
                out = Some(self.app.frame(ui, win, &env));
            });
        let Some(out) = out else { return };

        #[cfg(windows)]
        {
            // 先处理本帧输入和动画，再决定是否移动，避免悬停或按下的首帧仍使用旧状态。
            let hovering=env.cursor.is_some_and(|p|out.hit.contains(p)||out.notice_hit.is_some_and(|r|r.contains(p)));
            let shift=if !self.app.settings.oled_shift {(0,0)}
                else if !self.hidden && !hovering && self.app.oled_shift_idle(ctx) {
                    ctx.request_repaint_after(qingyu::oled_shift::until_next_step(self.t0.elapsed()));
                    let target=qingyu::oled_shift::shift(true,self.app.settings.dock,self.t0.elapsed());
                    qingyu::oled_shift::toward(self.oled_shift,target)
                } else {self.oled_shift};
            if shift!=self.oled_shift {
                self.oled_shift=shift;
                let display=qingyu::display_layout::selected(&self.app.displays,&self.app.settings.display_id);
                if display.is_some_and(|d|self.native.as_ref().is_some_and(|w|w.place_on_display(ctx.pixels_per_point(),qingyu::display_layout::placement_bounds(d,self.app.settings.avoid_top_bar),self.app.placement_offset(),self.app.settings.dock,shift))) {
                    self.placement_key.4=shift;
                }
                ctx.request_repaint();
            }
        }

        #[cfg(windows)]
        if let Some(native) = &mut self.native {
            let r = |r: egui::Rect| (r.left(), r.top(), r.width(), r.height());
            // 保留主窗口的事件循环，避免全屏隐藏后设置窗口无法关闭或重新打开。
            let region=if self.hidden {(0.0,0.0,0.0,0.0)} else {r(out.region)};
            let hit=if self.hidden {(0.0,0.0,0.0,0.0)} else {r(out.hit)};
            native.update(region, hit, if self.hidden{None}else{out.notice_hit.map(r)},if self.hidden{None}else{out.file_drop_zone.map(r)},if self.hidden{None}else{out.glass.as_deref()},ctx.pixels_per_point(), out.editing, !self.hidden && out.expanded);
        }
        #[cfg(target_os="macos")]
        if let Some(native)=&mut self.mac_window {
            let coordinates_ready=native.coordinates_ready(ctx.pixels_per_point());
            if !coordinates_ready {ctx.request_repaint();}
            native.update(out.hit,&out.hit_shapes,out.notice_hit,&out.notice_shapes,out.file_drop_zone,out.glass.as_deref(),out.editing,self.hidden||!coordinates_ready);
            let hovering=env.cursor.is_some_and(|p|out.hit_shapes.iter().chain(&out.notice_shapes).any(|shape|qingyu::shape::contains_outline(&shape.outline(),p)));
            if !self.app.settings.oled_shift {self.oled_shift=(0,0);}
            else if !self.hidden && !hovering && self.app.oled_shift_idle(ctx) {
                self.oled_shift=qingyu::oled_shift::toward(self.oled_shift,qingyu::oled_shift::shift(true,self.app.settings.dock,self.t0.elapsed()));
                ctx.request_repaint_after(qingyu::oled_shift::until_next_step(self.t0.elapsed()));
            }
        }
        for cmd in out.cmds {
            match cmd {
                Cmd::QqMusic(page)=>self.open_qq_music(page),
                Cmd::QqMusicResume=>{
                    if !self.app.module_on(app::MUSIC_PAGE){self.app.say(MODULE_OFF);continue;}
                    self.qq_playback_rx=None;
                    match qingyu::qq_music::resume(){
                        Ok(())=>{
                            #[cfg(target_os="macos")] {self.qq_music_opened=true;}
                            self.media_fast_until=Some(Instant::now()+Duration::from_secs(25));self.media_enabled.set_interval(smtc::POLL_FAST);
                        },
                        Err(error)=>{self.app.cancel_qq_playback();self.app.say(error);},
                    }
                    self.app.set_qq_music(qingyu::qq_music::library(),None);ctx.request_repaint();
                }
                Cmd::QqMusicStop=>{
                    self.qq_playback_rx=None;
                    self.app.stop_qq_playback();
                    qingyu::qq_music::shutdown();
                    #[cfg(target_os="macos")] {self.qq_music_opened=false;}
                    self.app.set_qq_music(qingyu::qq_music::library(),None);
                    self.app.say("已停止 QQ 音乐播放");
                }
                Cmd::QqMusicSetPlaying{request,playing}=>{
                    self.media_fast_until=Some(Instant::now()+Duration::from_secs(3));self.media_enabled.set_interval(smtc::POLL_FAST);
                    match qingyu::qq_music::set_playing(playing){
                        Ok(receiver)=>{self.qq_playback_rx=Some((request,Instant::now(),receiver));ctx.request_repaint();},
                        Err(_)=>self.app.complete_qq_playback(request,false),
                    }
                }
                Cmd::QqMusicRefresh=>{
                    if !self.app.module_on(app::MUSIC_PAGE){self.app.say(MODULE_OFF);continue;}
                    match qingyu::qq_music::refresh_library(){
                        Ok(())=>{#[cfg(target_os="macos")] {self.qq_music_opened=true;}},
                        Err(error)=>self.app.say(error),
                    }
                }
                Cmd::QqMusicPlayPlaylist(id)=>{
                    if !self.app.module_on(app::MUSIC_PAGE){self.app.say(MODULE_OFF);continue;}
                    self.qq_playback_rx=None;self.app.cancel_qq_playback();
                    match qingyu::qq_music::play_playlist(&id){
                        Ok(())=>{
                            #[cfg(target_os="macos")] {self.qq_music_opened=true;}
                            self.media_fast_until=Some(Instant::now()+Duration::from_secs(5));self.media_enabled.set_interval(smtc::POLL_FAST);
                            self.app.say("正在载入歌单…");
                        },
                        Err(error)=>self.app.say(error),
                    }
                    self.app.set_qq_music(qingyu::qq_music::library(),None);ctx.request_repaint();
                }
                Cmd::Media(t) => {
                    self.media_fast_until=Some(Instant::now()+Duration::from_secs(3));self.media_enabled.set_interval(smtc::POLL_FAST);
                    let source=self.media.lock().map(|media|media.source.clone()).unwrap_or_default();
                    let message=if source.eq_ignore_ascii_case("cloudmusic.exe"){"网易云暂未适配进度跳转，请在客户端调整"}
                        else if source.eq_ignore_ascii_case("bodian_pc.exe"){"波点音乐暂未适配进度跳转，请在客户端调整"}
                        else if source.eq_ignore_ascii_case("kwmusic.exe"){"酷我音乐暂未适配进度跳转，请在客户端调整"}
                        else{"播放器未完成跳转，请打开客户端后重试"};
                    if self.media_control.is_none(){self.media_control=smtc::ControlWorker::start(ctx.clone());}
                    match self.media_control.as_ref().ok_or(()).and_then(|worker|worker.submit(t,source)){
                        Ok(Some(rx))=>self.seek_rx=Some((message,rx)),Ok(None)=>{},Err(())=>self.app.say("播放器正在响应，请稍后重试"),
                    }
                }
                Cmd::MusicVolume(command,source)=>{
                    self.media_fast_until=Some(Instant::now()+Duration::from_secs(3));self.media_enabled.set_interval(smtc::POLL_FAST);
                    if self.media_control.is_none(){self.media_control=smtc::ControlWorker::start(ctx.clone());}
                    match self.media_control.as_ref().ok_or(()).and_then(|worker|worker.submit(command,source.clone())){
                        Ok(Some(rx))=>self.music_volume_rx=Some((source,rx)),
                        _=>self.app.say("音乐应用音量暂不可调，请稍后重试"),
                    }
                }

                Cmd::RestartApp=>{
                    // 先拉起新进程（它会等本进程退出），再像托盘「退出」一样摘图标后结束。
                    if diagnostics::spawn_replacement(){
                        self.app.save_window_before_exit();
                        #[cfg(target_os="macos")] qingyu::qq_music::shutdown();
                        #[cfg(windows)] if let Some(tray)=&self.tray {tray.exit_process();}
                        std::process::exit(0);
                    }else{self.app.say("无法重新启动轻屿，请从托盘退出后手动打开");}
                }
                Cmd::Open(p) => {if let Err(e)=open_path(&p){self.app.say(e);}},
                Cmd::OpenUrl(url)=>{
                    if let Some(url)=qingyu::mail::web_url(&url) {
                        if open_path(std::path::Path::new(&url)).is_err(){self.app.say("无法打开默认浏览器，请检查系统默认应用设置");}
                    }
                },
                Cmd::AddFiles=>{
                    #[cfg(target_os="macos")] self.mac_pick_files(1,"添加文件到置物架",true);
                    #[cfg(windows)] match qingyu::system::pick_files(){Ok(files)=>if !files.is_empty(){self.app.on_drop(files);self.app.show_page(app::SHELF_PAGE);},Err(e)=>self.app.say(format!("无法选择文件：{}",e.code()))}
                },
                Cmd::DragFile(p)=>{
                    #[cfg(target_os="macos")] if let Some(window)=&self.mac_window {if let Err(error)=window.drag_file(&p){self.app.say(error);}}
                    #[cfg(windows)] if let Err(e)=qingyu::system::drag_file(&p){self.app.say(format!("无法拖出文件：{}",e.code()));}
                    ctx.input_mut(|i| i.pointer=Default::default());
                },

                Cmd::Reveal(p)=>{
                    #[cfg(windows)] {let _=qingyu::child_process::spawn(std::process::Command::new("explorer.exe").arg(format!("/select,{}",p.display())));}
                    #[cfg(target_os="macos")] if let Err(error)=qingyu::macos::open(&p,true){self.app.say(error);}
                },
                Cmd::ToggleAutostart=>{
                    #[cfg(target_os="macos")] self.mac_toggle_autostart();
                    #[cfg(windows)] {let want=!self.autostart;match qingyu::tray::set_autostart(want){Ok(())=>self.autostart=want,Err(e)=>self.app.say(e)}}
                },
                Cmd::SetVolume(v)=>{
                    #[cfg(target_os="macos")] if !self.mac_system.set_volume(v){self.app.say("系统音量正在调整，请稍后重试");}
                    #[cfg(windows)] qingyu::system::set_volume(v);
                },
                Cmd::ToggleMute=>{
                    #[cfg(target_os="macos")] if !self.mac_system.toggle_mute(){self.app.say("系统音量正在调整，请稍后重试");}
                    #[cfg(windows)] qingyu::system::toggle_mute();
                },
                Cmd::Copy(text)=>{ctx.copy_text(text);self.app.say("已复制");},
                Cmd::RequestNotificationAccess=>{
                    #[cfg(target_os="macos")] qingyu::notifications::request_access();
                },
                Cmd::OpenNotification{bundle_id,activation}=>{
                    #[cfg(target_os="macos")] if !qingyu::notifications::open(&bundle_id,&activation){self.app.say("原通知已失效，或应用尚未安装");}
                    #[cfg(not(target_os="macos"))] {let _=(bundle_id,activation);}
                },
                Cmd::OpenAgent{agent,session_id,pid,observed_at_ms}=>{
                    if env.screen_locked{continue;}
                    if self.agent_open_rx.is_some(){self.app.say("正在打开对话窗口，请稍后重试");continue;}
                    let (sender,receiver)=std::sync::mpsc::sync_channel(1);let repaint=ctx.clone();
                    match std::thread::Builder::new().name("qingyu-open-agent".into()).spawn(move||{
                        let result=qingyu::agent_open::open_observed(agent,&session_id,pid,observed_at_ms);
                        let _=sender.send(result);repaint.request_repaint();
                    }){
                        Ok(_)=>self.agent_open_rx=Some(receiver),
                        Err(_)=>self.app.say("无法打开 Agent 应用，请重试"),
                    }
                },
                Cmd::Notify(title,body)=>{
                    #[cfg(target_os="macos")] qingyu::mac_services::notify(&title,&body,self.app.settings.sound);
                    #[cfg(windows)] if let Some(tray)=&self.tray{tray.notify(&title,&body,self.app.settings.sound);}
                },

                // 模块关闭时设置项已置灰；万一仍有命令到达，只提示，不为它临时启动服务。
                Cmd::MailSync=>if let Some(mail)=&self.mail {
                    let enabled=self.app.mail_active();
                    let accounts=if enabled{self.app.mail_accounts().to_vec()}else{Vec::new()};
                    mail.sync(&accounts,self.app.settings.mail_interval);
                    self.mail_sig=Some((enabled,self.app.settings.mail_interval,self.app.mail_accounts().to_vec()));
                },
                Cmd::MailTest(account,password)=>match &self.mail {Some(mail)=>mail.test(account,password),None=>self.app.say(MODULE_OFF)},
                Cmd::MailManage(request)=>match &self.mail {Some(mail)=>mail.manage(request),None=>self.app.say(MODULE_OFF)},
                Cmd::Focus => {
                    #[cfg(target_os="macos")] if let Some(window)=&self.mac_window {window.focus();}
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                },
            }
        }


        #[cfg(any(windows,target_os="macos"))] let autostart = self.autostart;
        #[cfg(not(any(windows,target_os="macos")))] let autostart = false;
        self.app.settings_ui(ctx, autostart);
        self.app.mail_reader_ui(ctx);
        #[cfg(any(windows,target_os="macos"))] let repaint_after=if self.hidden{Duration::from_secs(1)}else{out.repaint_after};
        #[cfg(not(any(windows,target_os="macos")))] let repaint_after=out.repaint_after;
        schedule_repaint(ctx,repaint_after);
    }
}


// ---------------- 系统信息 ----------------

fn schedule_repaint(ctx:&egui::Context,interval:Duration){
    qingyu::frame_pacing::schedule(ctx,interval);
}

#[cfg(test)]
mod repaint_tests {
    use super::*;

    fn scheduled_delay(interval:Duration,immediate:bool)->Duration{
        let ctx=egui::Context::default();
        let mut delay=Duration::ZERO;
        for frame in 0..4 {
            let output=ctx.run(egui::RawInput{time:Some(frame as f64),predicted_dt:1.0/60.0,..Default::default()},|ctx|{
                if immediate{ctx.request_repaint();}
                schedule_repaint(ctx,interval);
            });
            delay=output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
        }
        delay
    }

    #[test]
    fn background_repaint_keeps_the_requested_frame_interval(){
        for fps in [60.0,30.0,20.0,10.0,1.0] {
            let interval=Duration::from_secs_f64(1.0/fps);
            let actual=scheduled_delay(interval,false);
            assert!(actual.abs_diff(interval)<Duration::from_micros(1),"目标 {fps} 帧不应被提前唤醒：{actual:?}");
        }
    }

    #[test]
    fn interaction_and_service_events_still_repaint_immediately(){
        assert_eq!(scheduled_delay(Duration::from_secs(1),true),Duration::ZERO);
    }
    #[test]
    fn animation_requests_the_next_vsync_without_an_extra_timer_interval(){
        assert_eq!(scheduled_delay(Duration::ZERO,false),Duration::ZERO);
        // Reproduce the renderer's timer + display boundary ordering with
        // a 2ms draw cost at 60/120Hz. Fixed 16.7ms waits skip display slots.
        for refresh in [60.0_f64,120.0]{
            let display=1.0/refresh;let mut previous=0.0;let mut old_intervals=Vec::new();let mut new_intervals=Vec::new();
            for _ in 0..30{
                let old_next=((previous+0.002+1.0/60.0)/display).ceil()*display;
                old_intervals.push(old_next-previous);
                let new_next=((previous+0.002)/display).ceil()*display;
                new_intervals.push(new_next-previous);previous=old_next;
            }
            assert!(old_intervals.iter().all(|dt|*dt>=display*2.0-0.00001));
            assert!(new_intervals.iter().all(|dt|(*dt-display).abs()<0.00001));
        }
    }
}

#[cfg(windows)]
fn local_now() -> (String, String, String) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let st = unsafe { GetLocalTime() };
    let wd = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];
    (
        format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond),
        format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay),
        format!("{}月{}日 {}", st.wMonth, st.wDay, wd[(st.wDayOfWeek as usize).min(6)]),
    )
}

#[cfg(not(windows))]
fn local_now() -> (String, String, String) {
    use chrono::Datelike;
    let now=chrono::Local::now();
    let weekdays=["周日","周一","周二","周三","周四","周五","周六"];
    (now.format("%H:%M:%S").to_string(),now.format("%Y-%m-%d").to_string(),format!("{}月{}日 {}",now.month(),now.day(),weekdays[now.weekday().num_days_from_sunday() as usize]))
}

#[cfg(windows)]
fn cursor_logical(ppp: f32) -> Option<(f32, f32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some((p.x as f32 / ppp, p.y as f32 / ppp))
}

#[cfg(not(any(windows,target_os="macos")))]
fn cursor_logical(_ppp: f32) -> Option<(f32, f32)> {
    None
}

#[cfg(windows)]
fn open_path(p: &std::path::Path)->Result<(),String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{ShellExecuteExW,SHOpenWithDialog,SHELLEXECUTEINFOW,OPENASINFO,SEE_MASK_FLAG_NO_UI,OAIF_EXEC};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let mut wide: Vec<u16> = p.as_os_str().encode_wide().collect();
    wide.push(0);
    let mut info=SHELLEXECUTEINFOW{cbSize:std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,fMask:SEE_MASK_FLAG_NO_UI,lpVerb:windows::core::w!("open"),lpFile:PCWSTR(wide.as_ptr()),nShow:SW_SHOWNORMAL.0,..Default::default()};
    open_path_result(p,unsafe{ShellExecuteExW(&mut info)},||unsafe{
        SHOpenWithDialog(None,&OPENASINFO{pcszFile:PCWSTR(wide.as_ptr()),oaifInFlags:OAIF_EXEC,..Default::default()})
    })
}

#[cfg(windows)]
fn open_path_result(p:&std::path::Path,result:windows::core::Result<()>,choose:impl FnOnce()->windows::core::Result<()>)->Result<(),String>{
    use windows::Win32::Foundation::{ERROR_CANCELLED,ERROR_NO_ASSOCIATION};
    let result=match result{
        Err(error) if error.code()==windows::core::HRESULT::from_win32(ERROR_NO_ASSOCIATION.0)&&p.is_file()=>choose(),
        other=>other,
    };
    match result{
        Err(error) if error.code()==windows::core::HRESULT::from_win32(ERROR_CANCELLED.0)=>Ok(()),
        other=>other.map_err(|_|"无法打开，请检查文件位置和默认应用".into()),
    }
}

#[cfg(target_os="macos")]
fn open_path(path:&std::path::Path)->Result<(),String> {qingyu::macos::open(path,false)}
#[cfg(not(any(windows,target_os="macos")))]
fn open_path(_p: &std::path::Path)->Result<(),String> {Err("此平台暂未接入系统文件打开服务".into())}

#[cfg(all(test,windows))]
mod open_path_tests {
    use super::*;
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED,ERROR_CANCELLED,ERROR_FILE_NOT_FOUND,ERROR_NO_ASSOCIATION};
    fn error(code:windows::Win32::Foundation::WIN32_ERROR)->windows::core::Error{windows::core::Error::from_hresult(windows::core::HRESULT::from_win32(code.0))}

    #[test]
    fn unassociated_files_offer_open_with_and_cancellation_is_normal(){
        let path=std::env::temp_dir().join(format!("轻屿 无关联-{}.qingyu-open-test",std::process::id()));
        std::fs::write(&path,b"fixture").unwrap();
        for cancel in [false,true]{
            let mut chosen=false;
            let result=open_path_result(&path,Err(error(ERROR_NO_ASSOCIATION)),||{chosen=true;if cancel{Err(error(ERROR_CANCELLED))}else{Ok(())}});
            std::fs::remove_file(&path).unwrap();
            assert!(chosen,"文件没有默认关联时应提供系统打开方式");
            assert_eq!(result,Ok(()),"用户取消选择不应显示打开失败");
            if !cancel{std::fs::write(&path,b"fixture").unwrap();}
        }
    }

    #[test]
    fn default_apps_urls_and_other_errors_never_open_the_file_picker(){
        let file=std::env::temp_dir().join(format!("qingyu-open-errors-{}",std::process::id()));
        std::fs::write(&file,b"fixture").unwrap();
        for result in [Ok(()),Err(error(ERROR_ACCESS_DENIED)),Err(error(ERROR_FILE_NOT_FOUND))]{
            let success=result.is_ok();
            let actual=open_path_result(&file,result,||panic!("已有默认程序或其他打开错误不应弹出选择器"));
            assert_eq!(actual.is_ok(),success);
        }
        assert!(open_path_result(&file,Err(error(ERROR_NO_ASSOCIATION)),||Err(error(ERROR_ACCESS_DENIED))).is_err(),"选择器失败应保留错误提示");
        std::fs::remove_file(file).unwrap();
        for (path,result,success) in [
            ("https://example.com",Ok(()),true),
            ("https://example.com",Err(error(ERROR_NO_ASSOCIATION)),false),
            ("missing.qingyu-open-test",Err(error(ERROR_NO_ASSOCIATION)),false),
            ("missing.qingyu-open-test",Err(error(ERROR_FILE_NOT_FOUND)),false),
            ("blocked.qingyu-open-test",Err(error(ERROR_ACCESS_DENIED)),false),
        ]{
            let actual=open_path_result(std::path::Path::new(path),result,||panic!("非文件关联问题不能弹出打开方式"));
            assert_eq!(actual.is_ok(),success);
        }
    }
}

fn main() -> std::process::ExitCode {
    #[cfg(all(target_os="macos", debug_assertions))]
    if let Err(error)=dev_startup::prepare() {
        eprintln!("轻屿启动失败：{error}");
        return std::process::ExitCode::FAILURE;
    }
    #[cfg(all(target_os="macos", debug_assertions))]
    if std::env::args().any(|arg|arg=="--mail-import-development-credential") {
        use std::io::Read;
        // Only anonymous stdin carries the secret. No diagnostics, window,
        // instance lock, account-file changes, or Keychain dialogs on this path.
        let mut input=String::new();
        let result=(|| -> Result<(),String> {
            std::io::stdin().take(70001).read_to_string(&mut input).map_err(|_|"无法读取导入数据")?;
            if input.len()>70000{return Err("导入数据过长".into());}
            let value:serde_json::Value=serde_json::from_str(&input).map_err(|_|"导入格式无效")?;
            let account=value.get("account").and_then(|v|v.as_str()).ok_or("缺少账号")?;
            let password=value.get("password").and_then(|v|v.as_str()).ok_or("缺少授权码")?;
            qingyu::mail::secrets::import_development(account,password)
        })();
        return match result {
            Ok(())=>{println!("{{\"ready\":true}}");std::process::ExitCode::SUCCESS}
            Err(error)=>{eprintln!("{error}");std::process::ExitCode::FAILURE}
        };
    }
    #[cfg(target_os="macos")]
    if std::env::args().any(|arg|arg=="--mail-credential-check") {
        let accounts=qingyu::mail::load_accounts();
        let checks=accounts.iter().filter(|account|account.enabled).map(|account| {
            let ready=qingyu::mail::secrets::load(&account.credential_key()).is_ok_and(|password|!password.is_empty());
            serde_json::json!({"provider":account.provider.key(),"ready":ready})
        }).collect::<Vec<_>>();
        let ready=!checks.is_empty()&&checks.iter().all(|check|check["ready"]==true);
        println!("{}",serde_json::json!({"ready":ready,"accounts":checks}));
        return if ready{std::process::ExitCode::SUCCESS}else{std::process::ExitCode::from(2)};
    }
    if std::env::args().any(|arg|arg=="--library-install-hermes") {
        let root=qingyu::model::config_dir().join("library");
        return match qingyu::library::install_hermes(&root) {
            Ok(message)=>{println!("{message}");std::process::ExitCode::SUCCESS}
            Err(message)=>{eprintln!("{message}");std::process::ExitCode::FAILURE}
        };
    }
    if std::env::args().any(|arg|arg=="--library-check") {
        let store=qingyu::library::Store::load();
        println!("{}",serde_json::json!({"root":store.root(),"entries":store.entries.len(),
            "hermes_available":qingyu::library::hermes_available(),"skill_installed":qingyu::library::hermes_installed(store.root()),"error":store.error}));
        return if store.error.is_none(){std::process::ExitCode::SUCCESS}else{std::process::ExitCode::FAILURE};
    }
    // TCC may attribute CLI permissions to its launcher (Terminal/Codex).
    // Report the actual launch context; this cannot verify a separate GUI.
    #[cfg(target_os="macos")]
    if std::env::args().any(|arg|arg=="--notification-access-check") {
        let report=qingyu::notifications::diagnostics::access_report();
        let granted=report["accessibility_granted"].as_bool().unwrap_or(false);
        println!("{report}");
        return if granted{std::process::ExitCode::SUCCESS}else{std::process::ExitCode::from(2)};
    }
    #[cfg(target_os="macos")]
    if std::env::args().any(|arg|arg=="--notification-preview-check") {
        use qingyu::notifications::{Config,Kind};
        let settings=qingyu::model::Settings::load();
        let kinds=[(Kind::Qq,settings.qq_notifications),(Kind::Wechat,settings.wechat_notifications&&!settings.qq_only),
            (Kind::Wecom,settings.wecom_notifications&&!settings.qq_only),(Kind::Feishu,settings.feishu_notifications&&!settings.qq_only),
            (Kind::Dingtalk,settings.dingtalk_notifications&&!settings.qq_only),(Kind::DesktopMail,settings.desktop_mail_notifications&&!settings.qq_only)]
            .into_iter().filter_map(|(kind,on)|on.then_some(kind)).collect::<Vec<_>>();
        let enabled=settings.notifications&&settings.read_notifications&&settings.macos_dock_badges
            &&settings.visible_pages[app::NOTIFICATIONS_PAGE]&&!kinds.is_empty();
        let disabled=qingyu::model::NotificationApps::load().items.into_iter().filter_map(|(id,_,on)|(!on).then_some(id)).collect();
        let seconds=std::env::args().find_map(|arg|arg.strip_prefix("--notification-preview-seconds=").and_then(|value|value.parse().ok())).unwrap_or(5);
        let captured=qingyu::notifications::diagnostics::check(Config{enabled,kinds,previews:settings.notification_preview,disabled},seconds);
        return if captured{std::process::ExitCode::SUCCESS}else{std::process::ExitCode::from(2)};
    }
    #[cfg(target_os="macos")]
    if std::env::args().any(|arg|arg=="--notification-connection-check") {
        println!("{}",qingyu::notifications::diagnostics::access_report());
        use qingyu::notifications::{Config,Kind,Monitor,Status};
        let settings=qingyu::model::Settings::load();
        let kinds=[(Kind::Qq,settings.qq_notifications),(Kind::Wechat,settings.wechat_notifications&&!settings.qq_only)]
            .into_iter().filter_map(|(kind,on)|on.then_some(kind)).collect::<Vec<_>>();
        let enabled=settings.notifications&&settings.read_notifications&&settings.macos_dock_badges
            &&settings.visible_pages[app::NOTIFICATIONS_PAGE]&&!kinds.is_empty();
        let monitor=Monitor::start(egui::Context::default());
        monitor.configure(Config{enabled,kinds,previews:settings.notification_preview,..Default::default()});
        let deadline=Instant::now()+Duration::from_secs(5);
        let status=loop {
            let batch=monitor.take();
            if !matches!(batch.status,Status::Starting)||Instant::now()>=deadline {break batch.status;}
            std::thread::sleep(Duration::from_millis(100));
        };
        let ready=status.monitoring();
        // Report connection metadata only; sampled events stay in memory.
        println!("{}",serde_json::json!({"enabled":enabled,"ready":ready,"status":status.label()}));
        return if ready{std::process::ExitCode::SUCCESS}else{std::process::ExitCode::from(2)};
    }
    diagnostics::install();
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            diagnostics::report("轻屿启动或运行失败。", &error.to_string());
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> eframe::Result<()> {
    // 自动重启或「立即重启」拉起的新进程：先等旧进程退出，再做单实例检查。
    diagnostics::wait_for_previous_instance();
    #[cfg(target_os="macos")]
    let _instance=match qingyu::macos::Instance::acquire().map_err(|error|eframe::Error::AppCreation(error.into()))? {qingyu::macos::Startup::Primary(instance)=>instance,qingyu::macos::Startup::Activated=>return Ok(())};
    // 单实例（与原版同名互斥体）
    #[cfg(windows)]
    if !qingyu::tray::single_instance() {
        if !qingyu::tray::activate_existing() {
            diagnostics::report("轻屿已在运行，但暂时无法打开窗口。\n请稍后重试；若仍无法打开，请在任务管理器的“详细信息”中结束 qingyu.exe 后重试。", "已有实例可能正在启动或已无响应。请稍后重试；若仍无法打开，请在任务管理器的“详细信息”中结束 qingyu.exe，再重新启动。");
        }
        return Ok(());
    }
    #[cfg(windows)]
    diagnostics::startup("初始化图形窗口");
    let viewport = egui::ViewportBuilder::default()
        .with_title("轻屿").with_icon(qingyu::theme::app_icon())
        .with_inner_size([app::CANVAS_W, app::WIN_H])
        .with_decorations(false)
        .with_transparent(true)
        .with_active(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_always_on_top()
        .with_visible(false);
    let opts = eframe::NativeOptions { viewport, vsync: !cfg!(windows), ..Default::default() };
    graphics::run(opts,|opts|eframe::run_native("轻屿", opts, Box::new(|cc| Ok(Box::new(Shell::new(cc))))))
}
