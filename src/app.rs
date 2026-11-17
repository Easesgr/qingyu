//! 灵动岛主体：状态、悬停形变、切页、顶栏与公共层。与平台无关——
//! Windows 外壳（`main.rs`）和无窗口录制器（`qingyu-record`）共用同一份代码。
//!
//! 时间一律来自 `Env::t`（秒），不直接读系统时钟，所以录屏可逐帧复现。

use crate::extras::{Alert, AlertSource};
use crate::anim::{self, lerp, smoothstep, Spring};
use crate::bar_layout::{self as bar, Layout};
use crate::glass;
use crate::lyrics;
use crate::mail;
use crate::model;
use crate::paint::{self, Card, H};
use crate::shape;
use crate::smtc::{NowPlaying, Transport};
use crate::theme::{self, size, W};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use std::collections::HashMap;
use std::path::PathBuf;

// ---------- 几何（逻辑点；原版 125% 截图 ÷ 1.25） ----------
pub const PW: f32 = 419.0; // 展开面板宽
pub const PH: f32 = 140.0; // 展开面板高
pub const MUSIC_PAGE: usize = 0;
pub const SHELF_PAGE: usize = 1;
pub const LIBRARY_PAGE: usize = 2;
/// 保持原页面编号与已有设置兼容，提醒清单现在属于拾光。
pub const TODO_PAGE: usize = LIBRARY_PAGE;
pub const MAIL_PAGE: usize = 3;
pub const WEATHER_PAGE: usize = 4;
pub const AGENT_PAGE: usize = 5;
pub const CLIPBOARD_PAGE: usize = 6;
pub const MAC_STATS_PAGE: usize = 7;
pub const NOTIFICATIONS_PAGE: usize = 8;
pub const PAGE_COUNT: usize = 9;
pub(crate) const WEATHER_H: f32 = 232.0;
pub const BAR_W: f32 = 163.0; // 收起药丸宽
pub const BAR_H: f32 = 30.0; // 收起药丸高
pub(crate) const LYRIC_BAR_W: f32 = 300.0;
pub const BAR_W_MIN: f32 = 44.0; // 极简小条
pub const BAR_H_MIN: f32 = 4.5;
pub const FLARE: f32 = 17.6; // 顶部外翻肩宽
pub const FLARE_H: f32 = 16.0; // 外翻收敛高度
pub const RAD: f32 = 33.0; // 底部圆角
pub const WIN_W: f32 = 640.0; // 窗口（透明画布）
pub const CANVAS_W: f32 = WIN_W + 320.0; // 保留右侧通知及阴影的完整空间，原岛仍按 WIN_W 定位。
/// Transparent canvas includes a reserved lane below the stable content panel.
pub const WIN_H: f32 = if cfg!(target_os="macos"){404.0}else{368.0};
/// 胶囊皮肤与屏幕顶边的间隙。
pub const PILL_GAP: f32 = 6.0;

/// 胶囊的柔和投影：四周都有，多层叠加近似高斯。
fn pill_shadow(painter:&egui::Painter,rect:Rect,radius:f32,strength:f32){
    if strength<=0.01 {return;}
    for (grow,a) in [(10.0f32,0.035f32),(6.5,0.05),(3.5,0.07),(1.5,0.09)] {
        let r=Rect::from_min_max(rect.min-Vec2::new(grow*0.6,grow*0.3),rect.max+Vec2::new(grow*0.6,grow));
        painter.rect_filled(r,radius+grow*0.8,Color32::from_black_alpha((255.0*a*strength) as u8));
    }
}
pub(crate) const PAGE_NAMES: [&str; PAGE_COUNT] = ["音乐", "置物架", "拾光", "邮箱", "天气", "Agent", "粘贴板", "Mac 状态", "通知"];
pub(crate) const PAGE_SETTINGS_TABS: [usize; PAGE_COUNT] = [1,5,4,7,12,13,14,15,16];
pub(crate) const PAGE_GLYPHS: [&str; PAGE_COUNT] = ["music","shelf","link","mail","weather","agent","clipboard","mac-stats","bell"];

// ---------- 动效参数 ----------
/// 悬停意图缓冲：光标停留这么久才真正展开（期间药丸轻微胀大作为反馈），防误触。
const HOVER_INTENT: f64 = 0.20;
const MINIMAL_HOVER_INTENT: f64 = 0.40;
/// 移开后等待多久开始收起；这段时间内回来不会收。
const LEAVE_DELAY: f64 = 0.50;
const SWELL: (f32, f32) = (1.075, 1.13);
// 展开：欠阻尼，末段约 5% 过冲后回落；高度略软于宽度，像「垂落」下来。
const OPEN_W: (f32, f32) = (330.0, 0.70);
const OPEN_H: (f32, f32) = (290.0, 0.74);
// 收起：临界阻尼，约 0.18 s 干净收回，无回弹。
const CLOSE: (f32, f32) = (620.0, 1.0);
const SWELL_K: (f32, f32) = (420.0, 0.8);
const PAGE_SLIDE: f32 = 46.0;

pub struct Options {
    /// 是否联网查歌词（录制/测试时关掉）。
    pub fetch_lyrics: bool,
    /// 本机 Agent 后台监测；录制与界面测试关闭，与音乐资源保护独立。
    pub monitor_agents: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { fetch_lyrics: true, monitor_agents: true }
    }
}

/// 每帧由外壳提供的外部世界。
#[derive(Clone, Default)]
pub struct Env {
    pub t: f64,
    /// 外壳提供的 Unix 秒；录制与测试可指定固定值，界面不读取系统时钟。
    pub unix_time: u64,
    /// Millisecond clock for rejecting delayed task events after privacy pauses.
    /// Zero keeps older deterministic fixtures compatible with `unix_time`.
    pub unix_ms: u64,
    /// `HH:MM:SS`
    pub clock: String,
    /// `YYYY-MM-DD`
    pub today: String,
    /// 顶栏日期，例如 `10月28日 周三`
    pub date: String,
    pub battery: Option<u8>,
    pub charging: bool,
    /// 互联网连接状态；未知时不触发表情提醒。
    pub network: Option<bool>,
    /// 光标位置（与 `frame` 传入的 `win` 同一坐标系）；不在窗口附近时为 None。
    pub cursor: Option<Pos2>,
    /// macOS 拖动时的全局 AppKit 坐标与显示器清单。普通界面命中仍使用 `cursor`。
    pub drag_geometry: Option<DragGeometry>,
    pub idle_seconds: Option<f64>,
    /// 当前会话是否处于锁屏（Win + L、自动锁屏、唤醒后的登录界面）。
    pub screen_locked: bool,
    /// 前台浏览器的顶部是否与岛重叠；轻屿自身窗口或暂不可读时为 None。
    pub browser_at_top: Option<bool>,
    pub media: NowPlaying,
    /// 系统主音量（0..100, 是否静音）；v0.5.4 音量条与系统双向同步。
    pub volume: Option<(u32, bool)>,

    /// 岛所在屏幕（与 `win` 同一坐标系，逻辑点）；用于让通知详情留在屏幕内。
    pub screen: Option<Rect>,
    /// 毛玻璃皮肤：岛体四周采样得到的背景色（sRGB 0..1）。
    pub backdrop: Option<[f32; 3]>,
    /// 毛玻璃皮肤：系统背景模糊当前是否生效。
    pub glass_blur: bool,
    /// 不支持逐像素透明的兼容后端需要按内容轮廓裁剪窗口。
    pub opaque_surface: bool,
}

#[derive(Clone, Debug)]
pub struct DragGeometry {
    pub cursor: Pos2,
    pub displays: Vec<crate::display_layout::Display>,
    /// 每屏自由放置工作区，与原生定位共用；包含菜单栏，但避开 Dock。
    pub rooms: Vec<(String,crate::display_layout::Bounds)>,
    /// egui 根坐标每个 AppKit 点对应的比例，不含 backing scale。
    pub scale: f32,
}

impl Env {
    pub fn hour(&self) -> usize {
        self.clock.get(0..2).and_then(|s| s.parse().ok()).unwrap_or(12)
    }
    pub fn minute(&self) -> u32 {
        self.clock.get(3..5).and_then(|s| s.parse().ok()).unwrap_or(0)
    }
}

/// 交给外壳执行的副作用。
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Media(Transport),
    QqMusic(crate::qq_music::Page),
    QqMusicRefresh,
    QqMusicPlayPlaylist(String),
    QqMusicResume,
    QqMusicStop,
    QqMusicSetPlaying{request:u64,playing:bool},
    Open(PathBuf),
    OpenUrl(String),
    /// 需要键盘输入：外壳应让窗口可激活并取得焦点。
    Focus,
    Reveal(PathBuf),
    Copy(String),
    Notify(String, String),
    ToggleAutostart,
    RequestNotificationAccess,
    OpenNotification {bundle_id:String,activation:String},
    OpenAgent {agent:crate::agent_monitor::AgentKind,session_id:String,pid:Option<u32>,observed_at_ms:Option<u64>},
    AddFiles,
    DragFile(PathBuf),
    /// 把系统主音量设为 0..100（v0.5.4 音量条）。
    SetVolume(u32),
    /// 切换系统静音。
    ToggleMute,
    /// 音量命令携带点击时的播放器身份，避免切换歌曲来源后误调其他应用。
    MusicVolume(Transport,String),
    /// v0.5.5：账号清单变了，让外壳重新同步收信线程。
    MailSync,
    /// 测试某个邮箱的连接（授权码只在内存里传给工作线程）。
    MailTest(mail::Account, String),
    MailManage(crate::mail_manage::Request),
    /// 重启轻屿（切换渲染方式后生效）。
    RestartApp,
}

/// 毛玻璃皮肤下原生窗口区域的组成部分（相对 `win` 左上角，逻辑点）。
/// 系统模糊只出现在窗口区域内，所以区域要贴合岛体轮廓，而不是外接矩形。
#[derive(Clone, Debug, PartialEq)]
pub enum GlassShape {
    /// 任意闭合轮廓。
    Outline(Vec<Pos2>),
    /// 岛体 / 通知胶囊：顶部外翻肩部 + 竖直侧边 + 底部圆角（参数同 `shape::fill`）。
    Notch { rect: Rect, flare: f32, flare_h: f32, radius: f32 },
    /// 停靠在左右边缘的岛体：肩部沿屏幕边缘展开（参数同 `shape::fill_docked`）。
    Docked { rect: Rect, dock: model::Dock, flare: f32, flare_h: f32, radius: f32 },
    /// 圆角矩形（通知详情、提示、浮动确认）。
    Round(Rect, f32),
}

impl GlassShape {
    /// 闭合轮廓（逻辑点）；窗口区域按它裁剪。
    pub fn outline(&self) -> Vec<Pos2> {
        match self {
            Self::Outline(points) => points.clone(),
            Self::Notch { rect, flare, flare_h, radius } => shape::outline(*rect, *flare, *flare_h, *radius),
            Self::Docked { rect, dock, flare, flare_h, radius } => shape::outline_docked(*rect, *dock, *flare, *flare_h, *radius),
            Self::Round(rect, radius) => {
                let radius=radius.max(0.0).min(rect.width().min(rect.height())*0.5);
                let mut points=Vec::with_capacity(52);
                for (center,start) in [
                    (Pos2::new(rect.right()-radius,rect.top()+radius),-std::f32::consts::FRAC_PI_2),
                    (Pos2::new(rect.right()-radius,rect.bottom()-radius),0.0),
                    (Pos2::new(rect.left()+radius,rect.bottom()-radius),std::f32::consts::FRAC_PI_2),
                    (Pos2::new(rect.left()+radius,rect.top()+radius),std::f32::consts::PI),
                ] {
                    for step in 0..=12 {
                        let angle=start+step as f32/12.0*std::f32::consts::FRAC_PI_2;
                        points.push(center+Vec2::new(angle.cos(),angle.sin())*radius);
                    }
                }
                points
            },
        }
    }
}

/// 一块系统模糊：物理像素的窗口坐标与圆角半径（0 = 直角）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlurPiece {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radius: f32,
}

/// 把毛玻璃轮廓拆成合成引擎能直接表达的圆角矩形块：
/// 主体向上多延伸一个圆角半径，让顶部两角落在窗口外，只留下底部圆角；
/// 外翻的肩部按物理像素逐行拼出（相邻同宽的行合并），边缘误差不超过半个像素。
pub fn blur_pieces(shapes: &[GlassShape], ppp: f32) -> Vec<BlurPiece> {
    let mut out = Vec::new();
    for shape in shapes {
        match shape {
            GlassShape::Docked { rect, dock, flare, flare_h, radius } => {
                if rect.width() <= 0.0 || rect.height() <= 0.0 { continue; }
                // Rasterize the very same tangent-continuous side polygon.
                // Adjacent rows with identical extents merge into one native
                // piece; transparent shoulder corners never receive blur.
                out.extend(outline_blur_pieces(&shape::outline_docked(*rect,*dock,*flare,*flare_h,*radius),ppp));
            }
            GlassShape::Notch { rect, flare, flare_h, radius } => {
                if rect.width() <= 0.0 || rect.height() <= 0.0 { continue; }
                let r = radius.min(rect.height() * 0.5).min(rect.width() * 0.5).max(0.0) * ppp;
                let fh = flare_h.min(rect.height() * 0.5).max(0.5) * ppp;
                let (x0, x1, y0, y1) = ((rect.left() * ppp).round(), (rect.right() * ppp).round(), (rect.top() * ppp).round(), (rect.bottom() * ppp).round());
                out.push(BlurPiece { x: x0, y: y0 - r, w: x1 - x0, h: y1 - y0 + r, radius: r });
                let rows = fh.ceil() as i32;
                let mut i = 0;
                while i < rows {
                    let wing = |row: i32| (flare * ppp * (1.0 - ((row as f32 + 0.5) / fh).min(1.0)).powf(2.4)).round();
                    let w = wing(i);
                    if w < 1.0 { break; }
                    let mut h = 1;
                    while i + h < rows && wing(i + h) == w { h += 1; }
                    let y = y0 + i as f32;
                    out.push(BlurPiece { x: x0 - w, y, w, h: h as f32, radius: 0.0 });
                    out.push(BlurPiece { x: x1, y, w, h: h as f32, radius: 0.0 });
                    i += h;
                }
            }
            GlassShape::Round(r, radius) => {
                let (x,y,right,bottom)=((r.left()*ppp).round(),(r.top()*ppp).round(),(r.right()*ppp).round(),(r.bottom()*ppp).round());
                let (w,h)=(right-x,bottom-y);
                if w<=0.0 || h<=0.0 {continue;}
                // 收起动画先按物理边界取整，再限制圆角，避免上下圆弧在中线交叠。
                out.push(BlurPiece { x,y,w,h,radius:(radius*ppp).max(0.0).min(w.min(h)*0.5) });
            }
            GlassShape::Outline(points) => {
                // Preserve the concave fluid shoulders on platforms which
                // express the native blur mask as scanline pieces.
                out.extend(outline_blur_pieces(points,ppp));
            }
        }
    }
    out
}

fn outline_blur_pieces(points:&[Pos2],ppp:f32)->Vec<BlurPiece>{
    if points.len()<3||!ppp.is_finite()||ppp<=0.0{return Vec::new();}
    let bounds=Rect::from_points(points);let mut out:Vec<BlurPiece>=Vec::new();
    for row in (bounds.top()*ppp).floor() as i32..(bounds.bottom()*ppp).ceil() as i32{
        let y=(row as f32+0.5)/ppp;let mut crosses=Vec::new();let mut previous=*points.last().unwrap();
        for &next in points{if (next.y>y)!=(previous.y>y){crosses.push((next.x+(previous.x-next.x)*(y-next.y)/(previous.y-next.y))*ppp);}previous=next;}
        crosses.sort_by(f32::total_cmp);
        for pair in crosses.chunks_exact(2){
            let left=pair[0].round();let right=pair[1].round();if right<=left{continue;}
            if let Some(last)=out.last_mut(){if last.x==left&&last.w==right-left&&last.y+last.h==row as f32{last.h+=1.0;continue;}}
            out.push(BlurPiece{x:left,y:row as f32,w:right-left,h:1.0,radius:0.0});
        }
    }
    out
}

#[cfg(test)]
mod blur_piece_tests {
    use super::*;

    fn covered(pieces: &[BlurPiece], x: f32, y: f32) -> bool {
        pieces.iter().any(|p| {
            if !(x >= p.x && x < p.x + p.w && y >= p.y && y < p.y + p.h) { return false; }
            // 圆角：只判断四角的圆弧外侧
            let r = p.radius;
            let cx = x.clamp(p.x + r, p.x + p.w - r);
            let cy = y.clamp(p.y + r, p.y + p.h - r);
            (x - cx).powi(2) + (y - cy).powi(2) <= r * r + 1e-3
        })
    }

    #[test]
    fn blur_follows_the_island_outline_not_its_bounding_box() {
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            let rect = Rect::from_min_size(Pos2::new(110.0, 0.0), Vec2::new(419.0, 140.0));
            let shape = GlassShape::Notch { rect, flare: 17.6, flare_h: 16.0, radius: 33.0 };
            let pieces = blur_pieces(std::slice::from_ref(&shape), ppp);
            let outline = shape.outline();
            // 轮廓内部的点都被模糊覆盖，外部（含底部圆角外、肩部外）都不覆盖；边缘留 1.5 像素容差。
            let inside = |x: f32, y: f32| {
                let mut c = false;
                let mut prev = *outline.last().unwrap();
                for &next in &outline {
                    if (next.y > y) != (prev.y > y) && x < (prev.x - next.x) * (y - next.y) / (prev.y - next.y) + next.x { c = !c; }
                    prev = next;
                }
                c
            };
            let mut checked = 0;
            for yi in 0..(150.0 * ppp) as i32 {
                for xi in ((80.0 * ppp) as i32..(560.0 * ppp) as i32).step_by(3) {
                    let (x, y) = (xi as f32 + 0.5, yi as f32 + 0.5);
                    let (lx, ly) = (x / ppp, y / ppp);
                    let deep_in = inside(lx, ly) && [(-1.5, 0.0), (1.5, 0.0), (0.0, 1.5), (0.0, -1.5)].iter().all(|(dx, dy)| inside(lx + dx / ppp, (ly + dy / ppp).max(0.01)));
                    let far_out = !inside(lx, ly) && [(-1.5, 0.0), (1.5, 0.0), (0.0, 1.5), (0.0, -1.5)].iter().all(|(dx, dy)| !inside(lx + dx / ppp, ly + dy / ppp));
                    if deep_in { assert!(covered(&pieces, x, y), "{ppp} 缩放下岛体内 ({x},{y}) 没有模糊"); checked += 1; }
                    if far_out && y >= 0.0 { assert!(!covered(&pieces, x, y), "{ppp} 缩放下岛体外 ({x},{y}) 不能出现模糊"); }
                }
            }
            assert!(checked > 1000);
            assert!(pieces.len() < 60, "肩部合并后块数应很少：{}", pieces.len());
        }
    }

    #[test]
    fn side_docked_blur_follows_the_rotated_outline() {
        for dock in [model::Dock::Left, model::Dock::Right] {
            for ppp in [1.0, 1.5] {
                let x = if dock == model::Dock::Left { 0.0 } else { 81.0 };
                let shape = GlassShape::Docked { rect: Rect::from_min_size(Pos2::new(x, 200.0), Vec2::new(419.0, 140.0)), dock, flare: 17.6, flare_h: 16.0, radius: 33.0 };
                let pieces = blur_pieces(std::slice::from_ref(&shape), ppp);
                let outline = shape.outline();
                let inside = |x: f32, y: f32| {
                    let mut c = false;
                    let mut prev = *outline.last().unwrap();
                    for &next in &outline {
                        if (next.y > y) != (prev.y > y) && x < (prev.x - next.x) * (y - next.y) / (prev.y - next.y) + next.x { c = !c; }
                        prev = next;
                    }
                    c
                };
                let mut checked = 0;
                for yi in ((170.0 * ppp) as i32..(370.0 * ppp) as i32).step_by(2) {
                    for xi in (0..(500.0 * ppp) as i32).step_by(3) {
                        let (px, py) = (xi as f32 + 0.5, yi as f32 + 0.5);
                        let (lx, ly) = (px / ppp, py / ppp);
                        let near = [(-1.5, 0.0), (1.5, 0.0), (0.0, 1.5), (0.0, -1.5)];
                        let deep_in = inside(lx, ly) && near.iter().all(|(dx, dy)| inside((lx + dx / ppp).clamp(0.01, 499.99), ly + dy / ppp));
                        let far_out = !inside(lx, ly) && near.iter().all(|(dx, dy)| !inside(lx + dx / ppp, ly + dy / ppp));
                        if deep_in { assert!(covered(&pieces, px, py), "{dock:?} {ppp}: 岛体内 ({px},{py}) 没有模糊"); checked += 1; }
                        let on_canvas = lx > 0.5 && lx < 499.5;
                        if far_out && on_canvas { assert!(!covered(&pieces, px, py), "{dock:?} {ppp}: 岛体外 ({px},{py}) 不能有模糊"); }
                    }
                }
                assert!(checked > 1000);
            }
        }
    }

    #[test]
    fn collapsing_pill_blur_radius_fits_the_physical_bounds() {
        for ppp in [1.0,1.25,1.5,2.0] {
            for height in [30.02,30.3,30.49,30.5,30.8,31.0,31.2,32.1,35.3] {
                let rect=Rect::from_min_size(Pos2::new(230.23,6.0),Vec2::new(179.31,height));
                let radius=15.0+(height-30.0)*0.1;
                let pieces=blur_pieces(&[GlassShape::Round(rect,radius)],ppp);
                assert_eq!(pieces.len(),1);
                let p=pieces[0];
                assert!(p.radius*2.0<=p.w.min(p.h),"收起时圆弧不能超过物理边界：ppp={ppp}, height={height}, piece={p:?}");
                assert_eq!(p.x+p.w,(rect.right()*ppp).round(),"模糊与岛体右边界应使用同一像素位置");
                assert_eq!(p.y+p.h,(rect.bottom()*ppp).round(),"模糊与岛体下边界应使用同一像素位置");
            }
        }
    }

    #[test]
    fn rounded_rectangles_and_empty_shapes() {
        let pieces = blur_pieces(&[GlassShape::Round(Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 40.0)), 10.0)], 1.5);
        assert_eq!(pieces, vec![BlurPiece { x: 15.0, y: 30.0, w: 150.0, h: 60.0, radius: 15.0 }]);
        assert!(blur_pieces(&[GlassShape::Notch { rect: Rect::NOTHING, flare: 5.0, flare_h: 8.0, radius: 12.0 }], 1.0).is_empty());
        // 极简细条：没有肩部，只有一块
        let tiny = blur_pieces(&[GlassShape::Notch { rect: Rect::from_min_size(Pos2::new(298.0, 0.0), Vec2::new(44.0, 4.5)), flare: 0.0, flare_h: 8.0, radius: 2.25 }], 1.25);
        assert_eq!(tiny.len(), 1);
    }
}

/// Native desktop geometry, independent of a saved cosmetic shape preference.
/// The camera bridge is decorative; all content, hit testing and blur begin at
/// `content_top`, below the hardware and system menu bar.
#[derive(Clone,Copy,Debug,Default,PartialEq)]
pub enum DesktopPlacement {
    #[default]
    Legacy,
    Floating {content_top:f32},
    Camera {content_top:f32,bridge:Rect},
    Positioned(crate::island_position::Plan),
}

pub struct FrameOut {
    /// 原生窗口应保留的可见区域（相对 `win` 左上角）。
    pub region: Rect,
    /// 毛玻璃 + 系统模糊时，替代 `region` 的贴合轮廓；其余情况为 None。
    pub glass: Option<Vec<GlassShape>>,
    /// 鼠标命中区（相对 `win` 左上角），区域外点击穿透到下层窗口。
    pub hit: Rect,
    /// Primary painted surface, in the same points as `hit`.
    pub hit_outline:Option<Vec<Pos2>>,
    /// Exact painted surfaces. Horizontal navigation and disclosure share
    /// one continuous outline, including the fluid shoulders.
    pub hit_shapes:Vec<GlassShape>,
    pub notice_hit: Option<Rect>,
    /// Exact, independent floating surfaces. Native input uses these contours
    /// instead of treating the gaps and rounded corners as clickable content.
    pub notice_shapes: Vec<GlassShape>,
    /// 文件拖入感应区；独立于普通鼠标命中区。
    pub file_drop_zone: Option<Rect>,
    /// 展开状态立即生效，不等待展开动画结束。
    pub expanded: bool,
    pub editing: bool,
    pub animating: bool,
    pub repaint_after: std::time::Duration,
    pub cmds: Vec<Cmd>,
}

/// 本地歌词（酷狗、汽水）每 5 秒复查，最多约 1 分钟；网络失败每 30 秒重试，最多约 5 分钟。手动点重试会重新计数。
const NATIVE_RETRIES:u32=12;
const NETWORK_RETRIES:u32=10;

/// 与 `format!("{title}|{artist}|{duration:.0}")` 等价，但不分配内存：界面每帧要比较好几次。
fn lyric_key_matches(key:&str,media:&NowPlaying)->bool {
    use std::io::Write;
    let Some(rest)=key.strip_prefix(media.title.as_str()).and_then(|rest|rest.strip_prefix('|')) else{return false;};
    let Some(rest)=rest.strip_prefix(media.artist.as_str()).and_then(|rest|rest.strip_prefix('|')) else{return false;};
    let mut buffer=[0u8;32];
    let mut cursor=std::io::Cursor::new(&mut buffer[..]);
    if write!(cursor,"{:.0}",media.duration).is_err(){return rest==format!("{:.0}",media.duration);}
    let written=cursor.position() as usize;
    rest.as_bytes()==&buffer[..written]
}

#[test]
fn lyric_key_matches_the_formatted_key_without_allocating(){
    for (title,artist,duration) in [("歌","手",120.4),("A|B","C",0.0),("歌","",239.5),("歌","手",240.5),("","",f64::NAN),("t","a",1e30)] {
        let media=NowPlaying{title:title.into(),artist:artist.into(),duration,..Default::default()};
        let key=format!("{}|{}|{:.0}",title,artist,duration);
        assert!(lyric_key_matches(&key,&media),"{key}");
        assert!(!lyric_key_matches(&format!("{key}0"),&media));
        assert!(!lyric_key_matches(&format!("x{key}"),&media));
    }
}

#[derive(Default)]
pub(crate) struct LyricsState {
    pub key: String,
    pub island_animation_started: bool,
    pub scroll_animation_started: bool,
    pub loading: bool,
    pub error: Option<lyrics::FetchError>,
    pub retry_at: Option<f64>,
    /// 发起本次查询时已知的歌曲 ID；之后才拿到 ID 且还没有同步歌词时，按 ID 再查一次。
    pub song_id: Option<u64>,
    /// 同一首歌已自动重试的次数；达到上限后停止，避免整首歌期间反复读盘或联网。
    pub attempts: u32,
    /// 用户点了「重试」：下一次查询跳过近期「没有歌词」的记录。
    pub manual_retry: bool,
    pub lyrics: Option<lyrics::Lyrics>,
    pub rx: Option<std::sync::mpsc::Receiver<Result<(lyrics::Lyrics,bool), lyrics::FetchError>>>,
    pub native_rx: Option<std::sync::mpsc::Receiver<Option<lyrics::Lyrics>>>,
}

/// v0.5.5：邮件面板与设置表单的界面状态。正文只在内存里过一次，不落盘。
#[derive(Default)]
pub(crate) struct MailState {
    pub privacy:crate::mail_privacy::Privacy,
    pub manage:crate::mail_manage_ui::State,
    pub accounts: Vec<mail::Account>,
    /// 每个账号的连接状态，键是邮箱小写。
    pub status: HashMap<String, mail::Status>,
    /// 最近邮件，新的在前，最多 60 封。
    pub recent: Vec<mail::Item>,
    /// 设置页里「测试连接」的结果。
    pub note: String,
    pub testing: Option<String>,
    pub form: MailForm,
    /// 面板里点开的邮件（账号 + uid，避免两个账号 uid 撞车）。
    pub open_uid: Option<(String, u32)>,
    pub reading: Option<mail::Item>,
    pub reader_focus: bool,
}

#[derive(Default)]
pub(crate) struct MailForm {
    pub provider: mail::Provider,
    pub custom: mail::CustomImap,
    pub email: String,
    pub password: String,
    pub show: bool,
}

impl MailState {
    /// 最近一封带验证码的邮件。
    pub fn code_item(&self) -> Option<&mail::Item> {self.recent.iter().find(|m|m.code.is_some())}
    pub fn status_of(&self, email: &str) -> mail::Status {
        self.status
            .get(&email.to_ascii_lowercase())
            .cloned()
            .unwrap_or(mail::Status::Idle)
    }
}

#[derive(Clone)]
struct PositionDrag {
    /// Grab point within the painted body, independent of native frame moves.
    grab:Vec2,
    original:(String,model::Dock,i32,Option<[f32;2]>),
    display_at_start:String,
    display:String,
    expanded:bool,
}

// ---------- 拖动位置动效 ----------
/// 原地按住这么久即「拿起」岛体，不必先移动鼠标。
const LIFT_HOLD: f64 = 0.12;
/// 拿起 / 放下：略欠阻尼，提起时轻轻弹一下，放下时回落。
const LIFT_K: (f32, f32) = (420.0, 0.38);
/// 拿起后放大：沿停靠边方向 / 离开停靠边方向。
const LIFT_SCALE: (f32, f32) = (0.08, 0.14);
/// 窗口跟手：明显拖尾并略微冲过光标；松手后落位回弹一两下。
const GLIDE_FOLLOW: (f32, f32) = (320.0, 0.72);
const GLIDE_SETTLE: (f32, f32) = (380.0, 0.38);
/// 移动越快沿运动方向拉得越长（最多 22%），厚度方向相应收窄。
const STRETCH_MAX: f32 = 0.22;
const STRETCH_SPEED: f32 = 1400.0;

// NN/G recommends pausing over a hover target for 0.3–0.5s before exposing
// disruptive hidden content; 350ms keeps switching deliberate but responsive.
const NAVIGATION_HOVER_DELAY:f64=0.35;

#[derive(Clone)]
struct NavigationHover {
    pass: u64,
    rect: Rect,
    pointer: Option<Pos2>,
    selection: (bool,usize),
    armed: bool,
    since: Option<f64>,
}

pub struct App {
    pub opts: Options,
    pub settings: model::Settings,
    /// 本次运行实际使用兼容渲染（Direct3D 12）；设置里的开关与它不同时提示重启。
    pub render_compat_running: bool,
    pub(crate) todos: model::TodoList,
    pub(crate) library: crate::library::Store,
    pub(crate) library_view: crate::library_ui::View,
    pub(crate) shelf: model::Shelf,
    pub(crate) agents: crate::agent_monitor::Monitor,
    pub(crate) agent_snapshot: crate::agent_monitor::Snapshot,
    pub(crate) agent_tasks: crate::agent_tasks::List,
    pub(crate) agent_expanded: Option<String>,
    pub(crate) agent_focus: Option<String>,
    pub(crate) agent_feedback: crate::agent_activity::Feedback,
    pub(crate) agent_notice_motion: crate::agent_icons::NoticeMotion,
    pub(crate) activity_notice: Option<Alert>,
    pub(crate) mac_stats: crate::mac_stats::Monitor,
    pub(crate) mac_snapshot: crate::mac_stats::Snapshot,
    pub(crate) stats_history: crate::mac_stats_ui::History,

    // 形变
    pub(crate) desktop_placement:DesktopPlacement,
    pub(crate) reduced_motion:bool,
    pub(crate) w: Spring,
    pub(crate) h: Spring,
    side_motion:crate::side_motion::Motion,
    pub(crate) side_panel_override:Option<Vec2>,
    panel_resize:Option<(Vec2,Vec2)>,
    pub(crate) open: bool,
    pub(crate) panel_pinned:bool,
    hover_since: Option<f64>,
    leave_since: Option<f64>,
    /// 前台切换导致的强制收起：光标离开前不再自动展开。
    pub hover_lock: bool,
    pub(crate) reveal: f32,
    position_drag: Option<PositionDrag>,
    /// Escape cancels this complete press, including its remaining held frames.
    position_drag_cancelled:bool,
    /// Normalized two-axis motion, consumed by the same native placement plan.
    drag_glide: Option<[anim::Spring;2]>,
    position_body:Rect,
    /// 拿起程度 0..1（可略过冲）与速度拉伸量，只影响岛体外形，不影响展开进度。
    drag_lift: anim::Spring,
    drag_stretch: anim::Spring,
    /// 光标停在日期时间拖动区的起始时间：停留越久把手提示越明显，划过不打扰。
    grip_hover_since: Option<f64>,

    // 切页
    pub(crate) page: usize,
    /// 主控是临时导航层，不占用持久化的模块编号。
    pub(crate) overview: bool,
    page_from: usize,
    page_dir: f32,
    page_t0: f64,
    slide: Spring,
    dots: [Spring; PAGE_COUNT],
    wheel_delta: f32,
    wheel_at: f64,
    wheel_page_at: f64,

    // 拖入
    pub(crate) drag_hot: bool,
    pub(crate) drag_a: f32,
    drag_saved: Option<(usize, bool)>,

    // 置物架
    pub(crate) shelf_scroll: Spring,
    pub(crate) shelf_first: usize,
    pub(crate) shelf_menu_rect: Option<Rect>,
    pub(crate) shelf_x: HashMap<PathBuf, Spring>,
    pub(crate) shelf_born: HashMap<PathBuf, f64>,
    pub(crate) icons: crate::icons::Cache,
    pub(crate) avoidance: crate::avoidance::Picker,
    pub(crate) confirm_clear: Option<f64>,
    pub(crate) clip_icons: crate::icons::Cache,
    pub clipboard: crate::clipboard::Shared,
    pub(crate) clip_first: usize,
    pub(crate) clip_confirm_clear: Option<f64>,

    // 待办
    pub(crate) adding: bool,
    pub(crate) editing: Option<usize>,
    pub(crate) buf: String,
    pub(crate) todo_input_focused: bool,
    todo_ime_active: bool,
    todo_ime_frame: bool,
    pub(crate) todo_pop: Spring,

    // 音乐
    pub(crate) ly: LyricsState,
    pub(crate) ly_worker: Option<std::thread::JoinHandle<()>>,
    pub(crate) show_lyrics: bool,
    island_lyric_layout: Option<std::sync::Arc<egui::Galley>>,
    media_anchor: (String, f64, f64, bool),
    pub(crate) media: NowPlaying,
    pub(crate) music_artwork: Option<(std::sync::Arc<egui::ColorImage>,egui::TextureHandle)>,
    pub(crate) qq_library: crate::qq_music::Library,
    pub(crate) qq_lyrics: Option<crate::qq_music::NativeLyrics>,
    pub(crate) qq_library_open: bool,
    pub(crate) playback_feedback: crate::music_playback::Feedback,

    forced_open_until:f64,
    pub settings_open:bool,
    settings_window:std::sync::Arc<std::sync::Mutex<crate::settings_window::State>>,
    pub displays:Vec<crate::display_layout::Display>,
    browser_at_top:bool,
    browser_minimal_active:bool,
    /// 毛玻璃着色量（平滑过渡，避免背景变化时闪烁）；0 表示尚未初始化。
    glass_tint:f32,
    pub(crate) fluid:crate::fluid::Fluid,
    pub(crate) fluid_origin:Pos2,
    fluid_art:Option<egui::TextureId>,
    fluid_title:String,
    joined_surface:std::cell::RefCell<Option<(shape::JoinSpec,std::sync::Arc<shape::JoinedSurface>)>>,
    pub(crate) screen:Option<Rect>,
    pub(crate) settings_focus_requested:bool,
    pub(crate) settings_tab:usize,
    pub(crate) task_dialog:Option<crate::extras::TaskDraft>,
    pub(crate) notification_bell_until: f64,
    pub(crate) notification_bell_at: f64,
    pub(crate) notice: Option<Alert>,
    pub(crate) hud_notice: Option<Alert>,
    pub(crate) notice_motion:crate::island_notice::Motion,
    pub(crate) alerts: std::collections::VecDeque<Alert>,
    pub(crate) history: std::collections::VecDeque<Alert>,
    pub(crate) notification_consumed:std::collections::VecDeque<(String,String)>,
    pub notification_status:crate::notifications::Status,
    pub(crate) notification_apps:model::NotificationApps,
    pub(crate) notification_locked:bool,
    pub(crate) pending: Vec<Cmd>,
    pub(crate) todo_first: usize,
    pub(crate) todo_choosing: bool,
    // 其它
    toast: Option<(String, f64)>,
    toast_below: bool,
    last_input: f64,
    system_idle: Option<f64>,
    last_cursor: Option<Pos2>,
    last_t: Option<f64>,
    pub(crate) t: f64,
    pub(crate) dt: f32,
    pub(crate) today: String,
    pub(crate) current_hour: usize,
    pub(crate) cursor_local: Option<Pos2>,
    started: bool,
    /// v0.5.4：音量条展示值（拖动时先用本地值，随后跟上系统回报）。
    pub(crate) volume_ui: Option<(u32, bool)>,
    /// 当前播放器音量，独立于系统音量保存。
    pub(crate) music_volume_ui: Option<(u32,bool)>,
    pub(crate) music_volume_quiet_until: f64,
    pub(crate) music_volume_open: bool,
    /// v0.5.5：邮件账号、最近邮件与设置表单。
    pub(crate) mail: MailState,
    /// 岛内详情与独立阅读窗口各保留一份排版缓存。
    mail_lines_cache: Vec<(std::sync::Arc<str>, std::sync::Arc<str>, u32, std::sync::Arc<Vec<crate::mail_format::Line>>)>,
    /// v0.5.6：日历（CalDAV）配置、未来 24 小时事件与设置表单。
    /// 天气页的当前数据与后台拉取状态。
    pub(crate) weather: crate::weather::State,
    pub(crate) resident: bar::Motion,
}

impl App {
    pub fn new(ctx: &egui::Context, opts: Options) -> Self {
        theme::load_fonts(ctx);
        #[cfg(target_os="macos")]
        theme::prepare_panel_assets(ctx);
        egui_extras::install_image_loaders(ctx);
        ctx.set_theme(egui::ThemePreference::Dark);
        ctx.set_visuals(egui::Visuals::dark());
        ctx.style_mut(|s| {
            s.interaction.selectable_labels = false;
            s.visuals.text_cursor.stroke = egui::Stroke::new(1.5_f32, theme::BLUE);
            s.visuals.selection.bg_fill = theme::alpha_blue(0.45);
        });
        let settings = model::Settings::load();
        let clipboard_on = settings.visible_pages[CLIPBOARD_PAGE] && settings.clipboard_history;
        let (bw, bh) = if cfg!(target_os="macos")&&settings.dock.is_side(){
            if settings.macos_side_handle{(6.0,52.0)}else{(crate::side_dock::SIDE_BAR_W,74.0)}
        }else if settings.minimal { (BAR_W_MIN, BAR_H_MIN) } else { (BAR_W, BAR_H) };
        let initial_lyrics=settings.lyrics_enabled;
        let initial_page=settings.page_order.iter().copied().find(|page|settings.visible_pages[*page]).unwrap_or(0);
        let weather=crate::weather::State::load(&settings.weather_city,settings.weather_position.as_ref());
        Self {
            opts,
            render_compat_running: settings.render_compat,
            settings,
            todos: model::TodoList::load(),
            library: crate::library::Store::load(),
            library_view: Default::default(),
            shelf: model::Shelf::load(),
            agents: Default::default(),
            agent_snapshot: Default::default(),
            agent_tasks: Default::default(),
            agent_expanded: None,
            agent_focus: None,
            agent_feedback: Default::default(),
            agent_notice_motion: Default::default(),
            activity_notice: None,
            mac_stats: Default::default(),
            mac_snapshot: Default::default(),
            stats_history: Default::default(),
            desktop_placement:DesktopPlacement::Legacy,
            reduced_motion:false,
            w: Spring::new(bw, CLOSE.0, CLOSE.1),
            h: Spring::new(bh, CLOSE.0, CLOSE.1),
            side_motion:crate::side_motion::Motion::new(),
            side_panel_override:None,
            panel_resize:None,
            open: false,
            panel_pinned:false,
            hover_since: None,
            leave_since: None,
            hover_lock: false,
            reveal: 0.0,
            position_drag: None,
            position_drag_cancelled:false,
            drag_glide: None,
            position_body:Rect::NOTHING,
            drag_lift: anim::Spring::new(0.0, LIFT_K.0, LIFT_K.1),
            drag_stretch: anim::Spring::new(0.0, 260.0, 0.6),
            grip_hover_since: None,
            page: initial_page,
            overview: false,
            page_from: initial_page,
            page_dir: 1.0,
            page_t0: -10.0,
            slide: Spring::new(0.0, 260.0, 0.92),
            dots: std::array::from_fn(|i| Spring::new(if i == initial_page { 16.0 } else { 4.0 }, 380.0, 0.86)),
            wheel_delta: 0.0,
            wheel_at: -1.0,
            wheel_page_at: -1.0,
            drag_hot: false,
            drag_a: 0.0,
            drag_saved: None,
            shelf_scroll: Spring::new(0.0, 300.0, 0.9),
            shelf_first: 0,
            shelf_menu_rect: None,
            shelf_x: HashMap::new(),
            shelf_born: HashMap::new(),
            icons: Default::default(), avoidance: Default::default(),
            confirm_clear: None,
            clip_icons: Default::default(),
            clipboard: crate::clipboard::Shared::new(clipboard_on),
            clip_first: 0,
            clip_confirm_clear: None,
            adding: false,
            todo_input_focused: false,
            todo_ime_active: false,
            todo_ime_frame: false,
            editing: None,
            buf: String::new(),
            todo_pop: Spring::new(0.0, 500.0, 0.45),
            ly: LyricsState::default(),
            ly_worker: None,
            show_lyrics: initial_lyrics,
            island_lyric_layout: None,
            media_anchor: (String::new(), 0.0, 0.0, false),
            media: NowPlaying::default(),
            music_artwork: None,
            qq_library: Default::default(),
            qq_lyrics: None,
            qq_library_open: false,
            playback_feedback: Default::default(),
            joined_surface:Default::default(),
            forced_open_until:0.0, settings_open:false, settings_window:std::sync::Arc::new(std::sync::Mutex::new(crate::settings_window::State::load())), displays:Vec::new(), browser_at_top:false, browser_minimal_active:false, glass_tint:0.0, fluid:Default::default(), fluid_origin:Pos2::ZERO, fluid_art:None, fluid_title:String::new(), screen:None, settings_focus_requested:false, settings_tab:0, task_dialog:None,
            notification_bell_until: 0.0, notification_bell_at: -10.0, notice: None, hud_notice: None, notice_motion:Default::default(), alerts: Default::default(), history: Default::default(), notification_consumed:Default::default(), notification_status:Default::default(), notification_apps:model::NotificationApps::load(), notification_locked:false, pending: vec![],
            todo_first: 0, todo_choosing: false,
            toast: None, toast_below: false,
            last_input: 0.0, system_idle: None,
            last_cursor: None,
            last_t: None,
            t: 0.0,
            dt: 0.0,
            today: String::new(), current_hour: 0,
            cursor_local: None,
            started: false,
            volume_ui: None,
            music_volume_ui: None,
            music_volume_quiet_until: 0.0,
            music_volume_open: false,
            mail: MailState { accounts: mail::load_accounts(), ..Default::default() },
            mail_lines_cache: Vec::new(),
            weather,
            resident: Default::default(),
        }
    }

    pub fn say(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), self.t));
        self.toast_below = false;
    }

    pub(crate) fn code_copied(&mut self, _code:&str) {
        self.say("验证码已复制");
        self.toast_below = true;
    }

    pub(crate) fn mail_code_hidden(&self,item:&mail::Item)->bool {
        self.mail.privacy.hidden(item,self.t,self.settings.mail_code_hide_seconds)
    }
    pub(crate) fn reveal_mail_code(&mut self,item:&mail::Item){self.mail.privacy.reveal(item,self.t);}
    /// 同一封邮件、同一段正文只解析一次排版，避免每帧重复切分几百个段落。
    pub(crate) fn mail_body_lines(&mut self, item: &mail::Item) -> std::sync::Arc<Vec<crate::mail_format::Line>> {
        // display_mail 每帧只 clone Arc：正文与 HTML 原文的指针都不变就说明是同一封邮件，
        // 长邮件不必每帧重新哈希几十 KB，更不必重新跑正则解析。
        if let Some((_,_,_,lines))=self.mail_lines_cache.iter().find(|(summary, html, uid, _)|
            *uid == item.uid
            && std::sync::Arc::ptr_eq(summary, &item.summary)
            && std::sync::Arc::ptr_eq(html, &item.html)){return lines.clone();}
        let lines=std::sync::Arc::new(crate::mail_format::item_lines(item));
        if self.mail_lines_cache.len()==2{self.mail_lines_cache.remove(0);}
        self.mail_lines_cache.push((item.summary.clone(),item.html.clone(),item.uid,lines.clone()));lines
    }

    pub(crate) fn display_mail(&self,item:&mail::Item)->mail::Item {
        let mut safe=item.clone();
        if self.mail_code_hidden(item) {
            if let Some(code)=&item.code {
                safe.subject=crate::mail_privacy::mask(&safe.subject,code);
                safe.from=crate::mail_privacy::mask(&safe.from,code);
                safe.summary=self.mail.privacy.masked_body(item);
                // HTML 原文里可能还有未遮蔽的验证码，隐藏时必须一起丢弃。
                // 用共享的空 Arc，让排版缓存对隐藏邮件也能按指针命中，不必每帧重解析。
                static EMPTY: std::sync::OnceLock<std::sync::Arc<str>> = std::sync::OnceLock::new();
                safe.html=EMPTY.get_or_init(||std::sync::Arc::from("")).clone();
                safe.links.clear(); // Avoid tooltip/URL/copy leaks while hidden.
                safe.code=None;
            }
        }
        safe
    }

    /// 收信服务推来的事件；历史邮件静默，新邮件进入通知队列。
    pub fn mail_event(&mut self, ev: mail::Event) {
        let watching=|email:&str|self.mail_active() && self.mail.accounts.iter().any(|a|a.enabled && a.email.eq_ignore_ascii_case(email));
        match ev {
            mail::Event::Managed(done)=>self.mail_managed(done),
            mail::Event::Status { account, status } => {
                if !watching(&account){return;}
                if status==mail::Status::Ready{
                    self.mail.manage.data.remove(&(account.to_ascii_lowercase(),crate::mail_manage::Folder::Inbox));self.mail.manage.next_refresh=0.0;
                    if (!self.mail.manage.trash_open||!self.settings.mail_sync_operations)&&self.mail.open_uid.as_ref().is_some_and(|(a,_)|a.eq_ignore_ascii_case(&account)){
                        self.mail.open_uid=None;self.mail.manage.auto_read=None;
                    }
                }
                self.mail.status.insert(account.to_ascii_lowercase(), status);
            }
            mail::Event::Mail { item, new } => {
                if !watching(&item.account){return;}
                if !new&&self.mail.manage.data.get(&(item.account.to_ascii_lowercase(),crate::mail_manage::Folder::Inbox)).is_some_and(|s|!s.entries.iter().any(|(uid,_)|*uid==item.uid)){return;}
                self.mail.privacy.observe(&item,new,self.t,chrono::Utc::now().timestamp());
                if self.mail_locally_removed(&item){
                    self.mail.manage.trash.retain(|m|m.uid!=item.uid||!m.account.eq_ignore_ascii_case(&item.account));self.mail.manage.trash.insert(0,item);self.mail.manage.trash.truncate(60);return;
                }
                self.mail
                    .recent
                    .retain(|m| !(m.account.eq_ignore_ascii_case(&item.account) && m.uid == item.uid));
                self.mail.recent.insert(0, item);
                self.mail.recent.truncate(60);
                if new{self.mail.manage.next_refresh=0.0;}
                self.mail.privacy.retain(self.mail.recent.iter().chain(&self.mail.manage.trash).chain(&self.mail.reading));
                if new && self.mail_active() {
                    let item=&self.mail.recent[0];
                    self.mail_notification(item.clone());
                    self.notification_bell_at = self.t;
                    self.notification_bell_until = self.t + 6.0;
                }
            }
            mail::Event::Test { account, ok, note } => {
                self.mail.testing = None;
                let detail = if ok { "连接成功".to_string() } else { note };
                let detail = if detail.chars().count() > 120 {
                    format!("{}…", detail.chars().take(120).collect::<String>())
                } else {
                    detail
                };
                self.mail.note = format!("{account}：{detail}");
                self.say(if ok {
                    format!("{account} 连接成功")
                } else {
                    format!("{account} 连接失败")
                });
            }
        }
    }

    /// 设置窗口第 8 个标签页：邮件。
    pub fn open_mail_settings(&mut self) {
        self.settings_tab = 7;
        self.open_settings();
    }

    /// 外壳同步收信线程时读账号清单（不含任何凭据）。
    pub fn mail_accounts(&self) -> &[mail::Account] {
        &self.mail.accounts
    }

    /// 账号增删或开关变化后：落盘并让外壳重新对齐收信线程。
    pub(crate) fn mail_accounts_changed(&mut self) {
        self.mail_lines_cache.clear();
        let accounts=&self.mail.accounts;
        let known=&self.mail.manage.identities;
        let active=|email:&str|accounts.iter().any(|a|a.enabled && mail::key(a)==email && known.get(email).is_none_or(|old|old==a));
        self.mail.manage.data.retain(|(email,_),_|active(email));
        self.mail.recent.retain(|m|active(&m.account.to_ascii_lowercase()));
        self.mail.manage.trash.retain(|m|active(&m.account.to_ascii_lowercase()));
        if self.mail.reading.as_ref().is_some_and(|m|!active(&m.account.to_ascii_lowercase())){self.mail.reading=None;}
        if self.mail.open_uid.as_ref().is_some_and(|(email,_)|!active(&email.to_ascii_lowercase())){self.mail.open_uid=None;self.mail.manage.auto_read=None;}
        self.mail.manage.identities.retain(|_,old|accounts.contains(old));
        self.mail.manage.next_refresh=0.0;
        mail::save_accounts(accounts);
        self.pending.push(Cmd::MailSync);
    }

    /// 录制/截图用的邮件样例（只在 `record` feature 下编译）。
    ///
    /// `QINGYU_MAIL_DEMO=1|code` 面板带验证码，`=list` 只列邮件，`=fail` 用假授权码走一次
    /// 「测试连接失败」，方便在没有真实邮箱时验收失败路径。
    #[cfg(feature = "record")]
    pub fn mail_demo(&mut self) {
        use mail::{Account, Item, Provider, Status};
        let mode = std::env::var("QINGYU_MAIL_DEMO").unwrap_or_default();
        self.settings_tab = 7;
        if mode == "fail" {
            self.mail.form.provider = Provider::Qq;
            self.mail.form.email = "test@qq.com".into();
            self.mail.form.password = "invalid-auth-code".into();
            self.mail.note = "test@qq.com：正在测试连接…".into();
            self.pending.push(Cmd::MailTest(
                Account { custom: None, email: "test@qq.com".into(), provider: Provider::Qq, enabled: true },
                "invalid-auth-code".into(),
            ));
            return;
        }
        self.mail.accounts = vec![
            Account { custom: None, email: "demo@qq.com".into(), provider: Provider::Qq, enabled: false },
            Account { custom: None, email: "demo@icloud.com".into(), provider: Provider::ICloud, enabled: false },
        ];
        for a in &self.mail.accounts {
            self.mail.status.insert(a.email.clone(), Status::Ready);
        }
        let rows = [
            (
                "GitHub",
                "[GitHub] Please verify your device",
                "09:41",
                Some("482913"),
                "We noticed a sign-in attempt from a new device. Enter 482913 to finish signing in. If this wasn't you, secure your account now.",
            ),
            (
                "Apple",
                "Your Apple ID verification code",
                "09:38",
                None,
                "Your Apple ID code is 573104. Don't share it with anyone. If you didn't request it, you can ignore this message.",
            ),
            (
                "腾讯企业邮",
                "会议邀请：周五方案评审",
                "09:12",
                None,
                "本周五 14:30 在三楼会议室评审 v0.5.5 方案，请提前十分钟到场。会议邀请见附件。",
            ),
            (
                "顺丰速运",
                "您的快件已到达菜鸟驿站",
                "08:57",
                None,
                "您的快件已到达菜鸟驿站（望京 SOHO T1 店），请凭取件码 6-3-2 在 20:00 前领取。",
            ),
        ];
        let with_code = mode != "list";
        self.mail.recent = rows
            .iter()
            .enumerate()
            .map(|(i, (from, subject, time, code, summary))| Item {
                received_unix: None, account: "demo@qq.com".into(),
                from: (*from).into(),
                subject: (*subject).into(),
                time: (*time).into(),
                code: if with_code { (*code).map(str::to_string) } else { None },
                summary: (*summary).into(),
                html: String::new().into(),
                links: Vec::new(),
                uid: 900 - i as u32,
            })
            .collect();
        self.mail.note = "demo@qq.com：连接成功".into();
        self.page = MAIL_PAGE;
        self.dots_target();
    }

    /// 没有新的系统音量样本时，保留最近一次已知值。
    fn sync_device(&mut self, env:&Env){if let Some(volume)=env.volume{self.volume_ui=Some(volume);}}

    pub fn idle_for(&self) -> f64 {
        self.system_idle.unwrap_or(self.t - self.last_input)
    }

    pub fn show_page(&mut self,page:usize) {
        if page>=PAGE_COUNT{return;}
        if !self.settings.visible_pages[page] {return;}
        self.notice=None;
        self.go(page,1.0);
        self.open=true; self.forced_open_until=self.t+4.0;
        self.w.tune(OPEN_W.0,OPEN_W.1);self.h.tune(OPEN_H.0,OPEN_H.1);
        self.w.target=self.expanded_width();self.h.target=self.expanded_height();
    }
    pub(crate) fn window_layout(&self)->std::sync::MutexGuard<'_,crate::settings_window::State>{
        self.settings_window.lock().unwrap_or_else(|e|e.into_inner())
    }

    pub fn save_window_before_exit(&self){self.window_layout().close();}

    #[cfg(windows)]
    pub fn start_tray(&self,state:crate::tray::State)->Option<crate::tray::Tray>{
        let layout=self.settings_window.clone();
        // 原生文件选择框会占用界面线程，托盘退出时直接保存最近一次观察到的布局。
        crate::tray::Tray::start_with_exit(state,move||layout.lock().unwrap_or_else(|e|e.into_inner()).close())
    }

    pub fn open_settings(&mut self){
        self.settings_open=true;
        self.settings_focus_requested=true;
    }
    pub fn open_settings_category(&mut self,_category:usize){self.settings_tab=0;self.open_settings();}

    /// egui also accepts clicks near a widget. Settings require a press and
    /// release inside the actual gear; keyboard and accessibility clicks stay valid.
    pub(crate) fn settings_gear_clicked(response:&egui::Response)->bool {
        let primary=egui::PointerButton::Primary;
        let (pressed,released,origin)=response.ctx.input(|input|{
            let origin=input.events.iter().rev().find_map(|event|match event{egui::Event::PointerButton{pos,button,pressed:true,..}if *button==primary=>Some(*pos),_=>None});
            (input.pointer.button_pressed(primary),input.pointer.button_released(primary),origin)
        });
        let state=response.id.with("settings-press-inside");
        let started_inside=response.ctx.data_mut(|data|{
            if pressed{data.insert_temp(state,origin.is_some_and(|point|response.interact_rect.contains(point)));}
            let inside=data.get_temp::<bool>(state).unwrap_or(false);
            if released{data.remove::<bool>(state);}
            inside
        });
        response.clicked()&&(!response.clicked_by(primary)||(started_inside&&response.interact_pointer_pos().is_some_and(|point|response.interact_rect.contains(point))))
    }
    pub fn page(&self) -> usize {
        self.page
    }

    pub fn is_control_center(&self)->bool {self.overview}

    /// 明确打开主控；模块编号和已保存的页面顺序保持原有 schema。
    pub fn show_control_center(&mut self) {
        self.cancel_edit();
        self.enter_control_center();
        self.open=true;self.forced_open_until=self.t+4.0;
        self.w.tune(OPEN_W.0,OPEN_W.1);self.h.tune(OPEN_H.0,OPEN_H.1);
        self.w.target=self.expanded_width();self.h.target=self.expanded_height();
    }

    pub(crate) fn enter_control_center(&mut self) {
        self.overview=true;
        self.music_volume_open=false;
        self.wheel_delta=0.0;
        self.page_from=self.page;self.page_t0=self.t-1.0;self.slide.snap(0.0);
    }

    // ---------- 功能模块：页面开关即模块开关，关闭后页面隐藏、后台同步和提醒一并停止 ----------

    /// 模块是否开启（与「页面管理」里的页面开关一致）。
    pub fn module_on(&self,page:usize)->bool {self.settings.visible_pages.get(page).copied().unwrap_or(false)}
    /// 收信线程是否应运行：邮箱模块开启且打开了「新邮件提醒」。
    pub fn mail_active(&self)->bool {self.module_on(MAIL_PAGE) && self.settings.mail_enabled}
    /// 粘贴板监听是否应运行：粘贴板模块开启且打开了记录。
    pub fn clipboard_active(&self)->bool {self.module_on(CLIPBOARD_PAGE) && self.settings.clipboard_history}


    pub(crate) fn set_page_visible(&mut self,page:usize,visible:bool) {
        if page>=PAGE_COUNT{return;}
        if !visible && self.settings.visible_pages.iter().filter(|enabled|**enabled).count()==1
            && self.settings.visible_pages[page] { return; }
        self.settings.visible_pages[page]=visible;
        self.reconcile_notification_privacy();
        if !self.settings.visible_pages[self.page] {
            let overview=self.overview;
            if let Some(first)=self.settings.page_order.iter().copied().find(|page|self.settings.visible_pages[*page]) { self.go(first,1.0); }
            if overview {self.enter_control_center();}
        }
    }

    fn navigate_page(&mut self,next:bool) {
        let pages:Vec<_>=self.settings.page_order.iter().copied().filter(|page|self.module_on(*page)).collect();
        let current=if self.overview{0}else{pages.iter().position(|page|*page==self.page).unwrap_or(0)+1};
        let count=pages.len()+1;
        let target=(current+if next{1}else{count-1})%count;
        if target==0{self.cancel_edit();self.enter_control_center();}
        else{self.go(pages[target-1],if next{1.0}else{-1.0});}
    }

    /// OLED 像素偏移只在收起动画完成、没有鼠标交互、不在编辑时移动；
    /// 展开、点击、拖拽时保持当前位置，避免面板内容和位移一起跳。
    pub fn oled_shift_idle(&self,ctx:&egui::Context) -> bool {
        crate::oled_shift::should_move(self.open, self.position_drag.is_some() || self.drag_glide.is_some(), self.is_editing())
            && self.hover_since.is_none() && !self.drag_hot
            && self.w.settled(self.expanded_width()) && self.h.settled(self.expanded_height())
            && self.drag_lift.settled(1.0) && self.drag_stretch.settled(1.0)
            && ctx.input(|i|!i.pointer.any_down() && !i.pointer.any_pressed() && i.raw.hovered_files.is_empty())
    }

    pub fn is_editing(&self) -> bool {
        self.adding || self.editing.is_some() || self.todo_choosing || self.task_dialog.is_some()
            || self.library_view.composing || self.page == TODO_PAGE && self.library_view.text_focused
    }

    /// A short, deliberate hover selects a navigation tab. A tab moving under
    /// a stationary pointer (scrolling, opening or resizing) cannot arm it.
    pub(crate) fn navigation_activated(&self,c:&Card,response:&egui::Response,selected:bool)->bool {
        if !c.live || !response.enabled(){return false;}
        let ctx=c.ui.ctx();
        let key=response.id.with("navigation-hover-intent");
        if response.clicked(){ctx.data_mut(|data|data.remove::<NavigationHover>(key));return true;}
        let text_focused=ctx.memory(|memory|memory.focused())
            .is_some_and(|id|egui::TextEdit::load_state(ctx,id).is_some());
        let blocked=self.is_editing() || self.todo_ime_active || self.todo_ime_frame || text_focused
            || self.settings_open || self.drag_hot || self.position_drag.is_some() || self.panel_resize.is_some()
            || self.drag_glide.is_some() || ctx.dragged_id().is_some() || ctx.memory(|memory|memory.any_popup_open());
        let (now,pointer,input_blocked)=ctx.input(|input|(input.time,input.pointer.hover_pos(),
            !input.focused || input.pointer.any_down() || input.pointer.any_pressed() || input.pointer.any_released()
                || !input.raw.hovered_files.is_empty() || !input.raw.dropped_files.is_empty()
                || input.events.iter().any(|event|matches!(event,egui::Event::Ime(_)|egui::Event::Text(_)|egui::Event::Key{..}|egui::Event::MouseWheel{..}))));
        let pass=ctx.cumulative_pass_nr();
        let selection=(self.overview,self.page);
        let hovered=response.hovered() && pointer.is_some_and(|point|response.interact_rect.contains(point));
        let (activate,remaining)=ctx.data_mut(|data|{
            let previous=data.get_temp::<NavigationHover>(key);
            let continuous=previous.as_ref().is_some_and(|state|state.pass.saturating_add(1)>=pass);
            let stable=continuous && previous.as_ref().is_some_and(|state|state.rect==response.interact_rect);
            let moved=previous.as_ref().is_some_and(|state|state.pointer!=pointer);
            let same_selection=previous.as_ref().is_some_and(|state|state.selection==selection);
            let mut armed=previous.as_ref().is_some_and(|state|state.armed);
            let mut since=previous.and_then(|state|state.since);
            // Keyboard, notification and external navigation supersede an
            // unfinished hover; only fresh pointer motion may arm it again.
            if !same_selection{armed=false;since=None;}
            if selected || blocked || input_blocked || !hovered || !continuous {armed=false;since=None;}
            else {
                armed|=moved;
                if !stable{since=None;}else if armed && since.is_none(){since=Some(now);}
            }
            let activate=since.is_some_and(|since|now-since>=NAVIGATION_HOVER_DELAY);
            let remaining=since.map(|since|(NAVIGATION_HOVER_DELAY-(now-since)).max(0.0))
                .or_else(||armed.then_some(0.0));
            if activate{armed=false;since=None;}
            data.insert_temp(key,NavigationHover{pass,rect:response.interact_rect,pointer,selection,armed,since});
            (activate,remaining)
        });
        if let Some(remaining)=remaining.filter(|_|!activate){ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining));}
        activate
    }

    pub fn filter_todo_input(&mut self,input:&mut egui::RawInput) {
        self.todo_ime_frame=false;
        let dialog_edit=self.task_dialog.as_ref().is_some_and(|draft|draft.accepts_text());
        if !self.adding && self.editing.is_none() && !dialog_edit && !self.library_view.composing && !self.library_view.text_focused{self.todo_ime_active=false;return;}
        self.todo_ime_frame=self.todo_ime_active || input.events.iter().any(|event|matches!(event,egui::Event::Ime(egui::ImeEvent::Enabled|egui::ImeEvent::Preedit(_)|egui::ImeEvent::Commit(_))));
        for event in &input.events {
            match event {
                egui::Event::Ime(egui::ImeEvent::Enabled|egui::ImeEvent::Preedit(_))=>self.todo_ime_active=true,
                egui::Event::Ime(egui::ImeEvent::Commit(_)|egui::ImeEvent::Disabled)=>self.todo_ime_active=false,
                _=>{},
            }
        }
        // 首帧和结束帧的按键、失焦仍属于组合输入，不能透传给待办编辑。
        if self.todo_ime_frame {input.events.retain(|event|!matches!(event,egui::Event::Key{key:egui::Key::Backspace|egui::Key::Delete|egui::Key::Enter|egui::Key::Escape|egui::Key::ArrowLeft|egui::Key::ArrowRight|egui::Key::ArrowUp|egui::Key::ArrowDown,..}));}
    }

    pub fn foreground_changed(&mut self) {
        if self.todo_ime_frame {return;}
        if self.panel_pinned{return;}
        self.hover_lock=true;
        if self.adding && self.buf.trim().is_empty(){self.cancel_edit();}
    }

    /// 带过渡地切到某页。`dir` > 0 表示内容从右侧进入。
    pub(crate) fn go(&mut self, page: usize, dir: f32) {
        if !self.module_on(page) { return; }
        // 同一模块也可能是从主控进入；不能把它当作无操作。
        let from_overview=std::mem::replace(&mut self.overview,false);
        if page == self.page {
            if from_overview {self.page_from=page;self.page_t0=self.t-1.0;self.slide.snap(0.0);self.music_volume_open=false;}
            return;
        }
        self.music_volume_open=false;
        // 连点：上一段过渡还没结束时，以当前页为起点重新开始，位移速度保留。
        self.page_from = if from_overview{page}else{self.page};
        self.page = page;
        self.page_dir = dir;
        self.page_t0 = self.t;
        self.slide.pos = dir * PAGE_SLIDE + if self.page_e() < 0.4 { self.slide.pos * 0.3 } else { 0.0 };
        self.slide.target = 0.0;
        self.dots_target();
        self.cancel_edit();
    }

    fn page_e(&self) -> f32 {
        (self.t - self.page_t0) as f32
    }

    fn dots_target(&mut self) {
        for (i, d) in self.dots.iter_mut().enumerate() {
            d.target = if i == self.page { 16.0 } else { 4.0 };
        }
    }

    pub(crate) fn cancel_edit(&mut self) {
        self.adding = false;
        self.editing = None;
        self.todo_choosing = false;
        self.task_dialog=None;
        self.buf.clear();
        self.todo_input_focused=false;
        self.todo_ime_active=false;
        self.todo_ime_frame=false;
        self.library_view.composing=false;
        self.library_view.text_focused=false;
        self.library_view.focus_draft=false;
    }

    /// 外壳据此调整音乐采样频率：看着音乐页时最快，收起显示歌词次之，其余只需跟上歌名和播放状态。
    pub fn media_poll_interval(&self)->std::time::Duration {
        if self.open && self.page==MUSIC_PAGE {crate::smtc::POLL_FAST}
        else if self.island_lyrics_active() && !self.open {crate::smtc::POLL_LYRICS}
        else {crate::smtc::POLL_BACKGROUND}
    }

    fn island_lyrics_active(&self)->bool {
        if self.side() {return false;}
        self.settings.island_lyrics && self.settings.visible_pages[MUSIC_PAGE] && self.media.is_music()
            && !self.browser_minimal_active
    }
    pub(crate) fn compact_notices(&self)->bool {
        // Badge-only lyric mode; real notices use the independent lower row.
        self.island_lyrics_active()&&!self.open&&self.display_notice().is_none()
    }

    /// 换句、分段和展开高亮共用时间调整；正数延后，负数提前。
    pub(crate) fn lyric_position(&self)->f32 {(self.music_position()-self.settings.lyric_delay_ms as f64/1000.0)as f32}

    pub(crate) fn island_lyric(&self) -> Option<(usize,&str)> {
        if !self.island_lyrics_active(){return None;}
        if let Some(lyrics)=self.native_qq_lyrics(){
            if let Some(index)=lyrics.current{
                if let Some(text)=lyrics.lines.get(index).map(|text|text.trim()).filter(|text|!text.is_empty()) {return Some((index,text));}
            }
        }
        if !lyric_key_matches(&self.ly.key,&self.media) {return None;}
        let lyrics=self.ly.lyrics.as_ref().filter(|lyrics|lyrics.synced)?;
        let index=lyrics.index_at(self.lyric_position())?;
        let text=lyrics.lines.get(index)?.text.trim();
        (!text.is_empty()).then_some((index,text))
    }

    fn update_island_lyric_layout(&mut self,ctx:&egui::Context) {
        if !self.island_lyrics_active(){self.island_lyric_layout=None;return;}
        let current=self.island_lyric();
        let lyrics=self.ly.lyrics.as_ref().filter(|lyrics|lyric_key_matches(&self.ly.key,&self.media)
            && lyrics.synced && lyrics.lines.iter().any(|line|!line.text.trim().is_empty()));
        let native=self.native_qq_lyrics().is_some();
        let title=lyrics.is_none() && !native;
        let text=if let Some((_,line))=current {line}
            else if let Some(lyrics)=lyrics {
                if lyrics.index_at(self.lyric_position()).is_none() {"前奏"}
                else {"间奏"}
            } else {self.media.title.trim()};
        let ink=if current.is_some()||title{theme::WHITE}else{theme::GRAY};
        let (left,right)=self.lyric_content_insets();
        let layout=|text:String| {
            let mut job=egui::text::LayoutJob::simple(text,theme::font(11.0,W::Semibold),ink,(self.lyric_bar_width()-left-right).max(80.0));
            job.halign=egui::Align::Center;
            job.wrap.break_anywhere=true;
            // QQ supplies the active line without segment timing. Keep its
            // native projection in this one-row bar and elide overflow.
            if title || native {job.wrap.max_rows=1;}
            ctx.fonts(|fonts|fonts.layout_job(job))
        };
        let mut galley=layout(text.to_owned());
        if let Some((index,_))=current.filter(|_|galley.rows.len()>1 && !native) {
            let lyrics=self.ly.lyrics.as_ref().unwrap();
            let start=lyrics.lines[index].t;
            let end=lyrics.lines.iter().skip(index+1).find(|line|line.t>start).map_or(self.media.duration as f32,|line|line.t);
            let progress=if end>start {((self.lyric_position()-start)/(end-start)).clamp(0.0,1.0)}else{0.0};
            // 按当前句的时间范围与各段字数切换，暂停和跳转直接跟随播放位置。
            let total:usize=galley.rows.iter().map(|row|row.glyphs.len()).sum();
            let mut count=0;
            let page=galley.rows.iter().position(|row|{count+=row.glyphs.len();progress*(total as f32)<count as f32}).unwrap_or(galley.rows.len()-1);
            let segment=galley.rows[page].glyphs.iter().map(|glyph|glyph.chr).collect::<String>();
            galley=layout(segment.trim().to_owned());
        }
        self.island_lyric_layout=Some(galley);
    }

    pub(crate) fn effective_minimal(&self)->bool {
        // The side appearance is explicit. The top dock's saved minimal mode
        // must not silently hide a side island selected by the user.
        if cfg!(target_os="macos")&&self.side(){self.side_handle()||self.browser_minimal_active}
        else{self.settings.minimal||self.browser_minimal_active}
    }

    fn lyric_bar_width(&self)->f32 {LYRIC_BAR_W.max(self.settings.resident_length as f32)}
    fn lyric_content_insets(&self)->(f32,f32){
        // Cover ends at 56pt with the pet, 32pt without it. Reserve each
        // side's actual controls plus a small gap; the Agent group is wider.
        let left=if self.settings.pet{64.0}else{40.0};
        let right=if self.module_on(AGENT_PAGE){90.0}else{37.0};
        (left,right)
    }

    pub fn resident_size(&self)->Vec2{let (w,h)=self.base_size();Vec2::new(w,h)}

    pub fn set_minimal(&mut self,enabled:bool) {
        self.settings.minimal=enabled;self.settings.save();
    }
    fn base_size(&self) -> (f32, f32) {
        if self.side() {
            return self.side_base_size(self.effective_minimal());
        }
        let (width,height)=if self.island_lyrics_active() {
            (self.lyric_bar_width(),BAR_H)
        } else if self.effective_minimal() {
            (BAR_W_MIN.max(self.settings.resident_length as f32), BAR_H_MIN)
        } else {
            (self.resident.target.width.min(WIN_W-32.0), BAR_H)
        };
        // The connected body cannot be narrower than the physical camera: a
        // narrow bar would leave a detached black shelf under the housing.
        let width=if let DesktopPlacement::Camera{bridge,..}=self.desktop_placement {width.max(bridge.width())}else{width};
        (width,height)
    }

    /// 0 = 药丸，1 = 完整面板（展开时可略超过 1）。
    #[cfg(test)]
    pub(crate) fn current_width(&self) -> f32 { self.w.pos }

    pub fn progress(&self) -> f32 {
        let (_, bh) = self.base_size();
        // A side panel unfolds vertically around the resident handle.
        if self.side() {return self.side_motion.progress();}
        (self.h.pos - bh) / (self.expanded_height() - bh)
    }

    pub(crate) fn expanded_width(&self)->f32 {
        if self.side(){self.side_panel_size().x}else{PW}
    }

    /// Shared page content below the header, above the persistent navigation.
    pub(crate) fn content_rect(&self)->Rect {
        if self.side(){Rect::from_min_max(Pos2::new(10.0,110.0),Pos2::new(self.expanded_width()-10.0,self.expanded_height()-16.0))}
        else{Rect::from_min_max(Pos2::new(12.0,44.0),Pos2::new(PW-12.0,self.expanded_height()-52.0))}
    }

    pub(crate) fn expanded_height(&self)->f32 {
        // Module navigation changes content, never the expanded outer frame.
        // Overflow belongs to each page's scrolling content; side dimensions
        // continue to follow the user's saved size and native screen fitting.
        if self.side(){self.side_panel_size().y}else{crate::control_center::TOP_H}
    }

    // ---------------------------------------------------------------
    // 每帧入口
    // ---------------------------------------------------------------
    pub fn set_desktop_placement(&mut self,placement:DesktopPlacement) {self.desktop_placement=placement;}
    pub fn set_side_panel_size(&mut self,size:Vec2){self.side_panel_override=Some(size);}
    pub(crate) fn island_shape(&self)->model::IslandShape {
        if self.side(){return model::IslandShape::Notch;}
        match self.desktop_placement {DesktopPlacement::Legacy=>self.settings.island_shape,DesktopPlacement::Floating{..}|DesktopPlacement::Positioned(_)=>model::IslandShape::Pill,DesktopPlacement::Camera{..}=>model::IslandShape::Notch}
    }
    pub(crate) fn island_gap(&self)->f32 {
        if self.side(){return 0.0;}
        if !self.side() {match self.desktop_placement {DesktopPlacement::Floating{content_top}|DesktopPlacement::Camera{content_top,..}=>return content_top.max(0.0),DesktopPlacement::Positioned(_)=>return 0.0,DesktopPlacement::Legacy=>{}}}
        if self.island_shape()==model::IslandShape::Pill {PILL_GAP}else{0.0}
    }
    pub(crate) fn displayed_island_rect(&self,win:Rect)->Rect{
        if !self.side(){
            let p=anim::clamp01(self.progress());
            if p<=0.003{return self.resident_rect(win);}
            let resident=self.resident_size();
            let size=Vec2::new(lerp(resident.x.min(self.expanded_width()),self.expanded_width(),smoothstep(0.0,0.85,p)),self.expanded_height()*p);
            return self.horizontal_panel_rect(win,size);
        }
        let (sx,sy)=self.drag_scale();
        let (max_w,max_h)=if self.side(){let size=self.side_canvas_size();(size.x,size.y)}else{(WIN_W,WIN_H)};
        let w=(self.w.pos*sx).min((max_w-24.0).max(self.w.pos));
        let h=(self.h.pos*sy).min((max_h-24.0).max(self.h.pos));
        self.island_rect(win,w,h)
    }
    pub(crate) fn resident_rect(&self,win:Rect)->Rect{
        if let DesktopPlacement::Positioned(plan)=self.desktop_placement{return plan.resident_body(self.dock()).translate(win.min.to_vec2());}
        let size=self.resident_size();self.island_rect(win,size.x,size.y)
    }
    pub(crate) fn horizontal_panel_rect(&self,win:Rect,size:Vec2)->Rect{
        if let DesktopPlacement::Positioned(plan)=self.desktop_placement{return plan.panel_body(self.dock(),size).translate(win.min.to_vec2());}
        crate::island_position::disclosure(self.screen.unwrap_or(win).union(self.resident_rect(win)),self.resident_rect(win),Vec2::new(self.expanded_width(),self.expanded_height()),size,None)
    }
    /// Locking or an explicit platform hide must end a pinned disclosure, so
    /// waking cannot unexpectedly restore private panel content.
    pub fn suspend_island(&mut self){
        self.panel_pinned=false;self.open=false;self.hover_lock=true;
        self.hover_since=None;self.leave_since=None;self.forced_open_until=0.0;
        self.position_drag=None;self.drag_glide=None;self.panel_resize=None;
        let size=self.resident_size();self.w.snap(size.x);self.h.snap(size.y);self.side_motion.reset(false);
    }
    pub fn canvas_size(&self)->Vec2 {
        if let DesktopPlacement::Positioned(plan)=self.desktop_placement{return plan.canvas;}
        if self.side(){self.side_canvas_size()}
        else {Vec2::new(WIN_W,WIN_H+match self.desktop_placement {DesktopPlacement::Camera{content_top,..}|DesktopPlacement::Floating{content_top}=>content_top,_=>0.0})}
    }

    pub fn frame(&mut self, ui: &mut egui::Ui, win: Rect, env: &Env) -> FrameOut {
        let ctx = ui.ctx().clone();
        if !ctx.input(|input|input.pointer.primary_down()){self.position_drag_cancelled=false;}
        #[cfg(target_os="macos")]
        {self.reduced_motion=crate::mac_design::reduce_motion(&ctx);}
        let dt = match self.last_t {
            Some(t0) => (env.t - t0).clamp(0.0, 0.1) as f32,
            None => 0.0,
        };
        let elapsed = self.last_t.map(|at| (env.t - at).max(0.0) as f32).unwrap_or(0.0);
        self.last_t = Some(env.t);
        self.t = env.t;
        self.set_notification_session_locked(env.screen_locked);
        self.weather_tick();
        self.current_hour=env.hour();
        self.system_idle=env.idle_seconds;
        self.dt = dt;
        if let Some(at_top)=env.browser_at_top {self.browser_at_top=at_top;}
        self.screen=env.screen;
        if env.screen_locked {self.suspend_island();}
        if self.settings.skin.is_glass() {
            let target=if self.settings.skin==model::Skin::GlassLight {glass::light_tint(env.backdrop,env.glass_blur)}else{glass::glass_tint(env.backdrop,env.glass_blur)};
            self.glass_tint=if self.glass_tint<=0.0 {target}else{anim::approach(self.glass_tint,target,3.0,dt)};
        }
        let browser_minimal=self.settings.browser_minimal && self.browser_at_top;
        if browser_minimal && !self.browser_minimal_active {
            self.browser_minimal_active=true;
            self.suspend_island();
        }
        self.browser_minimal_active=browser_minimal;
        if !self.started {
            self.started = true;
            self.last_input = env.t;
            self.page_t0 = env.t - 10.0;
        }
        let mut cmds = Vec::new();

        self.tick(env, dt, elapsed);
        self.agents.update(self.opts.monitor_agents && self.module_on(AGENT_PAGE),&ctx);
        if self.opts.monitor_agents {self.agent_snapshot=self.agents.snapshot();}
        if self.module_on(AGENT_PAGE)&&!env.screen_locked {
            self.agent_tasks.update(&self.agent_snapshot,env.t,env.unix_ms.max(env.unix_time.saturating_mul(1000)));
            if let Some(expiry)=self.agent_tasks.next_expiry(){
                ctx.request_repaint_after(std::time::Duration::from_secs_f64((expiry-env.t).max(0.01)));
            }
        }else{
            self.agent_tasks.reset();self.agent_expanded=None;self.agent_focus=None;
        }
        let feedback_busy=self.notice.is_some()||!self.alerts.is_empty()||self.position_drag.is_some()||self.panel_resize.is_some()||self.drag_hot||self.is_editing();
        self.update_agent_feedback(env.unix_time,env.unix_ms.max(env.unix_time.saturating_mul(1000)),feedback_busy);
        self.mac_stats.update(self.opts.fetch_lyrics && self.module_on(MAC_STATS_PAGE) && !env.screen_locked,&ctx);
        if self.opts.fetch_lyrics {self.mac_snapshot=self.mac_stats.snapshot();}
        self.stats_history.observe(&self.mac_snapshot);
        self.sync_device(env);
        let notice_focused=ctx.memory(|memory|["notice-summary","notice-dismiss"].iter().any(|id|memory.has_focus(egui::Id::new(("card",true,*id)))));
        if notice_focused||env.cursor.is_some_and(|point|self.notice_hover_rect(win).is_some_and(|rect|rect.contains(point))) {
            // Keep a hovered card still long enough to read or click, including
            // after the computer wakes from an idle frame interval.
            for alert in self.notice.iter_mut().chain(self.activity_notice.iter_mut()) {alert.at+=elapsed as f64;}
        }
        self.tick_alerts();
        self.update_notice_motion(&ctx,dt);
        self.update_media(env,&ctx);
        let mut layout=Layout::new(self.media.is_music(),false,self.weather_pill().is_some());
        layout.reorder(&self.settings.resident_order);
        if self.settings.clock_period!=0 { layout.widen_clock(40.0); }
        if let Some(extra)=self.weather_pill_extra(&ctx){ layout.widen(bar::WEATHER,extra); }
        layout.companions(self.settings.pet,self.module_on(AGENT_PAGE));
        layout.enrich(self.settings.resident_length);
        layout.set_length(self.settings.resident_length);

        self.resident.step(layout,dt,!self.reduced_motion&&ctx.style().animation_time>0.0&&bar::animations_enabled());
        if self.opts.fetch_lyrics {
            self.poll_lyrics();
        }
        self.update_island_lyric_layout(&ctx);

        let previous_body=self.displayed_island_rect(win);
        self.position_body=if self.side(){previous_body}else{self.resident_rect(win)};
        // 胶囊皮肤悬浮在屏幕顶边下方。
        let pill = self.island_shape()==model::IslandShape::Pill;
        let side = self.side();
        let dock = self.dock();

        // ---------- 输入：拖放 ----------
        let (hovering_files, dropped) = ctx.input(|i| {
            (
                self.settings.visible_pages[SHELF_PAGE] && !i.raw.hovered_files.is_empty(),
                i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect::<Vec<_>>(),
            )
        });
        if !dropped.is_empty() && self.settings.visible_pages[SHELF_PAGE] {
            self.on_drop(dropped);
            self.drag_saved=None;
            self.show_page(SHELF_PAGE);
        } else if hovering_files && !self.drag_hot {
            self.drag_hot = true;
            self.drag_saved = Some((self.page,self.overview));
            self.go(SHELF_PAGE, if self.page < SHELF_PAGE { 1.0 } else { -1.0 });
        } else if !hovering_files && self.drag_hot {
            // 拖到外面松手 / 取消：安静地退回原页
            self.drag_hot = false;
            if let Some((page,overview)) = self.drag_saved.take() {
                self.go(page, if page < SHELF_PAGE { -1.0 } else { 1.0 });
                if overview {self.enter_control_center();}
            }
        }
        self.drag_a = anim::approach(self.drag_a, if self.drag_hot { 1.0 } else { 0.0 }, 16.0, dt);

        // ---------- 输入：悬停 ----------
        let p_prev = self.progress();
        let previous_menu=self.shelf_menu_rect.take();
        let resident=self.resident_rect(win);
        let panel_visible=!side&&p_prev>0.003;
        let inside = !env.screen_locked&&env.cursor.is_some_and(|c| (if side{self.side_contains(win,c)}else{
            if panel_visible&&!(self.effective_minimal()&&!self.open&&!self.island_lyrics_active()){shape::contains_outline(&self.horizontal_join(win,previous_body,p_prev).outline,c)}
            else{shape::contains_outline(&self.floating_outline(resident,0.0),c)}
        }) || previous_menu.is_some_and(|r|r.contains(c)));
        self.resident_position_input(ui,win,env);
        let on_notice=env.cursor.is_some_and(|point|self.notice_hover_rect(win).is_some_and(|rect|rect.contains(point)));
        if !inside {
            self.hover_lock = false;
        }
        if env.cursor != self.last_cursor || ctx.input(|i| !i.events.is_empty()) {
            if inside || hovering_files || ctx.input(|i| !i.events.is_empty()) {
                self.last_input = env.t;
            }
            self.last_cursor = env.cursor;
        }
        if self.music_volume_open && ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape)) {
            self.music_volume_open=false;
        }
        if self.qq_library_open && !self.overview && self.page==MUSIC_PAGE && ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape)){
            self.qq_library_open=false;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.is_editing() {
            self.cancel_edit();
            ctx.memory_mut(|memory| { if let Some(id)=memory.focused() { memory.surrender_focus(id); } });
        }
        if self.panel_pinned&&!self.is_editing()&&ctx.input(|i|i.key_pressed(egui::Key::Escape)){
            self.panel_pinned=false;self.forced_open_until=0.0;self.hover_lock=true;
        }
        if !self.overview && self.open && !self.is_editing()
            && ctx.input_mut(|i|i.consume_key(egui::Modifiers::COMMAND,egui::Key::OpenBracket)) {
            self.enter_control_center();
        }
        if (!self.is_editing() || self.todo_choosing) && inside && self.open && self.reveal > 0.95 {
            let (scroll, left, right, esc) = ctx.input(|i| (i.raw_scroll_delta.y, i.key_pressed(egui::Key::ArrowLeft), i.key_pressed(egui::Key::ArrowRight), i.key_pressed(egui::Key::Escape)));
            if esc { self.hover_lock = true;self.panel_pinned=false;self.forced_open_until=0.0; }
            let portrait_slider=side&&ctx.memory(|m|m.focused()).is_some_and(|focused|["portrait-volume","portrait-seek"].iter().any(|id|focused==egui::Id::new(("card-drag",true,*id))));
            let qq_library=!self.overview&&self.page==MUSIC_PAGE && self.qq_library_open;
            if (left || right) && !self.todo_choosing && !portrait_slider && !qq_library {
                self.wheel_delta=0.0;
                self.navigate_page(right);
            } else if scroll!=0.0 && !side && !self.overview {
                let cursor=env.cursor.unwrap()-self.content_origin(win).to_vec2();
                // Gesture routing uses the same lane as the painted page. The
                // resident/header and page content have different x transforms.
                let page_canvas=Self::horizontal_page_card(&Card::new(ui,self.content_origin(win),ui.clip_rect()));
                let page_cursor=page_canvas.to_local(env.cursor.unwrap());
                let mail_reading=self.page==MAIL_PAGE && self.mail.open_uid.is_some();
                let mail_list=self.page==MAIL_PAGE && !self.mail.accounts.is_empty() && self.mail.open_uid.is_none() && self.mail_list_bounds().contains(page_cursor);
                let agent_list=self.page==AGENT_PAGE && crate::agent_ui::TOP_LIST_BOUNDS.contains(page_cursor);
                let library_list=self.page==TODO_PAGE && !self.library_view.legacy_tasks;
                let notification_list=self.page==NOTIFICATIONS_PAGE && self.content_rect().contains(cursor);
                let shelf_list=self.page==SHELF_PAGE&&self.shelf_grid_overflows()&&self.content_rect().contains(cursor);
                let music_volume=self.page==MUSIC_PAGE && (Rect::from_min_size(Pos2::new(354.0,self.music_control_y()-14.0),Vec2::new(26.0,28.0)).contains(page_cursor)
                    || self.music_volume_open && self.music_volume_bounds().contains(page_cursor));
                if mail_reading || mail_list || music_volume || agent_list || notification_list || qq_library || library_list || shelf_list {self.wheel_delta=0.0;}
                else {
                    // 累积高精度滚轮的小幅输入；停顿或反向时丢弃上一段余量。
                    if self.t-self.wheel_at>0.18 || self.wheel_delta*scroll<0.0 {self.wheel_delta=0.0;}
                    self.wheel_at=self.t;
                    self.wheel_delta+=scroll;
                    if self.wheel_delta.abs()>=8.0 {
                        let next=self.wheel_delta<0.0;
                        self.wheel_delta=0.0;
                        let clipboard_body=self.page==CLIPBOARD_PAGE && self.content_rect().contains(cursor);
                        let todo_rows=((self.content_rect().bottom()-51.0)/22.0).floor()as usize;
                        let list=match self.page {
                            TODO_PAGE if Rect::from_min_max(Pos2::new(196.0,31.0),Pos2::new(377.0,self.content_rect().bottom())).contains(page_cursor)
                                =>Some((&mut self.todo_first,self.todos.items.len().saturating_sub(todo_rows))),
                            CLIPBOARD_PAGE if clipboard_body
                                =>Some((&mut self.clip_first,self.clipboard.lock().items.len().saturating_sub(crate::pages::CLIP_ROWS))),

                            _=>None,
                        };
                        let content_scrolls=list.as_ref().is_some_and(|(_,max)|*max>0);
                        let scrolled=list.is_some_and(|(first,max)|{
                            let target=if next {(*first+1).min(max)}else{first.saturating_sub(1)};
                            let changed=target!=*first;*first=target;changed
                        });
                        // 内容溢出时滚动列表并停在边界；未溢出时滚轮翻页。
                        if !scrolled && !content_scrolls && !self.todo_choosing && self.t-self.wheel_page_at>=0.20 {
                            self.navigate_page(next);
                            self.wheel_page_at=self.t;
                        }
                        ctx.input_mut(|i|{i.raw_scroll_delta=Vec2::ZERO;i.smooth_scroll_delta=Vec2::ZERO;});
                    }
                }
            }
        } else {self.wheel_delta=0.0;}
        let volume_dragging=(self.music_volume_open && ctx.is_being_dragged(egui::Id::new(("card-drag",true,"m-volume-track"))))
            || side && ["portrait-volume","portrait-seek"].iter().any(|id|ctx.is_being_dragged(egui::Id::new(("card-drag",true,*id))));
        let want = !env.screen_locked&&((inside && !self.hover_lock) || self.open && (on_notice||self.panel_pinned) || (self.open && self.t<self.forced_open_until) || self.drag_hot || volume_dragging || self.panel_resize.is_some() || (self.is_editing() && self.open));
        self.morph(want, inside, env.t, dt);
        if let Some(since)=self.leave_since{ctx.request_repaint_after(std::time::Duration::from_secs_f64((LEAVE_DELAY-(env.t-since)).max(0.001)));}
        if self.reduced_motion {
            self.w.snap(self.w.target);self.h.snap(self.h.target);self.slide.snap(self.slide.target);
            for dot in &mut self.dots{dot.snap(dot.target);}
        }

        // ---------- 绘制 ----------
        let p = self.progress();
        let panel=self.displayed_island_rect(win);
        let disclosure_visible=!side&&p>0.003;
        let resident=self.resident_rect(win);
        self.position_body=if side{panel}else{self.resident_rect(win)};
        let cx=panel.center().x;
        let top=panel.top();
        let (w,h)=(panel.width(),panel.height());
        let pc = anim::clamp01(p);
        let painter = ui.painter_at(ui.max_rect().expand(40.0));
        let feather = 1.0 / ctx.pixels_per_point();
        let body=panel;
        let minimal_pill = (self.effective_minimal() || self.side_handle()) && !self.island_lyrics_active() && pc < 0.02;
        let main_round=pill||disclosure_visible;
        let flare = if minimal_pill || main_round { 0.0 } else { lerp(5.0, FLARE, pc) };
        let k = (w / self.expanded_width()).clamp(0.3, 1.1);
        let thickness = if side { crate::side_dock::SIDE_BAR_W } else { BAR_H };
        let radius = if minimal_pill { w.min(h) / 2.0 } else if main_round { (w.min(h) * 0.5).min(lerp(thickness * 0.5, 26.0, pc)) } else { lerp(12.5, RAD * k.min(1.0), pc) };
        let flare_h = lerp(8.0, FLARE_H, pc);
        let (flare,flare_h,radius)=if side{crate::side_dock::shape_parameters(w,h,pc)}else{(flare,flare_h,radius)};
        let joined=disclosure_visible.then(||self.horizontal_join(win,body,pc));
        let thick = glass::thickness(PH);
        let glass_skin = self.settings.skin.is_glass();
        let layer = painter.layer_id();
        let palette = self.skin_palette();
        // 换色时跳过的图形范围（岛体底色与阴影）。
        let mut keep_colors:Vec<(usize,usize)>=Vec::new();
        // 毛玻璃：真实模糊下保留柔和投影增加层次；无模糊时阴影再减淡一些，保持轻盈。
        let skin_shadow = if !glass_skin {1.0} else if env.glass_blur {0.75} else {0.7};
        let shadow_k = glass::shadow_alpha(0.0, thick) / 0.12 * skin_shadow;
        let island_shadow=|r:Rect,flare:f32,flare_h:f32,radius:f32,strength:f32| if main_round {pill_shadow(&painter,r,radius,strength)} else if side {shape::shadow_docked(&painter,r,dock,flare,flare_h,radius,strength)} else {shape::shadow(&painter,r,flare,flare_h,radius,strength)};
        // 拿起时投影加深，像被从屏幕边缘提起来。
        let lifted = self.drag_lift.pos.clamp(0.0, 1.2);
        let shadow_strength=(smoothstep(0.1,0.9,pc)+lifted*0.9)*shadow_k;
        if let Some(joined)=&joined{joined.shadow(&painter,shadow_strength,feather);}
        else{island_shadow(body,flare,flare_h,radius,shadow_strength);}

        if self.settings.skin==model::Skin::Fluid {
            let fluid_bounds=joined.as_ref().map_or(body,|surface|surface.bounds);
            self.fluid_origin=fluid_bounds.min;
            let art=self.music_artwork.as_ref().map(|(_,texture)|texture.id());
            if art!=self.fluid_art || self.media.title!=self.fluid_title {
                let tint=if self.media.title.is_empty(){crate::fluid::Fluid::neutral()}
                    else if let Some((image,_))=&self.music_artwork{crate::fluid::Fluid::tint_from_image(image)}
                    else{crate::fluid::Fluid::tint_from_title(&self.media.title)};
                self.fluid.set_tint(tint);
                self.fluid_art=art;
                self.fluid_title.clone_from(&self.media.title);
            }
            self.fluid.set_cursor(env.cursor.filter(|p|joined.as_ref().map_or_else(||body.contains(*p),|surface|shape::contains_outline(&surface.outline,*p)))
                .map(|p|(p.x-fluid_bounds.left(),p.y-fluid_bounds.top())));
            if !self.open || self.page!=MUSIC_PAGE || self.media.title.is_empty(){self.fluid.set_seek(None,0.0);}
            self.fluid.advance(env.t,dt);
        }
        if let Some(joined)=&joined{self.paint_joined_body(&painter,joined,feather,pc);}
        else{self.paint_body(&painter,body,flare,flare_h,radius,feather,main_round);}
        let resident_minimal=self.effective_minimal()&&!self.island_lyrics_active();
        if lifted>0.01 {
            if let Some(joined)=&joined {
                painter.add(egui::Shape::closed_line(joined.inset_outline(-1.0),egui::Stroke::new(1.2,Color32::from_white_alpha((90.0*lifted.min(1.0)) as u8))));
            }else{self.paint_lift_halo(&painter,body,flare,flare_h,radius,lifted.min(1.0));}
        }
        // Connect the hardware camera to the content without placing text or
        // controls in its exclusion zone. Never include this in native hits.
        if !side {if let DesktopPlacement::Camera{bridge,..}=self.desktop_placement {
            let mut bridge=bridge.translate(win.min.to_vec2());
            bridge.max.y=resident.top()+1.0/ctx.pixels_per_point();
            let start=crate::skin_paint::shape_count(&ctx,layer);
            painter.rect_filled(bridge,0.0,Color32::BLACK);
            keep_colors.push((start,crate::skin_paint::shape_count(&ctx,layer)));
        }}
        let content_from=crate::skin_paint::shape_count(&ctx,layer);

        // 内容只随形体「揭开」：不缩放，按面板裁剪，到 55% 才开始淡入
        let reveal = if side{crate::side_motion::content_alpha(p)}else if self.open { smoothstep(0.50, 0.97, p) } else { smoothstep(0.55, 0.92, p) };
        self.reveal = reveal;
        let origin = self.content_origin(win);
        let clip = panel.intersect(win);
        let base = Card::new(ui, origin, clip);
        self.cursor_local = env.cursor.map(|p| base.to_local(p));

        if side {
            self.resident_header(&base, panel, cx, pc, reveal, env);
        }else if !resident_minimal{
            let resident_base=Card::new(ui,Pos2::new(resident.center().x-self.expanded_width()/2.0,resident.top()),resident.intersect(win));
            self.shared_header(&resident_base,resident.center().x,0.0,0.0,env);
        }
        if reveal > 0.004 {
            let c = base.shifted(0.0, (1.0 - reveal) * -5.0).faded(reveal);
            let c = if reveal < 0.6 { c.ghost() } else { c };
            if side {self.portrait_panel(&c,env,&mut cmds);}
            else {self.shared_header(&c,self.horizontal_panel_rect(win,Vec2::new(self.expanded_width(),self.expanded_height())).center().x,1.0,reveal,env);self.panel_body(&c,env,&mut cmds);}
            if !self.toast_below {
                let bounds=self.content_rect();
                self.toast_layer(&c.clipped(bounds.left(),bounds.top(),bounds.width(),bounds.height()),bounds.bottom()-26.0);
            }
        }
        // 岛下的确认提示使用独立裁剪区域，收起时跟随岛体下沿。
        let floating_toast=if self.toast_below && self.toast.is_some() {
            // 侧边停靠：提示出现在胶囊正下方，靠边时向内挪，保证完整显示。
            let toast_origin=if side {
                let center=if dock==model::Dock::Left {panel.center().x.max(win.left()+90.0)} else {panel.center().x.min(win.right()-90.0)};
                Pos2::new(center-self.expanded_width()/2.0,panel.top())
            } else {origin};
            self.toast_layer(&Card::new(ui,toast_origin,win),h+8.0)
        }else{None};
        if (self.adding||self.editing.is_some()) && reveal > 0.9 && self.page == TODO_PAGE && self.page_e() > 0.25 {
            self.input_field(ui, origin, &mut cmds);
        }

        if self.task_dialog.is_some() && reveal>0.9 {self.task_editor_inputs(ui,origin,&mut cmds);}

        // Independent row: paint after the island content while retaining the
        // same root point coordinates for native hit testing and its blur mask.
        let from=crate::skin_paint::shape_count(&ctx,layer);
        let notice_surface=self.draw_floating_notice(ui,win,env.opaque_surface,&mut cmds);
        keep_colors.push((from,crate::skin_paint::shape_count(&ctx,layer)));

        // ---------- 输出 ----------
        let rel = |r: Rect| r.translate(-win.min.to_vec2());
        let mut region = if matches!(self.desktop_placement,DesktopPlacement::Positioned(_)){panel.expand(32.0)} else if side {
            // 侧边：面板从边缘滑出，区域包含肩部（沿边缘上下）与外侧阴影，并一直延伸到屏幕边缘。
            let r=if p > 0.02 || self.open {
                let height=self.expanded_height().max(self.h.pos)*1.08+FLARE*2.0+24.0;
                self.island_rect(win,self.expanded_width()+20.0,height).union(panel.expand(20.0))
            } else {
                let (bw,bh)=self.base_size();
                self.island_rect(win,bw*SWELL.1+14.0,bh*SWELL.0+FLARE+16.0).union(panel.expand(12.0))
            };
            if dock==model::Dock::Left {Rect::from_min_max(Pos2::new(win.left(),r.top()),r.max)} else {Rect::from_min_max(r.min,Pos2::new(win.right(),r.bottom()))}
        } else if p > 0.02 || self.open {
            let height=self.expanded_height().max(self.h.pos)*1.08+16.0;
            Rect::from_center_size(Pos2::new(cx, top + height/2.0), Vec2::new(PW + FLARE * 2.0 + 56.0, height))
        } else {
            let (bw, bh) = self.base_size();
            Rect::from_center_size(Pos2::new(cx, top + (bh * SWELL.1 + 12.0) / 2.0), Vec2::new(bw * SWELL.0 + 24.0, bh * SWELL.1 + 12.0))
        };
        // 拿起放大、甩动拉伸以及加深的投影都要留在窗口区域内，否则两端会被系统裁掉。
        if self.drag_lift.pos.abs()>0.005 || self.drag_stretch.pos.abs()>0.001 {region=region.union(panel.expand2(Vec2::new(18.0,22.0)));}
        if let Some(joined)=&joined{region=region.union(joined.bounds.expand(18.0));}
        if let Some(surface)=notice_surface {region=region.union(surface.rect.expand(8.0));}
        if !side {if let DesktopPlacement::Camera{bridge,..}=self.desktop_placement {region=region.union(bridge.translate(win.min.to_vec2()));}}
        // 胶囊上方留空的那一小条也要在窗口区域内，阴影才不会被截断。
        if pill && !side && !matches!(self.desktop_placement,DesktopPlacement::Positioned(_)) {region.min.y=region.min.y.min(win.top());}
        // 浅色皮肤：把这一帧岛体内容统一换成皮肤配色。
        if let Some(palette)=palette {
            let end=crate::skin_paint::shape_count(&ctx,layer);
            let mut ranges=Vec::new();let mut at=content_from;
            keep_colors.sort();
            for (from,to) in keep_colors {if from>at {ranges.push((at,from));} at=at.max(to);}
            if end>at {ranges.push((at,end));}
            crate::skin_paint::recolor_layer(&ctx,layer,&ranges,&palette);
        }
        // 原生裁剪还需包含悬停提示；只扩大绘制区域，保持桌面点击穿透。
        let mut tooltips=Vec::new();
        ctx.memory(|memory|{
            for layer in memory.areas().visible_layer_ids() {
                if layer.order==egui::Order::Tooltip {
                    if let Some(rect)=memory.area_rect(layer.id){region=region.union(rect.expand(12.0));tooltips.push(rect);}
                }
            }
        });
        // 毛玻璃：窗口区域只取岛体与浮层的真实轮廓，系统模糊不会溢出到透明画布上。
        let outline=|r:Rect,flare:f32,flare_h:f32,radius:f32,round:bool| if round {GlassShape::Round(rel(r),radius.min(r.height().min(r.width())*0.5))}
                else if side {GlassShape::Docked{rect:rel(r),dock,flare,flare_h,radius}} else {GlassShape::Notch{rect:rel(r),flare,flare_h,radius}};
        let painted_shapes=vec![if let Some(joined)=&joined {
            GlassShape::Outline(joined.outline.iter().map(|point|*point-win.min.to_vec2()).collect())
        }else{outline(body,flare,flare_h,radius,main_round)}];
        let glass_region=(glass_skin && self.settings.glass_blur || env.opaque_surface).then(||{
            let mut shapes=painted_shapes.clone();
            if let Some(surface)=notice_surface {shapes.push(GlassShape::Round(rel(surface.rect),surface.radius));}
            if let Some(r)=floating_toast {shapes.push(GlassShape::Round(rel(r),11.0));}
            shapes.extend(tooltips.iter().map(|r|GlassShape::Round(rel(*r),6.0)));
            shapes
        });
        // Once a minimal island starts closing, its departing panel is only
        // decoration. Release browser tabs immediately and accept re-entry only
        // on the resident; its closing material and blur keep their full shape.
        let hit_shapes=if !side&&self.effective_minimal()&&!self.open&&!self.island_lyrics_active(){
            vec![GlassShape::Outline(self.floating_outline(resident,0.0).into_iter().map(|point|point-win.min.to_vec2()).collect())]
        }else{painted_shapes};
        // A drag can change the dock in update_position_drag. Preserve this
        // frame's painted geometry for AppKit; the next frame paints the new
        // edge. Input must never jump to an unpainted edge for one frame.
        let frame_hit_outline=Some(hit_shapes[0].outline());
        let frame_hit=hit_shapes.iter().map(|shape|Rect::from_points(&shape.outline())).reduce(Rect::union).unwrap();
        let frame_drop=self.settings.visible_pages[SHELF_PAGE].then(||rel(if side{self.island_rect(win,self.expanded_width(),self.expanded_height())}else{self.horizontal_panel_rect(win,Vec2::new(self.expanded_width(),self.expanded_height())).union(resident)}));
        self.update_position_drag(&ctx,env,dt);
        let transitioning = self.position_drag.is_some() || self.panel_resize.is_some() || self.drag_glide.is_some() || !self.drag_lift.settled(1.0) || !self.drag_stretch.settled(1.0) || !self.w.settled(self.expanded_width())
            || !self.h.settled(self.expanded_height())
            || self.page_e() < 0.7
            || self.dots.iter().any(|d| !d.settled(16.0))
            || self.hover_since.is_some() && !self.open
            || self.leave_since.is_some()
            || (self.drag_a > 0.001 && self.drag_a < 0.999)
            || self.drag_hot
            || !self.resident.settled()
            || self.open && self.page==SHELF_PAGE && self.shelf_x.values().any(|s| !s.settled(68.0))
            || self.toast.is_some()
            || self.notice_motion.animating();
        let companion_ambient=(!side||!self.open)&&!resident_minimal&&!self.notification_locked&&!self.reduced_motion
            && (self.settings.pet||self.resident_team().running>0);
        let ambient=companion_ambient || self.open&&self.overview&&!self.reduced_motion || self.settings.skin==model::Skin::Fluid && self.fluid.animating()
            || self.music_playing() && (self.open && self.page==MUSIC_PAGE || (!side||!self.open) && !resident_minimal && self.media.is_music());
        let animating=transitioning || ambient;
        // macOS presents with vsync. Request the next display opportunity
        // during transitions instead of waiting 16.7ms before another vsync.
        let repaint_after=if cfg!(target_os="macos")&&transitioning{std::time::Duration::ZERO}
            else{std::time::Duration::from_secs_f64(if transitioning{1.0/60.0}else if ambient{1.0/30.0}else{1.0})};
        cmds.append(&mut self.pending);
        FrameOut {
            region: rel(floating_toast.into_iter().chain(self.shelf_menu_rect.map(|r|r.union(r + ctx.style().visuals.popup_shadow.margin()).expand(1.0 / ctx.pixels_per_point()))).fold(region,Rect::union).intersect(ui.max_rect())),
            glass: glass_region,
            hit:frame_hit,
            hit_outline:frame_hit_outline,
            hit_shapes,
            notice_hit:notice_surface.map(|surface|surface.rect).into_iter().chain(self.shelf_menu_rect).reduce(Rect::union).map(rel),
            notice_shapes:notice_surface.map(|surface|GlassShape::Round(rel(surface.rect),surface.radius)).into_iter()
                .chain(self.shelf_menu_rect.map(|rect|GlassShape::Round(rel(rect),0.0))).collect(),
            file_drop_zone:frame_drop,
            expanded: self.open,
            // AppKit normally keeps the island nonactivating. A position
            // drag needs keyboard focus as well so Escape reaches this window.
            editing: self.is_editing() || self.position_drag.is_some(),
            animating,
            repaint_after,
            cmds,
        }
    }

    fn hit_rect(&self, cx: f32, top: f32, p: f32) -> Rect {
        let r=self.hit_rect_base(cx,top,p);
        // 胶囊离屏幕顶边有一段空隙：把光标推到屏幕最顶端也要能唤出岛。
        if self.island_shape()==model::IslandShape::Pill && self.desktop_placement==DesktopPlacement::Legacy {Rect::from_min_max(Pos2::new(r.left(),r.top()-PILL_GAP),r.max)} else {r}
    }

    fn floating_outline(&self,body:Rect,p:f32)->Vec<Pos2>{
        let minimal=self.effective_minimal()&&!self.island_lyrics_active()&&p<0.02;
        let radius=if minimal{body.height().min(body.width())/2.0}else{(body.width().min(body.height())*0.5).min(lerp(BAR_H*0.5,26.0,anim::clamp01(p)))};
        GlassShape::Round(body,radius).outline()
    }

    pub(crate) fn horizontal_join(&self,win:Rect,panel:Rect,p:f32)->std::sync::Arc<shape::JoinedSurface>{
        let resident=self.resident_rect(win);
        let round=self.island_shape()==model::IslandShape::Pill||(self.effective_minimal()&&!self.island_lyrics_active());
        let camera=match self.desktop_placement {
            DesktopPlacement::Positioned(plan)=>plan.camera.map(|rect|rect.translate(win.min.to_vec2())),
            _=>None,
        };
        let spec=shape::JoinSpec{
            resident,panel,
            resident_radius:if round{resident.width().min(resident.height())*0.5}else{12.5},
            panel_radius:(panel.width().min(panel.height())*0.5).min(lerp(BAR_H*0.5,26.0,anim::clamp01(p))),
            resident_flare:if round{0.0}else{5.0},progress:anim::clamp01(p),
            room:self.screen.unwrap_or(win),camera,
        };
        if let Some((previous,surface))=self.joined_surface.borrow().as_ref(){
            if *previous==spec{return surface.clone();}
        }
        let surface=std::sync::Arc::new(shape::JoinedSurface::new(spec));
        *self.joined_surface.borrow_mut()=Some((spec,surface.clone()));
        surface
    }

    fn hit_rect_base(&self, cx: f32, top: f32, p: f32) -> Rect {
        if self.effective_minimal() && !self.open {
            let (width,height)=self.base_size();
            let width=self.w.pos.min(width);
            let height=self.h.pos.min(height);
            Rect::from_min_size(Pos2::new(cx-width/2.0,top),Vec2::new(width,height))

        } else if p > 0.04 || self.open {
            Rect::from_min_size(Pos2::new(cx - PW / 2.0 - 4.0, top), Vec2::new(PW + 8.0, self.expanded_height() + 2.0))
        } else {
            let (bw,bh)=self.base_size();
            let (hit_w,hit_h)=(bw+12.0,bh+4.0);
            Rect::from_min_size(Pos2::new(cx-hit_w/2.0,top),Vec2::new(hit_w,hit_h))
        }
    }

    /// 展开胶囊高度：一个是圆，两个是短胶囊，三个及以上与面板等高（多出的滚动）。

    fn notice_hover_rect(&self,win:Rect)->Option<Rect>{
        self.display_notice()?;
        self.notice_surface(win).map(|surface|surface.rect)
    }

    /// 当前皮肤的换色调色板（经典为 None）。
    pub(crate) fn skin_palette(&self)->Option<crate::skin_paint::Palette> {
        match self.settings.skin {model::Skin::Cream=>Some(crate::skin_paint::CREAM),model::Skin::GlassLight=>Some(crate::skin_paint::GLASS_LIGHT),model::Skin::GlassDark=>Some(crate::skin_paint::GLASS_DARK),model::Skin::Classic|model::Skin::Fluid=>None}
    }

    /// 设置页预览用的岛体底色：没有真实模糊，毛玻璃按较实的玻璃表现。
    pub(crate) fn preview_fill(&self)->Color32 {
        match self.settings.skin {
            model::Skin::GlassDark=>{let (r,g,b)=glass::GROUND_DARK;Color32::from_rgba_unmultiplied(r,g,b,215)}
            model::Skin::GlassLight=>{let [r,g,b]=glass::LIGHT_GROUND.map(|v|(v*255.0).round() as u8);Color32::from_rgba_unmultiplied(r,g,b,225)}
            _=>self.glass_fill(),
        }
    }

    /// 岛体底色：经典为纯黑；奶油为暖白；毛玻璃按当前着色量叠加深色或浅色玻璃。
    pub(crate) fn glass_fill(&self)->Color32 {
        let alpha=(self.glass_tint.clamp(0.0,1.0)*255.0).round() as u8;
        match self.settings.skin {
            model::Skin::GlassDark=>{let (r,g,b)=glass::GROUND_DARK;Color32::from_rgba_unmultiplied(r,g,b,alpha)}
            model::Skin::GlassLight=>{let [r,g,b]=glass::LIGHT_GROUND.map(|v|(v*255.0).round() as u8);Color32::from_rgba_unmultiplied(r,g,b,alpha)}
            model::Skin::Cream=>{let [r,g,b]=crate::skin_paint::CREAM.paper;Color32::from_rgb(r,g,b)}
            model::Skin::Classic=>if cfg!(target_os="macos") && (self.side() || matches!(self.desktop_placement,DesktopPlacement::Floating{..}|DesktopPlacement::Positioned(_))){Color32::from_rgb(19,43,48)}else{Color32::BLACK},
            model::Skin::Fluid=>self.fluid.compose(0.5,0.5,1.0,1.0,0.0),
        }
    }

    /// 毛玻璃边缘高光：浅色玻璃更亮，像苹果的玻璃边缘。
    pub(crate) fn glass_rim(&self)->Color32 {
        let strength=if self.settings.skin==model::Skin::GlassLight {0.55} else {glass::rim_alpha()*0.7};
        Color32::from_white_alpha((255.0*strength).round() as u8)
    }

    /// 绘制岛体（或通知胶囊）的本体：经典皮肤为纯黑；毛玻璃为着色底 + 顶部柔光 + 边缘高光。
    fn paint_joined_body(&self,painter:&egui::Painter,surface:&shape::JoinedSurface,feather:f32,progress:f32){
        let skin=self.settings.skin;
        let bounds=surface.bounds;
        if skin==model::Skin::Fluid {
            painter.add(egui::Shape::mesh(surface.mesh(feather,&|x,y|{
                self.fluid.compose(x-bounds.left(),y-bounds.top(),bounds.width(),bounds.height(),progress)
            },true)));
            return;
        }
        let fill=self.glass_fill();
        painter.add(egui::Shape::mesh(surface.mesh(feather,&|_,_|fill,false)));
        if !skin.is_glass(){return;}
        if skin==model::Skin::GlassLight {
            let span=(bounds.height()*0.6).min(64.0);
            painter.add(egui::Shape::mesh(surface.mesh(feather,&|_,y|{
                Color32::from_white_alpha((15.0*(1.0-(y-bounds.top())/span).clamp(0.0,1.0)).round() as u8)
            },false)));
        }
        painter.add(egui::Shape::closed_line(surface.inset_outline(0.5),egui::Stroke::new(1.0,self.glass_rim())));
    }

    fn paint_body(&self,painter:&egui::Painter,rect:Rect,flare:f32,flare_h:f32,radius:f32,feather:f32,pill:bool){
        let skin=self.settings.skin;
        if skin==model::Skin::Fluid {
            let (map,frame)=shape::EdgeMap::of(rect,if pill {model::Dock::Top}else{self.dock()});
            let mut rows=if self.side(){shape::side_sections(frame,flare,flare_h,radius)}else{shape::sections(frame,flare,flare_h,radius)};
            if pill {
                let mid=frame.center().y;
                rows.retain(|row|row.0>mid && row.0>=frame.bottom()-radius);
                let top=rows.iter().rev().map(|&(y,l,r)|(2.0*mid-y,l,r)).collect::<Vec<_>>();
                rows.splice(0..0,top);
            }
            let expand=self.progress().clamp(0.0,1.0);
            let mut mesh=shape::shade_mesh(frame,rows,feather,&|x,y|{
                let p=map.apply(Pos2::new(x,y))-rect.min;
                self.fluid.compose(p.x,p.y,rect.width(),rect.height(),expand)
            });
            if self.dock().is_side() && !pill {
                for vertex in &mut mesh.vertices {vertex.pos=map.apply(vertex.pos);}
            }
            painter.add(egui::Shape::mesh(mesh));
            return;
        }
        if pill {
            let radius=radius.min(rect.height().min(rect.width())*0.5);
            painter.rect_filled(rect,radius,self.glass_fill());
            if skin.is_glass() && rect.height()>=10.0 {
                painter.rect_stroke(rect,radius,egui::Stroke::new(1.0_f32,self.glass_rim()),egui::StrokeKind::Inside);
            }
            return;
        }
        let dock=self.dock();
        if dock.is_side() {
            shape::fill_docked(painter,rect,dock,flare,flare_h,radius,self.glass_fill(),feather);
            if !skin.is_glass() {return;}
            if skin==model::Skin::GlassLight {
                let sheen=Color32::from_white_alpha(15);
                shape::fill_gradient_docked(painter,rect,dock,flare,flare_h,radius,sheen,Color32::TRANSPARENT,(rect.width()*0.6).min(64.0),feather);
            }
            if rect.height()>=20.0 && rect.width()>=10.0 {
                let edge=if dock==model::Dock::Left {Rect::from_min_max(rect.min+Vec2::new(0.0,0.5),rect.max-Vec2::new(0.5,0.5))}
                    else {Rect::from_min_max(rect.min+Vec2::new(0.5,0.5),rect.max-Vec2::new(0.0,0.5))};
                painter.add(egui::Shape::line(shape::outline_docked(edge,dock,flare,flare_h,(radius-0.5).max(0.0)),egui::Stroke::new(1.0_f32,self.glass_rim())));
            }
            return;
        }
        shape::fill(painter,rect,flare,flare_h,radius,self.glass_fill(),feather);
        if !skin.is_glass() {return;}
        // 玻璃厚度带来的上亮下暗：顶部白光在前 60% 高度（最多 64 点）内淡出；浅色玻璃更亮。
        if skin==model::Skin::GlassLight {
            let sheen=Color32::from_white_alpha(15);
            shape::fill_gradient(painter,rect,flare,flare_h,radius,sheen,Color32::TRANSPARENT,(rect.height()*0.6).min(64.0),feather);
        }
        if rect.height()>=10.0 && rect.width()>=20.0 {
            let edge=Rect::from_min_max(rect.min+Vec2::new(0.5,0.0),rect.max-Vec2::new(0.5,0.5));
            painter.add(egui::Shape::line(shape::outline(edge,flare,flare_h,(radius-0.5).max(0.0)),egui::Stroke::new(1.0_f32,self.glass_rim())));
        }
    }

    /// 拿起时沿岛体轮廓的一圈柔光：外圈宽而淡，内圈细而亮。
    fn paint_lift_halo(&self,painter:&egui::Painter,rect:Rect,flare:f32,flare_h:f32,radius:f32,amount:f32) {
        if rect.width()<10.0 || rect.height()<10.0 {return;}
        let pill=self.island_shape()==model::IslandShape::Pill;
        let dock=self.dock();
        for (grow,width,alpha) in [(2.0f32,4.0f32,22.0f32),(0.5,1.2,90.0)] {
            let stroke=egui::Stroke::new(width,Color32::from_white_alpha((alpha*amount) as u8));
            let edge=rect.expand(grow);
            if pill {
                let r=radius.min(rect.height().min(rect.width())*0.5)+grow;
                painter.rect_stroke(edge,r,stroke,egui::StrokeKind::Middle);
            } else if dock.is_side() {
                painter.add(egui::Shape::line(shape::outline_docked(edge,dock,flare,flare_h,radius+grow),stroke));
            } else {
                painter.add(egui::Shape::line(shape::outline(edge,flare,flare_h,radius+grow),stroke));
            }
        }
    }

    fn update_position_drag(&mut self,ctx:&egui::Context,env:&Env,dt:f32) {
        if let Some(mut drag)=self.position_drag.take() {
            let cancelled=ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape));
            if cancelled {
                self.position_drag_cancelled=true;
                self.settings.display_id=drag.original.0.clone();
                (self.settings.dock,self.settings.island_offset,self.settings.island_position)=(drag.original.1,drag.original.2,drag.original.3);
                self.drag_glide=None;self.hover_lock=!drag.expanded;
                let size=if drag.expanded{Vec2::new(self.expanded_width(),self.expanded_height())}else{self.resident_size()};
                self.w.snap(size.x);self.h.snap(size.y);self.side_motion.reset(drag.expanded);
            }else{
                let mut speed=0.0;
                let local_geometry=env.drag_geometry.as_ref().and_then(|geometry|{
                    let display=crate::island_position::nearest_display_in_rooms(geometry.cursor,&geometry.displays,&geometry.rooms)?;
                    let bounds=geometry.rooms.iter().find(|(id,_)|id==&display.id).map_or(display.bounds,|(_,bounds)|*bounds);
                    let (screen,cursor)=crate::island_position::display_local_geometry(geometry.cursor,bounds,geometry.scale)?;
                    Some((cursor,screen,geometry.scale,Some(display.id.clone())))
                });
                let scale=match self.desktop_placement{DesktopPlacement::Positioned(plan)=>plan.scale,_=>self.settings.island_scale()*ctx.zoom_factor()};
                let geometry=local_geometry.or_else(||env.cursor.zip(env.screen).map(|(cursor,screen)|(cursor,screen,scale,None)));
                if let Some((cursor,screen,scale,target_display))=geometry {
                    let display_changed=target_display.as_ref().is_some_and(|id|id!=&drag.display);
                    if let Some(target_display)=target_display {
                        drag.display=target_display.clone();
                        self.settings.display_id=if target_display==drag.display_at_start{drag.original.0.clone()}else{target_display};
                    }
                    let size=self.position_body.size();
                    let mut center=cursor-drag.grab*size;
                    // An expanded side panel has a wide center. Measure pull
                    // distance from its attached edge, with the same 44pt
                    // resident reference used by a collapsed side island.
                    let reference=match self.dock(){
                        model::Dock::Left=>center-Vec2::new(size.x/2.0-crate::side_dock::SIDE_BAR_W/2.0,0.0),
                        model::Dock::Right=>center+Vec2::new(size.x/2.0-crate::side_dock::SIDE_BAR_W/2.0,0.0),
                        model::Dock::Top=>center,
                    };
                    let dock=crate::island_position::snap(self.dock(),reference,screen,scale);
                    let switched=dock!=self.dock();
                    if switched {
                        self.settings.dock=dock;
                        let expanded=if drag.expanded{Vec2::new(self.expanded_width(),self.expanded_height())}else{self.resident_size()};
                        self.w.snap(expanded.x);self.h.snap(expanded.y);
                        self.side_motion.reset(drag.expanded);
                        let size=if dock.is_side(){expanded}else{self.resident_size()};
                        let body=crate::island_position::body(screen,center,dock,size);
                        drag.grab=(cursor-body.center())/body.size();
                        center=body.center();
                        self.drag_stretch.snap(0.0);
                    }
                    // Store the edge itself when the pointer reaches it. A
                    // lifted body's temporary height/changed grab offset must
                    // not leave a gap after release and spring settling.
                    if cursor.y<=screen.top()+1.0/scale {center.y=screen.top();}
                    let position=crate::island_position::normalized(center,screen).map(|value|value.clamp(0.0,1.0));
                    self.settings.island_position=Some(position);
                    // Keep an approximate legacy offset for older versions.
                    self.settings.island_offset=((if dock.is_side(){position[1]}else{position[0]})*200.0-100.0).round() as i32;
                    let glide=self.drag_glide.get_or_insert_with(||position.map(|value|Spring::new(value,GLIDE_FOLLOW.0,GLIDE_FOLLOW.1)));
                    for (spring,value) in glide.iter_mut().zip(position){spring.target=value;if switched||display_changed{spring.snap(value);}}
                    speed=Vec2::new(glide[0].vel*screen.width(),glide[1].vel*screen.height()).length()*scale;
                }
                self.drag_stretch.target=(speed/STRETCH_SPEED).min(1.0)*STRETCH_MAX;
                if !ctx.input(|i|i.pointer.primary_down()) {
                    self.hover_lock=!drag.expanded;
                    self.drag_stretch.target=0.0;
                    if let Some(glide)=&mut self.drag_glide {for spring in glide{spring.tune(GLIDE_SETTLE.0,GLIDE_SETTLE.1);}}
                    if self.settings.display_id!=drag.original.0 || (self.dock(),self.settings.island_offset,self.settings.island_position)!=(drag.original.1,drag.original.2,drag.original.3){self.settings.save();}
                }else{self.position_drag=Some(drag);}
            }
        }
        self.drag_lift.target=if self.position_drag.is_some(){1.0}else{0.0};
        if self.position_drag.is_none(){self.drag_stretch.target=0.0;}
        if self.reduced_motion{self.drag_lift.snap(self.drag_lift.target);self.drag_stretch.snap(self.drag_stretch.target);}
        else{self.drag_lift.step(dt);self.drag_stretch.step(dt);}
        if let Some(glide)=&mut self.drag_glide {
            for spring in glide.iter_mut(){if self.reduced_motion{spring.snap(spring.target);}else{spring.step(dt);}}
            if self.position_drag.is_none()&&glide.iter().all(|spring|spring.settled(0.2)){self.drag_glide=None;}
        }
    }

    pub fn placement_offset(&self)->f32{self.settings.island_offset_ratio()}

    /// Both native frame placement and the local drawing plan consume this
    /// position. A restored legacy configuration stays on its old path.
    pub fn placement_position(&self)->Option<[f32;2]>{
        self.settings.island_position.map(|position|self.drag_glide.map_or(position,|glide|glide.map(|spring|spring.pos.clamp(0.0,1.0))))
    }

    /// 拿起与速度拉伸带来的外形缩放：(宽, 高)。
    fn drag_scale(&self)->(f32,f32) {
        let lift=self.drag_lift.pos.max(0.0);let stretch=self.drag_stretch.pos.max(0.0);
        let along=1.0+LIFT_SCALE.0*lift+stretch;
        let across=1.0+LIFT_SCALE.1*lift-stretch*0.5;
        if self.side() {(across,along)}else{(along,across)}
    }

    fn start_position_drag(&mut self,start:Pos2,screen:Rect,env:&Env) {
        if self.position_drag.is_some()||self.position_drag_cancelled||!self.position_body.is_positive(){return;}
        let original=(self.settings.display_id.clone(),self.dock(),self.settings.island_offset,self.settings.island_position);
        // The first movement event can already be on another monitor. The
        // source is the window's selected display, never the moved pointer.
        let display_at_start=env.drag_geometry.as_ref().and_then(|geometry|crate::display_layout::selected(&geometry.displays,&self.settings.display_id))
            .map_or_else(||self.settings.display_id.clone(),|display|display.id.clone());
        let body=self.position_body;
        let position=crate::island_position::normalized(body.center(),screen).map(|value|value.clamp(0.0,1.0));
        self.position_drag=Some(PositionDrag{grab:(start-body.center())/body.size(),original,display:display_at_start.clone(),display_at_start,expanded:self.open});
        self.settings.island_position=Some(position);
        self.drag_glide=Some(position.map(|value|Spring::new(value,GLIDE_FOLLOW.0,GLIDE_FOLLOW.1)));
        self.drag_lift.vel=self.drag_lift.vel.max(4.0);
    }

    /// 悬停状态机：意图缓冲 → 弹簧展开 → 离开延迟 → 临界阻尼收起；全程可打断。
    fn morph(&mut self, want: bool, inside: bool, t: f64, dt: f32) {
        let (bw, bh) = self.base_size();
        if let Some(drag)=&self.position_drag {
            self.open=drag.expanded;
            self.hover_lock=true;
            self.hover_since=None;
            self.leave_since=None;
            self.forced_open_until=0.0;
            self.music_volume_open=false;
            self.w.tune(CLOSE.0, CLOSE.1);
            self.h.tune(CLOSE.0, CLOSE.1);
            self.w.target=if drag.expanded{self.expanded_width()}else{bw};
            self.h.target=if drag.expanded{self.expanded_height()}else{bh};
        } else if want {
            self.leave_since = None;
            let since = *self.hover_since.get_or_insert(t);
            let intent=if self.side(){0.45}else if self.effective_minimal() {MINIMAL_HOVER_INTENT}else{HOVER_INTENT};
            if !self.open && (self.drag_hot || t - since >= intent) {
                // 只有从收起态经普通悬停展开时回到主控。明确导航、
                // 文件拖入与正在编辑的内容保留其目的地。
                if !self.drag_hot && !self.is_editing() {self.enter_control_center();}
                self.open = true;
                self.w.tune(OPEN_W.0, OPEN_W.1);
                self.h.tune(OPEN_H.0, OPEN_H.1);
                self.w.target = self.expanded_width();
                self.h.target = self.expanded_height();
                if self.drag_hot {
                    // 拖入不等缓冲，直接给一点初速度，30 ms 内就能看到形变
                    self.w.vel = self.w.vel.max(600.0);
                    self.h.vel = self.h.vel.max(260.0);
                }
            } else if !self.open {
                self.w.tune(SWELL_K.0, SWELL_K.1);
                self.h.tune(SWELL_K.0, SWELL_K.1);
                // 悬停时轻微胀大：厚度方向胀得多一点（侧边停靠时厚度是宽度）。
                let (sw,sh)=if self.side(){(SWELL.1,SWELL.0)}else{SWELL};
                self.w.target = bw * if self.side(){sw}else{1.0};
                self.h.target = bh * if self.side(){sh}else{1.0};
            }
        } else {
            self.hover_since = None;
            if self.open {
                let since = *self.leave_since.get_or_insert(t);
                if t - since >= LEAVE_DELAY {
                    self.open = false;
                    self.music_volume_open=false;
                    self.leave_since = None;
                    self.w.tune(CLOSE.0, CLOSE.1);
                    self.h.tune(CLOSE.0, CLOSE.1);
                    self.w.target = bw;
                    self.h.target = bh;
                    self.cancel_edit();
                    self.confirm_clear = None;
                }
            } else {
                self.w.tune(CLOSE.0, CLOSE.1);
                self.h.tune(CLOSE.0, CLOSE.1);
                self.w.target = bw;
                self.h.target = bh;
            }
        }
        let _ = inside;
        if self.open {self.w.target=self.expanded_width();self.h.target=self.expanded_height();}
        let (prev_w,prev_h)=(self.w.pos,self.h.pos);
        if self.side(){
            let preview=want&&!self.open&&!self.hover_lock&&self.position_drag.is_none();
            let (size,velocity,target)=self.side_motion.step(Vec2::new(bw,bh),self.side_panel_size(),self.open,preview,dt,self.reduced_motion);
            self.w.pos=size.x;self.h.pos=size.y;self.w.vel=velocity.x;self.h.vel=velocity.y;
            self.w.target=target.x;self.h.target=target.y;
        }else{self.side_motion.deactivate();self.w.step(dt);self.h.step(dt);}
        // 仅在从外侧越过目标时收住，扩展提示条时不强制跳到目标宽度。
        if !self.open && !self.side(){
            if prev_w>=bw && self.w.pos<bw {self.w.snap(bw);}
            if prev_h>=bh && self.h.pos<bh {self.h.snap(bh);}
        }
        for d in &mut self.dots {
            d.step(dt);
        }
        self.slide.step(dt);
    }

    // ---------------------------------------------------------------
    // 顶栏：日期与时钟，常驻组件沿共享元素过渡到展开位置。
    // ---------------------------------------------------------------
    fn shared_header(&mut self, base: &Card, cx: f32, p: f32, reveal: f32, env: &Env) {
        let local_cx = cx - base.origin.x;
        let fit=((WIN_W-32.0)/self.resident.target.width).min(1.0);
        let fitted=base.scaled(local_cx,15.0,lerp(fit,1.0,p));
        let base=&fitted;
        let lyric=self.island_lyric().map(|(index,_)|index);
        let lyric_alpha=if self.island_lyrics_active() {1.0-smoothstep(0.0,0.35,p)}else{0.0};
        if let Some(galley)=self.island_lyric_layout.clone().filter(|_|lyric_alpha>0.003) {
            let c=base.faded(lyric_alpha);
            let c=if lyric_alpha<0.8{c.ghost()}else{c};
            let content_width=self.lyric_bar_width();
            let width=self.resident_size().x.max(content_width);
            let left=local_cx-width/2.0;
            let center_y=BAR_H/2.0;
            let art_x=left+if self.settings.pet{36.0}else{12.0};
            if self.settings.pet{self.draw_resident_pet(&c,left+18.0,center_y,26.0);}
            if let Some((image,texture))=&self.music_artwork {
                let [width,height]=image.size;
                let uv_size=if width>height {egui::vec2(height as f32/width as f32,1.0)}else{egui::vec2(1.0,width as f32/height as f32)};
                let uv=Rect::from_center_size(Pos2::new(0.5,0.5),uv_size);
                c.painter.add(egui::epaint::RectShape::filled(c.rect(art_x,center_y-10.0,20.0,20.0),5.0*c.s,Color32::WHITE).with_texture(texture.id(),uv));
            } else {
                c.fill(art_x,center_y-10.0,20.0,20.0,5.0,theme::CARD);
                paint::note(&c,art_x+10.0,center_y,13.0,theme::WHITE);
            }
            let index=lyric.unwrap_or(0);
            let id=egui::Id::new("island-lyric-line");
            if !self.ly.island_animation_started {
                c.ui.ctx().animate_value_with_time(id,index as f32,0.0);
                self.ly.island_animation_started=true;
            }
            let shown=c.ui.ctx().animate_value_with_time(id,index as f32,0.18);
            let fade=1.0-(index as f32-shown).abs().min(1.0);
            let (text_left,text_right)=self.lyric_content_insets();
            let position=c.pos(left+(text_left+width-text_right)/2.0,center_y-galley.size().y/2.0+(1.0-fade)*3.0);
            c.painter.galley(position,galley,if lyric.is_some(){theme::WHITE}else{theme::GRAY});
            if self.module_on(AGENT_PAGE){self.draw_resident_team(&c,left+width-47.0,center_y,false,70.0);}
            else{paint::playback_bars(&c,left+content_width-21.0,center_y,self.t,self.music_playing(),theme::WHITE);}

        }
        if lyric_alpha>=0.997 {return;}
        let header=base.faded(1.0-lyric_alpha);
        // Only the visible resident representation owns input. Both headers
        // are painted during this crossfade, but they are the same controls.
        let header=if lyric_alpha>0.2{header.ghost()}else{header};
        let base=&header;
        let m = anim::ease_in_out(smoothstep(0.0, 0.85, p));
        let clock = self.settings.clock_text(&env.clock);

        // 顶栏终点布局：日期与时钟作为一组整体居中
        let date_w = base.text_w(&env.date, size::DATE, W::Regular);
        let clock_w = paint::clock_width(base,&clock,size::CLOCK);
        let group = date_w + 8.0 + clock_w;
        let gx = local_cx - group / 2.0;
        // 顶栏从页面操作区右侧开始，避免较长日期挤到新增与刷新按钮。
        let hd = gx;
        let hc = hd + date_w + 8.0;
        // 药丸起点布局
        let has_music=self.media.is_music();
        let layout=self.resident.current();
        let pcx=local_cx+layout.x[bar::CLOCK].unwrap()-paint::clock_width(base,&clock,size::PILL_CLOCK)/2.0;

        if self.effective_minimal() && p < 0.02 { return; }
        // 时钟：两个字号沿同一路径交叉淡化，避免逐帧换字号
        let cpos = (lerp(pcx, hc, m), lerp(21.0, 23.0, m));
        let a_pill = (1.0 - smoothstep(0.25, 0.6, m));
        let a_head = smoothstep(0.35, 0.75, m);
        if a_pill > 0.0 {
            paint::clock(&base.faded(a_pill),cpos.0,cpos.1,&clock,size::PILL_CLOCK);
        }
        if a_head > 0.0 {
            paint::clock(&base.faded(a_head),cpos.0,cpos.1-0.5,&clock,size::CLOCK);
        }
        // 日期随内容淡入，从药丸方向轻轻滑出
        if reveal > 0.0 {
            base.faded(reveal).text(hd + (1.0 - reveal) * 10.0, 22.5, &env.date, size::DATE, W::Regular, Color32::from_rgb(210, 210, 214), H::Left);
        }
        if self.open && reveal>0.95 && p>0.98 && !self.is_editing() && !self.drag_hot {
            if let Some(screen)=env.screen {
                // 中间的日期时间区不占用两侧的新增、刷新和设置按钮。
                let left=hd.max(150.0);let right=(hc+clock_w).min(303.0);
                if right>left {
                    // Windows 没有「抓手」光标，Grab 会变成四向箭头；保持普通箭头，改用岛体内的把手提示。
                    let response=base.hit_drag(left,4.0,right-left,26.0,"island-position").on_hover_text("按住拖动位置");
                    let (_,press)=base.hover_t(&response);
                    // 停留才显现：划过（< 0.3 s）什么都不出现；先露出把手，再慢慢浮起底色；按下立即到位，移开快速淡出。
                    let dwell=(if response.hovered() {self.t-*self.grip_hover_since.get_or_insert(self.t)}else{self.grip_hover_since=None;0.0}) as f32;
                    if response.hovered() && dwell<1.2 {base.ui.ctx().request_repaint();}
                    let ctx=base.ui.ctx();
                    let grip_in=ctx.animate_value_with_time(response.id.with("grip"),smoothstep(0.30,0.65,dwell),0.12);
                    let fill_in=ctx.animate_value_with_time(response.id.with("grip-fill"),smoothstep(0.50,1.00,dwell),0.12);
                    let (grip_in,fill_in)=(grip_in.max(press),fill_in.max(press));
                    if grip_in>0.003 {
                        let pad=6.0;
                        let (x,w)=(left-pad,right-left+pad*2.0);
                        if fill_in>0.003 {base.fill(x,5.0,w,26.0,13.0,Color32::from_white_alpha((fill_in*7.0+press*12.0) as u8));}
                        let grip=lerp(14.0,22.0,press)*lerp(0.5,1.0,grip_in);
                        base.fill(x+w/2.0-grip/2.0,7.5,grip,2.6,1.3,Color32::from_white_alpha((grip_in*60.0+press*60.0) as u8));
                    }
                    // 开始移动立即拿起；原地按住片刻也会拿起，先给反馈再移动。
                    let held=response.is_pointer_button_down_on() && base.ui.input(|i|i.pointer.primary_down() && i.pointer.press_start_time().is_some_and(|at|i.time-at>=LIFT_HOLD));
                    if response.is_pointer_button_down_on() {base.ui.ctx().request_repaint();}
                    if response.drag_started_by(egui::PointerButton::Primary) || held {
                        if let Some(start)=base.ui.input(|i|i.pointer.press_origin()) {self.start_position_drag(start,screen,env);}
                    }
                }
            }
        }
        if p<0.1 && has_music && layout.x[bar::MUSIC].is_some() {
            let x=lerp(local_cx+layout.x[bar::MUSIC].unwrap_or(0.0),367.0,m);
            let y=lerp(15.0,18.0,m);
            let width=layout.widths[bar::MUSIC];
            let detail=if width>=60.0{1.0-smoothstep(0.0,0.3,p)}else{0.0};
            if detail>0.004{self.draw_resident_music(&base.faded(detail),local_cx+layout.x[bar::MUSIC].unwrap_or(0.0),15.0,width);}
            paint::playback_bars(&base.faded(1.0-detail),x,y,self.t,self.music_playing()&&!self.reduced_motion,theme::WHITE);
            let hit_width=lerp(width.max(26.0),26.0,m);
            if base.hit(x-hit_width/2.0,y-14.0,hit_width,28.0,"header-music")
                .on_hover_text(if self.settings.visible_pages[MUSIC_PAGE]{"返回音乐播放页"}else{"音乐模块已隐藏，可从右上角齿轮启用"})
                .on_hover_cursor(egui::CursorIcon::PointingHand).clicked(){self.show_page(MUSIC_PAGE);}
        }
        let compact_alpha=1.0-smoothstep(0.0,0.22,p);
        if self.t<self.notification_bell_until&&!self.compact_notices()&&p<0.1 {
            if let Some(x)=layout.x[bar::BELL]{
                let x=local_cx+x;paint::bell(&base.scaled(x,14.0,1.2),x,14.0,theme::WHITE,paint::bell_swing(self.t-self.notification_bell_at));
                base.circle(x+4.0,9.5,2.0,Color32::from_rgb(255,72,72));
                if base.hit(x-14.0,3.0,28.0,25.0,"bell-mail").clicked(){self.show_page(MAIL_PAGE);}
            }
        }
        if compact_alpha>0.004 {
            let c=base.faded(compact_alpha);let c=if compact_alpha<0.8{c.ghost()}else{c};
            if let Some(x)=layout.x[bar::PET]{self.draw_resident_pet(&c,local_cx+x,15.0,28.0);}
            if let Some(x)=layout.x[bar::TEAM]{self.draw_resident_team(&c,local_cx+x,15.0,false,layout.widths[bar::TEAM]);}
            if let Some(x)=layout.x[bar::WEATHER] { self.draw_pill_weather(&c,local_cx+x,&layout); }
        }

    }

    /// 常驻内容：顶部停靠走共享元素过渡（药丸里的时钟飞到顶栏）；
    /// 左右停靠时竖排内容随展开淡出，顶栏随面板内容淡入。
    fn resident_header(&mut self, base: &Card, panel: Rect, cx: f32, p: f32, reveal: f32, env: &Env) {
        if !self.side() {self.shared_header(base,cx,p,reveal,env);return;}
        let bar_alpha=1.0-smoothstep(0.0,0.3,p);
        let minimal=self.effective_minimal()&&!self.side_handle();
        if bar_alpha>0.004 && !minimal {
            let c=base.shifted(panel.min.x-base.origin.x,panel.min.y-base.origin.y).faded(bar_alpha);
            let c=if bar_alpha<0.8 {c.ghost()} else {c};
            self.side_bar(&c,panel.width(),panel.height(),env);
        }
        // The portrait header is drawn with the portrait content. Reusing the
        // 419pt shared header would place controls outside the narrow body.
        let _=reveal;
    }

    /// The side panel's clock is a native-point drag handle, independent of
    /// the horizontal page header and its 419pt coordinates.
    pub(crate) fn portrait_position_drag(&mut self,c:&Card,env:&Env){
        if !self.open||self.reveal<0.95||self.is_editing()||self.drag_hot{return;}
        let Some(screen)=env.screen else{return;};
        let center=self.expanded_width()/2.0;
        let response=c.hit_drag(center-54.0,4.0,108.0,34.0,"island-position").on_hover_cursor(egui::CursorIcon::Grab).on_hover_text("按住拖动位置");
        response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,"拖动岛体位置"));
        c.fill(center-14.0,7.0,28.0,2.5,1.25,if response.hovered(){theme::CYAN}else{theme::GRAY});
        let held=response.is_pointer_button_down_on()&&c.ui.input(|i|i.pointer.primary_down()&&i.pointer.press_start_time().is_some_and(|at|i.time-at>=LIFT_HOLD));
        if response.is_pointer_button_down_on(){c.ui.ctx().request_repaint();}
        if response.drag_started_by(egui::PointerButton::Primary)||held{
            if let Some(start)=c.ui.input(|i|i.pointer.press_origin()){self.start_position_drag(start,screen,env);}
        }
    }

    pub(crate) fn portrait_size_control(&mut self,c:&Card,env:&Env){
        let response=c.hit_drag(58.0,10.0,28.0,28.0,"portrait-size").on_hover_cursor(egui::CursorIcon::ResizeNwSe)
            .on_hover_text("拖动调节面板宽高");
        response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,"调节面板大小"));
        if response.hovered()||response.has_focus(){c.fill(58.0,10.0,28.0,28.0,8.0,theme::BTN_HOVER);}
        paint::glyph(c,"expand",72.0,24.0,17.0,theme::GRAY);
        let cursor=env.cursor.map(|point|point-env.screen.map_or(Pos2::ZERO,|screen|screen.min));
        if response.drag_started(){
            let origin=c.ui.input(|i|i.pointer.press_origin()).map(|point|point-env.screen.map_or(Pos2::ZERO,|screen|screen.min)).or(cursor);
            if let Some(origin)=origin{self.panel_resize=Some((origin,self.settings.side_panel_size()));}
        }
        if let Some((start,size))=self.panel_resize{
            if c.ui.input(|i|i.pointer.primary_down()){
                if let Some(cursor)=cursor{
                    let delta=cursor-start;let direction=if self.dock()==model::Dock::Left{1.0}else{-1.0};
                    self.settings.side_panel_width=(size.x+delta.x*direction).round().clamp(292.0,520.0) as u32;
                    self.settings.side_panel_height=(size.y+delta.y).round().clamp(532.0,900.0) as u32;
                }
            }else{self.panel_resize=None;self.settings.save();}
        }
    }

    /// Register resident-body dragging before hover intent is evaluated. A press
    /// belongs to repositioning and cannot accidentally start expansion.
    fn resident_position_input(&mut self,ui:&egui::Ui,win:Rect,env:&Env){
        if self.side()&&(self.open||self.progress()>0.10)||self.is_editing()||self.drag_hot{return;}
        let Some(screen)=env.screen else{return;};
        let rect=self.side_position_rect(win).intersect(ui.clip_rect());
        let response=ui.interact(rect,egui::Id::new(("card-drag",true,"side-position")),egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab).on_hover_text("按住拖动位置 · 靠近左右边缘吸附");
        response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,true,"拖动岛体位置"));
        if response.is_pointer_button_down_on(){self.hover_lock=true;self.hover_since=None;ui.ctx().request_repaint();}
        let held=response.is_pointer_button_down_on()&&ui.input(|i|i.pointer.primary_down()&&i.pointer.press_start_time().is_some_and(|at|i.time-at>=LIFT_HOLD));
        if self.position_drag.is_none()&&(response.drag_started_by(egui::PointerButton::Primary)||held){
            if let Some(start)=ui.input(|i|i.pointer.press_origin()){self.start_position_drag(start,screen,env);}
        }
    }

    // ---------------------------------------------------------------
    // 面板主体：共享页眉、内容（带切页过渡）与常驻的主控/模块导航
    // ---------------------------------------------------------------
    fn horizontal_page_card<'a>(card:&Card<'a>)->Card<'a>{
        card.horizontal_fit(40.0,380.0,16.0,PW-16.0)
    }

    fn panel_body(&mut self, c: &Card, env: &Env, cmds: &mut Vec<Cmd>) {
        self.panel_pin_control(c,360.0,18.0);
        let settings=c.hit(380.0,4.0,28.0,28.0,"page-settings").on_hover_text(if self.overview{"打开轻屿设置".to_owned()}else{format!("打开{}设置窗口",PAGE_NAMES[self.page])});
        settings.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,if self.overview{"打开轻屿设置".to_owned()}else{format!("打开{}设置",PAGE_NAMES[self.page])}));
        let (hover,press)=c.hover_t(&settings);
        c.circle(394.0,18.0,13.0,Color32::from_white_alpha((hover*24.0) as u8));
        if settings.has_focus(){c.stroke(380.0,4.0,28.0,28.0,8.0,1.4,theme::CYAN);}
        paint::gear(&c.scaled(394.0,18.0,0.8*(1.0-press*0.08)),394.0,18.0,paint::mix(theme::GRAY,theme::WHITE,hover));
        if Self::settings_gear_clicked(&settings){self.settings_tab=if self.overview{0}else{PAGE_SETTINGS_TABS[self.page]};self.open_settings();}

        // Existing page coordinates use a 340pt lane. Fit that lane to the
        // expanded content bounds so every module reaches the fluid shoulders.
        let page_c=Self::horizontal_page_card(c);

        let e = self.page_e();
        let in_tx = e < 0.55 && self.page_from != self.page;
        if self.overview {
            self.control_center_symbol(c,29.0,19.0,23.0,true,theme::CYAN);
            self.control_center(c,env,cmds);
        } else if self.task_dialog.is_some(){self.draw_task_editor(c);}
        else if in_tx {
            // 旧页：原地向反方向轻移并快速淡出；新页：弹簧滑入 + 稍晚淡入
            let old_a = 1.0 - smoothstep(0.0, 0.16, e);
            let new_a = smoothstep(0.03, 0.28, e);
            let old_dx = -self.page_dir * 18.0 * anim::ease_out_cubic(e / 0.3);
            if old_a > 0.0 {
                let oc = c.faded(old_a).ghost();
                self.page_title(self.page_from, &oc);
                self.render_page(self.page_from, &page_c.faded(old_a).ghost().shifted(old_dx, 0.0), env, &mut Vec::new());
            }
            let nc = page_c.faded(new_a);
            self.page_title(self.page, &c.faded(new_a));
            let nc = nc.shifted(self.slide.pos, 0.0);
            let nc = if e < 0.12 { nc.ghost() } else { nc };
            self.render_page(self.page, &nc, env, cmds);
        } else {
            self.page_title(self.page, c);
            self.render_page(self.page, &page_c, env, cmds);
        }
        self.nav(c);
        if !self.overview && self.page==MUSIC_PAGE && c.live {self.draw_music_volume(&page_c,cmds);}
    }

    pub(crate) fn panel_pin_control(&mut self,c:&Card,x:f32,y:f32){
        let name=if self.panel_pinned{"取消钉住，离开后自动收起"}else{"钉在这里，保持展开"};
        let response=c.hit(x-14.0,y-14.0,28.0,28.0,"panel-pin").on_hover_text(name).on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::Checkbox,c.live,self.panel_pinned,name));
        let (hover,press)=c.hover_t(&response);
        if self.panel_pinned||hover>0.001{c.fill(x-13.0,y-13.0,26.0,26.0,8.0,if self.panel_pinned{theme::BTN_HOVER}else{Color32::from_white_alpha((hover*24.0) as u8)});}
        if response.has_focus(){c.stroke(x-14.0,y-14.0,28.0,28.0,8.0,1.4,theme::CYAN);}
        paint::pin(&c.scaled(x,y,1.0-press*0.08),x,y,16.0,if self.panel_pinned{theme::CYAN}else{paint::mix(theme::GRAY,theme::WHITE,hover)});
        if Self::settings_gear_clicked(&response){
            self.panel_pinned=!self.panel_pinned;self.leave_since=None;self.hover_lock=false;
            if !self.panel_pinned{self.forced_open_until=0.0;}
            c.ui.ctx().request_repaint();
        }
    }

    fn page_title(&self, page: usize, c: &Card) {
        let base = 25.0;
        let (a,b,col)=match page {
            MUSIC_PAGE=>return,
            SHELF_PAGE=>("置物架","",theme::CYAN),CLIPBOARD_PAGE=>("粘贴板","",theme::CYAN),
            TODO_PAGE=>if self.library_view.legacy_tasks{("提醒","清单",theme::CYAN)}else{("拾光","",theme::CYAN)},MAIL_PAGE=>("邮箱","",theme::CYAN),
            WEATHER_PAGE if self.weather.solar_open=>("日出","日落",theme::YELLOW),
            WEATHER_PAGE=>("当地","天气",theme::CYAN),AGENT_PAGE=>("Agent","",theme::CYAN),MAC_STATS_PAGE=>("Mac ","状态",theme::CYAN),NOTIFICATIONS_PAGE=>("通知","",theme::CYAN),_=>return,
        };
        let wa = c.text(15.0, base, a, size::TITLE, W::Bold, theme::WHITE, H::Left);
        if !b.is_empty() {
            c.text(15.0 + wa + 1.5, base, b, size::TITLE, W::Bold, col, H::Left);
        }
    }

    fn nav(&mut self, c: &Card) {
        let pages:Vec<_>=self.settings.page_order.iter().copied().filter(|page|self.module_on(*page)).collect();
        let y=self.expanded_height()-20.0;
        let width=PW-24.0;
        let cell=width/(pages.len()+1) as f32;
        let left=PW/2.0-width/2.0;
        let fluid=self.settings.skin==model::Skin::Fluid;
        if fluid {
            c.fill(left,y-18.0,width,36.0,11.0,crate::fluid::NAV_SURFACE);
            c.stroke(left,y-18.0,width,36.0,11.0,0.7,crate::fluid::NAV_EDGE);
        }else{c.seg((left,y-19.0),(left+width,y-19.0),0.5,Color32::from_white_alpha(28));}
        self.control_center_tab(c,left+(cell-28.0)/2.0,y-14.0,28.0,28.0);
        let mut clicked=None;
        for (position,&page) in pages.iter().enumerate(){
            let x=left+cell*((position+1) as f32+0.5);
            let selected=!self.overview&&self.page==page;
            let id=if cfg!(target_os="macos"){"page-tab"}else{"dot"};
            let response=c.hit(x-14.0,y-14.0,28.0,28.0,(id,page)).on_hover_text(PAGE_NAMES[page]).on_hover_cursor(egui::CursorIcon::PointingHand);
            response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel,c.live,selected,PAGE_NAMES[page]));
            if selected||response.hovered(){c.fill(x-14.0,y-14.0,28.0,28.0,7.0,if selected{Color32::from_rgb(33,75,81)}else{theme::BTN_HOVER});}
            if response.has_focus(){c.stroke(x-14.0,y-14.0,28.0,28.0,7.0,1.4,theme::CYAN);}
            paint::glyph(c,PAGE_GLYPHS[page],x,y,17.0,if selected{theme::CYAN}else if fluid{crate::fluid::NAV_INK}else{theme::GRAY});
            if fluid&&selected{c.fill(x-5.0,y+11.0,10.0,2.0,1.0,theme::CYAN);}
            if self.navigation_activated(c,&response,selected){clicked=Some(page);}
        }
        if let Some(page)=clicked{self.go(page,if page>=self.page{1.0}else{-1.0});}
    }

    fn render_page(&mut self,page:usize,c:&Card,env:&Env,cmds:&mut Vec<Cmd>){
        match page {
            MUSIC_PAGE=>self.page_music(c,env,cmds),SHELF_PAGE=>self.page_shelf(c,env,cmds),CLIPBOARD_PAGE=>self.page_clipboard(c,env,cmds),
            TODO_PAGE=>self.page_library(c,env,cmds,false),MAIL_PAGE=>self.page_mail(c,env,cmds),
            WEATHER_PAGE=>self.page_weather(c,env,cmds),AGENT_PAGE=>self.page_agents(c,env,false),MAC_STATS_PAGE=>self.page_mac_stats(c,false),NOTIFICATIONS_PAGE=>self.page_notifications(c,false,cmds),_=>{},
        }
    }

    fn toast_layer(&mut self, c: &Card, base_y:f32)->Option<Rect> {
        let Some((msg, at)) = self.toast.clone() else { return None };
        let e = (self.t - at) as f32;
        if e > 2.4 {
            self.toast = None;
            return None;
        }
        let a = smoothstep(0.0, 0.16, e) * (1.0 - smoothstep(2.1, 2.4, e));
        let rise = (1.0 - anim::ease_out_cubic(e / 0.3)) * 6.0;
        let width=self.expanded_width();
        let msg=c.fit(&msg,size::BODY,W::Regular,(width-42.0).min(310.0));
        let w = c.text_w(&msg, size::BODY, W::Regular) + 26.0;
        let tc = c.faded(a);
        let y = base_y + rise;
        tc.fill(width / 2.0 - w / 2.0, y, w, 22.0, 11.0, Color32::from_rgb(44, 44, 48));
        tc.text(width / 2.0, y + 15.0, &msg, size::BODY, W::Regular, theme::WHITE, H::Center);
        Some(tc.rect(width/2.0-w/2.0,y,w,22.0))
    }

    // ---------------------------------------------------------------
    // 每帧推进：待办提醒、跨天
    // ---------------------------------------------------------------
    fn tick(&mut self, env: &Env, dt: f32, _elapsed: f32) {
        if self.toast.as_ref().is_some_and(|(_,at)|self.t-at>2.4){self.toast=None;}
        if env.today != self.today {self.today=env.today.clone();}
        self.todo_pop.step(dt);
        self.library_poll();
        let now = format!("{} {}", env.today, env.clock.get(..5).unwrap_or("00:00"));
        let todo_on=self.module_on(TODO_PAGE);
        let due: Vec<_> = self.todos.items.iter().enumerate().filter(|(_, item)| todo_on && item.reminder_due(&now)).map(|(i,item)| (i,item.text.clone())).collect();
        for (i,text) in due { self.todos.items[i].mark_reminded(&now); self.todos.save(); self.alert("待办到时间了", text, TODO_PAGE, true); }
        // 独立粘贴板关闭时暂停收集，已有记录保留。
        if self.clipboard.is_enabled() != self.clipboard_active() {
            self.clipboard.set_enabled(self.clipboard_active());
        }
        self.shelf_scroll.target = self.shelf_first as f32;
        self.shelf_scroll.step(dt);
        if let Some(at) = self.confirm_clear {
            if self.t - at > 3.0 {
                self.confirm_clear = None;
            }
        }
    }

    /// 媒体进度在两次 SMTC 刷新之间按时间外推，进度条才会连续走动。
    fn update_media(&mut self, env: &Env, ctx: &egui::Context) {
        #[cfg(target_os="macos")]
        let reconciled=self.reconcile_qq_media(&env.media);
        #[cfg(target_os="macos")]
        let m=&reconciled;
        #[cfg(not(target_os="macos"))]
        let m = &env.media;
        self.observe_music_playback(m);
        if !m.is_music() {
            self.music_volume_ui=None;
            self.music_volume_quiet_until=0.0;
            self.music_volume_open=false;
            self.media=NowPlaying::default();
            self.media_anchor=(String::new(),0.0,0.0,false);
            self.music_artwork=None;
            self.ly=LyricsState::default();
            return;
        }
        let a = &self.media_anchor;
        if a.0 != m.title || (a.1 - m.position).abs() > 0.01 || a.3 != m.playing {
            self.media_anchor = (m.title.clone(), m.position, env.t, m.playing);
        }
        let mut np = m.clone();
        if self.media.source!=m.source || m.volume.is_none(){self.music_volume_open=false;}
        if self.media.source!=m.source || self.t>=self.music_volume_quiet_until || m.volume.is_none() {
            self.music_volume_ui=m.volume;
        }
        if m.playing {
            let (_, pos, _, _) = &self.media_anchor;
            np.position = if let Some(sampled)=m.sampled_at {m.position+sampled.elapsed().as_secs_f64()}
                else {*pos};
            if m.duration>0.0 {np.position=np.position.min(m.duration);}
        }
        // 网易云的 song_id 只来自本机日志，SMTC 那一路没有；None 与 Some 之间来回不是换歌，不能每次都清空歌词重查。
        let other_song=matches!((self.media.song_id,np.song_id),(Some(old),Some(new)) if old!=new);
        if self.media.source != np.source || other_song { self.ly=LyricsState::default(); }
        self.media = np;
        if let Some(image)=&self.media.artwork {
            if !self.music_artwork.as_ref().is_some_and(|(old,_)|std::sync::Arc::ptr_eq(old,image)) {
                self.music_artwork=Some((image.clone(),ctx.load_texture("music-artwork",image.clone(),egui::TextureOptions::LINEAR)));
            }
        } else {self.music_artwork=None;}
    }

    fn poll_lyrics(&mut self) {
        if !self.media.is_music(){self.ly=LyricsState::default();return;}
        if !self.show_lyrics && !self.settings.island_lyrics {self.ly=LyricsState::default();return;}
        if self.native_qq_lyrics().is_some(){return;}
        let np = &self.media;
        if np.title.is_empty(){self.ly=LyricsState::default();return;}
        let key = format!("{}|{}|{:.0}", np.title, np.artist, np.duration);
        let id_arrived=key==self.ly.key && !self.ly.loading && self.ly.song_id.is_none() && np.song_id.is_some()
            && !self.ly.lyrics.as_ref().is_some_and(|lyrics|lyrics.synced);
        if key != self.ly.key || id_arrived || self.ly.retry_at.is_some_and(|at|self.t>=at) {
            // 歌曲信息抖动或快速切歌不能叠加网络线程；旧请求完成后只查询当前歌曲。
            if self.ly_worker.as_ref().is_some_and(|worker|!worker.is_finished()){return;}
            let Some(lease)=crate::resources::LYRICS.acquire()else{return;};
            let native_only=key==self.ly.key && np.source.to_ascii_lowercase().contains("kugou")
                && self.ly.error==Some(lyrics::FetchError::NotFound);
            let cached=if key==self.ly.key && np.source.to_ascii_lowercase().contains("kugou"){self.ly.lyrics.take()}else{None};
            let attempts=if key==self.ly.key {self.ly.attempts.saturating_add(1)}else{0};
            let manual=self.ly.manual_retry;
            self.ly = LyricsState { key, loading: cached.is_none() && !native_only, lyrics:cached.clone(),
                error:native_only.then_some(lyrics::FetchError::NotFound), song_id:np.song_id, attempts, ..Default::default() };
            let (tx, rx) = std::sync::mpsc::channel();
            self.ly.rx = Some(rx);
            let (source, title, artist, dur) = (np.source.clone(), np.title.clone(), np.artist.clone(), np.duration);
            let song_id=np.song_id;
            if source.to_ascii_lowercase().contains("kugou") || lyrics::is_soda(&source) {
                let (tx,rx)=std::sync::mpsc::sync_channel(1);self.ly.native_rx=Some(rx);
                let (source,title,artist)=(source.clone(),title.clone(),artist.clone());
                crate::resources::NATIVE_LYRICS.spawn(move ||{
                    // 客户端通常在开播几秒内写好歌词：先密后疏（0.5s 起，最长 5s 一次），最多等 90 秒。
                    // 汽水的缓存文件可达 32MB，修改时间和大小都没变时不重复整份读取，但每 15 秒仍完整核对一次。
                    let started=std::time::Instant::now();
                    let mut wait=std::time::Duration::from_millis(500);
                    let mut last:Option<((std::time::SystemTime,u64),std::time::Instant)>=None;
                    loop {
                        let print=lyrics::native_fingerprint(&source);
                        let unchanged=print.is_some_and(|print|last.is_some_and(|(seen,at)|seen==print && at.elapsed()<std::time::Duration::from_secs(15)));
                        let found=if unchanged {None}else{
                            if let Some(print)=print {last=Some((print,std::time::Instant::now()));}
                            lyrics::read_native(&source,&title,&artist,dur)
                        };
                        let done=found.is_some();
                        match tx.try_send(found) {
                            Ok(())=>{if done{break;}}
                            Err(std::sync::mpsc::TrySendError::Disconnected(_))=>break,
                            Err(std::sync::mpsc::TrySendError::Full(_))=>{}
                        }
                        if started.elapsed()>=std::time::Duration::from_secs(90){break;}
                        std::thread::sleep(wait);
                        wait=(wait*2).min(std::time::Duration::from_secs(5));
                    }
                });
            }
            self.ly_worker=lease.spawn(move || {
                let result=if native_only {
                    lyrics::kugou_cache::read(&title,&artist,dur).map(|lyrics|(lyrics,false)).ok_or(lyrics::FetchError::NotFound)
                }else{lyrics::fetch_for_display_with(&source, &title, &artist, dur, song_id, cached, manual)};
                let _ = tx.send(result);
            });
            if self.ly_worker.is_none(){self.ly.loading=false;self.ly.key.clear();}
        }
        if let Some(rx)=&self.ly.native_rx {
            let mut found=None;
            while let Ok(result)=rx.try_recv(){if result.is_some(){found=result;}}
            if let Some(lyrics)=found {
                self.ly.lyrics=Some(lyrics);self.ly.loading=false;self.ly.error=None;
                self.ly.retry_at=None;self.ly.rx=None;self.ly.native_rx=None;
            }
        }
        if let Some(rx) = &self.ly.rx {
            if let Ok(r) = rx.try_recv() {
                self.ly.loading = false;
                match r {
                    Ok((l,waiting_native)) => {
                        self.ly.error=None;
                        self.ly.retry_at=None;
                        if (waiting_native || lyrics::is_soda(&self.media.source) && !l.synced) && self.ly.attempts<NATIVE_RETRIES {self.ly.retry_at=Some(self.t+5.0);}
                        self.ly.lyrics = Some(l);
                    },
                    Err(error) => {
                        let native=lyrics::is_soda(&self.media.source) || self.media.source.to_ascii_lowercase().contains("kugou") && error==lyrics::FetchError::NotFound;
                        if native {if self.ly.attempts<NATIVE_RETRIES{self.ly.retry_at=Some(self.t+5.0);}}
                        else if matches!(error,lyrics::FetchError::Unavailable(_)|lyrics::FetchError::InvalidResponse(_)) && self.ly.attempts<NETWORK_RETRIES {self.ly.retry_at=Some(self.t+30.0);}
                        self.ly.error = Some(error);
                    },
                }
                self.ly.rx = None;
            }
        }
    }

    /// 外部直接提供歌词（录制演示用）。
    pub fn set_lyrics(&mut self, title: &str, artist: &str, duration: f64, l: lyrics::Lyrics) {
        self.ly = LyricsState { key: format!("{title}|{artist}|{duration:.0}"), lyrics: Some(l), ..Default::default() };
    }

    pub fn on_drop(&mut self, files: Vec<PathBuf>) {
        if !self.settings.visible_pages[SHELF_PAGE] || files.is_empty() { return; }
        self.drag_hot = false;
        self.drag_saved = None;
        if self.page != SHELF_PAGE {
            self.go(SHELF_PAGE, 1.0);
        }
        let (mut added,mut duplicate,mut failed) = (0,0,0);
        for p in files {
            if !p.exists() {failed+=1;continue;}
            if self.shelf.items.contains(&p) {
                duplicate+=1;
                continue;
            }
            if self.shelf.add(&p).is_ok() {
                self.shelf_born.insert(p.clone(), self.t);
                added += 1;
            }else{failed+=1;}
        }
        let message=if added==0 && failed==0 {
            if duplicate==1{"该文件已在置物架中".into()}else{"这些文件已在置物架中".into()}
        }else if added==0 && duplicate==0 {"文件不存在或无法访问".into()}
        else {
            let mut parts=Vec::new();
            if added>0{parts.push(format!("已添加 {added} 个文件"));}
            if duplicate>0{parts.push(format!("{duplicate} 个已存在"));}
            if failed>0{parts.push(format!("{failed} 个无法添加"));}
            parts.join("，")
        };
        self.say(message);
        if added>0 {
            self.shelf_first = self.shelf.items.len().saturating_sub(5);
        }
    }

    /// 待办输入框：真正的 egui 文本框，放在待办卡片顶部那一行。
    fn input_field(&mut self, ui: &mut egui::Ui, origin: Pos2, cmds: &mut Vec<Cmd>) {
        let rect = if self.side(){Rect::from_min_size(origin+Vec2::new(34.0,178.0),Vec2::new(self.expanded_width()-68.0,22.0))}
            else{Rect::from_min_size(origin + Vec2::new(222.0, 37.0), Vec2::new(142.0, 18.0))};
        let mut buf = std::mem::take(&mut self.buf);
        let te = egui::TextEdit::singleline(&mut buf)
            .id(egui::Id::new("todo-input"))
            .frame(false)
            .margin(egui::Margin::ZERO)
            .text_color(theme::WHITE)
            .font(theme::font(size::BODY, W::Regular))
            .hint_text(egui::RichText::new("要做点什么？回车保存").color(theme::DIM).font(theme::font(size::BODY, W::Regular)))
            .desired_width(rect.width());
        let resp = ui.put(rect, te);
        let first_focus=!self.todo_input_focused;
        self.buf=buf;
        let lost_window=ui.input(|i|i.events.iter().any(|event|matches!(event,egui::Event::WindowFocused(false))));
        if !first_focus && !self.todo_ime_frame && self.adding && self.buf.trim().is_empty() && (resp.clicked_elsewhere() || lost_window) {
            resp.surrender_focus();
            self.cancel_edit();
            return;
        }
        if first_focus {
            self.todo_input_focused=true;
            resp.request_focus();
            cmds.push(Cmd::Focus);
        }
        let submit = ui.input(|i| i.key_pressed(egui::Key::Enter));
        if submit {
            let text = self.buf.trim().to_string();
            let mut added=false;
            if let Some(i) = self.editing {
                if !text.is_empty() && i < self.todos.items.len() {
                    self.todos.items[i].text = text;
                    self.todos.save();
                }
            } else if !text.is_empty() {
                self.todo_first = 0;
                self.todos.items.insert(0, model::TodoItem::new(text));
                self.todos.save();
                self.todo_pop.vel += 6.0;
                added=true;
            }
            self.cancel_edit();
            if added {self.open_task_reminder(0);}
        }
    }
}

#[cfg(test)]
#[path="desktop_placement_tests.rs"]
mod desktop_placement_tests;
