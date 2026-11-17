//! 左右停靠：岛体贴在屏幕左 / 右边缘，收起时是竖向胶囊，展开后面板从边缘滑出。
//!
//! 展开后使用 292 × 532 的独立竖向页面，沿屏幕边缘上下展开；
//! 收起态换成竖排：时间（时 / 分两行）、天气、音乐与提醒铃铛。
//! 岛上歌词是横向内容，侧边停靠时不显示。

use crate::app::{App, Env, BAR_H_MIN, BAR_W_MIN};
use crate::model::Dock;
use crate::paint::{self, Card, H};
use crate::theme::{self, W};
use eframe::egui::{Color32, Pos2, Rect, Vec2};

/// 侧边停靠时的透明画布（窗口）大小，逻辑点。
pub const SIDE_W: f32 = 500.0;
pub const SIDE_H: f32 = 712.0;
/// 左右两侧共用的竖向面板尺寸；透明画布不参与内容布局。
pub const PANEL_W: f32 = 292.0;
pub const PANEL_H: f32 = 532.0;
/// 收起时竖向胶囊的厚度。
pub const SIDE_BAR_W: f32 = 44.0;
/// 展开面板半高 + 肩部、弹簧过冲和阴影，统一用于原生定位与拖动限制。
pub const SIDE_HALF_EXTENT: f32 = SIDE_H / 2.0;

/// Symmetric transparent lanes let an external row fit below or above the
/// island without resizing the native canvas when notifications arrive.
pub fn canvas_for(panel:Vec2)->Vec2{Vec2::new((panel.x+52.0).max(SIDE_W),panel.y+(SIDE_H-PANEL_H))}
/// Fit the content viewport before reducing the root's scale. Larger panels
/// show more rows with the same fonts; the requested size remains saved.
pub fn fit_panel(requested:Vec2,room:Vec2)->Vec2{
    requested.clamp(Vec2::new(PANEL_W,PANEL_H),Vec2::new(520.0,900.0))
        .min(Vec2::new((room.x-40.0).max(PANEL_W),(room.y-(SIDE_H-PANEL_H)).max(PANEL_H)))
}

/// One geometry contract for the shoulder mesh, input polygon and blur mask.
pub(crate) fn shape_parameters(width:f32,height:f32,progress:f32)->(f32,f32,f32){
    let t=progress.clamp(0.0,1.0);
    // Resident summaries grow through the same width spring while expansion
    // progress remains zero. Derive their softer corners from painted width,
    // so arrival/dismissal cannot jump the mesh, native hit or blur silhouette.
    let summary=crate::anim::smoothstep(0.0,1.0,(width-SIDE_BAR_W)/(PANEL_W-SIDE_BAR_W));
    let flare=crate::anim::lerp(1.5,22.0,t).max(crate::anim::lerp(1.5,9.0,summary)).min(height*0.12);
    let depth=crate::anim::lerp(3.0,22.0,t).max(crate::anim::lerp(3.0,12.0,summary)).min(width*0.5).max(0.1);
    let radius=crate::anim::lerp(3.0,26.0,t).max(crate::anim::lerp(3.0,18.0,summary)).min(width*0.5).min(height*0.5).max(0.0);
    (flare,depth,radius)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SideItem {
    Pet,Clock, Weather, Music, Bell,Agents,
}

impl SideItem {
    fn height(self) -> f32 {
        match self {Self::Pet=>34.0,Self::Agents=>48.0, Self::Clock => 32.0, Self::Weather => 40.0, Self::Music => 18.0, Self::Bell => 22.0 }
    }
}

impl App {
    fn side_item_height(&self,item:SideItem)->f32{
        if item==SideItem::Music&&self.settings.resident_length>=300{if self.settings.resident_length>=440{64.0}else{48.0}}
        else{item.height()}
    }
    pub(crate) fn side_panel_size(&self)->Vec2{self.side_panel_override.unwrap_or_else(||self.settings.side_panel_size())}
    pub(crate) fn side_canvas_size(&self)->Vec2{canvas_for(self.side_panel_size())}
    pub(crate) fn dock(&self) -> Dock { self.settings.dock }
    pub(crate) fn side(&self) -> bool { self.settings.dock.is_side() }
    pub(crate) fn side_handle(&self)->bool {cfg!(target_os="macos") && self.side() && self.settings.macos_side_handle}

    /// 竖向胶囊里依次显示的内容。
    pub(crate) fn side_items(&self) -> Vec<SideItem> {
        let mut items = Vec::new();
        if self.settings.pet{items.push(SideItem::Pet);}
        items.push(SideItem::Clock);
        if self.weather_pill().is_some() {items.push(SideItem::Weather);}
        if self.media.is_music() { items.push(SideItem::Music); }
        if self.module_on(crate::app::AGENT_PAGE){items.push(SideItem::Agents);}
        items
    }

    /// 收起时竖向胶囊的长度。
    pub(crate) fn side_bar_len(&self) -> f32 {
        14.0 + self.side_items().iter().map(|i| self.side_item_height(*i)).sum::<f32>()
    }

    /// 收起态尺寸（屏幕宽 × 高）。极简模式是贴边的一小段竖线。
    pub(crate) fn side_base_size(&self, minimal: bool) -> (f32, f32) {
        let requested=self.settings.resident_length as f32;
        if self.side_handle(){(6.0,52.0f32.max(requested))}else if minimal { (BAR_H_MIN, BAR_W_MIN.max(requested)) } else { (SIDE_BAR_W, self.side_bar_len().max(requested).min(SIDE_H-24.0)) }
    }


    /// 宽 `w`、高 `h` 的岛体在画布 `win` 里的位置：顶部停靠水平居中贴顶，
    /// 左右停靠贴边、竖直居中。
    pub(crate) fn island_rect(&self, win: Rect, w: f32, h: f32) -> Rect {
        if let crate::app::DesktopPlacement::Positioned(plan)=self.desktop_placement{
            return plan.body(self.dock(),Vec2::new(w,h)).translate(win.min.to_vec2());
        }
        let gap = self.island_gap();
        let rect=match self.dock() {
            Dock::Top => Rect::from_min_size(Pos2::new(win.center().x - w / 2.0, win.top() + gap), Vec2::new(w, h)),
            Dock::Left => Rect::from_min_size(Pos2::new(win.left() + gap, win.center().y - h / 2.0), Vec2::new(w, h)),
            Dock::Right => Rect::from_min_size(Pos2::new(win.right() - gap - w, win.center().y - h / 2.0), Vec2::new(w, h)),
        };
        // A custom resident can be wider than the expanded panel. Preserve
        // legacy positioning while keeping that longer capsule on the display.
        if self.settings.resident_length>0&&!self.side(){if let Some(room)=self.screen{
            let dx=(rect.left().max(room.left()+8.0).min((room.right()-8.0-w).max(room.left()+8.0)))-rect.left();
            return rect.translate(Vec2::new(dx,0.0));
        }}
        rect
    }

    /// 展开面板（页面内容）的左上角。
    pub(crate) fn content_origin(&self, win: Rect) -> Pos2 {
        if self.side(){self.displayed_island_rect(win).min}
        else{self.horizontal_panel_rect(win,Vec2::new(self.expanded_width(),self.expanded_height())).min}
    }

    /// The resident body itself is draggable, so it needs no separate footer.
    /// The native silhouette still excludes the transparent desktop.
    pub(crate) fn side_position_rect(&self,win:Rect)->Rect{
        let rect=if self.side(){self.displayed_island_rect(win)}else{self.resident_rect(win)};
        let inset=3.0f32.min(rect.height()*0.2);
        Rect::from_min_max(rect.min+Vec2::new(0.0,inset),rect.max-Vec2::new(0.0,inset))
    }

    /// Exact side outline's bounding rectangle; AppKit uses the polygon below
    /// to exclude transparent shoulder and corner regions from input.
    pub(crate) fn side_hit_rect(&self, win: Rect, p: f32) -> Rect {
        let _=p;
        Rect::from_points(&self.side_hit_outline(win))
    }

    pub(crate) fn side_hit_outline(&self,win:Rect)->Vec<Pos2>{
        let rect=self.displayed_island_rect(win);
        let (flare,depth,radius)=shape_parameters(rect.width(),rect.height(),self.progress());
        crate::shape::outline_docked(rect,self.dock(),flare,depth,radius)
    }

    pub(crate) fn side_contains(&self,win:Rect,point:Pos2)->bool{
        crate::shape::contains_outline(&self.side_hit_outline(win),point)
    }

    /// 毛玻璃取样点（画布坐标）：岛体外侧上下左右各取几处估计身后明暗。
    pub fn backdrop_points(&self) -> Vec<(f32, f32)> {
        if let crate::app::DesktopPlacement::Positioned(plan)=self.desktop_placement{
            let rect=if self.open{plan.panel_body(self.dock(),Vec2::new(self.expanded_width(),self.expanded_height())).union(plan.resident_body(self.dock()))}else{plan.resident_body(self.dock())}.expand(10.0);
            return [rect.left_top(),rect.center_top(),rect.right_top(),rect.right_center(),rect.right_bottom(),rect.center_bottom(),rect.left_bottom(),rect.left_center()]
                .into_iter().map(|point|plan.room.clamp(point)).map(|point|(point.x,point.y)).collect();
        }
        match self.dock() {
            Dock::Top => {
                let cx = crate::app::WIN_W / 2.0;
                let bottom = crate::app::WIN_H - 6.0;
                vec![(cx - 270.0, 6.0), (cx - 270.0, 26.0), (cx + 270.0, 6.0), (cx + 270.0, 26.0), (cx - 130.0, bottom), (cx, bottom), (cx + 130.0, bottom)]
            }
            dock => {
                let (near, far) = if dock == Dock::Left { (6.0, SIDE_W - 6.0) } else { (SIDE_W - 6.0, 6.0) };
                let mid = if dock == Dock::Left { 26.0 } else { SIDE_W - 26.0 };
                let cy = SIDE_H / 2.0;
                vec![(near, cy - 170.0), (mid, cy - 170.0), (near, cy + 170.0), (mid, cy + 170.0), (far, cy - 60.0), (far, cy), (far, cy + 60.0)]
            }
        }
    }

    /// 竖向胶囊的内容。`c` 的原点是胶囊左上角。
    pub(crate) fn side_bar(&mut self, c: &Card, width: f32, height: f32, env: &Env) {
        if self.side_handle() && width<18.0 {
            let dot=if self.notice.is_some() || self.t<self.notification_bell_until{theme::RED_TOP}else{Color32::from_gray(160)};
            c.fill((width-2.0)/2.0,(height-22.0)/2.0,2.0,22.0,1.0,dot);
            return;
        }
        let items = self.side_items();
        let total: f32 = items.iter().map(|i| self.side_item_height(*i)).sum();
        let cx = width / 2.0;
        let gap=((height-total-14.0).max(0.0))/(items.len()+1) as f32;
        let mut y = 7.0+gap;
        for item in items {
            let h = self.side_item_height(item);
            let mid = y + h / 2.0;
            match item {
                SideItem::Pet=>self.draw_resident_pet(c,cx,mid,31.0),
                SideItem::Agents=>self.draw_resident_team(c,cx,mid,true,70.0),
                SideItem::Clock => {
                    let (hour, minute) = side_clock(&self.settings.clock_text(&env.clock));
                    c.text(cx, mid - 1.0, &hour, 13.5, W::Bold, theme::WHITE, H::Center);
                    c.text(cx, mid + 13.0, &minute, 13.5, W::Bold, theme::WHITE, H::Center);
                }
                SideItem::Weather => self.draw_side_weather(c,cx,mid),
                SideItem::Music => {
                    let bars_y=if h>18.0{y+8.0}else{mid};
                    paint::playback_bars(c,cx,bars_y,self.t,self.music_playing()&&!self.reduced_motion,theme::WHITE);
                    if h>18.0{
                        c.centered_text(cx,y+28.0,&c.fit(&self.media.title,10.5,W::Semibold,34.0),10.5,W::Semibold,theme::WHITE,H::Center);
                        if h>=64.0{c.centered_text(cx,y+44.0,&c.fit(&self.media.artist,10.0,W::Regular,34.0),10.0,W::Regular,theme::GRAY,H::Center);}
                    }
                    if c.hit(cx-17.0,y-3.0,34.0,h.max(24.0),"side-music").on_hover_text(format!("{} · {}\n打开音乐",self.media.title,self.media.artist)).on_hover_cursor(eframe::egui::CursorIcon::PointingHand).clicked(){self.show_page(crate::app::MUSIC_PAGE);}
                }

                SideItem::Bell => {
                    paint::bell(&c.scaled(cx, mid, 1.1), cx, mid, theme::WHITE, paint::bell_swing(self.t - self.notification_bell_at));
                    c.circle(cx + 4.0, mid - 4.5, 2.0, Color32::from_rgb(255, 72, 72));
                }
            }
            y += h+gap;
        }
    }
}

/// 竖排时间：「09:41」→ 「09」「41」；12 小时制去掉上午 / 下午等文字。
pub(crate) fn side_clock(text: &str) -> (String, String) {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit() || *c == ':').collect();
    match digits.split_once(':') {
        Some((h, m)) if !h.is_empty() => (h.to_string(), m.chars().take(2).collect()),
        _ => ("--".into(), "--".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clock_splits_into_two_lines() {
        assert_eq!(side_clock("09:41"), ("09".into(), "41".into()));
        assert_eq!(side_clock("下午 9:05"), ("9".into(), "05".into()));
        assert_eq!(side_clock("--:--"), ("--".into(), "--".into()));
    }
    #[test]
    fn summary_corners_follow_the_painted_width_continuously(){
        let mut previous=shape_parameters(44.0,94.0,0.0).2;
        assert_eq!(previous,3.0);
        for width in 45..=340{
            let radius=shape_parameters(width as f32,94.0,0.0).2;
            assert!(radius>=previous&&(radius-previous)<0.1);
            previous=radius;
        }
        assert_eq!(previous,18.0);
        assert_eq!(shape_parameters(292.0,532.0,1.0).2,26.0);
    }
}
