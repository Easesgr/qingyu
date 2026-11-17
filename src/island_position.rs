//! One point geometry for the movable island, its native window and overlays.
use crate::model::Dock;
use crate::display_layout::{Bounds,Display};
use eframe::egui::{pos2,vec2,Pos2,Rect,Vec2};

pub const LENGTH_MIN:u32=80;
pub const LENGTH_MAX:u32=520;
/// Space for the fused shoulder between the resident's content and panel.
/// This is painted material, shared by drawing, native input and blur.
pub const PANEL_GAP:f32=8.0;

pub fn length(value:u32)->u32{if value==0{0}else{value.clamp(LENGTH_MIN,LENGTH_MAX)}}
pub fn parse_position(value:&str)->Option<[f32;2]>{
    serde_json::from_str::<Option<[f32;2]>>(value).ok().flatten()
        .filter(|point|point.iter().all(|v|v.is_finite()))
        .map(|point|point.map(|v|v.clamp(0.0,1.0)))
}
pub fn normalized(point:Pos2,room:Rect)->[f32;2]{
    [(point.x-room.left())/room.width().max(1.0),(point.y-room.top())/room.height().max(1.0)]
}
pub fn anchor(position:[f32;2],room:Rect)->Pos2{
    room.min+vec2(position[0].clamp(0.0,1.0)*room.width(),position[1].clamp(0.0,1.0)*room.height())
}

/// Attach and release use different native-point distances. The middle of a
/// display always stays free; the top edge retains its explicit legacy mode.
pub fn snap(current:Dock,point:Pos2,room:Rect,scale:f32)->Dock{
    let scale=scale.max(0.1);
    let left=(point.x-room.left())*scale;
    let right=(room.right()-point.x)*scale;
    match current{
        Dock::Left if left<=76.0=>Dock::Left,
        Dock::Right if right<=76.0=>Dock::Right,
        _ if left<=40.0&&left<right=>Dock::Left,
        _ if right<=40.0&&right<left=>Dock::Right,
        _=>Dock::Top,
    }
}

/// Resolve a global pointer to the monitor under it, or the nearest monitor
/// while it crosses a gap between staggered displays.
pub fn nearest_display<'a>(cursor:Pos2,displays:&'a [Display])->Option<&'a Display>{
    nearest_display_in_rooms(cursor,displays,&[])
}

pub fn nearest_display_in_rooms<'a>(cursor:Pos2,displays:&'a [Display],rooms:&[(String,Bounds)])->Option<&'a Display>{
    if !cursor.is_finite(){return None;}
    let mut nearest:Option<(&Display,f64)>=None;
    for display in displays {
        let bounds=display.bounds;
        if bounds.right<=bounds.left||bounds.bottom<=bounds.top{continue;}
        let left=bounds.left as f64;let top=bounds.top as f64;let right=bounds.right as f64;let bottom=bounds.bottom as f64;
        let x=cursor.x as f64;let y=cursor.y as f64;
        if x>=left&&x<right&&y>=top&&y<bottom{return Some(display);}
        // A real monitor owns its whole frame, including the menu and Dock.
        // In a physical gap, measure distance to a usable placement room.
        let bounds=rooms.iter().find(|(id,bounds)|id==&display.id&&bounds.right>bounds.left&&bounds.bottom>bounds.top).map_or(bounds,|(_,bounds)|*bounds);
        let left=bounds.left as f64;let top=bounds.top as f64;let right=bounds.right as f64;let bottom=bounds.bottom as f64;
        let dx=if x<left{left-x}else if x>right{x-right}else{0.0};
        let dy=if y<top{top-y}else if y>bottom{y-bottom}else{0.0};
        let distance=dx*dx+dy*dy;
        if nearest.is_none_or(|(_,best)|distance<best){nearest=Some((display,distance));}
    }
    nearest.map(|(display,_)|display)
}

/// Convert a primary-screen, top-down AppKit point to the target monitor's
/// egui room. The display origin is removed before scaling, which matters for
/// negative X and for monitors above or below the primary display.
pub fn display_local_geometry(cursor:Pos2,bounds:Bounds,scale:f32)->Option<(Rect,Pos2)>{
    if bounds.right<=bounds.left||bounds.bottom<=bounds.top||!scale.is_finite()||scale<=0.0||!cursor.is_finite(){return None;}
    let size=vec2((bounds.right-bounds.left) as f32/scale,(bounds.bottom-bounds.top) as f32/scale);
    if !size.is_finite()||size.x<=0.0||size.y<=0.0{return None;}
    let room=Rect::from_min_size(Pos2::ZERO,size);
    let point=pos2((cursor.x-bounds.left as f32)/scale,(cursor.y-bounds.top as f32)/scale);
    Some((room,point))
}

/// All fields use root logical points. The native frame is the same canvas,
/// translated to AppKit points once by the platform adapter.
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct Plan{
    pub anchor:Pos2,
    pub room:Rect,
    pub canvas:Vec2,
    pub scale:f32,
    pub camera:Option<Rect>,
    pub resident:Vec2,
    pub panel:Vec2,
}
impl Plan{
    pub fn body(&self,dock:Dock,size:Vec2)->Rect{body_avoiding_camera(self.room,self.anchor,dock,size,self.camera)}
    pub fn resident_body(&self,dock:Dock)->Rect{self.body(dock,self.resident)}
    pub fn panel_body(&self,dock:Dock,size:Vec2)->Rect{
        if dock.is_side(){self.body(dock,size)}else{disclosure(self.room,self.resident_body(dock),self.panel,size,self.camera)}
    }
}

/// Select a direction from the final panel, then reveal every intermediate
/// size from inside that same resident edge. Closing retraces the same lane.
/// Native root fitting reserves a complete lane; retaining the edge here also
/// prevents overlap for an undersized caller.
pub fn disclosure(room:Rect,resident:Rect,panel:Vec2,size:Vec2,camera:Option<Rect>)->Rect{
    let panel=panel.min((room.size()-Vec2::splat(16.0)).max(Vec2::splat(1.0)));
    let x=(resident.center().x-panel.x/2.0).clamp(room.left()+8.0,(room.right()-8.0-panel.x).max(room.left()+8.0));
    let mut below=resident.bottom()+PANEL_GAP;
    if let Some(camera)=camera.filter(|camera|camera.intersects(Rect::from_min_size(pos2(x,below),panel))){below=below.max(camera.bottom()+PANEL_GAP);}
    let above=resident.top()-PANEL_GAP-panel.y;
    let down=below+panel.y<=room.bottom()-8.0 || above<room.top()+8.0&&room.bottom()-below>=resident.top()-PANEL_GAP-room.top();
    let final_y=if down{below}else{above};
    let size=size.min(panel).max(Vec2::ZERO);
    let progress=(size.y/panel.y.max(1.0)).clamp(0.0,1.0);
    // A new surface starts inside the pill. Its center follows the resident
    // before moving into the fitted panel lane, including near display edges.
    // A fixed gap or an immediate jump to the final x would detach the first
    // animation frames from a short, freely placed resident.
    let center=resident.center().x+(x+panel.x/2.0-resident.center().x)*progress;
    let edge=resident.center().y+(if down{final_y}else{final_y+panel.y}-resident.center().y)*progress;
    Rect::from_min_size(pos2((center-size.x/2.0).clamp(room.left()+8.0,(room.right()-8.0-size.x).max(room.left()+8.0)),if down{edge}else{edge-size.y}),size)
}

pub fn body(room:Rect,anchor:Pos2,dock:Dock,size:Vec2)->Rect{
    // Side shoulders extend along the edge. Floating capsules have no flare.
    let margin=vec2(if dock.is_side(){0.0}else{8.0},if dock.is_side(){24.0}else{8.0});
    // A freely placed island can reach the physical top edge. The window's
    // transparent shadow/shoulder allowance must not become a drag boundary.
    let size=size.min((room.size()-vec2(margin.x*2.0,margin.y)).max(Vec2::splat(1.0))).max(Vec2::splat(1.0));
    let x=match dock{
        Dock::Left=>room.left(),Dock::Right=>room.right()-size.x,
        Dock::Top=>(anchor.x-size.x/2.0).clamp(room.left()+margin.x,(room.right()-margin.x-size.x).max(room.left()+margin.x)),
    };
    let y=(anchor.y-size.y/2.0).clamp(room.top(),(room.bottom()-margin.y-size.y).max(room.top()));
    Rect::from_min_size(pos2(x,y),size)
}

fn body_avoiding_camera(room:Rect,anchor:Pos2,dock:Dock,size:Vec2,camera:Option<Rect>)->Rect{
    let rect=body(room,anchor,dock,size);
    if let Some(camera)=camera.filter(|camera|camera.intersects(rect)){
        let safe=Rect::from_min_max(pos2(room.left(),camera.bottom().max(room.top())),room.max);
        return body(safe,anchor,dock,size);
    }
    rect
}

/// Reserve the full expansion and both notification lanes before moving the
/// window. Intermediate spring sizes, shadows and lifted shapes fit this same
/// canvas; no frame resize is needed when a notification arrives.
pub fn plan(room_size:Vec2,position:[f32;2],dock:Dock,resident:Vec2,panel:Vec2,scale:f32)->(Vec2,Plan){
    plan_with_camera(room_size,position,dock,resident,panel,scale,None)
}

pub fn plan_with_camera(room_size:Vec2,position:[f32;2],dock:Dock,resident:Vec2,panel:Vec2,scale:f32,camera:Option<Rect>)->(Vec2,Plan){
    let room=Rect::from_min_size(Pos2::ZERO,room_size);
    let anchor=anchor(position,room);
    let body=|size|body_avoiding_camera(room,anchor,dock,size,camera);
    let max_size=resident.max(panel)*1.4;
    let resident_body=body(resident);
    let panel_body=|size|if dock.is_side(){body(size)}else{disclosure(room,resident_body,panel,size,camera)};
    let envelope=body(max_size).union(resident_body).union(panel_body(panel));
    let mut canvas=envelope.expand2(vec2(32.0,80.0));
    for size in [resident,panel,max_size]{
        let body=resident_body.union(panel_body(size));
        let width=380.0f32.min((room.width()-16.0).max(1.0));
        let x=match dock{Dock::Left=>body.left()+8.0,Dock::Right=>body.right()-8.0-width,Dock::Top=>body.center().x-width/2.0}
            .clamp(room.left()+8.0,(room.right()-8.0-width).max(room.left()+8.0));
        canvas=canvas.union(Rect::from_min_size(pos2(x,body.top()-60.0),vec2(width,body.height()+120.0)));
    }
    let origin=canvas.min.to_vec2();
    (origin,Plan{anchor:anchor-origin,room:room.translate(-origin),canvas:canvas.size(),scale,camera:camera.map(|rect|rect.translate(-origin)),resident,panel})
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn free_placement_reaches_top_and_avoids_only_the_camera_footprint(){
        let room=vec2(1512.0,922.0);
        let camera=Rect::from_min_size(pos2(666.0,0.0),vec2(180.0,38.0));
        for dock in Dock::ALL{for x in [0.15,0.5,0.85]{for scale in [0.8,1.0,1.25,2.0]{
            let resident=if dock.is_side(){vec2(44.0,220.0)}else{vec2(220.0,30.0)};
            let panel=if dock.is_side(){vec2(292.0,532.0)}else{vec2(419.0,286.0)};
            let (_,plan)=plan_with_camera(room/scale,[x,0.0],dock,resident,panel,scale,
                Some(Rect::from_min_max(camera.min/scale,camera.max/scale)));
            let canvas=Rect::from_min_size(Pos2::ZERO,plan.canvas);
            for progress in [0.0,0.3,0.7,1.0]{
                let body=plan.body(dock,resident+(panel-resident)*progress);
                assert!(plan.room.expand(0.001).contains_rect(body)&&canvas.contains_rect(body));
                assert!(!body.shrink(0.001).intersects(plan.camera.unwrap()));
                let local=body.translate(-plan.room.min.to_vec2());
                let crosses_camera=local.left()<camera.right()/scale&&local.right()>camera.left()/scale;
                let top=if crosses_camera{camera.bottom()/scale}else{0.0};
                assert!((local.top()-top).abs()<0.001,"{dock:?}, {x}, {scale}, {progress}: {local:?}");
            }
        }}}
    }
    #[test]
    fn old_positions_remain_legacy_and_new_values_are_bounded(){
        assert_eq!(parse_position(""),None);assert_eq!(parse_position("null"),None);
        assert_eq!(parse_position("[0.25,0.81]"),Some([0.25,0.81]));
        assert_eq!(parse_position("[-3,9]"),Some([0.0,1.0]));
        for bad in ["[NaN,0]","[1]","{}","[1e300,0]"]{assert_eq!(parse_position(bad),None);}
        assert_eq!((length(0),length(1),length(900)),(0,80,520));
    }
    #[test]
    fn free_center_does_not_snap_and_edges_have_hysteresis_at_every_scale(){
        let room=Rect::from_min_max(pos2(-800.0,-200.0),pos2(800.0,800.0));
        for scale in [0.8,1.0,1.4,3.0]{
            for current in Dock::ALL{assert_eq!(snap(current,room.center(),room,scale),Dock::Top);}
            for (dock,sign,edge) in [(Dock::Left,1.0,room.left()),(Dock::Right,-1.0,room.right())]{
                let point=|native:f32|pos2(edge+sign*native/scale,100.0);
                assert_eq!(snap(Dock::Top,point(39.0),room,scale),dock);
                assert_eq!(snap(Dock::Top,point(42.0),room,scale),Dock::Top);
                assert_eq!(snap(dock,point(72.0),room,scale),dock);
                assert_eq!(snap(dock,point(80.0),room,scale),Dock::Top);
            }
        }
    }
    #[test]
    fn one_canvas_contains_bodies_and_separate_notices_at_all_positions(){
        for room in [vec2(1440.0,840.0),vec2(900.0,720.0)]{for dock in Dock::ALL{for x in [0.0,0.13,0.5,0.87,1.0]{for y in [0.0,0.12,0.5,0.88,1.0]{
            let panel=if dock.is_side(){vec2(292.0,532.0)}else{vec2(419.0,286.0)};
            let resident=if dock.is_side(){vec2(44.0,520.0)}else{vec2(520.0,30.0)};
            let (_,plan)=plan(room,[x,y],dock,resident,panel,1.0);
            let canvas=Rect::from_min_size(Pos2::ZERO,plan.canvas);
            for progress in [0.0,0.2,0.6,1.0]{
                let body=plan.body(dock,resident+(panel-resident)*progress);
                assert!(canvas.contains_rect(body)&&plan.room.contains_rect(body));
                let silhouette=if dock.is_side(){let (f,d,r)=crate::side_dock::shape_parameters(body.width(),body.height(),progress);Rect::from_points(&crate::shape::outline_docked(body,dock,f,d,r))}else{body};
                let strip=crate::island_notice::surface(silhouette,canvas,Some(plan.room),dock,360.0,1.0).expect("notification lane fits at either edge");
                assert!(!strip.rect.intersects(silhouette));assert!(plan.room.contains_rect(strip.rect));
            }
        }}}}
    }
}

#[cfg(test)]
mod cross_display_tests {
    use super::*;

    fn display(id:&str,bounds:Bounds)->Display {
        Display{id:id.into(),label:id.into(),bounds,work_top:bounds.top,primary:false}
    }

    #[test]
    fn global_points_map_to_negative_and_staggered_displays_at_each_root_scale(){
        let left=Bounds{left:-1512,top:98,right:0,bottom:1080};
        let cursor=pos2(-756.0,589.0);
        for scale in [1.0,2.0]{
            let (room,local)=display_local_geometry(cursor,left,scale).unwrap();
            assert_eq!(normalized(local,room),[0.5,0.5]);
        }

        let above=Bounds{left:1920,top:-600,right:3840,bottom:480};
        let (room,local)=display_local_geometry(pos2(2400.0,-60.0),above,2.0).unwrap();
        assert_eq!(normalized(local,room),[0.25,0.5]);
    }

    #[test]
    fn pointer_gaps_choose_the_nearest_valid_display_without_invalid_bounds(){
        let displays=[
            display("primary",Bounds{left:0,top:0,right:1000,bottom:800}),
            display("bad",Bounds{left:10,top:10,right:10,bottom:90}),
            display("external",Bounds{left:1100,top:100,right:2100,bottom:900}),
        ];
        assert_eq!(nearest_display(pos2(1075.0,450.0),&displays).unwrap().id,"external");
        assert_eq!(nearest_display(pos2(1040.0,450.0),&displays).unwrap().id,"primary");
        let rooms=[("primary".to_string(),Bounds{left:0,top:0,right:900,bottom:800})];
        assert_eq!(nearest_display_in_rooms(pos2(1010.0,450.0),&displays,&rooms).unwrap().id,"external","屏间空隙采用最近可放置工作区");
        assert_eq!(nearest_display_in_rooms(pos2(990.0,450.0),&displays,&rooms).unwrap().id,"primary","真实屏幕包含Dock的边条仍归本屏");
        assert!(display_local_geometry(pos2(0.0,0.0),Bounds{left:0,top:0,right:0,bottom:100},1.0).is_none());
        assert!(display_local_geometry(pos2(0.0,0.0),Bounds{left:0,top:0,right:100,bottom:100},f32::NAN).is_none());
    }
}
