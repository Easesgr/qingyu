//! Agent identities share the same small icon in lists and island notices.
use crate::{agent_monitor::AgentKind,agent_activity::NoticeKind,paint::{self,Card},theme};
use eframe::egui::{self,Color32,Pos2,Rect,Vec2};

pub(crate) fn source(agent:AgentKind)->Option<&'static [u8]>{
    macro_rules! png{($name:literal)=>{include_bytes!(concat!("../assets/qingyu/agents/",$name,".png")) as &[u8]};}
    Some(match agent{
        AgentKind::Codex=>png!("codex"),AgentKind::Claude=>png!("claude"),AgentKind::Hermes=>png!("hermes"),
        AgentKind::Gemini=>png!("gemini"),AgentKind::OpenCode=>png!("opencode"),AgentKind::Cursor=>png!("cursor"),
        AgentKind::Aider=>return None,
    })
}

pub(crate) fn texture(ctx:&egui::Context,agent:AgentKind)->Option<egui::TextureHandle>{
    let id=egui::Id::new(("qingyu-agent-icon",agent));
    if let Some(texture)=ctx.data(|data|data.get_temp::<egui::TextureHandle>(id)){return Some(texture);}
    let image=image::load_from_memory(source(agent)?).ok()?.into_rgba8();
    let (width,height)=image.dimensions();
    let bounds=image.enumerate_pixels().filter(|(_,_,pixel)|pixel.0[3]>8)
        .fold((width,height,0,0),|(x0,y0,x1,y1),(x,y,_)|(x0.min(x),y0.min(y),x1.max(x+1),y1.max(y+1)));
    if bounds.0>=bounds.2||bounds.1>=bounds.3{return None;}
    let artwork=image::imageops::crop_imm(&image,bounds.0,bounds.1,bounds.2-bounds.0,bounds.3-bounds.1).to_image();
    let side=64u32;let max_side=artwork.width().max(artwork.height()) as f64;
    let width=(artwork.width() as f64/max_side*56.0).round().max(1.0) as u32;
    let height=(artwork.height() as f64/max_side*56.0).round().max(1.0) as u32;
    let artwork=image::imageops::resize(&artwork,width,height,image::imageops::FilterType::Lanczos3);
    // A fixed dark tile keeps white brand artwork visible across Qingyu skins.
    let mut canvas=image::RgbaImage::new(side,side);
    for (x,y,pixel) in canvas.enumerate_pixels_mut(){
        let dx=(x as f32-31.5).abs()-19.5;let dy=(y as f32-31.5).abs()-19.5;
        if dx.max(0.0).hypot(dy.max(0.0))+dx.max(dy).min(0.0)<=12.0{*pixel=image::Rgba([23,39,44,255]);}
    }
    image::imageops::overlay(&mut canvas,&artwork,((side-width)/2).into(),((side-height)/2).into());
    let texture=ctx.load_texture(format!("agent-{:?}",agent),egui::ColorImage::from_rgba_unmultiplied([side as usize,side as usize],canvas.as_raw()),egui::TextureOptions::LINEAR);
    crate::skin_paint::mark_colored(texture.id());
    ctx.data_mut(|data|data.insert_temp(id,texture.clone()));Some(texture)
}

pub(crate) fn paint(c:&Card,agent:AgentKind,x:f32,y:f32,size:f32){
    if let Some(texture)=texture(c.ui.ctx(),agent){
        crate::skin_paint::mark_colored(texture.id());
        c.painter.image(texture.id(),Rect::from_center_size(c.pos(x,y),Vec2::splat(size*c.s)),Rect::from_min_max(Pos2::ZERO,egui::pos2(1.0,1.0)),Color32::WHITE);
    }else{paint::glyph(c,"agent",x,y,size,theme::CYAN);}
}

/// The animation belongs to the event identity, not the popup's hover/expiry
/// clock. Queued events animate when shown, and hovering never restarts them.
#[derive(Default)]
pub(crate) struct NoticeMotion { event:Option<(String,u64,NoticeKind,u64)>, since:f64 }
impl NoticeMotion {
    pub(crate) fn elapsed(&mut self,id:&str,turn:u64,kind:NoticeKind,request:u64,now:f64)->f32{
        if !self.event.as_ref().is_some_and(|(old_id,old_turn,old_kind,old_request)|old_id==id&&*old_turn==turn&&*old_kind==kind&&*old_request==request){
            self.event=Some((id.into(),turn,kind,request));self.since=now;
        }
        (now-self.since).max(0.0) as f32
    }
}

pub(crate) fn paint_notice(c:&Card,agent:Option<AgentKind>,kind:NoticeKind,x:f32,y:f32,elapsed:f32,reduced:bool){
    let phase=if reduced{1.0}else{(elapsed/0.65).clamp(0.0,1.0)};
    let reveal=1.0-(1.0-phase).powi(3);
    let accent=match kind{NoticeKind::Started=>theme::CYAN,NoticeKind::Waiting|NoticeKind::Approval=>theme::YELLOW,NoticeKind::Interrupted=>theme::RED_TOP,NoticeKind::Completed=>theme::SOFT_GREEN};
    let shift=if matches!(kind,NoticeKind::Waiting|NoticeKind::Interrupted){(phase*std::f32::consts::TAU*2.0).sin()*1.8*(1.0-reveal)}else{0.0};
    let c=c.shifted(shift,0.0).scaled(x,y,0.88+0.12*reveal);
    if let Some(agent)=agent{paint(&c,agent,x,y,21.0);}else{paint::glyph(&c,"agent",x,y,20.0,theme::CYAN);}
    // A single sweep signals arrival/completion; it never implies progress.
    if phase<1.0&&matches!(kind,NoticeKind::Started|NoticeKind::Completed){
        let sweep=std::f32::consts::TAU*reveal;
        let points:Vec<_>=(0..=24).map(|step|{
            let angle=-std::f32::consts::FRAC_PI_2+sweep*step as f32/24.0;
            c.pos(x+12.7*angle.cos(),y+12.7*angle.sin())
        }).collect();
        c.painter.add(egui::Shape::line(points,egui::Stroke::new(1.3*c.s,accent.gamma_multiply((1.0-phase)*0.9))));
    }
    let badge=c.scaled(x+7.5,y+7.5,0.7+0.3*reveal);
    let (bx,by)=(x+7.5,y+7.5);
    badge.circle(bx,by,6.2,Color32::from_rgb(23,39,44));
    badge.circle(bx,by,5.0,accent);
    let ink=Color32::from_rgb(16,33,37);
    let stroke=egui::Stroke::new(1.4*badge.s,ink);
    match kind{
        NoticeKind::Started=>{
            let points=vec![badge.pos(bx-1.3,by-2.4),badge.pos(bx+2.1,by),badge.pos(bx-1.3,by+2.4)];
            badge.painter.add(egui::Shape::convex_polygon(points,ink,egui::Stroke::NONE));
        }
        NoticeKind::Waiting=>{
            for dx in [-1.5,1.5]{badge.painter.line_segment([badge.pos(bx+dx,by-2.2),badge.pos(bx+dx,by+2.2)],stroke);}
        }
        NoticeKind::Approval=>{
            // A closed permission shield identifies a human decision. Its
            // brief entrance settles; it never flashes or implies approval.
            let shield=vec![badge.pos(bx-2.7,by-2.1),badge.pos(bx,by-3.1),badge.pos(bx+2.7,by-2.1),
                badge.pos(bx+2.3,by+0.8),badge.pos(bx,by+3.0),badge.pos(bx-2.3,by+0.8),badge.pos(bx-2.7,by-2.1)];
            badge.painter.add(egui::Shape::line(shield,egui::Stroke::new(1.0*badge.s,ink)));
            badge.circle(bx,by-0.8,0.8,ink);
            badge.painter.line_segment([badge.pos(bx,by-0.2),badge.pos(bx,by+1.1)],egui::Stroke::new(1.0*badge.s,ink));
        }
        NoticeKind::Interrupted=>{
            badge.painter.line_segment([badge.pos(bx,by-2.6),badge.pos(bx,by+0.6)],stroke);
            badge.circle(bx,by+2.4,0.7,ink);
        }
        NoticeKind::Completed=>{
            let a=badge.pos(bx-2.6,by-0.1);let b=badge.pos(bx-0.7,by+1.9);let d=badge.pos(bx+2.8,by-2.1);
            let draw=(reveal*2.0).clamp(0.0,2.0);
            if draw<=1.0{badge.painter.line_segment([a,a+(b-a)*draw],stroke);}
            else{badge.painter.add(egui::Shape::line(vec![a,b,b+(d-b)*(draw-1.0)],stroke));}
        }
    }
    if !reduced&&phase<1.0&&c.live{c.ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));}
}

pub(crate) fn ui(ui:&mut egui::Ui,agent:AgentKind,size:f32){
    if let Some(texture)=texture(ui.ctx(),agent){ui.add(egui::Image::new((texture.id(),Vec2::splat(size))));}
    else if let Some(texture)=theme::glyph(ui.ctx(),"agent"){ui.add(egui::Image::new((texture.id(),Vec2::splat(size))).tint(ui.visuals().text_color()));}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn lifecycle_icons_animate_once_settle_and_remain_distinct_with_reduced_motion(){
        let ctx=egui::Context::default();
        let mut frame=0;
        let mut render=|kind,elapsed,reduced|{
            frame+=1;
            let output=ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::splat(64.0))),time:Some(frame as f64),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                    let card=Card::new(ui,Pos2::ZERO,ui.clip_rect());
                    paint_notice(&card,Some(AgentKind::Codex),kind,32.0,32.0,elapsed,reduced);
                });
            });
            format!("{:?}",output.shapes)
        };
        let mut settled=std::collections::HashSet::new();
        for kind in [NoticeKind::Started,NoticeKind::Waiting,NoticeKind::Approval,NoticeKind::Interrupted,NoticeKind::Completed]{
            let early=render(kind,0.05,false);
            let done=render(kind,1.0,false);
            assert_ne!(early,done,"each lifecycle event needs visible motion");
            assert_eq!(done,render(kind,40.0,false),"the icon must stop animating after its brief entrance");
            assert_eq!(done,render(kind,0.0,true),"reduced motion must show the final, meaningful state immediately");
            assert!(settled.insert(done),"start, attention, interruption and completion need different symbols");
        }
    }

    #[test]
    fn agent_brand_icons_have_unique_visible_artwork_and_bounded_cached_textures(){
        let ctx=egui::Context::default();let mut ids=std::collections::HashSet::new();
        for agent in [AgentKind::Codex,AgentKind::Claude,AgentKind::Hermes,AgentKind::Gemini,AgentKind::OpenCode,AgentKind::Cursor]{
            let texture=texture(&ctx,agent).expect("bundled agent icon");
            assert_eq!(texture.size(),[64,64]);assert!(ids.insert(texture.id()));
            assert_eq!(self::texture(&ctx,agent).unwrap().id(),texture.id());
            let image=image::load_from_memory(source(agent).unwrap()).unwrap().into_rgba8();
            assert!(image.pixels().any(|pixel|pixel.0[3]>0),"{agent:?} must have visible artwork");
        }
    }
}
