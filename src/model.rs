//! 轻屿的持久化与状态模型。默认使用各平台的 Qingyu 数据目录；首次启动安全迁移旧 Daozai 目录。
//! QINGYU_DATA_DIR 可显式指定数据位置，兼容旧 DAOZAI_DATA_DIR。

use std::fs;
use std::path::{Path, PathBuf};

/// 新环境变量优先；兼容旧变量只影响显式指定的目录，不自动移动它。
pub fn config_dir() -> PathBuf {
    if let Some(path)=data_dir_override(std::env::var_os("QINGYU_DATA_DIR"),std::env::var_os("DAOZAI_DATA_DIR")){return path;}
    let path=default_config_dir(std::env::consts::OS,std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("APPDATA").map(PathBuf::from),std::env::var_os("XDG_DATA_HOME").map(PathBuf::from));
    // 配置由多个后台模块读取；进程内仅允许一次迁移操作进入文件系统。
    static MIGRATION:std::sync::Mutex<()>=std::sync::Mutex::new(());
    let _guard=MIGRATION.lock().unwrap_or_else(|e|e.into_inner());
    match resolve_data_dir(path,|old,new|fs::rename(old,new)){
        Ok(path)=>path,
        Err((path,error))=>{
            static REPORTED:std::sync::Once=std::sync::Once::new();
            REPORTED.call_once(||eprintln!("轻屿无法迁移数据目录：{error}。继续使用原目录 {}，原文件保持完整。",path.display()));
            path
        }
    }
}
fn data_dir_override(current:Option<std::ffi::OsString>,legacy:Option<std::ffi::OsString>)->Option<PathBuf>{
    [current,legacy].into_iter().flatten().find(|value|!value.is_empty()).map(PathBuf::from)
}
fn default_config_dir(os:&str,home:Option<PathBuf>,appdata:Option<PathBuf>,xdg:Option<PathBuf>)->PathBuf{
    match os{
        "macos"=>home.unwrap_or_else(std::env::temp_dir).join("Library/Application Support/Qingyu"),
        "windows"=>appdata.unwrap_or_else(std::env::temp_dir).join("Qingyu"),
        _=>xdg.unwrap_or_else(||home.unwrap_or_else(std::env::temp_dir).join(".local/share")).join("Qingyu"),
    }
}
/// 已存在的新目录优先，不合并或覆盖；迁移失败则返回旧目录作为安全回退。
fn resolve_data_dir(new:PathBuf,rename:impl FnOnce(&Path,&Path)->std::io::Result<()>)->Result<PathBuf,(PathBuf,std::io::Error)>{
    let legacy=new.with_file_name("Daozai");
    if new.exists()||!legacy.is_dir(){return Ok(new);}
    match rename(&legacy,&new){
        Ok(())=>Ok(new),
        Err(_) if !legacy.exists()&&new.is_dir()=>Ok(new), // 另一进程已完成同一迁移。
        Err(error)=>Err((legacy,error)),
    }
}

fn read_kv(path: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(text) = fs::read_to_string(path) {
        for line in text.trim_start_matches('\u{feff}').lines() {
            if let Some((k, v)) = line.split_once('=') {
                out.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    out
}

fn write_kv(path: &Path, pairs: &[(String, String)]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut preserved=read_kv(path);
    preserved.retain(|(key,_)|!pairs.iter().any(|(written,_)|written==key));
    let body: String = pairs
        .iter().chain(preserved.iter())
        .map(|(k, v)| format!("{k}={v}\n"))
        .collect();
    atomic_write(path, body.as_bytes())
}

pub(crate) fn atomic_write(path:&Path,body:&[u8])->std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64,Ordering};
    static SERIAL:AtomicU64=AtomicU64::new(0);
    let (tmp,mut file)=loop {
        let mut name=path.file_name().ok_or_else(||std::io::Error::other("缺少文件名"))?.to_os_string();
        name.push(format!(".{}-{}.tmp",std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed)));
        let tmp=path.with_file_name(name);
        let mut options=fs::OpenOptions::new();options.write(true).create_new(true);
        #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
        match options.open(&tmp) {
            Ok(file)=>break (tmp,file),
            Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>continue,
            Err(e)=>return Err(e),
        }
    };
    let result=file.write_all(body).and_then(|_|file.sync_all());
    drop(file);
    let result=result.and_then(|_|replace_file(&tmp,path));
    if result.is_err(){let _=fs::remove_file(&tmp);}
    result
}

fn replace_file(tmp:&Path,path:&Path)->std::io::Result<()> {
    let result=fs::rename(tmp,path);
    #[cfg(windows)]
    if result.as_ref().is_err_and(|e|e.kind()==std::io::ErrorKind::CrossesDevices) {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{MoveFileExW,MOVEFILE_COPY_ALLOWED,MOVEFILE_REPLACE_EXISTING,MOVEFILE_WRITE_THROUGH};
        // Windows 目录映射可能将同目录替换报告为跨卷，交给系统复制并替换。
        let source:Vec<u16>=tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        let target:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();
        return unsafe{MoveFileExW(PCWSTR(source.as_ptr()),PCWSTR(target.as_ptr()),MOVEFILE_REPLACE_EXISTING|MOVEFILE_COPY_ALLOWED|MOVEFILE_WRITE_THROUGH)}.map_err(std::io::Error::other);
    }
    result
}

/// 岛体配色皮肤（与形状独立，可任意组合）。
/// - 经典：纯黑；奶油：暖白底深棕字、橙色强调（参考 Oyen）；
/// - 毛玻璃·浅 / 毛玻璃·深：系统实时模糊 + 浅色或深色玻璃。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Skin {
    #[default]
    Classic,
    Cream,
    GlassLight,
    GlassDark,
    Fluid,
}

impl Skin {
    pub const ALL: [Skin; 5] = [Skin::Classic, Skin::Cream, Skin::GlassLight, Skin::GlassDark, Skin::Fluid];
    pub fn key(self) -> &'static str {
        match self { Self::Classic => "classic", Self::Fluid => "fluid", Self::Cream => "cream", Self::GlassLight => "glass_light", Self::GlassDark => "glass" }
    }
    pub fn parse(value: &str) -> Self {
        match value { "fluid" => Self::Fluid, "cream" => Self::Cream, "glass_light" => Self::GlassLight, "glass" | "glass_dark" => Self::GlassDark, _ => Self::Classic }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Classic => "经典", Self::Fluid => "悬浮云", Self::Cream => "奶油", Self::GlassLight => "毛玻璃·浅", Self::GlassDark => "毛玻璃·深" }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Classic => "纯黑岛体，与屏幕刘海融为一体，资源占用最低。",
            Self::Fluid => "半透明灰底随音乐封面换色，光晕跟随鼠标，播放进度带有渐隐光尾。皮肤作者：啵（@hanhb123）。",
            Self::Cream => "暖白底色配深棕文字和橙色强调，柔和明亮。",
            Self::GlassLight => "浅色磨砂玻璃，透出身后桌面的颜色，文字为深色；按背景明暗自动调整，保证清晰。",
            Self::GlassDark => "深色磨砂玻璃，透出身后桌面的颜色，文字为白色；按背景明暗自动调整，保证清晰。",
        }
    }
    pub fn is_glass(self) -> bool { matches!(self, Self::GlassLight | Self::GlassDark) }
}

/// 岛体形状：贴着屏幕顶边的刘海，或悬浮在顶边下方的胶囊。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IslandShape {
    #[default]
    Notch,
    Pill,
}

impl IslandShape {
    pub const ALL: [IslandShape; 2] = [IslandShape::Notch, IslandShape::Pill];
    pub fn key(self) -> &'static str {
        match self { Self::Notch => "notch", Self::Pill => "pill" }
    }
    pub fn parse(value: &str) -> Self {
        match value { "pill" => Self::Pill, _ => Self::Notch }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Notch => "刘海", Self::Pill => "胶囊" }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Notch => "贴着屏幕顶边，顶部两侧向外柔和过渡，像屏幕刘海。",
            Self::Pill => "悬浮在屏幕顶边下方，四角圆润，收起时是完整的胶囊。",
        }
    }
}

/// 岛体停靠的屏幕边缘：顶部为横向刘海 / 胶囊，左右两侧为竖向排列。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dock {
    #[default]
    Top,
    Left,
    Right,
}

impl Dock {
    pub const ALL: [Dock; 3] = [Dock::Top, Dock::Left, Dock::Right];
    pub fn key(self) -> &'static str {
        match self { Self::Top => "top", Self::Left => "left", Self::Right => "right" }
    }
    pub fn parse(value: &str) -> Self {
        match value { "left" => Self::Left, "right" => Self::Right, _ => Self::Top }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Top => "顶部", Self::Left => "左侧", Self::Right => "右侧" }
    }
    pub fn is_side(self) -> bool { self != Self::Top }
}

/// macOS 顶部定位策略；手动保留用户选择的形状与旧版顶栏避让。
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub enum MacosPlacement {
    #[default]
    Auto,
    BelowMenuBar,
    Manual,
}
impl MacosPlacement {
    pub const ALL:[Self;3]=[Self::Auto,Self::BelowMenuBar,Self::Manual];
    pub fn key(self)->&'static str {match self {Self::Auto=>"auto",Self::BelowMenuBar=>"below",Self::Manual=>"manual"}}
    pub fn parse(value:&str)->Self {match value {"below"=>Self::BelowMenuBar,"manual"=>Self::Manual,_=>Self::Auto}}
    pub fn label(self)->&'static str {match self {Self::Auto=>"自动适配",Self::BelowMenuBar=>"菜单栏下方",Self::Manual=>"手动定位"}}
}

/// 岛体大小（百分比）的范围与步进。
pub const ISLAND_SIZE_MIN: u32 = 80;
pub const ISLAND_SIZE_MAX: u32 = 140;
pub const ISLAND_SIZE_STEP: u32 = 5;
/// 水平位置：-100 = 最左，0 = 居中，100 = 最右（按可移动范围的百分比）。
pub const ISLAND_OFFSET_MAX: i32 = 100;

pub fn island_size_value(value: u32) -> u32 {
    let value = value.clamp(ISLAND_SIZE_MIN, ISLAND_SIZE_MAX);
    (value + ISLAND_SIZE_STEP / 2) / ISLAND_SIZE_STEP * ISLAND_SIZE_STEP
}

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
#[serde(rename_all="snake_case")]
pub enum PetKind {#[default] Tan,Fox,Puppy,Bunny,Panda,Kitten,Frog}
impl PetKind {
    pub const ALL:[Self;7]=[Self::Tan,Self::Fox,Self::Puppy,Self::Bunny,Self::Panda,Self::Kitten,Self::Frog];
    pub fn key(self)->&'static str{match self{Self::Tan=>"tan",Self::Fox=>"fox",Self::Puppy=>"puppy",Self::Bunny=>"bunny",Self::Panda=>"panda",Self::Kitten=>"kitten",Self::Frog=>"frog"}}
    pub fn label(self)->&'static str{match self{Self::Tan=>"团团",Self::Fox=>"小狐",Self::Puppy=>"布丁",Self::Bunny=>"绵绵",Self::Panda=>"饭团",Self::Kitten=>"芝麻",Self::Frog=>"青豆"}}
    fn parse(value:&str)->Self{Self::ALL.into_iter().find(|pet|pet.key()==value).unwrap_or_default()}
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub display_id: String,
    /// 岛体皮肤、毛玻璃是否使用系统模糊、大小百分比、水平位置。
    pub skin: Skin,
    pub island_shape: IslandShape,
    pub glass_blur: bool,
    pub island_size: u32,
    pub island_offset: i32,
    /// None keeps the legacy edge/offset plan. New drags save a normalized
    /// resident center in the selected display's free-placement area, including
    /// the menu-bar strip. The physical camera is excluded when laying out content.
    pub island_position:Option<[f32;2]>,
    /// Collapsed long axis in logical points; zero follows actual content.
    pub resident_length:u32,
    /// OLED 像素偏移：岛体收起静止时沿停靠边缓慢移动几个像素，减轻 OLED 静态显示留下的痕迹。
    /// 默认关闭，开启后也不改动 `island_offset`。
    pub oled_shift: bool,
    /// 停靠边缘；左右停靠时 `island_offset` 表示上下位置。
    pub dock: Dock,
    pub macos_placement: MacosPlacement,
    pub settings_theme: crate::settings_theme::SettingsTheme,
    /// Resident pet avatar, stored as island_pet; the retired pet setting is not
    /// reused so old migrations cannot silently hide the new companion.
    pub pet: bool,
    pub resident_pet: PetKind,
    pub resident_pet_agent: Option<String>,
    /// Stable Agent keys, independent from the freely selected resident pet.
    pub agent_pets: std::collections::BTreeMap<String,PetKind>,
    pub minimal: bool,
    pub browser_minimal: bool,
    pub avoid_browsers: bool,
    pub avoid_apps: Vec<(String,String)>,
    pub visible_pages: [bool; crate::app::PAGE_COUNT],
    pub page_order: [usize; crate::app::PAGE_COUNT],
    pub show_battery: bool,
    pub clock_12h: bool,
    pub clock_period: u8,
    pub resident_order: Vec<usize>,
    pub system_hud: bool,
    pub bluetooth_notifications: bool,
    pub read_notifications:bool,
    /// Optional Dock badge reader, gated by macOS Accessibility permission.
    pub macos_dock_badges:bool,
    /// Explicit side appearance: false = visible island (default), true =
    /// thin handle. Keep the legacy key so saved choices remain unchanged.
    pub macos_side_handle:bool,
    /// Content viewport in logical points; independent of font/island scale.
    pub side_panel_width:u32,
    pub side_panel_height:u32,
    pub qq_notifications:bool,
    pub mail_notifications:bool,
    pub desktop_mail_notifications:bool,
    pub wechat_notifications:bool,
    pub wecom_notifications:bool,
    pub feishu_notifications:bool,
    pub dingtalk_notifications:bool,
    /// 微信缩在托盘时，点提醒通过系统托盘按钮唤起微信（默认关闭）。
    pub wechat_tray_open:bool,
    pub qq_only:bool,
    pub notification_preview:bool,
    pub notifications: bool,
    pub agent_activity: bool,
    pub agent_completion: bool,
    pub agent_activity_interval: u32,
    pub sound: bool,
    pub lyrics_enabled:bool,
    pub island_lyrics:bool,
    pub lyric_delay_ms:i32,
    pub hide_fullscreen: bool,
    /// 顶栏类软件（含置顶的任务栏）登记占用屏幕顶部时，岛体从它的下沿开始摆放。
    pub avoid_top_bar: bool,
    pub focus_minutes: u32,
    pub rest_minutes: u32,
    pub water_minutes: u32,
    pub serving_ml: u32,
    pub water_start: u32,
    pub water_end: u32,
    /// 久坐提醒：连续使用电脑达到 sit_minutes 分钟后提醒起身；与喝水共用提醒时段。
    pub sit_remind: bool,
    pub sit_minutes: u32,
    /// 午休免打扰：lunch_start..lunch_end（一天中的分钟数）内喝水和久坐都不提醒。
    pub lunch_rest: bool,
    pub lunch_start: u32,
    pub lunch_end: u32,
    /// v0.5.5 邮件提醒：总开关、验证码高亮、轮询间隔（秒）。
    pub mail_enabled: bool,
    pub mail_sync_operations:bool,
    pub mail_code: bool,
    pub mail_code_hide_seconds:u32,
    pub mail_interval: u32,
    /// 记录最近复制的文字、图片和视频（最近记录与固定条目均在本机保存）。
    pub clipboard_history: bool,
    /// 天气页要看的城市；留空表示按网络出口自动定位。
    pub weather_city: String,
    /// 用户确认的具体地点；刷新时直接使用坐标，避免同名城市重新匹配。
    pub weather_position: Option<crate::weather::Position>,
    /// 已保存、可在岛上下拉切换的城市（不含自动定位），最多 `weather::MAX_CITIES` 个。
    pub weather_saved: Vec<crate::weather::Position>,
    /// 兼容渲染：用 Direct3D 12 代替 OpenGL（部分显卡的 OpenGL 驱动会崩溃或画面错乱）。重启后生效。
    pub render_compat: bool,
    /// 自动切换到兼容渲染的原因；手动开启时为空。
    pub render_compat_reason: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { display_id: String::new(), skin: Skin::Classic, island_shape: IslandShape::Notch, glass_blur: true, island_size: 100, island_offset: 0, island_position:None, resident_length:0, oled_shift: false, dock: Dock::Top, macos_placement:MacosPlacement::Auto, settings_theme: Default::default(), pet: true, resident_pet:PetKind::default(), resident_pet_agent:None, agent_pets:Default::default(), minimal: false, browser_minimal: false, avoid_browsers: true, avoid_apps: Vec::new(), visible_pages: [true; crate::app::PAGE_COUNT], page_order: [0,1,6,2,3,4,5,7,8], show_battery: false, clock_12h: false, clock_period: 0, resident_order: Vec::new(), system_hud: false, bluetooth_notifications: false, read_notifications:false,macos_dock_badges:false,macos_side_handle:false, side_panel_width:292, side_panel_height:532, qq_notifications:true, mail_notifications:true, desktop_mail_notifications:false, wechat_notifications:false, wecom_notifications:false, feishu_notifications:false, dingtalk_notifications:false, wechat_tray_open:false, qq_only:false, notification_preview:false, notifications: true, agent_activity: true, agent_completion: true, agent_activity_interval: 45, sound: false, lyrics_enabled:true, island_lyrics:false, lyric_delay_ms:-350, hide_fullscreen: true, avoid_top_bar: true, focus_minutes: 25, rest_minutes: 5, water_minutes: 45, serving_ml: 250, water_start: 9, water_end: 22, sit_remind: true, sit_minutes: 60, lunch_rest: true, lunch_start: 720, lunch_end: 810, mail_enabled: true, mail_sync_operations:false, mail_code: true, mail_code_hide_seconds:60, mail_interval: 120, clipboard_history: true, weather_city: String::new(), weather_position: None, weather_saved: Vec::new(), render_compat: false, render_compat_reason: String::new() }
    }
}

impl Settings {
    /// 主窗口（灵动岛）的界面缩放倍率。
    pub fn island_scale(&self) -> f32 {
        island_size_value(self.island_size) as f32 / 100.0
    }

    pub fn side_panel_size(&self)->eframe::egui::Vec2{
        eframe::egui::vec2(self.side_panel_width.clamp(292,520) as f32,self.side_panel_height.clamp(532,900) as f32)
    }

    /// 水平位置，-1..=1。
    pub fn island_offset_ratio(&self) -> f32 {
        self.island_offset.clamp(-ISLAND_OFFSET_MAX, ISLAND_OFFSET_MAX) as f32 / ISLAND_OFFSET_MAX as f32
    }

    pub fn clock_text(&self, clock: &str) -> String {
        use chrono::Timelike;
        let Ok(time) = chrono::NaiveTime::parse_from_str(clock.get(..5).unwrap_or(""), "%H:%M") else { return "--:--".into(); };
        let (pm, hour) = time.hour12();
        let digits=if self.clock_12h {format!("{}:{:02}",hour,time.minute())}else{time.format("%H:%M").to_string()};
        let period=match self.clock_period {
            1=>if pm {"下午"}else{"上午"},
            2=>match time.hour() {0..=5=>"凌晨",6..=10=>"上午",11..=12=>"中午",13..=16=>"下午",17..=18=>"傍晚",_=>"晚上"},
            _=>return digits,
        };
        format!("{period} {digits}")
    }

    pub fn load() -> Self {
        let mut s = Self::default();
        let values=read_kv(&config_dir().join("settings.txt"));
        let value=|key:&str|values.iter().find(|(k,_)|k==key).map(|(_,v)|v.as_str());
        (s.visible_pages,s.page_order)=migrate_page_preferences(value("page_schema"),value("visible_pages"),value("page_order"));
        for (k, v) in values {
            match k.as_str() {
                "settings_theme" => s.settings_theme=crate::settings_theme::SettingsTheme::parse(&v),
                "display_id" => s.display_id = v,
                "skin" => {
                    // 早期测试版把胶囊当作皮肤保存。
                    if v == "pill" { s.island_shape = IslandShape::Pill; }
                    s.skin = Skin::parse(&v);
                }
                "island_shape" => s.island_shape = IslandShape::parse(&v),
                "dock" => s.dock = Dock::parse(&v),
                "macos_placement" => s.macos_placement=MacosPlacement::parse(&v),
                "glass_blur" => s.glass_blur = v != "0",
                "island_size" => s.island_size = island_size_value(v.parse::<u32>().unwrap_or(100)),
                "island_offset" => s.island_offset = v.parse::<i32>().unwrap_or(0).clamp(-ISLAND_OFFSET_MAX, ISLAND_OFFSET_MAX),
                "island_position" => s.island_position=crate::island_position::parse_position(&v),
                "resident_length" => s.resident_length=crate::island_position::length(v.parse().unwrap_or(0)),
                "island_pet" => s.pet=v!="0",
                "resident_pet" => s.resident_pet=PetKind::parse(&v),
                "resident_pet_agent" => s.resident_pet_agent=(!v.is_empty()).then_some(v),
                "agent_pets" => s.agent_pets=serde_json::from_str(&v).unwrap_or_default(),
                "oled_shift" => s.oled_shift = v != "0",
                "minimal" => s.minimal = v == "1" || v.eq_ignore_ascii_case("true"),
                "browser_minimal" => s.browser_minimal = v == "1",
                "avoid_browsers" => s.avoid_browsers = v != "0",
                "avoid_apps" => s.avoid_apps = serde_json::from_str(&v).unwrap_or_default(),
                "resident_order" => {
                    let order:Vec<usize>=v.split(',').filter_map(|n|n.parse().ok()).collect();
                    s.resident_order=crate::bar_layout::migrate_order(&order);
                }
                "clock_period" => s.clock_period = v.parse::<u8>().ok().filter(|v|*v<=2).unwrap_or(0),
                "clock_12h" => s.clock_12h = v == "1",
                "macos_side_handle" => s.macos_side_handle=v=="1",
                "side_panel_width"=>s.side_panel_width=v.parse::<u32>().unwrap_or(292).clamp(292,520),
                "side_panel_height"=>s.side_panel_height=v.parse::<u32>().unwrap_or(532).clamp(532,900),
                "read_notifications" => s.read_notifications=v=="1",
                "macos_dock_badges" => s.macos_dock_badges=v=="1",
                "qq_notifications" => s.qq_notifications=v!="0",
                "mail_notifications" => s.mail_notifications=v!="0",
                "desktop_mail_notifications" => s.desktop_mail_notifications=v=="1",
                "wechat_notifications" => s.wechat_notifications=v=="1",
                "wecom_notifications" => s.wecom_notifications=v=="1",
                "feishu_notifications" => s.feishu_notifications=v=="1",
                "dingtalk_notifications" => s.dingtalk_notifications=v=="1",
                "qq_only" => s.qq_only=v=="1",
                "wechat_tray_open" => s.wechat_tray_open=v=="1",
                "notification_preview" => s.notification_preview=v=="1",
                "notifications" => s.notifications = v != "0",
                "agent_activity" => s.agent_activity = v != "0",
                "agent_completion" => s.agent_completion = v != "0",
                "agent_activity_interval" => s.agent_activity_interval = v.parse::<u32>().unwrap_or(45).clamp(30,120),
                "sound" => s.sound = v == "1",
                "lyrics_enabled" => s.lyrics_enabled=v!="0",
                "island_lyrics" => s.island_lyrics=v=="1",
                "lyric_delay_ms" => s.lyric_delay_ms=v.parse::<i32>().unwrap_or(-350).clamp(-5000,5000),
                "hide_fullscreen" => s.hide_fullscreen = v != "0",
                "avoid_top_bar" => s.avoid_top_bar = v != "0",
                "mail_enabled" => s.mail_enabled = v != "0",
                "mail_sync_operations" => s.mail_sync_operations=v=="1",
                "mail_code" => s.mail_code = v != "0",
                "mail_code_hide_seconds" => s.mail_code_hide_seconds=v.parse::<u32>().unwrap_or(60).clamp(15,300),
                "mail_interval" => s.mail_interval = match v.parse::<u32>().unwrap_or(120) { 60 => 60, 300 => 300, _ => 120 },
                "clipboard_history" => s.clipboard_history = v != "0",
                "weather_city" => s.weather_city = v.trim().to_string(),
                "weather_position" => s.weather_position=serde_json::from_str::<Option<crate::weather::Position>>(&v).ok().flatten().filter(|p|p.valid()),
                "weather_saved" => s.weather_saved=serde_json::from_str::<Vec<crate::weather::Position>>(&v).unwrap_or_default().into_iter().filter(|p|p.valid()).collect(),
                "render_compat" => s.render_compat = v == "1",
                "render_compat_reason" => s.render_compat_reason = v.chars().filter(|c| !c.is_control()).take(200).collect(),
                _ => {}
            }
        }
        if !s.visible_pages.contains(&true) { s.visible_pages[0]=true; }
        // 旧版只有一个指定城市：迁移进已保存列表，升级后下拉框里能直接看到它。
        if let Some(position)=s.weather_position.clone().filter(|p|p.label==s.weather_city) {
            if !s.weather_saved.contains(&position) {s.weather_saved.insert(0,position);}
        }
        s.weather_saved.dedup();s.weather_saved.truncate(crate::weather::MAX_CITIES);
        s
    }

    pub fn save(&self) {
        let _ = write_kv(
            &config_dir().join("settings.txt"),
            &[
                ("display_id".into(),self.display_id.clone()),
                ("skin".into(),self.skin.key().into()),
                ("island_shape".into(),self.island_shape.key().into()),
                ("dock".into(),self.dock.key().into()),
                ("macos_placement".into(),self.macos_placement.key().into()),
                ("glass_blur".into(),(self.glass_blur as u8).to_string()),
                ("island_size".into(),island_size_value(self.island_size).to_string()),
                ("island_offset".into(),self.island_offset.clamp(-ISLAND_OFFSET_MAX,ISLAND_OFFSET_MAX).to_string()),
                ("island_position".into(),serde_json::to_string(&self.island_position).unwrap_or_else(|_|"null".into())),
                ("resident_length".into(),crate::island_position::length(self.resident_length).to_string()),
                ("island_pet".into(),(self.pet as u8).to_string()),
                ("resident_pet".into(),self.resident_pet.key().into()),
                ("resident_pet_agent".into(),self.resident_pet_agent.clone().unwrap_or_default()),
                ("agent_pets".into(),serde_json::to_string(&self.agent_pets).unwrap_or_else(|_|"{}".into())),
                ("oled_shift".into(),(self.oled_shift as u8).to_string()),
                ("settings_theme".into(),self.settings_theme.key().into()),
                ("minimal".into(), if self.minimal { "1".to_string() } else { "0".to_string() }),
                ("browser_minimal".into(), (self.browser_minimal as u8).to_string()),
                ("avoid_browsers".into(), (self.avoid_browsers as u8).to_string()),
                ("avoid_apps".into(), serde_json::to_string(&self.avoid_apps).unwrap_or_default()),
                ("page_schema".into(),"7".into()),
                ("visible_pages".into(), self.visible_pages.iter().map(|enabled|if *enabled {'1'}else{'0'}).collect()),
                ("page_order".into(),self.page_order.iter().map(usize::to_string).collect::<Vec<_>>().join(",")),
                ("resident_order".into(),self.resident_order.iter().map(usize::to_string).collect::<Vec<_>>().join(",")),
                ("clock_period".into(), self.clock_period.to_string()),
                ("clock_12h".into(), (self.clock_12h as u8).to_string()),
                ("macos_side_handle".into(),(self.macos_side_handle as u8).to_string()),
                ("side_panel_width".into(),self.side_panel_width.clamp(292,520).to_string()),
                ("side_panel_height".into(),self.side_panel_height.clamp(532,900).to_string()),
                ("read_notifications".into(),(self.read_notifications as u8).to_string()),
                ("macos_dock_badges".into(),(self.macos_dock_badges as u8).to_string()),
                ("qq_notifications".into(),(self.qq_notifications as u8).to_string()),
                ("mail_notifications".into(),(self.mail_notifications as u8).to_string()),
                ("desktop_mail_notifications".into(),(self.desktop_mail_notifications as u8).to_string()),
                ("wechat_notifications".into(),(self.wechat_notifications as u8).to_string()),
                ("wecom_notifications".into(),(self.wecom_notifications as u8).to_string()),
                ("feishu_notifications".into(),(self.feishu_notifications as u8).to_string()),
                ("dingtalk_notifications".into(),(self.dingtalk_notifications as u8).to_string()),
                ("qq_only".into(),(self.qq_only as u8).to_string()),
                ("wechat_tray_open".into(),(self.wechat_tray_open as u8).to_string()),
                ("notification_preview".into(),(self.notification_preview as u8).to_string()),
                ("notifications".into(), (self.notifications as u8).to_string()),
                ("agent_activity".into(), (self.agent_activity as u8).to_string()),
                ("agent_completion".into(), (self.agent_completion as u8).to_string()),
                ("agent_activity_interval".into(), self.agent_activity_interval.clamp(30,120).to_string()),
                ("sound".into(), (self.sound as u8).to_string()),
                ("lyrics_enabled".into(),(self.lyrics_enabled as u8).to_string()),
                ("island_lyrics".into(),(self.island_lyrics as u8).to_string()),
                ("lyric_delay_ms".into(),self.lyric_delay_ms.to_string()),
                ("hide_fullscreen".into(), (self.hide_fullscreen as u8).to_string()),
                ("avoid_top_bar".into(), (self.avoid_top_bar as u8).to_string()),
                ("mail_enabled".into(), (self.mail_enabled as u8).to_string()),
                ("mail_sync_operations".into(),(self.mail_sync_operations as u8).to_string()),
                ("mail_code".into(), (self.mail_code as u8).to_string()),
                ("mail_code_hide_seconds".into(), self.mail_code_hide_seconds.to_string()),
                ("mail_interval".into(), self.mail_interval.to_string()),
                ("clipboard_history".into(), (self.clipboard_history as u8).to_string()),
                ("weather_city".into(), self.weather_city.clone()),
                ("weather_position".into(), serde_json::to_string(&self.weather_position).unwrap()),
                ("weather_saved".into(), serde_json::to_string(&self.weather_saved).unwrap()),
                ("render_compat".into(), (self.render_compat as u8).to_string()),
                ("render_compat_reason".into(), self.render_compat_reason.clone()),
            ],
        );
    }
}

/// schema 7 追加通知，保持已保存页面 ID、顺序和模块开关。
fn migrate_page_preferences(schema:Option<&str>,visible:Option<&str>,order:Option<&str>)->([bool;crate::app::PAGE_COUNT],[usize;crate::app::PAGE_COUNT]){
    // Stable IDs keep existing module switches and relative order intact.
    let retained:&[usize]=match schema{Some("7")=>&[0,1,2,3,4,5,6,7,8],Some("6")=>&[0,1,2,3,4,5,6,7],Some("5")=>&[0,1,2,3,4,5,6],Some("4")=>&[0,1,2,3,4,5],Some("3")=>&[0,1,2,3,4],Some("2")=>&[0,1,2,3,6],_=>&[0,3,4,5,8]};
    let valid_len=|n|match schema{Some("7")=>n==9,Some("6")=>n==8,Some("5")=>n==7,Some("4")=>n==6,Some("3")=>n==5,Some("2")=>n==7,_=>(6..=9).contains(&n)};
    let mut enabled=[true;crate::app::PAGE_COUNT];
    if let Some(bits)=visible.filter(|value|valid_len(value.len())&&value.bytes().all(|b|matches!(b,b'0'|b'1'))){
        let newly_added=bits.bytes().any(|b|b==b'1');
        for (page,old) in retained.iter().enumerate(){enabled[page]=bits.as_bytes().get(*old).map_or(newly_added,|b|*b==b'1');}
        if !matches!(schema,Some("4"|"5"|"6"|"7")){
            if !enabled[..5].contains(&true){enabled[0]=true;}
            enabled[5]=newly_added;
        }
        // The former shelf subpage inherits its module switch on upgrade.
        if !matches!(schema,Some("5"|"6"|"7")){enabled[6]=enabled[1];}
        if !matches!(schema,Some("6"|"7")){enabled[crate::app::MAC_STATS_PAGE]=newly_added;}
        if schema!=Some("7"){enabled[crate::app::NOTIFICATIONS_PAGE]=newly_added;}
    }
    if !enabled.contains(&true){enabled[0]=true;}
    let mut pages=Vec::new();
    if let Some(raw)=order.and_then(|value|value.split(',').map(str::parse::<usize>).collect::<Result<Vec<_>,_>>().ok()){
        if valid_len(raw.len())&&(0..raw.len()).all(|page|raw.contains(&page)){
            for old in raw{if let Some(page)=retained.iter().position(|id|*id==old){pages.push(page);}}
        }
    }
    for page in 0..6{if !pages.contains(&page){pages.push(page);}}
    if !pages.contains(&6){let after_shelf=pages.iter().position(|page|*page==1).unwrap()+1;pages.insert(after_shelf,6);}
    if !pages.contains(&crate::app::MAC_STATS_PAGE){pages.push(crate::app::MAC_STATS_PAGE);}
    if !pages.contains(&crate::app::NOTIFICATIONS_PAGE){pages.push(crate::app::NOTIFICATIONS_PAGE);}
    (enabled,pages.try_into().unwrap())
}
#[cfg(test)]
mod page_migration_tests{
    use super::migrate_page_preferences as migrate;
    #[test]
    fn legacy_six_to_nine_pages_keep_retained_switches_and_order(){
        for schema in [None,Some("1")]{for count in 6..=9{
            let bits="100100001"[..count].to_string();
            let order=(0..count).rev().map(|page|page.to_string()).collect::<Vec<_>>().join(",");
            let (visible,pages)=migrate(schema,Some(&bits),Some(&order));
            assert_eq!(visible,[true,true,false,false,true,true,true,true,true]);
            assert_eq!(pages,if count==9{[4,3,2,1,6,0,5,7,8]}else{[3,2,1,6,0,4,5,7,8]});
        }}
    }
    #[test]
    fn schema_two_removes_retired_pages_without_shifting_weather_preferences(){
        assert_eq!(migrate(Some("2"),Some("0100101"),Some("6,4,5,3,2,1,0")),([false,true,false,false,true,true,true,true,true],[4,3,2,1,6,0,5,7,8]));
        assert_eq!(migrate(Some("2"),Some("0000100"),Some("4,6,5,0,1,2,3")),([true,false,false,false,false,true,false,true,true],[4,0,1,6,2,3,5,7,8]));
        assert_eq!(migrate(Some("2"),Some("0000010"),None).0,[true,false,false,false,false,true,false,true,true]);
        assert_eq!(migrate(Some("2"),Some("0000011"),None).0,[false,false,false,false,true,true,false,true,true]);
    }
    #[test]
    fn schema_three_preserves_every_choice_and_splits_clipboard(){
        for mask in 1u32..32{
            let bits=(0..5).map(|page|if mask&(1<<page)!=0{'1'}else{'0'}).collect::<String>();
            let (visible,pages)=migrate(Some("3"),Some(&bits),Some("4,3,2,1,0"));
            assert_eq!(visible[..5],(0..5).map(|page|mask&(1<<page)!=0).collect::<Vec<_>>());
            assert!(visible[5]);assert_eq!(visible[6],visible[1]);assert_eq!(pages,[4,3,2,1,6,0,5,7,8]);
        }
    }
    #[test]
    fn schema_four_keeps_agent_and_inherits_the_former_shelf_switch(){
        assert_eq!(migrate(Some("4"),Some("010110"),Some("5,4,3,2,1,0")),([false,true,false,true,true,false,true,true,true],[5,4,3,2,1,6,0,7,8]));
        assert_eq!(migrate(Some("4"),Some("000001"),None).0,[false,false,false,false,false,true,false,true,true]);
        for mask in 1u32..64{
            let bits=(0..6).map(|page|if mask&(1<<page)!=0{'1'}else{'0'}).collect::<String>();
            let (visible,_)=migrate(Some("4"),Some(&bits),None);
            assert_eq!(visible[..6],(0..6).map(|page|mask&(1<<page)!=0).collect::<Vec<_>>());
            assert_eq!(visible[6],visible[1]);
        }
    }
    #[test]
    fn schema_five_upgrade_preserves_independent_switches_and_custom_order(){
        for mask in 1u32..128{
            let bits=(0..7).map(|page|if mask&(1<<page)!=0{'1'}else{'0'}).collect::<String>();
            let (visible,pages)=migrate(Some("5"),Some(&bits),Some("6,5,4,3,2,1,0"));
            assert!(visible[7]);
            assert_eq!(&visible[..7],(0..7).map(|page|mask&(1<<page)!=0).collect::<Vec<_>>().as_slice());
            assert_eq!(pages,[6,5,4,3,2,1,0,7,8]);
        }
    }
    #[test]
    fn schema_six_roundtrips_mac_status_and_all_existing_pages(){
        for mask in 1u32..256{
            let bits=(0..8).map(|page|if mask&(1<<page)!=0{'1'}else{'0'}).collect::<String>();
            let (visible,pages)=migrate(Some("6"),Some(&bits),Some("7,6,5,4,3,2,1,0"));
            assert_eq!(&visible[..8],(0..8).map(|page|mask&(1<<page)!=0).collect::<Vec<_>>().as_slice());
            assert!(visible[8]);
            assert_eq!(pages,[7,6,5,4,3,2,1,0,8]);
        }
    }
    #[test]
    fn schema_seven_roundtrips_notification_opt_out_and_all_saved_orders(){
        for mask in 1u32..512{
            let bits=(0..9).map(|page|if mask&(1<<page)!=0{'1'}else{'0'}).collect::<String>();
            let (visible,pages)=migrate(Some("7"),Some(&bits),Some("8,5,4,3,7,2,1,6,0"));
            assert_eq!(visible.as_slice(),(0..9).map(|page|mask&(1<<page)!=0).collect::<Vec<_>>().as_slice());
            assert_eq!(pages,[8,5,4,3,7,2,1,6,0]);
        }
    }
    #[test]
    fn invalid_input_has_a_visible_page_and_unique_order(){
        for schema in [None,Some("1"),Some("2"),Some("3"),Some("4"),Some("5"),Some("6")]{
            assert_eq!(migrate(schema,Some("broken"),Some("999")),([true;crate::app::PAGE_COUNT],[0,1,6,2,3,4,5,7,8]));
        }
        for (schema,bits) in [("3","00000"),("4","000000"),("5","0000000"),("6","00000000")]{
            assert_eq!(migrate(Some(schema),Some(bits),Some("0,0,0,0,0")),([true,false,false,false,false,false,false,false,false],[0,1,6,2,3,4,5,7,8]));
        }
    }
}

#[cfg(test)]
mod data_directory_tests{
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new()->Self {
            static NEXT:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
            let path=std::env::temp_dir().join(format!("qingyu-directory-migration-{}-{}",std::process::id(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));
            fs::create_dir_all(&path).unwrap();Self(path)
        }
        fn old(&self)->PathBuf{self.0.join("Daozai")}
        fn new_path(&self)->PathBuf{self.0.join("Qingyu")}
    }
    impl Drop for Fixture{fn drop(&mut self){let _=fs::remove_dir_all(&self.0);}}
    #[test]
    fn explicit_data_paths_prioritize_qingyu_without_moving_legacy_override(){
        let current=Some(std::ffi::OsString::from("current"));let legacy=Some(std::ffi::OsString::from("legacy"));
        assert_eq!(data_dir_override(current,legacy.clone()),Some(PathBuf::from("current")));
        assert_eq!(data_dir_override(Some(std::ffi::OsString::new()),legacy),Some(PathBuf::from("legacy")));
        assert_eq!(data_dir_override(None,Some(std::ffi::OsString::new())),None);
    }
    #[test]
    fn default_migration_preserves_all_files_including_retired_data(){
        let f=Fixture::new();fs::create_dir_all(f.old().join("credentials")).unwrap();
        for (name,body) in [("settings.txt","visible_pages=100100001"),("todos.txt","keep task"),("mail.json","keep mail"),("credentials/retired-calendar.bin","keep old calendar"),("credentials/retired-ai.bin","keep old provider")]{fs::write(f.old().join(name),body).unwrap();}
        let result=resolve_data_dir(f.new_path(),|a,b|fs::rename(a,b)).unwrap();
        assert_eq!(result,f.new_path());assert!(!f.old().exists());
        assert_eq!(fs::read_to_string(result.join("todos.txt")).unwrap(),"keep task");
        assert_eq!(fs::read_to_string(result.join("mail.json")).unwrap(),"keep mail");
        assert_eq!(fs::read_to_string(result.join("credentials/retired-calendar.bin")).unwrap(),"keep old calendar");
        assert_eq!(fs::read_to_string(result.join("credentials/retired-ai.bin")).unwrap(),"keep old provider");
    }
    #[test]
    fn existing_qingyu_directory_is_not_merged_or_overwritten(){
        let f=Fixture::new();fs::create_dir_all(f.old()).unwrap();fs::create_dir_all(f.new_path()).unwrap();
        fs::write(f.old().join("settings.txt"),"old").unwrap();fs::write(f.new_path().join("settings.txt"),"new").unwrap();
        assert_eq!(resolve_data_dir(f.new_path(),|_,_|panic!("existing directory must not be renamed")).unwrap(),f.new_path());
        assert_eq!(fs::read_to_string(f.old().join("settings.txt")).unwrap(),"old");
        assert_eq!(fs::read_to_string(f.new_path().join("settings.txt")).unwrap(),"new");
    }
    #[test]
    fn failed_migration_keeps_old_path_and_files_available(){
        let f=Fixture::new();fs::create_dir_all(f.old()).unwrap();fs::write(f.old().join("todos.txt"),"retained").unwrap();
        let (fallback,error)=resolve_data_dir(f.new_path(),|_,_|Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied,"injected"))).unwrap_err();
        assert_eq!(fallback,f.old());assert_eq!(error.kind(),std::io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read_to_string(fallback.join("todos.txt")).unwrap(),"retained");assert!(!f.new_path().exists());
    }
    #[test]
    fn another_process_completing_the_same_move_uses_qingyu(){
        let f=Fixture::new();fs::create_dir_all(f.old()).unwrap();
        let result=resolve_data_dir(f.new_path(),|a,b|{fs::rename(a,b)?;Err(std::io::Error::new(std::io::ErrorKind::NotFound,"already moved"))}).unwrap();
        assert_eq!(result,f.new_path());assert!(result.is_dir());
    }
}

#[derive(Clone, Debug, Default)]
pub struct TodoItem {
    pub text: String,
    /// 一次性待办：完成日期；每周重复：最近一次完成的日期（只算当天）。YYYY-MM-DD
    pub done_on: Option<String>,
    /// 本地日期与时间：YYYY-MM-DD HH:MM；每周重复只用其中的时间。
    pub due: Option<String>,
    pub reminded: bool,
    /// 每周重复：bit0 = 周一 … bit6 = 周日；0 = 不重复（一次性待办）。
    pub repeat: u8,
    /// 每周重复：当天已提醒的日期，第二天自动重新提醒。
    pub reminded_on: Option<String>,
}

pub const WEEKDAY_NAMES: [&str; 7] = ["一", "二", "三", "四", "五", "六", "日"];
pub const REPEAT_ALL: u8 = 0b111_1111;
pub const REPEAT_WORKDAYS: u8 = 0b001_1111;
pub const REPEAT_WEEKEND: u8 = 0b110_0000;

fn weekday_bit(date: &str) -> Option<u8> {
    use chrono::Datelike;
    let day = chrono::NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d").ok()?;
    Some(1 << day.weekday().num_days_from_monday())
}

/// 「每天」「工作日」「周末」「每周一、三、五」。
pub fn repeat_label(mask: u8) -> String {
    match mask & REPEAT_ALL {
        0 => "不重复".into(),
        REPEAT_ALL => "每天".into(),
        REPEAT_WORKDAYS => "工作日".into(),
        REPEAT_WEEKEND => "周末".into(),
        mask => {
            let days: Vec<&str> = (0..7).filter(|i| mask & (1 << i) != 0).map(|i| WEEKDAY_NAMES[i]).collect();
            format!("每周{}", days.join("、"))
        }
    }
}

/// 列表里的短标签：「每天」「工作日」「周末」「周一三五」。
pub fn repeat_tag(mask: u8) -> String {
    match mask & REPEAT_ALL {
        0 => String::new(),
        REPEAT_ALL => "每天".into(),
        REPEAT_WORKDAYS => "工作日".into(),
        REPEAT_WEEKEND => "周末".into(),
        mask => format!("周{}", (0..7).filter(|i| mask & (1 << i) != 0).map(|i| WEEKDAY_NAMES[i]).collect::<String>()),
    }
}

impl TodoItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), ..Self::default() }
    }

    pub fn is_repeating(&self) -> bool { self.repeat & REPEAT_ALL != 0 }

    /// 这一天是否要做：一次性待办每天都显示；重复待办只在选中的星期几显示。
    pub fn scheduled_on(&self, today: &str) -> bool {
        !self.is_repeating() || weekday_bit(today).is_some_and(|bit| self.repeat & bit != 0)
    }

    /// 今天是否已完成：重复待办第二天自动变回未完成。
    pub fn done_for(&self, today: &str) -> bool {
        if self.is_repeating() { self.done_on.as_deref() == Some(today) } else { self.done_on.is_some() }
    }

    /// 勾选 / 取消勾选。
    pub fn toggle(&mut self, today: &str) {
        self.done_on = if self.done_for(today) { None } else { Some(today.to_string()) };
    }

    /// `now` 为 `YYYY-MM-DD HH:MM`。一次性待办按完整时间提醒一次；
    /// 重复待办在每个选中的日子到点提醒一次。
    pub fn reminder_due(&self, now: &str) -> bool {
        let Some(due) = self.due.as_deref() else { return false; };
        let today = now.get(..10).unwrap_or("");
        if self.done_for(today) { return false; }
        if !self.is_repeating() { return !self.reminded && due <= now; }
        let (Some(at), Some(clock)) = (due.get(11..16), now.get(11..16)) else { return false; };
        self.scheduled_on(today) && self.reminded_on.as_deref() != Some(today) && at <= clock
    }

    pub fn mark_reminded(&mut self, now: &str) {
        if self.is_repeating() { self.reminded_on = now.get(..10).map(str::to_string); } else { self.reminded = true; }
    }

    /// 悬停提示里的提醒说明。
    pub fn reminder_text(&self) -> Option<String> {
        let due = self.due.as_deref()?;
        Some(if self.is_repeating() { format!("{} {}", repeat_label(self.repeat), due.get(11..16).unwrap_or(due)) } else { due.to_string() })
    }
}

#[derive(Clone, Debug, Default)]
pub struct TodoList {
    pub items: Vec<TodoItem>,
}

impl TodoList {
    pub fn load() -> Self {
        fs::read_to_string(config_dir().join("todo.txt")).map(|text| Self::parse(&text)).unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut list = Self::default();
        {
            for line in text.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                // 格式：<x 或 o>|<完成日期或 ->|正文；也接受纯文本行（视为未完成）
                if !line.contains('|') {
                    list.items.push(TodoItem::new(line.trim()));
                    continue;
                }
                // 每周重复的标记写成 `r<星期位>`（旧版本读到会当作未完成的普通待办）。
                let mut parts = line.splitn(3, '|');
                let flag = parts.next().unwrap_or("o");
                let when = parts.next().unwrap_or("-");
                let raw = parts.next().unwrap_or("").trim();
                let repeat = flag.strip_prefix('r').and_then(|bits| bits.parse::<u8>().ok()).unwrap_or(0) & REPEAT_ALL;
                let (due, reminded_field, text) = if let Some(rest) = raw.strip_prefix("@due:") {
                    let mut fields = rest.splitn(3, '|');
                    let due = fields.next().unwrap_or("").to_string();
                    let reminded = fields.next().unwrap_or("0").to_string();
                    (Some(due), reminded, fields.next().unwrap_or("").to_string())
                } else { (None, "0".to_string(), raw.to_string()) };
                if text.is_empty() {
                    continue;
                }
                let done = (flag == "x" || repeat != 0) && when != "-";
                list.items.push(TodoItem {
                    text, due, repeat,
                    reminded: repeat == 0 && reminded_field == "1",
                    reminded_on: (repeat != 0 && reminded_field.len() == 10).then_some(reminded_field),
                    done_on: done.then(|| when.to_string()),
                });
            }
        }
        list
    }

    pub fn save(&self) {
        let body = self.serialize();
        let path = config_dir().join("todo.txt");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = atomic_write(&path, body.as_bytes());
    }

    pub fn serialize(&self) -> String {
        self
            .items
            .iter()
            .map(|i| {
                let state = if i.is_repeating() {
                    format!("r{}|{}", i.repeat & REPEAT_ALL, i.done_on.as_deref().unwrap_or("-"))
                } else {
                    i.done_on.as_ref().map(|d| format!("x|{d}")).unwrap_or_else(|| "o|-".into())
                };
                let text = i.text.replace(['\n', '\r'], " ");
                let reminded = if i.is_repeating() { i.reminded_on.clone().unwrap_or_else(|| "0".into()) } else { (i.reminded as u8).to_string() };
                match &i.due {
                    Some(due) => format!("{state}|@due:{due}|{reminded}|{text}\n"),
                    None => format!("{state}|{text}\n"),
                }
            })
            .collect()
    }

    pub fn done_today(&self, today: &str) -> usize {
        self.items
            .iter()
            .filter(|i| i.done_on.as_deref() == Some(today))
            .count()
    }

    /// 今天要做的待办（不在今天的重复待办除外）中，(已完成, 总数)。
    pub fn progress(&self, today: &str) -> (usize, usize) {
        let scheduled = self.items.iter().filter(|i| i.scheduled_on(today));
        let (mut done, mut total) = (0, 0);
        for item in scheduled { total += 1; if item.done_for(today) { done += 1; } }
        (done, total)
    }

    /// 今天未完成在前，其他日期的未完成其次，已完成沉底；原始存储顺序不变。
    pub fn display_order(&self, today: &str) -> Vec<usize> {
        let mut order:Vec<_>=(0..self.items.len()).collect();
        order.sort_by_key(|&i|{let item=&self.items[i];(item.done_for(today),!item.scheduled_on(today))});
        order
    }
}

#[derive(Clone, Debug)]
pub struct Water {
    pub bottle_ml: u32,
    pub goal_ml: u32,
    pub start_day: String,
    pub drunk_ml: u32,
    pub refills: u32,
    /// 每小时饮水量（ml），给饮水页的柱状图用。
    pub hours: [u32; 24],
    /// 定时提醒开关。
    pub remind: bool,
}

impl Default for Water {
    fn default() -> Self {
        Self { bottle_ml: 2000, goal_ml: 2000, start_day: String::new(), drunk_ml: 0, refills: 0, hours: [0; 24], remind: true }
    }
}

impl Water {
    /// `water.txt` 只写原工具已知的三个键；复刻版新增的进度键写到 `water-extra.txt`，
    /// 避免给原工具的文件引入未知键。
    pub fn load(today: &str) -> Self {
        let mut w = Self::default();
        for (k, v) in read_kv(&config_dir().join("water.txt")) {
            match k.as_str() {
                "bottle" => w.bottle_ml = v.parse::<f32>().map(|x| x as u32).unwrap_or(2000),
                "goal" => w.goal_ml = v.parse().unwrap_or(2000),
                "start" => w.start_day = v,
                _ => {}
            }
        }
        for (k, v) in read_kv(&config_dir().join("water-extra.txt")) {
            match k.as_str() {
                "drunk" => w.drunk_ml = v.parse().unwrap_or(0),
                "refills" => w.refills = v.parse().unwrap_or(0),
                "remind" => w.remind = v != "0",
                "hours" => {
                    for (i, x) in v.split(',').take(24).enumerate() {
                        w.hours[i] = x.trim().parse().unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
        w.goal_ml=w.goal_ml.clamp(500,6000);
        w.bottle_ml=w.bottle_ml.clamp(50,6000);
        if w.start_day != today {
            // 跨天重置，与原工具 start= 字段语义一致
            w.start_day = today.to_string();
            w.drunk_ml = 0;
            w.refills = 0;
            w.hours = [0; 24];
            w.save();
        }
        w
    }

    pub fn save(&self) {
        let _ = write_kv(
            &config_dir().join("water.txt"),
            &[
                ("bottle".into(), self.bottle_ml.to_string()),
                ("goal".into(), self.goal_ml.to_string()),
                ("start".into(), self.start_day.clone()),
            ],
        );
        let _ = write_kv(
            &config_dir().join("water-extra.txt"),
            &[
                ("drunk".into(), self.drunk_ml.to_string()),
                ("refills".into(), self.refills.to_string()),
                ("remind".into(), if self.remind { "1".into() } else { "0".into() }),
                (
                    "hours".into(),
                    self.hours.iter().map(|h| h.to_string()).collect::<Vec<_>>().join(","),
                ),
            ],
        );
    }

    /// 喝一口：计入总量与当前小时。
    pub fn drink(&mut self, ml: u32, hour: usize) {
        let total=self.drunk_ml.saturating_add(ml).min(self.goal_ml.max(ml).saturating_mul(4));
        let added=total.saturating_sub(self.drunk_ml);
        self.drunk_ml=total;
        self.hours[hour.min(23)]=self.hours[hour.min(23)].saturating_add(added);
        self.save();
    }

    pub fn percent(&self) -> f32 {
        if self.goal_ml == 0 {
            0.0
        } else {
            (self.drunk_ml as f32 / self.goal_ml as f32 * 100.0).min(100.0)
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Shelf {
    pub items: Vec<PathBuf>,
}

impl Shelf {
    pub fn load() -> Self {
        let mut s = Self::default();
        if let Ok(text) = fs::read_to_string(config_dir().join("shelf.txt")) {
            for line in text.lines() {
                let p = PathBuf::from(line.trim());
                if !line.trim().is_empty() && !s.items.contains(&p) {
                    s.items.push(p);
                }
            }
        }
        s
    }

    pub fn save(&self) {
        let body: String = self
            .items
            .iter()
            .map(|p| format!("{}\n", p.display()))
            .collect();
        let path = config_dir().join("shelf.txt");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = atomic_write(&path, body.as_bytes());
    }

    /// 原工具的做法：记录文件所在位置，不复制文件本体（官网说明
    /// "the shelf remembers where your files live"）。
    pub fn add(&mut self, src: &Path) -> std::io::Result<()> {
        if !src.exists() { return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "文件不存在")); }
        if !self.items.iter().any(|p| p == src) {
            self.items.push(src.to_path_buf());
            self.save();
        }
        Ok(())
    }

    pub fn remove(&mut self, p: &Path) {
        self.items.retain(|item| item != p);
        self.save();
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.save();
    }
}

pub fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB {
        format!("{:.1} GB", b / (KB * KB * KB))
    } else if b >= KB * KB {
        format!("{:.1} MB", b / (KB * KB))
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_appearance_values_are_normalized() {
        assert_eq!(island_size_value(100), 100);
        assert_eq!(island_size_value(10), ISLAND_SIZE_MIN);
        assert_eq!(island_size_value(999), ISLAND_SIZE_MAX);
        assert_eq!(island_size_value(113), 115);
        assert_eq!(island_size_value(112), 110);
        for skin in Skin::ALL { assert_eq!(Skin::parse(skin.key()), skin); }
        assert_eq!(Skin::parse("unknown"), Skin::Classic);
        assert_eq!(Skin::parse("glass"), Skin::GlassDark, "旧版本保存的毛玻璃即深色毛玻璃");
        for shape in IslandShape::ALL { assert_eq!(IslandShape::parse(shape.key()), shape); }
        assert_eq!(Settings::default().island_shape, IslandShape::Notch);
        let mut s = Settings::default();
        assert_eq!((s.skin, s.glass_blur, s.island_scale(), s.island_offset_ratio()), (Skin::Classic, true, 1.0, 0.0));
        s.island_size = 125; s.island_offset = -250;
        assert_eq!((s.island_scale(), s.island_offset_ratio()), (1.25, -1.0));
    }

    #[test]
    fn quality_save_preserves_neighbor_and_cleans_failed_writes() {
        let dir=std::env::temp_dir().join(format!("qingyu-quality-save-{}",std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path=dir.join("settings.txt");let neighbor=dir.join("settings.tmp");
        fs::write(&neighbor,b"other data").unwrap();
        let saved=atomic_write(&path,b"settings");
        let untouched=fs::read(&neighbor).ok()==Some(b"other data".to_vec());
        let blocked=dir.join("blocked.txt");fs::create_dir_all(&blocked).unwrap();
        assert!(atomic_write(&blocked,b"cannot replace directory").is_err());
        let leftovers=fs::read_dir(&dir).unwrap().flatten().filter(|e|e.path()!=path && e.path()!=neighbor && e.path()!=blocked).count();
        let failed_tmp=blocked.with_extension("tmp").exists();
        for entry in fs::read_dir(&dir).unwrap().flatten(){if entry.file_type().unwrap().is_file(){fs::remove_file(entry.path()).unwrap();}}
        fs::remove_dir(blocked).unwrap();fs::remove_dir(dir).unwrap();
        assert!(saved.is_ok() && untouched,"保存不能覆盖或移走同名 .tmp 文件");
        assert!(!failed_tmp && leftovers==0,"失败的保存应清除本次临时文件");
    }

    #[test]
    fn atomic_write_replaces_existing_settings() {
        let dir=std::env::temp_dir().join(format!("qingyu-save-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&dir).unwrap();
        let path=dir.join("settings.txt");
        atomic_write(&path,b"focus_minutes=25\n").unwrap();
        atomic_write(&path,b"focus_minutes=26\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(),b"focus_minutes=26\n");
        assert!(!path.with_extension("tmp").exists(),"保存后不应遗留临时配置");
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn quality_concurrent_saves_publish_one_complete_value() {
        let dir=std::env::temp_dir().join(format!("qingyu-quality-concurrent-{}",std::process::id()));fs::create_dir_all(&dir).unwrap();
        let path=dir.join("cache.lrc");let gate=std::sync::Arc::new(std::sync::Barrier::new(4));
        let workers:Vec<_>=(0..4u8).map(|byte|{let path=path.clone();let gate=gate.clone();std::thread::spawn(move||{gate.wait();atomic_write(&path,&vec![byte;128*1024])})}).collect();
        for worker in workers{worker.join().unwrap().unwrap();}
        let content=fs::read(&path).unwrap();assert_eq!(content.len(),128*1024);assert!(content.iter().all(|b|*b==content[0]));
        assert_eq!(fs::read_dir(&dir).unwrap().count(),1);fs::remove_file(path).unwrap();fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn weekly_todos_round_trip_and_reset_each_day() {
        // 2026-11-02 是周一。
        let mut weekly = TodoItem::new("周会 | 准备材料");
        weekly.repeat = 0b000_0101; // 周一、周三
        weekly.due = Some("2026-11-02 09:30".into());
        weekly.toggle("2026-11-02");
        weekly.reminded_on = Some("2026-11-02".into());
        let once = TodoItem { done_on: Some("2026-11-01".into()), reminded: true, due: Some("2026-11-01 08:00".into()), ..TodoItem::new("一次性") };
        let list = TodoList { items: vec![weekly.clone(), once.clone()] };
        let loaded = TodoList::parse(&list.serialize());
        assert_eq!(loaded.items.len(), 2);
        let w = &loaded.items[0];
        assert_eq!((w.text.as_str(), w.repeat, w.done_on.as_deref(), w.reminded_on.as_deref()), ("周会 | 准备材料", 0b101, Some("2026-11-02"), Some("2026-11-02")));
        assert!(w.done_for("2026-11-02") && !w.done_for("2026-11-04"), "重复待办第二天自动变回未完成");
        assert!(w.scheduled_on("2026-11-04") && !w.scheduled_on("2026-11-03"));
        assert!(!w.reminder_due("2026-11-02 10:00"), "当天已提醒");
        assert!(!w.reminder_due("2026-11-04 09:00") && w.reminder_due("2026-11-04 09:30"));
        assert!(!w.reminder_due("2026-11-03 10:00"), "不在选中的星期几不提醒");
        let o = &loaded.items[1];
        assert_eq!((o.repeat, o.reminded, o.done_on.as_deref()), (0, true, Some("2026-11-01")));
        assert!(o.done_for("2026-11-09") && o.scheduled_on("2026-11-03"));
        assert_eq!(loaded.progress("2026-11-03"), (1, 1));
        assert_eq!(loaded.progress("2026-11-04"), (1, 2));
        assert_eq!(loaded.display_order("2026-11-03"), vec![0, 1],"已完成事项应排在其他日期的未完成事项后面");
        assert_eq!(repeat_label(REPEAT_WORKDAYS), "工作日");
        assert_eq!(repeat_label(0b101_0101), "每周一、三、五、日");
        assert_eq!(repeat_tag(0b001_0101), "周一三五");
    }

    #[test]
    fn human_size_formats_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536 * 1024), "1.5 MB");
        assert_eq!(human_size(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn water_percent_clamps_at_100() {
        let w = Water { drunk_ml: 1000, ..Water::default() };
        assert!((w.percent() - 50.0).abs() < 0.01);
        let w = Water { drunk_ml: 5000, ..w };
        assert!((w.percent() - 100.0).abs() < 0.01);
    }
}

/// 仅保存应用标识与开关，不保存通知内容。
#[derive(Clone,Debug,Default)]
pub struct NotificationApps {pub items:Vec<(String,String,bool)>}
impl NotificationApps {
    pub fn load()->Self {
        let path=config_dir().join("notification-apps.json");
        let Ok(raw)=fs::read(path) else{return Self::default()};
        let Ok(json)=serde_json::from_slice::<serde_json::Value>(&raw) else{return Self::default()};
        let items=json.as_array().into_iter().flatten().filter_map(|a|Some((a.get("id")?.as_str()?.to_owned(),a.get("name")?.as_str()?.to_owned(),a.get("enabled")?.as_bool()?))).collect();Self{items}
    }
    pub fn save(&self){
        let list:Vec<_>=self.items.iter().map(|(id,name,enabled)|serde_json::json!({"id":id,"name":name,"enabled":enabled})).collect();
        let path=config_dir().join("notification-apps.json");if let Some(parent)=path.parent(){let _=fs::create_dir_all(parent);}if let Ok(data)=serde_json::to_vec(&list){let _=atomic_write(&path,&data);}
    }
    pub fn observe(&mut self,id:&str,name:&str)->bool{
        if let Some(item)=self.items.iter().find(|a|a.0==id){return item.2;}
        let enabled=true;
        self.items.push((id.to_string(),name.to_string(),enabled));self.save();enabled
    }
}

#[cfg(test)]
mod platform_storage_tests {
    use super::*;
    #[test]
    fn macos_storage_is_independent_of_working_directory_and_windows_environment() {
        assert_eq!(default_config_dir("macos", Some(PathBuf::from("/Users/example")), Some("other-platform".into()), None),
            PathBuf::from("/Users/example/Library/Application Support/Qingyu"));
        assert_eq!(default_config_dir("windows", None, Some("roaming".into()), None), PathBuf::from("roaming/Qingyu"));
        assert_eq!(default_config_dir("linux", Some("/home/example".into()), None, Some("/data".into())), PathBuf::from("/data/Qingyu"));
    }
    #[cfg(unix)]
    #[test]
    fn atomic_files_containing_local_user_data_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let path=std::env::temp_dir().join(format!("qingyu-private-write-{}",std::process::id()));
        atomic_write(&path,b"synthetic local data").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,0o600);
        std::fs::remove_file(path).unwrap();
    }
}
