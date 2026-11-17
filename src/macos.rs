//! macOS native shell. Display coordinates are **AppKit points**, top-down from
//! the primary screen. Retina scaling is local to a window and must never scale
//! a global display origin (adjacent displays can have different pixel ratios).
use eframe::egui::{self, Pos2, Rect};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{ffi::{c_char,c_void,CStr,CString},path::{Path,PathBuf},sync::{atomic::{AtomicBool,AtomicU32,Ordering},mpsc,Arc,Mutex,OnceLock},time::Duration};
use crate::{display_layout::{Bounds,Display},model::{Dock,MacosPlacement},app::DesktopPlacement};

pub const SETTINGS:u32=1;
pub const PET:u32=4;
pub const MINIMAL:u32=8;
pub const AUTOSTART:u32=16;
pub const QUIT:u32=32;
pub const FOREGROUND_CHANGED:u32=64;
pub const QQ_MUSIC_LIBRARY:u32=128;
pub const QQ_MUSIC_PLAYER:u32=256;

fn activation_event(request:&[u8])->u32 {
    match request{b"settings"=>SETTINGS,b"qq-library"=>QQ_MUSIC_LIBRARY,b"qq-player"=>QQ_MUSIC_PLAYER,_=>0}
}

#[repr(C)]
#[derive(Default,Clone,Copy,Debug,PartialEq)]
struct NativeRect {x:f64,y:f64,w:f64,h:f64}
impl NativeRect {
    fn from_rect(rect:Rect)->Self {if rect.is_finite()&&rect.is_positive(){Self{x:rect.min.x as f64,y:rect.min.y as f64,w:rect.width() as f64,h:rect.height() as f64}}else{Self::default()}}
    fn bounds(self)->Bounds {Bounds{left:self.x.round() as i32,top:self.y.round() as i32,right:(self.x+self.w).round() as i32,bottom:(self.y+self.h).round() as i32}}
    fn rect(self)->Rect {Rect::from_min_size(egui::pos2(self.x as f32,self.y as f32),egui::vec2(self.w as f32,self.h as f32))}
}
#[repr(C)]
#[derive(Clone,Copy)]
struct NativeScreen {frame:NativeRect,work:NativeRect,camera:NativeRect,scale:f64,safe_top:f64,display_id:u32,primary:u32,name:[c_char;256],identifier:[c_char;128]}
#[repr(C)]
#[derive(Default)]
struct NativeSystem {battery:i32,charging:i32,volume:i32,muted:i32,network:i32,locked:i32,audio_device:u32,idle:f64,fullscreen:i32,fullscreen_rect:NativeRect}
extern "C" {
    fn qy_shell_create(view:*mut c_void,wake:extern "C" fn(*mut c_void),context:*mut c_void)->*mut c_void;
    fn qy_shell_destroy(shell:*mut c_void);
    fn qy_shell_events(shell:*mut c_void)->u32;
    fn qy_shell_menu(shell:*mut c_void,pet:i32,minimal:i32,autostart:i32);
    fn qy_shell_update(shell:*mut c_void,hit:NativeRect,notice:NativeRect,drop:NativeRect,scale:f64,editing:i32,hidden:i32);
    fn qy_shell_hit_outlines(shell:*mut c_void,xy:*const f64,counts:*const u32,count:usize);
    fn qy_shell_notice_outlines(shell:*mut c_void,xy:*const f64,counts:*const u32,count:usize);
    fn qy_shell_focus(shell:*mut c_void);
    fn qy_activate();
    fn qy_shell_geometry(shell:*mut c_void,frame:*mut NativeRect,scale:*mut f64)->i32;
    fn qy_shell_place(shell:*mut c_void,frame:NativeRect)->i32;
    fn qy_shell_blur(shell:*mut c_void,xy:*const f64,counts:*const u32,paths:usize)->i32;
    fn qy_shell_cursor(shell:*mut c_void,x:*mut f64,y:*mut f64)->i32;
    fn qy_shell_global_cursor(shell:*mut c_void,x:*mut f64,y:*mut f64)->i32;
    fn qy_screens(screens:*mut NativeScreen,capacity:usize)->usize;
    fn qy_open(path:*const c_char,reveal:i32)->i32;
    fn qy_pick_files(shell:*mut c_void,kind:u32,title:*const c_char,multiple:i32)->i32;
    fn qy_take_picker(shell:*mut c_void)->*mut c_char;
    fn qy_string_free(value:*mut c_char);
    fn qy_drag_file(shell:*mut c_void,path:*const c_char)->i32;
    fn qy_system_sample(out:*mut NativeSystem);
    fn qy_audio_command(volume:i32,toggle:i32)->i32;
    fn flock(fd:i32,operation:i32)->i32;
    fn getuid()->u32;
}
extern "C" fn repaint(context:*mut c_void) {
    // AppKit callbacks are removed before this allocation is dropped.
    if let Some(ctx)=unsafe{(context as *const egui::Context).as_ref()} {ctx.request_repaint();}
}
static IPC_EVENTS:AtomicU32=AtomicU32::new(0);
static CONTEXT:OnceLock<Mutex<Option<egui::Context>>>=OnceLock::new();

pub struct Window {native:*mut c_void,_context:Box<egui::Context>,scale:f32,blur:bool,blur_key:Vec<Vec<Pos2>>,blur_scale:f32,blur_at:std::time::Instant,layout:Option<Layout>}
impl Window {
    pub fn new(cc:&eframe::CreationContext<'_>)->Option<Self> {
        migrate_login_item();
        let RawWindowHandle::AppKit(handle)=cc.window_handle().ok()?.as_raw() else{return None;};
        let mut context=Box::new(cc.egui_ctx.clone());
        let native=unsafe{qy_shell_create(handle.ns_view.as_ptr(),repaint,(&mut *context as *mut egui::Context).cast())};
        if native.is_null(){return None;}
        if let Ok(mut ctx)=CONTEXT.get_or_init(||Mutex::new(None)).lock() {*ctx=Some(cc.egui_ctx.clone());}
        Some(Self{native,_context:context,scale:1.0,blur:false,blur_key:Vec::new(),blur_scale:1.0,blur_at:std::time::Instant::now(),layout:None})
    }
    pub fn events(&self)->u32 {(unsafe{qy_shell_events(self.native)})|IPC_EVENTS.swap(0,Ordering::AcqRel)}
    pub fn sync_menu(&self,pet:bool,minimal:bool,autostart:bool) {unsafe{qy_shell_menu(self.native,pet as i32,minimal as i32,autostart as i32)}}
    pub fn focus(&self) {unsafe{qy_shell_focus(self.native)}}
    pub fn cursor(&self)->Option<Pos2> {
        let(mut x,mut y)=(0.,0.);
        (unsafe{qy_shell_cursor(self.native,&mut x,&mut y)}!=0).then(||egui::pos2(x as f32/self.scale,y as f32/self.scale))
    }
    /// Primary-screen, top-down AppKit points, matching `Display.bounds`.
    pub fn global_cursor(&self)->Option<Pos2> {
        let(mut x,mut y)=(0.,0.);
        (unsafe{qy_shell_global_cursor(self.native,&mut x,&mut y)}!=0).then(||egui::pos2(x as f32,y as f32))
    }
    pub fn screen(&self)->Option<Rect> {
        let layout=self.layout?;
        let mut frame=NativeRect::default();let mut backing=1.;
        if unsafe{qy_shell_geometry(self.native,&mut frame,&mut backing)}==0{return None;}
        let b=layout.usable;
        Some(Rect::from_min_max(egui::pos2((b.left as f32-frame.x as f32)/self.scale,(b.top as f32-frame.y as f32)/self.scale),egui::pos2((b.right as f32-frame.x as f32)/self.scale,(b.bottom as f32-frame.y as f32)/self.scale)))
    }
    pub fn place(&mut self,display:&Display,settings:&crate::model::Settings,scale:f32,offset:f32,position:Option<[f32;2]>,resident:egui::Vec2,shift:(i32,i32))->bool {
        let screen=screen_geometry(display);
        let root_floor=if settings.dock.is_side(){0.1}else{0.5};
        let layout=if let Some(position)=position{
            positioned_layout(screen,scale,(scale/settings.island_scale())*0.1,position,settings.dock,settings.side_panel_size(),resident)
        }else{layout_sized(screen,settings.macos_placement,settings.avoid_top_bar,scale,(scale/settings.island_scale())*root_floor,offset,settings.dock,shift,settings.side_panel_size())};
        let Some(layout)=layout else{return false;};
        self.scale=layout.scale;
        if unsafe{qy_shell_place(self.native,layout.frame)}==0{return false;}
        self.layout=Some(layout);true
    }
    pub fn desktop_placement(&self)->DesktopPlacement {self.layout.map(|layout|layout.surface).unwrap_or_default()}
    pub fn scale(&self)->f32 {self.scale}
    pub fn side_panel_size(&self)->egui::Vec2{self.layout.map_or(egui::vec2(crate::side_dock::PANEL_W,crate::side_dock::PANEL_H),|layout|layout.panel)}
    /// Moving between 1x/2x displays or changing island size updates AppKit
    /// before winit's next input frame. Do not expose one frame with mismatched
    /// text/hit coordinates (especially under a hardware camera).
    pub fn coordinates_ready(&self,pixels_per_point:f32)->bool {
        let mut frame=NativeRect::default();let mut backing=1.;
        (unsafe{qy_shell_geometry(self.native,&mut frame,&mut backing)})!=0 && pixels_per_point.is_finite() && (pixels_per_point as f64-self.scale as f64*backing).abs()<0.02
    }
    pub fn update(&mut self,hit:Rect,hit_shapes:&[crate::app::GlassShape],notice:Option<Rect>,notice_shapes:&[crate::app::GlassShape],drop_zone:Option<Rect>,glass:Option<&[crate::app::GlassShape]>,editing:bool,hidden:bool) {
        let paths:Vec<_>=hit_shapes.iter().map(|shape|shape.outline()).filter(|points|points.len()>=3&&points.iter().all(|point|point.is_finite())).collect();
        let xy:Vec<_>=paths.iter().flatten().flat_map(|p|[p.x as f64,p.y as f64]).collect();
        let counts:Vec<_>=paths.iter().map(|points|points.len() as u32).collect();
        unsafe{qy_shell_hit_outlines(self.native,xy.as_ptr(),counts.as_ptr(),counts.len())};
        let notice_paths:Vec<_>=notice_shapes.iter().map(|shape|shape.outline()).filter(|points|points.len()>=3&&points.iter().all(|point|point.is_finite())).collect();
        let notice_xy:Vec<_>=notice_paths.iter().flatten().flat_map(|point|[point.x as f64,point.y as f64]).collect();
        let notice_counts:Vec<_>=notice_paths.iter().map(|points|points.len() as u32).collect();
        unsafe{qy_shell_notice_outlines(self.native,notice_xy.as_ptr(),notice_counts.as_ptr(),notice_counts.len())};
        unsafe{qy_shell_update(self.native,NativeRect::from_rect(hit),notice.map(NativeRect::from_rect).unwrap_or_default(),drop_zone.map(NativeRect::from_rect).unwrap_or_default(),self.scale as f64,editing as i32,hidden as i32)}
        let outlines:Vec<_>=glass.unwrap_or_default().iter().map(|shape|shape.outline()).filter(|points|points.len()>=3 && points.iter().all(|p|p.is_finite())).collect();
        if self.blur_key!=outlines || self.blur_scale!=self.scale || self.blur_at.elapsed()>=Duration::from_secs(1) {
            let xy:Vec<_>=outlines.iter().flatten().flat_map(|p|[p.x as f64,p.y as f64]).collect();
            let counts:Vec<_>=outlines.iter().map(|points|points.len() as u32).collect();
            self.blur=unsafe{qy_shell_blur(self.native,xy.as_ptr(),counts.as_ptr(),counts.len())}!=0;
            self.blur_key=outlines;self.blur_scale=self.scale;self.blur_at=std::time::Instant::now();
        }
    }
    pub fn blur_active(&self)->bool {self.blur}
    pub fn pick_files(&self,kind:u32,title:&str,multiple:bool)->Result<(),String> {
        let title=CString::new(title).map_err(|_|"文件选择标题无效")?;
        if unsafe{qy_pick_files(self.native,kind,title.as_ptr(),multiple as i32)}==0 {Err("请先完成已打开的文件选择窗口".into())}else{Ok(())}
    }
    pub fn take_picker(&self)->Option<Picker> {
        let text=unsafe{qy_take_picker(self.native)};if text.is_null(){return None;}
        let result=serde_json::from_slice(unsafe{CStr::from_ptr(text)}.to_bytes()).ok();
        unsafe{qy_string_free(text)};result
    }
    pub fn drag_file(&self,path:&Path)->Result<(),String> {
        let path=CString::new(path.to_string_lossy().as_bytes()).map_err(|_|"文件路径无效")?;
        if unsafe{qy_drag_file(self.native,path.as_ptr())}!=0 {Ok(())}else{Err("请按住文件图标拖出；也可使用「在 Finder 中显示」".into())}
    }
}
impl Drop for Window {fn drop(&mut self){unsafe{qy_shell_destroy(self.native)};if let Some(ctx)=CONTEXT.get(){if let Ok(mut ctx)=ctx.lock(){*ctx=None;}}}}

#[derive(serde::Deserialize)]
pub struct Picker {pub kind:u32,pub paths:Vec<PathBuf>}
pub fn activate() {unsafe{qy_activate()}}
pub fn open(path:&Path,reveal:bool)->Result<(),String> {
    let path=CString::new(path.to_string_lossy().as_bytes()).map_err(|_|"文件路径无效")?;
    if unsafe{qy_open(path.as_ptr(),reveal as i32)}!=0 {Ok(())}else{Err("无法打开，请检查文件位置和 macOS 默认应用".into())}
}
fn screens()->Vec<NativeScreen> {
    // A bounded buffer also prevents a display hot-plug between count and fill.
    let mut out:Vec<NativeScreen>=vec![unsafe{std::mem::zeroed()};32];
    let count=unsafe{qy_screens(out.as_mut_ptr(),out.len())}.min(out.len());out.truncate(count);out
}
#[derive(Clone,Copy,Debug)]
struct ScreenGeometry {frame:NativeRect,work:NativeRect,camera:Option<NativeRect>}
impl ScreenGeometry {
    fn fallback(display:&Display)->Self {
        let b=display.bounds;
        Self{frame:NativeRect{x:b.left as f64,y:b.top as f64,w:(b.right-b.left) as f64,h:(b.bottom-b.top) as f64},work:NativeRect{x:b.left as f64,y:display.work_top as f64,w:(b.right-b.left) as f64,h:(b.bottom-display.work_top) as f64},camera:None}
    }
}
thread_local! {static SCREEN_GEOMETRY:std::cell::RefCell<Vec<(String,ScreenGeometry)>>=const{std::cell::RefCell::new(Vec::new())};}
fn screen_geometry(display:&Display)->ScreenGeometry {
    SCREEN_GEOMETRY.with(|screens|screens.borrow().iter().find(|(id,_)|id==&display.id).map(|(_,screen)|*screen))
        .unwrap_or_else(||ScreenGeometry::fallback(display))
}
pub fn displays()->Vec<Display> {
    let screens=screens();let mut geometry=Vec::with_capacity(screens.len());
    let displays=screens.iter().map(|s|{
        let name=unsafe{CStr::from_ptr(s.name.as_ptr())}.to_string_lossy();
        let identifier=unsafe{CStr::from_ptr(s.identifier.as_ptr())}.to_string_lossy();
        let id=if identifier.is_empty(){format!("display-{}",s.display_id)}else{identifier.into_owned()};
        let camera=(s.camera.w>0.0 && s.camera.h>0.0).then_some(s.camera);
        geometry.push((id.clone(),ScreenGeometry{frame:s.frame,work:s.work,camera}));
        Display{id,label:format!("{name} · {:.0} × {:.0}{}",s.frame.w,s.frame.h,if s.primary!=0{" · 主屏"}else{""}),bounds:s.frame.bounds(),work_top:s.work.y.max(s.frame.y+s.safe_top).round() as i32,primary:s.primary!=0}
    }).collect();
    SCREEN_GEOMETRY.with(|cache|*cache.borrow_mut()=geometry);displays
}
/// Drag normalization must use the exact room consumed by positioned_layout,
/// including a side/bottom Dock while retaining the physical menu-bar strip.
pub fn placement_rooms(displays:&[Display])->Vec<(String,Bounds)> {
    displays.iter().filter_map(|display|positioned_usable(screen_geometry(display)).map(|room|(display.id.clone(),NativeRect::from_rect(room).bounds()))).collect()
}
/// Physical camera width and depth in AppKit points, independent of Retina.
pub fn notch_size(display:&Display)->Option<(f32,f32)> {
    SCREEN_GEOMETRY.with(|screens|screens.borrow().iter().find(|(id,_)|id==&display.id).and_then(|(_,screen)|screen.camera).map(|camera|(camera.w as f32,camera.h as f32)))
}
pub fn placement_description(settings:&crate::model::Settings,display:&Display)->String {
    if settings.island_position.is_some(){return "自由放置，靠近左右边缘时吸附".into();}
    if settings.dock.is_side(){return "沿所选屏幕侧边停靠，避开菜单栏与 Dock".into();}
    match settings.macos_placement {
        MacosPlacement::Manual=>"使用自定形状和顶栏避让，可拖动调整位置".into(),
        MacosPlacement::BelowMenuBar=>"悬浮胶囊位于菜单栏下方，保留 8 点间距".into(),
        MacosPlacement::Auto if notch_size(display).is_some() && settings.island_offset==0=>"贴合摄像头下沿，内容与点击区域避开摄像头".into(),
        MacosPlacement::Auto=>"在当前屏幕悬浮显示，距菜单栏 8 点".into(),
    }
}
pub(crate) fn work_areas()->Vec<Rect> {screens().into_iter().map(|s|s.work.rect()).collect()}

/// The native window, egui content, cursor and visual-effect mask share this
/// single plan. Backing scale never changes global AppKit coordinates.
#[derive(Clone,Copy,Debug)]
struct Layout {frame:NativeRect,usable:Bounds,scale:f32,surface:DesktopPlacement,panel:egui::Vec2}

fn positioned_usable(screen:ScreenGeometry)->Option<Rect> {
    let mut usable=screen.work.rect().intersect(screen.frame.rect());
    if !usable.is_finite()||!usable.is_positive(){usable=screen.frame.rect();}
    if !usable.is_finite()||!usable.is_positive(){return None;}
    // Explicit free placement includes the menu-bar strip. Only the actual
    // camera housing excludes content; it must not reserve an entire row.
    usable.min.y=screen.frame.y as f32;
    Some(usable)
}
fn positioned_layout(screen:ScreenGeometry,requested_scale:f32,minimum_scale:f32,position:[f32;2],dock:Dock,requested_panel:egui::Vec2,resident:egui::Vec2)->Option<Layout>{
    if !requested_scale.is_finite()||requested_scale<=0.0||!position.iter().all(|value|value.is_finite()){return None;}
    let usable=positioned_usable(screen)?;
    let camera=screen.camera.filter(|r|r.rect().is_finite()&&r.w>0.0&&r.h>0.0&&r.x>=screen.frame.x&&r.x+r.w<=screen.frame.x+screen.frame.w&&r.y>=screen.frame.y&&r.y+r.h<screen.frame.y+screen.frame.h/3.0);
    let bounds=NativeRect::from_rect(usable).bounds();
    let room=egui::vec2((bounds.right-bounds.left) as f32,(bounds.bottom-bounds.top) as f32);
    let required=if dock.is_side(){egui::vec2(crate::side_dock::PANEL_W+40.0,crate::side_dock::SIDE_H)}
        else{egui::vec2(crate::app::PW.max(resident.x)+48.0,crate::control_center::TOP_H*2.0+resident.y+crate::island_position::PANEL_GAP*2.0+16.0)};
    let camera_depth=camera.map_or(0.0,|r|(r.y+r.h-usable.top() as f64) as f32);
    let fitting_room=room-egui::vec2(0.0,camera_depth);
    let scale=requested_scale.min(fitting_room.x/required.x).min(fitting_room.y/required.y).max(minimum_scale.min(requested_scale).max(0.1));
    let panel=if dock.is_side(){crate::side_dock::fit_panel(requested_panel,fitting_room/scale)}else{requested_panel};
    let expanded=if dock.is_side(){panel}else{egui::vec2(crate::app::PW,crate::control_center::TOP_H)};
    let camera=camera.map(|r|Rect::from_min_size((r.rect().min-usable.min.to_vec2())/scale,r.rect().size()/scale));
    let (origin,plan)=crate::island_position::plan_with_camera(room/scale,position,dock,resident,expanded,scale,camera);
    let frame=NativeRect{x:bounds.left as f64+(origin.x*scale) as f64,y:bounds.top as f64+(origin.y*scale) as f64,w:(plan.canvas.x*scale) as f64,h:(plan.canvas.y*scale) as f64};
    Some(Layout{frame,usable:bounds,scale,surface:DesktopPlacement::Positioned(plan),panel})
}
fn layout(screen:ScreenGeometry,mode:MacosPlacement,avoid_top:bool,requested_scale:f32,minimum_scale:f32,offset:f32,dock:Dock,shift:(i32,i32))->Option<Layout> {
    layout_sized(screen,mode,avoid_top,requested_scale,minimum_scale,offset,dock,shift,egui::vec2(crate::side_dock::PANEL_W,crate::side_dock::PANEL_H))
}
fn layout_sized(screen:ScreenGeometry,mode:MacosPlacement,avoid_top:bool,requested_scale:f32,minimum_scale:f32,offset:f32,dock:Dock,shift:(i32,i32),panel:egui::Vec2)->Option<Layout> {
    if !requested_scale.is_finite() || requested_scale<=0.0 || screen.frame.w<=0.0 || screen.frame.h<=0.0{return None;}
    let mut usable=screen.work.rect().intersect(screen.frame.rect());
    if !usable.is_finite() || !usable.is_positive(){usable=screen.frame.rect();}
    let camera=screen.camera.filter(|r|r.w.is_finite() && r.h.is_finite() && r.w>0.0 && r.h>0.0 && r.x>=screen.frame.x && r.x+r.w<=screen.frame.x+screen.frame.w && r.y>=screen.frame.y && r.y+r.h<screen.frame.y+screen.frame.h/3.0);
    if let Some(camera)=camera {usable.min.y=usable.min.y.max((camera.y+camera.h) as f32);}
    if dock.is_side(){
        let bounds=if mode==MacosPlacement::Manual{
            let mut bounds=screen.frame.bounds();if avoid_top{bounds.top=usable.top().ceil() as i32;}bounds
        }else{NativeRect::from_rect(usable).bounds()};
        let width=(bounds.right-bounds.left) as f32;let height=(bounds.bottom-bounds.top) as f32;
        // Only the root island adapts. Child settings and reader viewports
        // keep egui's normal scale and their existing system font sizes.
        let scale=requested_scale.min(width/(crate::side_dock::PANEL_W+40.0)).min(height/crate::side_dock::SIDE_H).max(minimum_scale.min(requested_scale).max(0.1));
        let panel=crate::side_dock::fit_panel(panel,egui::vec2(width/scale,height/scale));
        return Some(Layout{frame:placement_sized(bounds,scale,offset,dock,shift,panel)?,usable:bounds,scale,surface:DesktopPlacement::Legacy,panel});
    }
    if mode==MacosPlacement::Manual {
        let mut bounds=screen.frame.bounds();
        if avoid_top {bounds.top=usable.top().ceil() as i32;}
        return Some(Layout{frame:placement(bounds,requested_scale,offset,dock,shift)?,usable:bounds,scale:requested_scale,surface:DesktopPlacement::Legacy,panel});
    }
    // Preserve all body content on small displays and at large accessibility
    // zoom factors. Keep the user's saved scale; only the current screen adapts.
    let required_height=if dock.is_side(){2.0*crate::side_dock::SIDE_HALF_EXTENT}else{crate::app::WIN_H};
    let required_width=if dock.is_side(){crate::side_dock::SIDE_W}else{crate::app::WIN_W};
    let room_height=(usable.height()-if dock.is_side(){0.0}else{8.0}).max(1.0);
    let scale=requested_scale.min(usable.width()/required_width).min(room_height/required_height).max(minimum_scale.min(requested_scale).max(0.1));
    let mut bounds=NativeRect::from_rect(usable).bounds();
    if dock.is_side() {
        return Some(Layout{frame:placement(bounds,scale,offset,dock,shift)?,usable:bounds,scale,surface:DesktopPlacement::Legacy,panel});
    }
    // Auto attaches only near the camera. Offsets still work across the whole
    // screen; moving away from the camera becomes a floating pill smoothly in Y.
    let base=placement(bounds,scale,offset,dock,shift)?;
    let center=base.x+crate::app::WIN_W as f64*scale as f64/2.0;
    let displacement=center-usable.center().x as f64;
    if mode==MacosPlacement::Auto {
        if let Some(camera)=camera.filter(|_|displacement.abs()<=8.0) {
            let depth=(usable.top() as f64-screen.frame.y).max(camera.h);
            // A Dock on the left/right changes visibleFrame's center. The
            // hardware camera itself remains the zero-offset anchor.
            let x=camera.x+camera.w/2.0-crate::app::WIN_W as f64*scale as f64/2.0+displacement;
            let frame=NativeRect{x,y:screen.frame.y,h:base.h+depth,..base};
            let bridge=Rect::from_min_size(egui::pos2(((camera.x-frame.x)/scale as f64) as f32,0.0),egui::vec2((camera.w/scale as f64) as f32,(depth/scale as f64) as f32));
            return Some(Layout{frame,usable:bounds,scale,surface:DesktopPlacement::Camera{content_top:(depth/scale as f64) as f32,bridge},panel});
        }
    }
    // Native points, not egui points: the breathing room stays eight points at
    // every display backing scale and user island size.
    // Keep a transparent eight-point strip in the canvas so the upper shadow
    // is not clipped. The strip remains outside all input and blur regions.
    let frame=NativeRect{h:base.h+8.0,..base};
    bounds.top=(usable.top()+8.0).ceil() as i32;
    Some(Layout{frame,usable:bounds,scale,surface:DesktopPlacement::Floating{content_top:8.0/scale},panel})
}

fn placement(bounds:Bounds,scale:f32,offset:f32,dock:Dock,shift:(i32,i32))->Option<NativeRect> {
    placement_sized(bounds,scale,offset,dock,shift,egui::vec2(crate::side_dock::PANEL_W,crate::side_dock::PANEL_H))
}
fn placement_sized(bounds:Bounds,scale:f32,offset:f32,dock:Dock,shift:(i32,i32),panel:egui::Vec2)->Option<NativeRect>{
    let canvas=crate::side_dock::canvas_for(panel);let side_half=canvas.y/2.0;
    let (x,y)=if dock.is_side(){crate::display_layout::side_origin(bounds,dock,(canvas.x,canvas.y),side_half,scale,offset)}
        else{crate::display_layout::island_origin_offset(bounds,crate::app::WIN_W,scale,offset)}?;
    let half=if dock.is_side(){side_half}else{crate::display_layout::ISLAND_HALF_EXTENT};
    let(back,forth)=crate::display_layout::shift_slack(bounds,scale,half,offset,dock);
    let(x,y)=if dock.is_side(){(x,y+shift.1.clamp(-back,forth))}else{(x+shift.0.clamp(-back,forth),y)};
    let(w,h)=if dock.is_side(){(canvas.x,canvas.y)}else{(crate::app::CANVAS_W,crate::app::WIN_H)};
    Some(NativeRect{x:x as f64,y:y as f64,w:(w*scale) as f64,h:(h*scale) as f64})
}

#[derive(Default,Clone,Debug)]
pub struct Snapshot {
    pub battery:Option<u8>,pub charging:bool,pub volume:Option<(u32,bool)>,pub network:Option<bool>,pub idle:Option<f64>,pub locked:bool,pub fullscreen:Option<Bounds>,pub audio_device:u32,
}
impl Snapshot {
    fn read()->Self {
        let mut value=NativeSystem::default();unsafe{qy_system_sample(&mut value)};
        Self{battery:(value.battery>=0).then_some(value.battery.clamp(0,100) as u8),charging:value.charging!=0,volume:(value.volume>=0).then_some((value.volume.clamp(0,100) as u32,value.muted!=0)),network:(value.network>=0).then_some(value.network!=0),idle:value.idle.is_finite().then_some(value.idle.max(0.)),locked:value.locked!=0,fullscreen:(value.fullscreen!=0).then_some(value.fullscreen_rect.bounds()),audio_device:value.audio_device}
    }
}
pub enum SystemEvent {Sample(Snapshot),Error(&'static str)}
enum AudioCommand {Volume(u32),Mute}
pub struct System {pub rx:mpsc::Receiver<SystemEvent>,commands:mpsc::SyncSender<AudioCommand>,stop:Arc<AtomicBool>}
impl System {
    pub fn start(ctx:egui::Context)->Self {
        let(tx,rx)=mpsc::channel();let(commands,requests)=mpsc::sync_channel(8);let stop=Arc::new(AtomicBool::new(false));let running=stop.clone();
        std::thread::spawn(move||{
            while !running.load(Ordering::Acquire) {
                if tx.send(SystemEvent::Sample(Snapshot::read())).is_err(){break;}ctx.request_repaint();
                match requests.recv_timeout(Duration::from_secs(1)) {
                    Ok(command)=>{
                        let(volume,toggle)=match command{AudioCommand::Volume(v)=>(v.min(100) as i32,0),AudioCommand::Mute=>(0,1)};
                        if unsafe{qy_audio_command(volume,toggle)}==0 {let _=tx.send(SystemEvent::Error("当前音频输出不支持系统音量调整，请使用设备音量或播放器音量"));ctx.request_repaint();}
                    },
                    Err(mpsc::RecvTimeoutError::Disconnected)=>break,
                    Err(mpsc::RecvTimeoutError::Timeout)=>{},
                }
            }
        });
        Self{rx,commands,stop}
    }
    pub fn set_volume(&self,volume:u32)->bool {self.commands.try_send(AudioCommand::Volume(volume)).is_ok()}
    pub fn toggle_mute(&self)->bool {self.commands.try_send(AudioCommand::Mute).is_ok()}
}
impl Drop for System {fn drop(&mut self){self.stop.store(true,Ordering::Release);}}

fn login_path()->Option<PathBuf> {std::env::var_os("HOME").map(|p|PathBuf::from(p).join("Library/LaunchAgents/com.qingyu.macos.plist"))}
fn legacy_login_path()->Option<PathBuf> {std::env::var_os("HOME").map(|p|PathBuf::from(p).join("Library/LaunchAgents/com.daozai.macos.plist"))}
fn migrate_login_item() {
    // Isolated development runs must not change the user's login items.
    if std::env::var_os("QINGYU_DATA_DIR").is_some() || std::env::var_os("DAOZAI_DATA_DIR").is_some(){return;}
    let Some(legacy)=legacy_login_path() else{return};
    let Ok(content)=std::fs::read_to_string(&legacy) else{return};
    if !content.contains("<string>com.daozai.macos</string>") {return;}
    if login_path().is_some_and(|path|!path.exists()) && set_autostart(true).is_ok(){let _=std::fs::remove_file(legacy);}
}
fn xml_escape(value:&str)->String {value.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;").replace('\'',"&apos;")}
fn login_plist(exe:&Path)->String {format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>com.qingyu.macos</string><key>ProgramArguments</key><array><string>{}</string></array><key>RunAtLoad</key><true/><key>ProcessType</key><string>Interactive</string><key>LimitLoadToSessionType</key><string>Aqua</string></dict></plist>\n",xml_escape(&exe.to_string_lossy()))}
pub fn autostart_enabled()->bool {login_path().and_then(|p|std::fs::read_to_string(p).ok()).zip(std::env::current_exe().ok()).is_some_and(|(content,exe)|content==login_plist(&exe))}
pub fn set_autostart(enabled:bool)->Result<(),String> {
    let path=login_path().ok_or("无法找到用户主目录")?;
    if enabled {
        let exe=std::env::current_exe().map_err(|e|e.to_string())?;
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e|e.to_string())?;
        crate::model::atomic_write(&path,login_plist(&exe).as_bytes()).map_err(|e|format!("无法设置登录启动：{e}"))
    }else {match std::fs::remove_file(path){Ok(())=>Ok(()),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),Err(e)=>Err(format!("无法取消登录启动：{e}"))}}
}

/// Hold the advisory lock until the event loop exits. A same-user Unix socket
/// delivers activation requests; the lock, not a stale socket, is authoritative.
pub struct Instance {file:std::fs::File,stop:Arc<AtomicBool>,thread:Option<std::thread::JoinHandle<()>>,socket:PathBuf}
pub enum Startup {Primary(Instance),Activated}
impl Instance {
    pub fn acquire()->Result<Startup,String> {
        use std::{io::{Read,Write},os::unix::{fs::{MetadataExt,OpenOptionsExt,PermissionsExt},io::AsRawFd,net::{UnixListener,UnixStream}}};
        let directory=PathBuf::from(format!("/tmp/qingyu-{}",unsafe{getuid()}));
        match std::fs::create_dir(&directory){Ok(())=>{},Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>{},Err(e)=>return Err(e.to_string())}
        let meta=std::fs::symlink_metadata(&directory).map_err(|e|e.to_string())?;
        if !meta.is_dir() || meta.uid()!=unsafe{getuid()} {return Err("单实例目录不是当前用户所有的普通目录".into());}
        std::fs::set_permissions(&directory,std::fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;
        let file=std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).mode(0o600).custom_flags(0x100/* O_NOFOLLOW */).open(directory.join("instance.lock")).map_err(|e|e.to_string())?;
        let restart=std::env::args().any(|a|a=="--restart-after");
        let request:&[u8]=if std::env::args().any(|a|a=="--qq-music-player"){b"qq-player"}
            else if std::env::args().any(|a|a=="--qq-music"){b"qq-library"}else{b"settings"};
        let start=std::time::Instant::now();
        let locked=loop {if unsafe{flock(file.as_raw_fd(),2|4)}==0{break true;}if !restart||start.elapsed()>=Duration::from_secs(15){break false;}std::thread::sleep(Duration::from_millis(50));};
        let socket=directory.join("instance.sock");
        if !locked {
            if restart{return Err("等待旧实例退出超时，请从菜单栏退出后重试".into());}
            for _ in 0..50 {
                if let Ok(mut stream)=UnixStream::connect(&socket) {let _=stream.set_write_timeout(Some(Duration::from_secs(1)));if stream.write_all(request).is_ok(){return Ok(Startup::Activated);}}
                std::thread::sleep(Duration::from_millis(100));
            }
            return Err("轻屿已运行，但当前实例暂未响应；请从菜单栏退出后重试".into());
        }
        let _=std::fs::remove_file(&socket);
        let listener=UnixListener::bind(&socket).map_err(|e|e.to_string())?;
        listener.set_nonblocking(true).map_err(|e|e.to_string())?;
        if request!=b"settings"{IPC_EVENTS.fetch_or(activation_event(request),Ordering::Release);}
        let stop=Arc::new(AtomicBool::new(false));let stopping=stop.clone();
        let thread=std::thread::spawn(move||{
            while !stopping.load(Ordering::Acquire) {
                match listener.accept(){
                    Ok((mut stream,_))=>{
                        let _=stream.set_read_timeout(Some(Duration::from_millis(200)));let mut bytes=[0;16];
                        if let Ok(count)=stream.read(&mut bytes) {let event=activation_event(&bytes[..count]);IPC_EVENTS.fetch_or(event,Ordering::Release);if let Some(ctx)=CONTEXT.get(){if let Ok(ctx)=ctx.lock(){if let Some(ctx)=ctx.as_ref(){ctx.request_repaint();}}}}
                    },
                    Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>std::thread::sleep(Duration::from_millis(50)),
                    Err(_)=>break,
                }
            }
        });
        Ok(Startup::Primary(Self{file,stop,thread:Some(thread),socket}))
    }
}
impl Drop for Instance {
    fn drop(&mut self){use std::os::unix::io::AsRawFd;self.stop.store(true,Ordering::Release);if let Some(thread)=self.thread.take(){let _=thread.join();}let _=std::fs::remove_file(&self.socket);unsafe{flock(self.file.as_raw_fd(),8);}}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retina_placement_keeps_global_origins_in_points() {
        let external=Bounds{left:-1920,top:-240,right:0,bottom:840};
        let frame=placement(external,1.,0.,Dock::Top,(0,0)).unwrap();
        assert_eq!(frame,NativeRect{x:-1280.,y:-240.,w:960.,h:crate::app::WIN_H as f64});
        // At 2x backing scale the same frame is 1920 physical pixels wide,
        // without multiplying the global -1920 origin. Only user scale changes it.
        let large=placement(external,1.25,0.,Dock::Top,(0,0)).unwrap();
        assert_eq!(large.x,-1360.);assert_eq!(large.w,1200.);assert_eq!(large.y,-240.);
    }
    #[test]
    fn side_docks_and_offsets_keep_visible_contents_on_screen() {
        let display=Bounds{left:1440,top:-900,right:3360,bottom:180};
        for dock in [Dock::Left,Dock::Right] {for scale in [0.75,1.,1.5] {for offset in [-1.,0.,1.] {
            let frame=placement(display,scale,offset,dock,(999,999)).unwrap();
            assert_eq!(frame.x,if dock==Dock::Left{1440.}else{3360.-frame.w});
            let center=frame.y+frame.h/2.;let half=(crate::side_dock::SIDE_HALF_EXTENT*scale) as f64;
            assert!(center-half>=display.top as f64-1.);assert!(center+half<=display.bottom as f64+1.);
        }}}
    }
    #[test]
    fn invalid_geometry_and_menu_bar_reservation() {
        let display=Display{id:"screen".into(),label:"Screen".into(),bounds:Bounds{left:0,top:0,right:1512,bottom:982},work_top:38,primary:true};
        let frame=placement(crate::display_layout::placement_bounds(&display,true),1.,0.,Dock::Top,(0,0)).unwrap();assert_eq!(frame.y,38.);
        assert!(placement(display.bounds,f32::NAN,0.,Dock::Top,(0,0)).is_none());
        assert_eq!(NativeRect::from_rect(Rect::NOTHING),NativeRect::default());
    }
    #[test]
    fn login_paths_are_xml_escaped_without_shell_interpretation() {
        let plist=login_plist(Path::new("/Applications/轻屿 & <测试> '/Contents/MacOS/qingyu"));
        assert!(plist.contains("轻屿 &amp; &lt;测试&gt; &apos;"));assert!(!plist.contains("/bin/sh"));assert!(plist.contains("<key>RunAtLoad</key><true/>"));
    }
}

#[cfg(test)]
mod placement_tests {
    use super::*;
    fn notebook()->ScreenGeometry {
        ScreenGeometry{frame:NativeRect{x:0.,y:0.,w:1512.,h:982.},work:NativeRect{x:0.,y:38.,w:1512.,h:884.},camera:Some(NativeRect{x:666.,y:0.,w:180.,h:38.})}
    }
    #[test]
    fn drag_rooms_and_native_positioned_layout_share_dock_insets(){
        for screen in [
            ScreenGeometry{frame:NativeRect{x:0.0,y:0.0,w:1920.0,h:1080.0},work:NativeRect{x:0.0,y:25.0,w:1920.0,h:997.0},camera:None},
            ScreenGeometry{frame:NativeRect{x:-1512.0,y:98.0,w:1512.0,h:982.0},work:NativeRect{x:-1454.0,y:130.0,w:1454.0,h:950.0},camera:None},
        ] {
            let bounds=NativeRect::from_rect(positioned_usable(screen).unwrap()).bounds();
            for root_scale in [0.8,1.0,1.4] {
                let cursor=egui::pos2(bounds.left as f32+(bounds.right-bounds.left) as f32*0.63,bounds.top as f32+(bounds.bottom-bounds.top) as f32*0.58);
                let (room,point)=crate::island_position::display_local_geometry(cursor,bounds,root_scale).unwrap();
                let normalized=crate::island_position::normalized(point,room);
                let out=positioned_layout(screen,root_scale,0.1,normalized,Dock::Top,egui::vec2(292.0,532.0),egui::vec2(220.0,30.0)).unwrap();
                assert_eq!(out.usable,bounds);
                let DesktopPlacement::Positioned(plan)=out.surface else{panic!("positioned plan")};
                let native=egui::pos2(out.frame.x as f32+plan.resident_body(Dock::Top).center().x*out.scale,out.frame.y as f32+plan.resident_body(Dock::Top).center().y*out.scale);
                assert!((native-cursor).length()<0.01,"{screen:?}/{root_scale}: {native:?} != {cursor:?}");
            }
        }
    }
    #[test]
    fn free_top_positions_use_screen_edge_and_reserve_only_actual_camera(){
        let external=ScreenGeometry{frame:NativeRect{x:-1920.0,y:-500.0,w:1920.0,h:1080.0},work:NativeRect{x:-1920.0,y:-476.0,w:1920.0,h:1000.0},camera:None};
        for screen in [notebook(),external]{for dock in Dock::ALL{for zoom in [0.8,1.0,1.4,2.0]{for x in [0.15,0.5,0.85]{
            let resident=if dock.is_side(){egui::vec2(44.0,220.0)}else{egui::vec2(220.0,30.0)};
            let out=positioned_layout(screen,zoom,0.1,[x,0.0],dock,egui::vec2(292.0,532.0),resident).unwrap();
            assert_eq!(out.usable.top,screen.frame.y as i32,"menu bar is not a free-placement boundary");
            let DesktopPlacement::Positioned(plan)=out.surface else{panic!("positioned plan")};
            for size in [resident,if dock.is_side(){out.panel}else{egui::vec2(crate::app::PW,crate::control_center::TOP_H)}]{
                let body=plan.body(dock,size);
                let actual=Rect::from_min_size(egui::pos2(out.frame.x as f32+body.left()*out.scale,out.frame.y as f32+body.top()*out.scale),body.size()*out.scale);
                assert!(screen.frame.rect().expand(0.001).contains_rect(actual));
                if let Some(camera)=screen.camera{
                    assert!(!actual.shrink(0.001).intersects(camera.rect()),"content remains visible beside the camera");
                    let expected=if actual.left()<camera.x as f32+camera.w as f32&&actual.right()>camera.x as f32{(camera.y+camera.h) as f32}else{screen.frame.y as f32};
                    assert!((actual.top()-expected).abs()<0.001);
                }else{assert!((actual.top()-screen.frame.y as f32).abs()<0.001);}
            }
        }}}}
    }
    #[test]
    fn arbitrary_positions_share_native_points_at_negative_origins_zoom_and_display_changes(){
        let screens=[notebook(),ScreenGeometry{frame:NativeRect{x:-2560.0,y:-640.0,w:1920.0,h:1080.0},work:NativeRect{x:-2490.0,y:-616.0,w:1850.0,h:1000.0},camera:None},ScreenGeometry{frame:NativeRect{x:320.0,y:-500.0,w:640.0,h:400.0},work:NativeRect{x:320.0,y:-476.0,w:640.0,h:360.0},camera:None}];
        for screen in screens{for dock in Dock::ALL{for zoom in [1.0,2.0,3.0]{for position in [[0.0,0.0],[0.12,0.82],[0.5,0.5],[0.9,0.98],[1.0,1.0]]{for length in [80.0,520.0]{
            let resident=if dock.is_side(){egui::vec2(44.0,length)}else{egui::vec2(length,30.0)};
            let out=positioned_layout(screen,1.4*zoom,0.1*zoom,position,dock,egui::vec2(520.0,900.0),resident).unwrap();
            let DesktopPlacement::Positioned(plan)=out.surface else{panic!("one positioned plan")};
            assert!(out.scale/zoom>=0.1-0.001);
            let usable=Rect::from_min_max(egui::pos2(out.usable.left as f32,out.usable.top as f32),egui::pos2(out.usable.right as f32,out.usable.bottom as f32));
            let to_native=|point:Pos2|egui::pos2(out.frame.x as f32+point.x*out.scale,out.frame.y as f32+point.y*out.scale);
            let native_room=Rect::from_min_max(to_native(plan.room.min),to_native(plan.room.max));
            assert!((native_room.min-usable.min).length()<0.001&&(native_room.max-usable.max).length()<0.001,"global origins never scale with Retina or island zoom");
            for size in [resident,if dock.is_side(){out.panel}else{egui::vec2(crate::app::PW,crate::control_center::TOP_H)}]{
                let body=plan.body(dock,size);
                assert!(plan.room.contains_rect(body));
                let native=Rect::from_min_max(to_native(body.min),to_native(body.max));assert!(usable.expand(0.01).contains_rect(native));
                let point=body.center();let projected=to_native(point);let back=egui::pos2((projected.x-out.frame.x as f32)/out.scale,(projected.y-out.frame.y as f32)/out.scale);assert!((back-point).length()<0.001);
                let silhouette=if dock.is_side(){let (f,d,r)=crate::side_dock::shape_parameters(body.width(),body.height(),if size.x>100.0{1.0}else{0.0});Rect::from_points(&crate::shape::outline_docked(body,dock,f,d,r))}else{body};
                let strip=crate::island_notice::surface(silhouette,Rect::from_min_size(Pos2::ZERO,plan.canvas),Some(plan.room),dock,360.0,1.0).unwrap();assert!(!strip.rect.intersects(silhouette));
                assert!(usable.expand(0.01).contains_rect(Rect::from_min_max(to_native(strip.rect.min),to_native(strip.rect.max))));
            }
            if !dock.is_side(){
                let resident=plan.resident_body(dock);let panel=plan.panel_body(dock,egui::vec2(crate::app::PW,crate::control_center::TOP_H));
                assert!(!resident.intersects(panel),"a short display adapts root scale before placing either disclosure lane: {screen:?}/{zoom}/{position:?}");
                assert!(plan.room.expand(0.001).contains_rect(panel)&&Rect::from_min_size(Pos2::ZERO,plan.canvas).contains_rect(panel));
                assert!((panel.size()-egui::vec2(crate::app::PW,crate::control_center::TOP_H)).length()<0.001,"logical page dimensions stay intact");
                if let Some(camera)=plan.camera{assert!(!panel.shrink(0.001).intersects(camera));}
                let silhouette=resident.union(panel);
                let strip=crate::island_notice::surface(silhouette,Rect::from_min_size(Pos2::ZERO,plan.canvas),Some(plan.room),dock,360.0,1.0).unwrap();
                assert!(!strip.rect.intersects(resident)&&!strip.rect.intersects(panel));
            }
        }}}}}
    }
    #[test]
    fn floating_rows_share_native_canvas_at_screen_edges_and_negative_origins(){
        for screen in [notebook(),ScreenGeometry{frame:NativeRect{x:-2560.,y:-640.,w:1920.,h:1080.},work:NativeRect{x:-2560.,y:-616.,w:1920.,h:1032.},camera:None}]{
            for dock in Dock::ALL {for offset in [-1.0,0.0,1.0]{for scale in [0.8,1.0,1.5]{for requested in [egui::vec2(292.,532.),egui::vec2(400.,720.),egui::vec2(520.,900.)]{
                let out=layout_sized(screen,MacosPlacement::Auto,true,scale,0.1,offset,dock,(0,0),requested).unwrap();
                let factor=out.scale as f64;
                let size=if dock.is_side(){crate::side_dock::canvas_for(out.panel)}else{egui::vec2(crate::app::WIN_W,(out.frame.h/factor) as f32)};
                let canvas=Rect::from_min_size(Pos2::ZERO,size);
                let visible=Rect::from_min_max(egui::pos2(((out.usable.left as f64-out.frame.x)/factor)as f32,((out.usable.top as f64-out.frame.y)/factor)as f32),egui::pos2(((out.usable.right as f64-out.frame.x)/factor)as f32,((out.usable.bottom as f64-out.frame.y)/factor)as f32));
                for expanded in [false,true]{
                    let dimensions=if expanded{if dock.is_side(){out.panel}else{egui::vec2(crate::app::PW,crate::control_center::TOP_H)}}else if dock.is_side(){egui::vec2(44.,72.)}else{egui::vec2(180.,30.)};
                    let gap=match out.surface{DesktopPlacement::Camera{content_top,..}|DesktopPlacement::Floating{content_top}=>content_top,_=>6.0};
                    let body=match dock{
                        Dock::Top=>Rect::from_min_size(egui::pos2((size.x-dimensions.x)/2.,gap),dimensions),
                        Dock::Left=>Rect::from_min_size(egui::pos2(0.,(size.y-dimensions.y)/2.),dimensions),
                        Dock::Right=>Rect::from_min_size(egui::pos2(size.x-dimensions.x,(size.y-dimensions.y)/2.),dimensions),
                    };
                    let silhouette=if dock.is_side(){let (flare,depth,radius)=crate::side_dock::shape_parameters(body.width(),body.height(),if expanded{1.0}else{0.0});Rect::from_points(&crate::shape::outline_docked(body,dock,flare,depth,radius))}else{body};
                    let strip=crate::island_notice::surface(silhouette,canvas,Some(visible),dock,360.0,1.0).unwrap_or_else(||panic!("{dock:?}/{offset}/{scale}/{requested:?}, body={body:?}, canvas={canvas:?}, visible={visible:?}"));
                    assert!(!strip.rect.intersects(silhouette));assert!(canvas.contains_rect(strip.rect)&&visible.contains_rect(strip.rect));
                    assert!((strip.rect.height()-crate::island_notice::NOTICE_HEIGHT).abs()<0.001);
                }
            }}}}
        }
    }
    #[test]
    fn side_panel_size_fits_the_viewport_before_reducing_fonts_and_shares_native_canvas(){
        for screen in [notebook(),ScreenGeometry{frame:NativeRect{x:-2560.,y:-400.,w:2560.,h:1440.},work:NativeRect{x:-2560.,y:-376.,w:2560.,h:1350.},camera:None}]{for dock in [Dock::Left,Dock::Right]{for offset in [-1.,0.,1.]{
            let out=layout_sized(screen,MacosPlacement::Auto,true,1.0,0.1,offset,dock,(0,0),egui::vec2(520.,900.)).unwrap();
            assert_eq!(out.scale,1.0,"fit content area without shrinking fonts on normal Mac displays");
            let canvas=crate::side_dock::canvas_for(out.panel);assert!((out.frame.h-canvas.y as f64).abs()<0.01);assert!((out.frame.w-canvas.x as f64).abs()<0.01);
            assert!(out.frame.y>=out.usable.top as f64-1.0&&out.frame.y+out.frame.h<=out.usable.bottom as f64+1.0);
            assert!(out.panel.y>=532.0&&out.panel.y<=900.0);
        }}}
    }
    #[test]
    fn camera_content_and_bridge_share_one_coordinate_system_at_every_scale() {
        for scale in [0.8,1.,1.25,1.4] {
            let screen=notebook();let out=layout(screen,MacosPlacement::Auto,true,scale,0.5,0.,Dock::Top,(0,0)).unwrap();
            let DesktopPlacement::Camera{content_top,bridge}=out.surface else{panic!("centered camera must attach")};
            assert_eq!(out.frame.y,0.);assert!((content_top*scale-38.).abs()<0.001);
            assert!((out.frame.x+bridge.left() as f64*scale as f64-666.).abs()<0.01);
            assert!((bridge.width()*scale-180.).abs()<0.01);
            assert!((out.frame.h-(crate::app::WIN_H*scale+38.) as f64).abs()<0.01);
            assert_eq!(out.usable.top,38);
        }
    }
    #[test]
    fn noncamera_and_offset_modes_keep_eight_native_points_below_menu_bar() {
        for scale in [0.8,1.,1.4] {for offset in [-1.,0.,1.] {
            let screen=ScreenGeometry{frame:NativeRect{x:-1920.,y:-340.,w:1920.,h:1080.},work:NativeRect{x:-1920.,y:-316.,w:1920.,h:1000.},camera:None};
            for mode in [MacosPlacement::Auto,MacosPlacement::BelowMenuBar] {
                let out=layout(screen,mode,true,scale,0.5,offset,Dock::Top,(0,0)).unwrap();
                assert!(matches!(out.surface,DesktopPlacement::Floating{content_top} if (content_top*scale-8.).abs()<0.001));assert_eq!(out.frame.y,-316.);assert_eq!(out.usable.top,-308);
                let center=out.frame.x+crate::app::WIN_W as f64*scale as f64/2.;
                assert!(center-240.*scale as f64>=screen.frame.x-1.);assert!(center+240.*scale as f64<=screen.frame.x+screen.frame.w+1.);
            }
        }}
        for mode in [MacosPlacement::Auto,MacosPlacement::BelowMenuBar] {
            let out=layout(notebook(),mode,true,1.,0.5,0.5,Dock::Top,(0,0)).unwrap();
            assert_eq!(out.surface,DesktopPlacement::Floating{content_top:8.});assert_eq!(out.frame.y,38.);
        }
        assert_eq!(layout(notebook(),MacosPlacement::BelowMenuBar,true,1.,0.5,0.,Dock::Top,(0,0)).unwrap().frame.y,38.);
    }
    #[test]
    fn display_changes_manual_mode_and_side_docking_have_no_stale_camera_inset() {
        let screen=notebook();
        let attached=layout(screen,MacosPlacement::Auto,true,1.,0.5,0.,Dock::Top,(0,0)).unwrap();
        assert!(matches!(attached.surface,DesktopPlacement::Camera{..}));
        let plain=ScreenGeometry{camera:None,..screen};
        assert_eq!(layout(plain,MacosPlacement::Auto,true,1.,0.5,0.,Dock::Top,(0,0)).unwrap().surface,DesktopPlacement::Floating{content_top:8.});
        for avoid in [false,true] {
            let out=layout(screen,MacosPlacement::Manual,avoid,1.25,0.5,0.4,Dock::Top,(0,0)).unwrap();
            assert_eq!(out.surface,DesktopPlacement::Legacy);assert_eq!(out.frame.y,if avoid{38.}else{0.});assert_eq!(out.frame.h,(crate::app::WIN_H*1.25) as f64);
        }
        for dock in [Dock::Left,Dock::Right] {for offset in [-1.,1.] {
            let out=layout(screen,MacosPlacement::Auto,true,1.4,0.5,offset,dock,(0,0)).unwrap();
            assert_eq!(out.surface,DesktopPlacement::Legacy);
            let middle=out.frame.y+out.frame.h/2.;let half=(crate::side_dock::SIDE_HALF_EXTENT*out.scale) as f64;
            assert!(middle-half>=38.-1.);assert!(middle+half<=922.+1.);
        }}
    }
    #[test]
    fn side_dock_reservation_does_not_move_the_camera_anchor() {
        let screen=ScreenGeometry{work:NativeRect{x:80.,y:38.,w:1432.,h:944.},..notebook()};
        let out=layout(screen,MacosPlacement::Auto,true,1.,0.5,0.,Dock::Top,(0,0)).unwrap();
        let DesktopPlacement::Camera{bridge,..}=out.surface else{panic!("camera anchor must survive side Dock")};
        assert!((out.frame.x+bridge.left() as f64-666.).abs()<0.01);
        assert!((out.frame.x+crate::app::WIN_W as f64/2.-756.).abs()<0.01);
    }
    #[test]
    fn accessibility_zoom_respects_eframe_root_scale_floor() {
        let tiny=ScreenGeometry{frame:NativeRect{x:0.,y:0.,w:640.,h:400.},work:NativeRect{x:0.,y:24.,w:640.,h:360.},camera:None};
        let zoom=3.;let out=layout(tiny,MacosPlacement::Auto,true,1.4*zoom,0.5*zoom,0.,Dock::Top,(0,0)).unwrap();
        assert_eq!(out.scale/zoom,0.5,"native geometry and egui must use the same supported root scale");
    }
    #[test]
    fn portrait_small_displays_and_global_zoom_fit_the_same_point_geometry(){
        let tiny=ScreenGeometry{frame:NativeRect{x:-640.,y:-500.,w:640.,h:400.},work:NativeRect{x:-640.,y:-476.,w:640.,h:360.},camera:None};
        for mode in [MacosPlacement::Auto,MacosPlacement::BelowMenuBar,MacosPlacement::Manual]{for dock in [Dock::Left,Dock::Right]{for zoom in [1.,2.,3.]{for offset in [-1.,0.,1.]{
            let out=layout(tiny,mode,true,1.4*zoom,0.1*zoom,offset,dock,(999,999)).unwrap();
            assert!(out.scale/zoom>=0.1-0.001);
            let cy=out.frame.y+out.frame.h/2.;let half=crate::side_dock::SIDE_HALF_EXTENT as f64*out.scale as f64;
            assert!(cy-half>=out.usable.top as f64-1.&&cy+half<=out.usable.bottom as f64+1.,"{mode:?} / {dock:?} / {zoom} / {offset}: {out:?}");
            let body_w=crate::side_dock::PANEL_W as f64*out.scale as f64;assert!(body_w<640.);
            let logical=egui::pos2(if dock==Dock::Left{26.}else{crate::side_dock::SIDE_W-26.},crate::side_dock::SIDE_H/2.);
            let native=egui::pos2(out.frame.x as f32+logical.x*out.scale,out.frame.y as f32+logical.y*out.scale);
            let back=egui::pos2((native.x-out.frame.x as f32)/out.scale,(native.y-out.frame.y as f32)/out.scale);
            assert!((back-logical).length()<0.001,"Retina backing scale must never multiply the negative global origin");
        }}}}
    }
    #[test]
    fn small_displays_fit_content_and_invalid_camera_data_falls_back() {
        let tiny=ScreenGeometry{frame:NativeRect{x:300.,y:-500.,w:640.,h:400.},work:NativeRect{x:300.,y:-476.,w:640.,h:360.},camera:None};
        let out=layout(tiny,MacosPlacement::Auto,true,1.4,0.5,0.,Dock::Top,(0,0)).unwrap();
        let fitted=(tiny.work.h as f32-8.0)/crate::app::WIN_H;
        assert!((out.scale-fitted).abs()<0.001,"the notification lane shares the fitted canvas");assert!(out.frame.y+out.frame.h<=tiny.work.y+tiny.work.h+0.001);
        let bad=ScreenGeometry{camera:Some(NativeRect{x:-500.,y:0.,w:180.,h:38.}),..notebook()};
        assert_eq!(layout(bad,MacosPlacement::Auto,true,1.,0.5,0.,Dock::Top,(0,0)).unwrap().surface,DesktopPlacement::Floating{content_top:8.});
        for scale in [0.,-1.,f32::NAN,f32::INFINITY] {assert!(layout(notebook(),MacosPlacement::Auto,true,scale,0.5,0.,Dock::Top,(0,0)).is_none());}
    }
}
