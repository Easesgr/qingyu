//! 配色、字体与字号。
//!
//! 所有应用界面优先使用内置资源圆体：中文、拉丁与数字保持同一圆润风格，
//! 三档字重为常规 / 中等 / 粗。系统字体仅补足少见字符与其他文字。

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ---------- 颜色（原版逐像素取色，少量为审美微调） ----------
pub const RED: Color32 = Color32::from_rgb(242, 48, 70);
pub const RED_TOP: Color32 = Color32::from_rgb(253, 104, 92);
pub const BLUE: Color32 = Color32::from_rgb(77, 194, 196);
pub const BLUE_DEEP: Color32 = Color32::from_rgb(19, 84, 92);
pub const CYAN: Color32 = Color32::from_rgb(152, 226, 226);
pub const SOFT_GREEN: Color32 = Color32::from_rgb(118, 196, 151);
pub const GREEN: Color32 = Color32::from_rgb(48, 209, 88);
/// 健康页「久坐」模式的主色（与喝水的蓝成对）。
pub const SIT_GREEN: Color32 = Color32::from_rgb(70, 200, 125);
pub const CHARGE: Color32 = Color32::from_rgb(33, 235, 84);
pub const WHITE: Color32 = Color32::WHITE;
pub const GRAY: Color32 = Color32::from_rgb(177, 190, 192);
pub const DIM: Color32 = Color32::from_rgb(142, 161, 164);
pub const BTN: Color32 = Color32::from_rgb(30, 43, 47);
pub const BTN_HOVER: Color32 = Color32::from_rgb(43, 60, 64);
pub const CARD: Color32 = Color32::from_rgb(20, 29, 33);
pub const TRACK: Color32 = Color32::from_rgb(35, 48, 52);
pub const ARROW_BG: Color32 = Color32::from_rgb(39, 55, 59);
pub const WATER_TRACK: Color32 = Color32::from_rgb(15, 38, 64);
pub const YELLOW: Color32 = Color32::from_rgb(255, 196, 61);

pub fn alpha_blue(a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(BLUE.r(), BLUE.g(), BLUE.b(), (255.0 * a) as u8)
}

/// 字重档位。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum W {
    Regular,
    Semibold,
    Bold,
}

pub fn font(size: f32, w: W) -> FontId {
    match w {
        W::Regular => FontId::proportional(size),
        W::Semibold => FontId::new(size, FontFamily::Name("semibold".into())),
        W::Bold => FontId::new(size, FontFamily::Name("bold".into())),
    }
}

/// 字号表（逻辑点）。汉字天然比拉丁字母「显大」，中文字号单独一套，
/// 让两种文字的视觉重量一致，而不是硬把墨高压到拉丁大写高度。
pub mod size {
    pub const TITLE: f32 = 14.5; // 页标题（中文）
    pub const DATE: f32 = 11.0; // 顶栏日期
    pub const CLOCK: f32 = 12.5; // 顶栏时钟
    pub const PILL_CLOCK: f32 = 15.5; // 药丸时钟
    pub const HERO: f32 = 29.0; // 大数字（25:00 / 0.1 / 0/0）
    pub const BODY: f32 = 11.5; // 正文、按钮
    pub const SMALL: f32 = 11.0; // 说明
    pub const TINY: f32 = 10.0; // 脚注、坐标轴
}

struct Face {
    path: PathBuf,
    index: u32,
}

fn find(cands: &[(&str, u32)], dirs: &[PathBuf]) -> Option<Face> {
    for (name, index) in cands {
        let p = Path::new(name);
        if p.is_absolute() && p.exists() {
            return Some(Face { path: p.to_path_buf(), index: *index });
        }
        for d in dirs {
            let q = d.join(name);
            if q.exists() {
                return Some(Face { path: q, index: *index });
            }
        }
    }
    None
}

fn font_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(d) = std::env::var("QINGYU_FONT_DIR") {
        v.push(PathBuf::from(d));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            v.push(d.join("fonts"));
        }
    }
    v.push(PathBuf::from("assets/fonts"));
    #[cfg(target_os="macos")]
    if let Ok(entries)=std::fs::read_dir("/System/Library/AssetsV2/com_apple_MobileAsset_Font7") {
        for entry in entries.flatten(){let dir=entry.path().join("AssetData");if dir.join("PingFang.ttc").is_file(){v.push(dir);}}
    }
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    v.push(PathBuf::from(windir).join("Fonts"));
    for d in [
        "/System/Library/Fonts",
        "/System/Library/Fonts/Supplemental",
        "/Library/Fonts",
        "/usr/share/fonts/google-noto-cjk",
        "/usr/share/fonts/opentype/noto",
        "/usr/share/fonts/noto-cjk",
        "/usr/share/fonts/truetype",
    ] {
        v.push(PathBuf::from(d));
    }
    v
}

/// 装载三档字重。每档 = 拉丁字体 + 中文回退 + 韩文回退；缺哪个就跳过，最后兜底 egui 内置字体。
pub fn load_fonts(ctx: &egui::Context) {
    ctx.set_fonts(shared_fonts().clone());
}

fn shared_fonts()->&'static FontDefinitions {
    static FONTS:std::sync::OnceLock<FontDefinitions>=std::sync::OnceLock::new();
    FONTS.get_or_init(build_fonts)
}

#[cfg(any(windows,target_os="macos"))]
struct MappedFont {
    _source:std::fs::File,
    bytes:memmap2::Mmap,
}

#[cfg(any(windows,target_os="macos"))]
impl MappedFont {
    fn open(path:&Path)->std::io::Result<Self>{
        #[cfg(windows)] use std::os::windows::fs::OpenOptionsExt;
        // 保留文件句柄并禁止原地写入；允许系统以替换文件的方式更新字体。
        let mut options=std::fs::OpenOptions::new();options.read(true);
        #[cfg(windows)] options.share_mode(0x1|0x4);
        let source=options.open(path)?;
        let bytes=unsafe{memmap2::Mmap::map(&source)?};
        Ok(Self{_source:source,bytes})
    }
}

fn font_bytes(path:&Path)->Option<&'static [u8]>{
    static CACHE:std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf,&'static [u8]>>>=std::sync::OnceLock::new();
    let mut cache=CACHE.get_or_init(Default::default).lock().unwrap_or_else(|e|e.into_inner());
    if let Some(bytes)=cache.get(path){return Some(bytes);}
    #[cfg(any(windows,target_os="macos"))]
    if let Ok(mapped)=MappedFont::open(path){
        // 所有字体由进程级 OnceLock 持有，映射和只读句柄与排版器同寿命。
        let mapped=Box::leak(Box::new(mapped));let bytes:&'static [u8]=&mapped.bytes;cache.insert(path.into(),bytes);return Some(bytes);
    }
    let bytes:&'static [u8]=Box::leak(std::fs::read(path).ok()?.into_boxed_slice());cache.insert(path.into(),bytes);Some(bytes)
}

fn build_fonts()->FontDefinitions {
    let dirs = font_dirs();
    let latin = [
        find(&[("segoeui.ttf", 0), ("selawk.ttf", 0), ("SFNS.ttf", 0), ("Helvetica.ttc", 0)], &dirs),
        find(&[("seguisb.ttf", 0), ("selawksb.ttf", 0), ("segoeuib.ttf", 0), ("Helvetica.ttc", 1)], &dirs),
        find(&[("segoeuib.ttf", 0), ("selawkb.ttf", 0), ("Helvetica.ttc", 1)], &dirs),
    ];
    let cjk = [
        find(&[("msyh.ttc", 1), ("NotoSansSC-Regular.otf", 0), ("NotoSansCJK-Regular.ttc", 2), ("PingFang.ttc", 3), ("STHeiti Light.ttc", 1)], &dirs),
        find(&[("msyhbd.ttc", 1), ("NotoSansSC-Medium.otf", 0), ("NotoSansCJK-Medium.ttc", 2), ("PingFang.ttc", 7), ("STHeiti Medium.ttc", 1)], &dirs),
        find(&[("msyhbd.ttc", 1), ("NotoSansSC-Bold.otf", 0), ("NotoSansCJK-Bold.ttc", 2), ("PingFang.ttc", 11), ("STHeiti Medium.ttc", 1)], &dirs),
    ];

    let korean = [
        find(&[("malgun.ttf", 0), ("NotoSansKR-Regular.otf", 0), ("NotoSansCJK-Regular.ttc", 1), ("AppleSDGothicNeo.ttc",0)], &dirs),
        find(&[("malgunbd.ttf", 0), ("malgun.ttf", 0), ("NotoSansKR-Bold.otf", 0), ("NotoSansCJK-Bold.ttc", 1), ("AppleSDGothicNeo.ttc",6)], &dirs),
    ];

    let mut fonts = FontDefinitions::default();
    let rounded:[(&str,&'static [u8]);3]=[
        ("qingyu-rounded-regular",include_bytes!("../assets/qingyu/fonts/ResourceHanRoundedCN-Regular.ttf")),
        ("qingyu-rounded-medium",include_bytes!("../assets/qingyu/fonts/ResourceHanRoundedCN-Medium.ttf")),
        ("qingyu-rounded-bold",include_bytes!("../assets/qingyu/fonts/ResourceHanRoundedCN-Bold.ttf")),
    ];
    for (name,bytes) in rounded {
        fonts.font_data.insert(name.to_owned(),Arc::new(FontData::from_static(bytes)));
    }
    let mut cache: Vec<(PathBuf, u32, String)> = Vec::new();
    let mut add = |fonts: &mut FontDefinitions, face: &Option<Face>, key: String| -> Option<String> {
        let face = face.as_ref()?;
        if let Some((_, _, k)) = cache.iter().find(|(p, i, _)| *p == face.path && *i == face.index) {
            return Some(k.clone());
        }
        let mut d = FontData::from_static(font_bytes(&face.path)?);
        d.index = face.index;
        fonts.font_data.insert(key.clone(), Arc::new(d));
        cache.push((face.path.clone(), face.index, key.clone()));
        Some(key)
    };

    let builtin = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let families = [
        FontFamily::Proportional,
        FontFamily::Name("semibold".into()),
        FontFamily::Name("bold".into()),
    ];
    for (i, fam) in families.iter().enumerate() {
        let mut list = vec![rounded[i].0.to_owned()];
        if let Some(k) = add(&mut fonts, &latin[i], format!("latin{i}")) {
            list.push(k);
        }
        if let Some(k) = add(&mut fonts, &cjk[i], format!("cjk{i}")) {
            list.push(k);
        }
        if let Some(k) = add(&mut fonts, &korean[usize::from(i > 0)], format!("korean{i}")) {
            list.push(k);
        }
        list.extend(builtin.iter().cloned());
        fonts.families.insert(fam.clone(), list);
    }
    // TextEdit/code-style widgets share the same rounded identity, too. Numeric
    // alignment in the island is positioned by layout rather than glyph width.
    fonts.families.insert(FontFamily::Monospace,fonts.families[&FontFamily::Proportional].clone());
    fonts
}

/// All windows share Qingyu's transparent AnyiOS-generated brand emblem.
pub fn app_icon()->std::sync::Arc<eframe::egui::IconData>{
    static ICON:std::sync::OnceLock<std::sync::Arc<eframe::egui::IconData>>=std::sync::OnceLock::new();
    ICON.get_or_init(||{
        let image=image::load_from_memory(include_bytes!("../assets/qingyu/app-icon.png")).expect("轻屿图标资源应有效").thumbnail(128,128).to_rgba8();
        std::sync::Arc::new(eframe::egui::IconData{width:image.width(),height:image.height(),rgba:image.into_raw()})
    }).clone()
}

/// Sidebar artwork is decoded once per context; lettering follows the native
/// foreground color while the two-plane emblem retains its generated colors.
pub(crate) fn brand_art(ctx:&egui::Context,wordmark:bool)->egui::TextureHandle {
    let id=egui::Id::new(("qingyu-transparent-brand-v2",wordmark));
    let texture=ctx.data(|data|data.get_temp::<egui::TextureHandle>(id)).unwrap_or_else(||{
        let bytes:&[u8]=if wordmark{include_bytes!("../assets/qingyu/brand-wordmark.png")}else{include_bytes!("../assets/qingyu/brand-logo.png")};
        let image=image::load_from_memory(bytes).expect("透明品牌资源应有效").into_rgba8();
        let texture=ctx.load_texture(if wordmark{"qingyu-artistic-wordmark"}else{"qingyu-transparent-logo"},egui::ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize],image.as_raw()),egui::TextureOptions::LINEAR);
        ctx.data_mut(|data|data.insert_temp(id,texture.clone()));texture
    });
    if wordmark{crate::skin_paint::mark_monochrome(texture.id());}else{crate::skin_paint::mark_colored(texture.id());}
    texture
}

/// An original generated glyph, cached per egui context and tinted by its caller.
pub fn glyph(ctx:&egui::Context,name:&str)->Option<egui::TextureHandle> {
    let id=egui::Id::new(("qingyu-generated-glyph",name));
    if let Some(texture)=ctx.data(|d|d.get_temp::<egui::TextureHandle>(id)){return Some(texture);}
    let bytes=glyph_bytes(name)?;
    let image=image::load_from_memory(bytes).ok()?.to_rgba8();
    let texture=ctx.load_texture(format!("qingyu-{name}"),egui::ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize],&image),egui::TextureOptions::LINEAR);
    crate::skin_paint::mark_monochrome(texture.id());
    ctx.data_mut(|d|d.insert_temp(id,texture.clone()));Some(texture)
}

pub(crate) fn music_placeholder(ctx:&egui::Context)->egui::TextureHandle{
    let id=egui::Id::new("portrait-music-placeholder");
    let texture=ctx.data(|data|data.get_temp::<egui::TextureHandle>(id)).unwrap_or_else(||{
        let image=image::load_from_memory(include_bytes!("../assets/qingyu/music-placeholder.png")).expect("bundled music artwork").into_rgba8();
        let texture=ctx.load_texture("music-placeholder",egui::ColorImage::from_rgba_unmultiplied([image.width() as usize,image.height() as usize],image.as_raw()),egui::TextureOptions::LINEAR);
        ctx.data_mut(|data|data.insert_temp(id,texture.clone()));texture
    });
    crate::skin_paint::mark_colored(texture.id());texture
}

/// Avoid decoding the first page's PNGs on an expansion frame. Keeping this
/// bounded to navigation/music assets also keeps idle texture use predictable.
pub(crate) fn prepare_panel_assets(ctx:&egui::Context){
    music_placeholder(ctx);
    for name in ["music","shelf","clipboard","todo","link","mail","weather","agent","settings","expand","play","pause","previous","next","volume","mute"]{glyph(ctx,name);}
}

/// Original PNG coverage for screen-size-specific icon caches.
pub fn glyph_bytes(name:&str)->Option<&'static [u8]> {
    macro_rules! asset {($name:literal)=>{include_bytes!(concat!("../assets/qingyu/glyphs/",$name,".png")) as &[u8]};}
    Some(match name {
        "music"=>asset!("music"),"shelf"=>asset!("shelf"),"todo"=>asset!("todo"),"mail"=>asset!("mail"),
        "weather"=>asset!("weather"),"agent"=>asset!("agent"),"settings"=>asset!("settings"),
        "pages"=>asset!("pages"),"placement"=>asset!("placement"),"play"=>asset!("play"),"pause"=>asset!("pause"),
        "previous"=>asset!("previous"),"next"=>asset!("next"),"volume"=>asset!("volume"),"mute"=>asset!("mute"),
        "close"=>asset!("close"),"left"=>asset!("left"),"right"=>asset!("right"),"plus"=>asset!("plus"),
        "check"=>asset!("check"),"pencil"=>asset!("pencil"),"refresh"=>asset!("refresh"),"pin"=>asset!("pin"),
        "clipboard"=>asset!("clipboard"),"search"=>asset!("search"),"folder"=>asset!("folder"),"file"=>asset!("file"),
        "link"=>asset!("link"),"trash"=>asset!("trash"),"download"=>asset!("download"),"expand"=>asset!("expand"),
        "sun"=>asset!("sun"),"moon"=>asset!("moon"),"sun-cloud"=>asset!("sun-cloud"),"moon-cloud"=>asset!("moon-cloud"),
        "cloud"=>asset!("cloud"),"overcast"=>asset!("overcast"),"rain"=>asset!("rain"),"drizzle"=>asset!("drizzle"),
        "snow"=>asset!("snow"),"thunder"=>asset!("thunder"),"fog"=>asset!("fog"),"wind"=>asset!("wind"),
        "temperature"=>asset!("temperature"),"humidity"=>asset!("humidity"),"clock"=>asset!("clock"),"bell"=>asset!("bell"),_=>return None,
    })

}

#[cfg(test)]
mod tests {
    #[test]
    fn rounded_interface_fonts_cover_chinese_latin_numbers_and_controls(){
        let ctx=eframe::egui::Context::default();super::load_fonts(&ctx);
        let _=ctx.run(Default::default(),|ctx|{
            for font in [super::font(13.0,super::W::Regular),super::font(13.0,super::W::Semibold),super::font(13.0,super::W::Bold),eframe::egui::FontId::monospace(13.0)]{
                for ch in "轻屿聆听暂存架剪贴簿拾光信箱晴雨智能体本机概览消息通知查看关闭执行等待CPUabcdefghijklmnopqrstuvwxyz0123456789%℃↑↓⌘".chars(){
                    assert!(ctx.fonts(|fonts|fonts.has_glyph(&font,ch)),"missing rounded interface glyph {ch}");
                }
            }
        });
    }

    #[cfg(windows)]
    #[test]
    fn korean_titles_and_lyrics_have_glyphs_in_all_weights(){
        let ctx=eframe::egui::Context::default();
        super::load_fonts(&ctx);
        let _=ctx.run(Default::default(),|ctx|{
            for weight in [super::W::Regular,super::W::Semibold,super::W::Bold]{
                let font=super::font(14.0,weight);
                for ch in "눈, 코, 입 향기로운 숨을 담은 너의 코 사랑해 眼鼻嘴 TAEYANG ㄱㄴㅏ".chars().filter(|c|!c.is_whitespace()){
                    assert!(ctx.fonts(|f|f.has_glyph(&font,ch)),"缺少字形：{ch}，字体：{font:?}");
                }
            }
        });
    }

    #[cfg(windows)]
    #[test]
    fn mapped_fonts_prevent_in_place_writes_and_keep_shared_reads(){
        let path=std::env::temp_dir().join(format!("qingyu-font-map-{}-{}.bin",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::write(&path,b"readonly font fixture").unwrap();
        let mapped=super::MappedFont::open(&path).unwrap();
        assert_eq!(&mapped.bytes[..],b"readonly font fixture");
        assert_eq!(std::fs::read(&path).unwrap(),b"readonly font fixture");
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err(),"映射存活期间不能允许原地改写字体");
        drop(mapped);std::fs::remove_file(path).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn system_font_bytes_use_read_only_file_mapping(){
        #[link(name="kernel32")]
        unsafe extern "system"{
            fn K32GetMappedFileNameW(process:*mut core::ffi::c_void,address:*const core::ffi::c_void,buffer:*mut u16,size:u32)->u32;
        }
        let fonts=super::shared_fonts();let font=&fonts.font_data["cjk0"];
        let mut name=[0u16;1024];
        let count=unsafe{K32GetMappedFileNameW(windows::Win32::System::Threading::GetCurrentProcess().0,font.font.as_ptr().cast(),name.as_mut_ptr(),name.len()as u32)};
        assert!(count>0,"中文字体仍占用完整的私有堆副本，没有复用系统字体文件页");
        let name=String::from_utf16_lossy(&name[..count as usize]).to_lowercase();
        assert!([".ttc",".ttf",".otf"].iter().any(|suffix|name.ends_with(suffix)),"映射必须来自实际字体文件");
    }

    #[test]
    fn font_data_is_shared_across_contexts_without_owned_byte_copies(){
        let fonts=super::shared_fonts();
        assert!(fonts.font_data.values().all(|font|matches!(font.font,std::borrow::Cow::Borrowed(_))),"字体不能在每个排版上下文中产生完整副本");
        let cloned=fonts.clone();
        for (name,font) in &fonts.font_data {assert!(std::sync::Arc::ptr_eq(font,&cloned.font_data[name]));}
    }
}
