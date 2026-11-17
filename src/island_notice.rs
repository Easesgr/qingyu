//! Shared transient summaries: one content model for the top and both edges.
use crate::{agent_activity, agent_monitor::Status, app::{App,Cmd,AGENT_PAGE,NOTIFICATIONS_PAGE}, extras::{Alert,AlertSource}, paint::{self,Card,H},theme::{self,W},anim::{self,Spring},model::Dock};
use eframe::egui::{self,Pos2,Rect,Vec2};

pub(crate) const NOTICE_HEIGHT:f32=36.0;
const NOTICE_MIN_WIDTH:f32=180.0;
const NOTICE_MAX_WIDTH:f32=360.0;
const NOTICE_GAP:f32=8.0;
const TEXT_X:f32=32.0;
const TEXT_GAP:f32=6.0;
const ROW_INSETS:f32=70.0;

/// Separate from the island's expansion: a new notification never resizes its
/// parent or takes over its resident content. Reversals retain spring velocity,
/// including when the next queued message arrives during an exit.
pub(crate) struct Motion { reveal:Spring,width:Spring,fresh:bool }
impl Default for Motion {fn default()->Self{Self{reveal:Spring::new(0.0,420.0,1.0),width:Spring::new(NOTICE_MIN_WIDTH,420.0,1.0),fresh:false}}}
impl Motion {
    fn set_width(&mut self,width:f32){
        let width=width.clamp(NOTICE_MIN_WIDTH,NOTICE_MAX_WIDTH);
        if self.progress()<0.003{self.width.snap(width);}else{self.width.target=width;}
    }
    fn width(&self)->f32{self.width.pos.clamp(NOTICE_MIN_WIDTH,NOTICE_MAX_WIDTH)}
    pub fn update(&mut self,visible:bool,dt:f32,reduced:bool){
        let target=if visible{1.0}else{0.0};
        // The elapsed frame interval can include seconds spent idle before a
        // delivery/expiry. Its first animated frame starts now, so that idle
        // interval must not consume most of the newly requested transition.
        let dt=if self.fresh||self.reveal.target!=target{dt.min(1.0/60.0)}else{dt};
        self.fresh=false;
        self.reveal.target=target;
        self.reveal.tune(if visible{420.0}else{680.0},1.0);
        if reduced{self.reveal.snap(self.reveal.target);self.width.snap(self.width.target);}else{
            self.reveal.step(dt);
            self.width.step(dt);
            if self.reveal.settled(1.0){self.reveal.snap(self.reveal.target);}
            if self.width.settled(1.0){self.width.snap(self.width.target);}
        }
    }
    pub fn progress(&self)->f32{self.reveal.pos.clamp(0.0,1.0)}
    fn content_hidden(&self)->bool{self.progress()<=0.34}
    fn readable(&self)->bool{self.progress()>=0.9}
    pub fn hide(&mut self){self.reveal.target=0.0;self.fresh=true;}
    pub fn animating(&self)->bool{!self.reveal.settled(1.0)||!self.width.settled(1.0)}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn top_notice_lane_stays_beside_the_island_and_outside_the_camera(){
        let canvas=Rect::from_min_size(Pos2::ZERO,Vec2::new(1512.0,982.0));
        let camera=Rect::from_min_size(Pos2::new(666.0,0.0),Vec2::new(180.0,38.0));
        // The body fits to the left; the wider notification reaches the camera.
        let body=Rect::from_min_size(Pos2::new(400.0,0.0),Vec2::new(220.0,24.0));
        for dock in Dock::ALL{for frame in 1..=100{
            let strip=surface_avoiding_camera(body,canvas,Some(canvas),dock,NOTICE_MAX_WIDTH,frame as f32/100.0,Some(camera)).unwrap();
            assert!(canvas.contains_rect(strip.rect));
            assert!(!strip.rect.intersects(body)&&!strip.rect.intersects(camera));
        }}
    }
    #[test]
    fn floating_surface_fits_negative_origins_and_flips_above_without_covering_body(){
        for dock in Dock::ALL {for scale in [0.8,1.0,1.5,2.0]{
            let canvas=Rect::from_min_size(Pos2::new(-640.0/scale,-240.0/scale),Vec2::new(640.0,600.0)/scale);
            let room=canvas.shrink(8.0);
            for bottom in [canvas.top()+100.0/scale,canvas.bottom()-18.0/scale]{
                let body=Rect::from_min_size(Pos2::new(canvas.left()+40.0/scale,bottom-80.0/scale),Vec2::new(160.0,80.0)/scale);
                for progress in [0.05,0.25,0.5,0.9,1.0]{
                    let strip=surface(body,canvas,Some(canvas),dock,NOTICE_MAX_WIDTH,progress).unwrap();
                    assert!(room.contains_rect(strip.rect));assert!(!strip.rect.intersects(body));
                    if bottom>canvas.center().y{assert!(strip.rect.bottom()<body.top());}else{assert!(strip.rect.top()>body.bottom());}
                }
            }
        }}
    }
    #[test]
    fn notice_animation_is_brief_interruptible_and_settles_in_both_motion_modes(){
        for hz in [30.0,60.0,120.0,144.0]{
            let mut motion=Motion::default();let mut peak=0.0f32;
            for frame in 0..(hz as usize){
                motion.update(true,1.0/hz,false);peak=peak.max(motion.progress());
                if frame==(hz*0.30) as usize{assert!(motion.progress()>0.98);}
            }
            assert_eq!(peak,1.0);assert!(!motion.animating());
            motion.update(false,1.0/hz,false);let before=motion.progress();
            motion.update(true,0.0,false);assert_eq!(motion.progress(),before,"changing direction must not reset position");
            for _ in 0..hz as usize{motion.update(false,1.0/hz,false);}
            assert_eq!(motion.progress(),0.0);assert!(!motion.animating());
            motion.update(true,1.0/hz,true);assert_eq!(motion.progress(),1.0);assert!(!motion.animating());
            motion.update(false,1.0/hz,true);assert_eq!(motion.progress(),0.0);
        }
    }
    #[test]
    fn floating_notice_emerges_at_the_boundary_with_a_stable_single_line_layout(){
        let canvas=Rect::from_min_max(Pos2::new(-720.0,-300.0),Pos2::new(-80.0,400.0));
        for dock in Dock::ALL{for top in [-280.0,330.0]{
            let body=Rect::from_min_size(Pos2::new(-700.0,top),Vec2::new(64.0,50.0));
            let start=surface(body,canvas,Some(canvas),dock,NOTICE_MAX_WIDTH,0.02).unwrap();
            let end=surface(body,canvas,Some(canvas),dock,NOTICE_MAX_WIDTH,1.0).unwrap();
            assert!(start.rect.width()<end.rect.width()*0.25);
            assert!(start.rect.height()<1.0&&start.content_alpha==0.0);
            let boundary_gap=if top>0.0{body.top()-start.rect.bottom()}else{start.rect.top()-body.bottom()};
            assert!(boundary_gap>0.0&&boundary_gap<1.0,"the material must emerge beside the island, without crossing it");
            let mut previous=start;
            for frame in 3..=100{
                let next=surface(body,canvas,Some(canvas),dock,NOTICE_MAX_WIDTH,frame as f32/100.0).unwrap();
                assert!(canvas.shrink(8.0).contains_rect(next.rect));assert!(!next.rect.intersects(body));
                assert!((next.rect.min-previous.rect.min).length()<8.0&&(next.rect.max-previous.rect.max).length()<8.0);
                assert_eq!(next.content_rect.width(),end.content_rect.width());assert_eq!(next.content_rect.height(),NOTICE_HEIGHT);
                previous=next;
            }
            assert_eq!(end.content_rect,end.rect);assert_eq!(end.content_alpha,1.0);
        }}
    }
}

#[derive(Clone,Copy,Debug)]
pub(crate) struct Surface {
    pub rect:Rect,pub radius:f32,pub alpha:f32,
    /// Text and controls keep their final line layout while the material
    /// expands around them; the material's rect supplies their only clip.
    pub content_rect:Rect,pub content_alpha:f32,
}

/// All arguments use the root's logical points, including negative monitor
/// coordinates. The painted strip stays outside the island's full silhouette.
pub(crate) fn surface(body:Rect,canvas:Rect,screen:Option<Rect>,dock:Dock,width:f32,progress:f32)->Option<Surface>{
    let progress=progress.clamp(0.0,1.0);
    if progress<0.003{return None;}
    let room=screen.map_or(canvas,|screen|canvas.intersect(screen)).shrink(8.0);
    let final_width=width.clamp(NOTICE_MIN_WIDTH,NOTICE_MAX_WIDTH).min(room.width());
    if final_width<NOTICE_MIN_WIDTH||room.height()<NOTICE_HEIGHT{return None;}
    let final_x=match dock{Dock::Top=>body.center().x-final_width/2.0,Dock::Left=>body.left()+8.0,Dock::Right=>body.right()-8.0-final_width}.clamp(room.left(),room.right()-final_width);
    // Check the final height so the growing mask never selects a location
    // which only fits the first, shallow frames of its entrance.
    let below=body.bottom()+NOTICE_GAP;
    let above=body.top()-NOTICE_GAP-NOTICE_HEIGHT;
    let below=if below>=room.top()&&below+NOTICE_HEIGHT<=room.bottom(){true}
        else if above>=room.top()&&above+NOTICE_HEIGHT<=room.bottom(){false}else{return None;};
    // Start at the island's boundary, then widen, separate and deepen into an
    // independent row. Interpolating two in-bounds rectangles keeps every
    // intermediate frame on the selected screen, even at negative origins.
    let seed_width=body.width().clamp(28.0,final_width*0.44);
    let seed_x=(body.center().x-seed_width/2.0).clamp(room.left(),room.right()-seed_width);
    let breadth=anim::smoothstep(0.0,0.92,progress);
    let depth=anim::smoothstep(0.0,1.0,progress);
    let width=anim::lerp(seed_width,final_width,breadth);
    let height=NOTICE_HEIGHT*depth;
    let seed_gap=if below{(room.top()-body.bottom()).max(0.8)}else{(body.top()-room.bottom()).max(0.8)};
    let gap=anim::lerp(seed_gap,NOTICE_GAP,depth);
    let x=anim::lerp(seed_x,final_x,breadth);
    let y=if below{body.bottom()+gap}else{body.top()-gap-height};
    let rect=Rect::from_min_size(Pos2::new(x,y),Vec2::new(width,height));
    let content_alpha=anim::smoothstep(0.34,0.9,progress);
    let translation=(1.0-content_alpha)*6.0;
    let content_y=if below{rect.top()-translation}else{rect.bottom()-NOTICE_HEIGHT+translation};
    Some(Surface{rect,radius:height*0.5,alpha:anim::smoothstep(0.0,0.45,progress),
        content_rect:Rect::from_min_size(Pos2::new(final_x,content_y),Vec2::new(final_width,NOTICE_HEIGHT)),content_alpha})
}

fn surface_avoiding_camera(mut body:Rect,canvas:Rect,screen:Option<Rect>,dock:Dock,width:f32,progress:f32,camera:Option<Rect>)->Option<Surface>{
    if let Some(camera)=camera{
        // A short island beside the camera may have a wider notification. Use
        // the full animation lane so no growing frame passes behind hardware.
        let end=surface(body,canvas,screen,dock,width,1.0)?;
        if body.union(end.rect).intersects(camera){body.max.y=body.max.y.max(camera.bottom());}
    }
    surface(body,canvas,screen,dock,width,progress)
}

fn row_text(alert:&Alert,queued:usize)->(String,String){
    let single_line=|text:&str|text.split_whitespace().collect::<Vec<_>>().join(" ");
    let title=single_line(&alert.title);
    let mut detail=if let Some((id,_,kind))=alert.source.agent_notice(){
        let agent=agent_activity::agent_from_session_id(id);
        let body=agent.and_then(|agent|alert.body.strip_prefix(agent.label()).and_then(|body|body.strip_prefix(' '))).unwrap_or(&alert.body);
        let (status,detail)=body.split_once(" · ").unwrap_or((body,""));
        let detail=if matches!(kind,agent_activity::NoticeKind::Waiting|agent_activity::NoticeKind::Interrupted|agent_activity::NoticeKind::Approval){detail.to_owned()}
            else if detail=="总用时未知"{"用时未知".into()}
            else{detail.strip_prefix("总用时 ").or_else(||detail.strip_prefix("用时 ")).or_else(||detail.strip_prefix("已运行 "))
                .map(|text|text.split_whitespace().collect::<String>().replace("小时","时")).unwrap_or_default()};
        single_line(&if detail.is_empty(){status.into()}else{format!("{status} · {detail}")})
    }else if alert.body.trim().is_empty(){String::new()}else{format!("· {}",single_line(&alert.body))};
    if queued>0{if !detail.is_empty(){detail.push_str(" · ");}detail.push_str(&format!("+{queued}"));}
    (title,detail)
}

impl App {
    pub(crate) fn display_notice(&self)->Option<&Alert>{self.notice.as_ref().or(self.activity_notice.as_ref())}

    pub(crate) fn notice_is_presented(&self,alert:&Alert)->bool{
        self.notice_motion.readable()&&self.hud_notice.as_ref().is_some_and(|shown|shown.same_delivery(alert))
    }

    /// Queue selection and presentation are independent. Keep the outgoing
    /// delivery until its text is invisible, then reverse the same spring with
    /// the next delivery. Privacy may clear the content at any time; an empty
    /// material surface still completes its exit without retaining that data.
    pub(crate) fn update_notice_motion(&mut self,ctx:&egui::Context,dt:f32){
        let active=self.display_notice().cloned();
        if let Some(alert)=&active{
            if self.notice_motion.content_hidden()||self.hud_notice.as_ref().is_some_and(|shown|shown.same_delivery(alert)){
                self.hud_notice=Some(alert.clone());
            }
        }
        let visible=active.as_ref().zip(self.hud_notice.as_ref()).is_some_and(|(active,shown)|shown.same_delivery(active));
        if visible{self.notice_motion.set_width(self.preferred_notice_width(ctx,self.hud_notice.as_ref().unwrap()));}
        self.notice_motion.update(visible,dt,self.reduced_motion);
        if !visible&&self.notice_motion.content_hidden(){
            self.hud_notice=active;
            if let Some(alert)=self.hud_notice.as_ref(){
                self.notice_motion.set_width(self.preferred_notice_width(ctx,alert));
                self.notice_motion.update(true,0.0,self.reduced_motion);
            }
        }
    }

    pub(crate) fn update_agent_feedback(&mut self,now:u64,now_ms:u64,busy:bool){
        use agent_activity::NoticeKind;
        self.history.retain(|alert|alert.source.agent_notice().is_none()||alert.requires_acknowledgement());
        let enabled=self.settings.notifications&&self.module_on(AGENT_PAGE)&&self.module_on(NOTIFICATIONS_PAGE)&&!self.notification_locked;
        let policy=agent_activity::Policy{enabled,activity:self.settings.agent_activity,completion:self.settings.agent_completion,interval:self.settings.agent_activity_interval};
        let update=self.agent_feedback.update(&self.agent_snapshot,policy,self.t,now_ms,busy);
        for event in update.events{
            let session=&event.session;
            let state=match event.kind{
                NoticeKind::Started=>"已开始".into(),
                NoticeKind::Waiting=>"等待确认 · 需要你的操作".into(),
                NoticeKind::Approval=>"需要审批 · 请在原应用处理".into(),
                NoticeKind::Interrupted=>format!("已中断 · {}",match session.status{
                    Status::Error=>"发生错误",Status::Idle=>"已停止本轮任务",
                    Status::Stopped if session.detail.contains("对话已关闭")=>"对话已关闭",
                    Status::Stopped=>"进程已退出",_=>"请打开对话查看",
                }),
                NoticeKind::Completed=>format!("已完成 · {}",if session.elapsed_seconds(now).is_some(){
                    agent_activity::duration(session,now).replacen("用时","总用时",1)
                }else{"总用时未知".into()}),
            };
            self.enqueue_alert(Alert{
                source:if event.kind==NoticeKind::Approval{AlertSource::AgentApproval{session_id:session.id.clone(),turn_key:session.turn_key,
                    request_key:session.approval_key,pid:session.pid,observed_at_ms:(session.updated_at>0).then(||session.updated_at.saturating_mul(1000))}}
                    else{AlertSource::AgentEvent{session_id:session.id.clone(),turn_key:session.turn_key,kind:event.kind}},
                title:agent_activity::title(session),body:format!("{} {state}",session.agent.label()),
                page:AGENT_PAGE,at:self.t,duration:if matches!(event.kind,NoticeKind::Waiting|NoticeKind::Interrupted){crate::extras::ATTENTION_NOTICE_DURATION}else{crate::extras::NOTICE_DURATION},
            },event.kind==NoticeKind::Completed);
        }
        // Refresh only identity metadata. Completion duration and the event's
        // state remain frozen even when a newer turn starts in this dialogue.
        for alert in self.history.iter_mut().chain(self.alerts.iter_mut()).chain(self.notice.iter_mut()).chain(self.hud_notice.iter_mut()).chain(self.activity_notice.iter_mut()){
            let Some((id,turn,kind))=alert.source.agent_notice()else{continue;};
            if let Some(session)=self.agent_snapshot.sessions.iter().find(|session|session.id==id){
                alert.title=if self.settings.notification_preview{crate::notification_ui::safe_preview(&agent_activity::title(session))}
                    else{agent_activity::conversation_label(id)};
                if kind==NoticeKind::Approval{
                    let state=if session.turn_key!=turn{"曾需审批 · 已进入下一轮"}
                        else{match session.status{
                            Status::Approval if session.approval_key==alert.source.approval_key()=>"需要审批 · 请在原应用处理",
                            Status::Approval=>"曾需审批 · 已有新的审批请求",
                            Status::Completed=>"曾需审批 · 本轮已完成",
                            Status::Error|Status::Idle|Status::Stopped=>"曾需审批 · 本轮已结束",
                            Status::Thinking|Status::Working=>"曾需审批 · 执行已继续",
                            _=>"曾需审批 · 打开原应用查看",
                        }};
                    alert.body=format!("{} {state}",session.agent.label());
                }
            }
        }
    }

    pub(crate) fn notice_source<'a>(&self,alert:&'a Alert)->&'a str{
        if let Some((id,_,_))=alert.source.agent_notice(){
            return agent_activity::agent_from_session_id(id).map_or("Agent",|agent|agent.label());
        }
        match &alert.source{
            AlertSource::Desktop{name,..}=>name,AlertSource::Mail{..}=>"邮箱",AlertSource::LocalTest=>"本地预览",
            AlertSource::Internal=>crate::app::PAGE_NAMES.get(alert.page).copied().unwrap_or("轻屿"),_=>"Agent",
        }
    }

    pub(crate) fn dismiss_notice(&mut self,alert:&Alert){
        if alert.requires_acknowledgement(){self.consume_notification(alert);return;}
        if self.notice.as_ref().is_some_and(|a|a.same_delivery(alert)){self.notice=None;}
        if self.activity_notice.as_ref().is_some_and(|a|a.same_delivery(alert)){self.activity_notice=None;}
        if self.hud_notice.as_ref().is_some_and(|a|a.same_delivery(alert)){self.notice_motion.hide();}
        // Keep the last safe content only for the short exit transition. Privacy
        // reconciliation still removes it immediately on lock or opt-out.
    }

    pub(crate) fn notice_close(&mut self,c:&Card,x:f32,y:f32,alert:&Alert,id:&str)->bool{
        let dismiss=c.hit(x,y,28.0,28.0,id).on_hover_text("关闭此条提醒");
        dismiss.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,"关闭此条提醒"));
        if dismiss.hovered()||dismiss.has_focus(){c.circle(x+14.0,y+14.0,14.0,theme::BTN_HOVER);}
        if dismiss.has_focus(){c.ring(x+14.0,y+14.0,14.0,1.4,theme::CYAN);}
        paint::glyph(c,"close",x+14.0,y+14.0,11.0,theme::GRAY);
        if dismiss.clicked(){self.dismiss_notice(alert);return true;}
        false
    }

    /// The overview's tiny activity indicator shares the app's motion policy.
    pub(crate) fn activity_dots(&self,c:&Card,x:f32,y:f32){
        for index in 0..3 {
            let alpha=if self.reduced_motion{0.85}else{0.5+0.4*(self.t*3.0-index as f64*0.8).sin() as f32};
            c.faded(alpha).circle(x+(index as f32-1.0)*4.5,y,1.4,theme::CYAN);
        }
        if !self.reduced_motion{c.ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));}
    }

    fn notice_preview(&self,target:&Alert)->Alert{
        let mut preview=target.clone();
        preview.title=crate::notification_ui::safe_preview(&preview.title);
        preview.body=crate::notification_ui::safe_preview(&preview.body);
        if !self.settings.notification_preview&&matches!(preview.source,AlertSource::Internal|AlertSource::LocalTest){
            preview.title=format!("{}提醒",self.notice_source(&preview));preview.body="打开相关功能查看".into();
        }
        preview
    }

    fn preferred_notice_width(&self,ctx:&egui::Context,target:&Alert)->f32{
        let (title,detail)=row_text(&self.notice_preview(target),self.alerts.len());
        let measure=|text:String,size:f32,weight:W|ctx.fonts(|fonts|fonts.layout_no_wrap(text,theme::font(size,weight),theme::WHITE).size().x);
        let gap=if title.is_empty()||detail.is_empty(){0.0}else{TEXT_GAP};
        (ROW_INSETS+measure(title,12.0,W::Semibold)+gap+measure(detail,11.0,W::Regular)).ceil().clamp(NOTICE_MIN_WIDTH,NOTICE_MAX_WIDTH)
    }

    /// The same text feeds width measurement and painting. Long messages share
    /// a bounded line; short status messages leave no unused button column.
    fn draw_notice_text(&self,c:&Card,y:f32,width:f32,alert:&Alert){
        let (title,detail)=row_text(alert,self.alerts.len());
        let gap=if title.is_empty()||detail.is_empty(){0.0}else{TEXT_GAP};
        let detail_width=c.text_w(&detail,11.0,W::Regular);
        let title_budget=(width-detail_width.min(width*0.54)-gap).max(0.0);
        let title=c.fit(&title,12.0,W::Semibold,title_budget);
        c.centered_text(TEXT_X,y,&title,12.0,W::Semibold,theme::WHITE,H::Left);
        let title_width=c.text_w(&title,12.0,W::Semibold);
        let queue=if self.alerts.is_empty(){String::new()}else{format!(" · +{}",self.alerts.len())};
        let queue=if detail.ends_with(&queue){queue}else{String::new()};
        let queue_width=c.text_w(&queue,11.0,W::Regular);
        let content=detail.strip_suffix(&queue).unwrap_or(&detail);
        let detail=c.fit(content,11.0,W::Regular,(width-title_width-gap-queue_width).max(0.0));
        c.centered_text(TEXT_X+title_width+gap,y,&detail,11.0,W::Regular,theme::GRAY,H::Left);
        c.centered_text(TEXT_X+width,y,&queue,11.0,W::Regular,theme::GRAY,H::Right);
    }

    pub(crate) fn draw_agent_notice(&mut self,c:&Card,panel:Rect,target:&Alert,cmds:&mut Vec<Cmd>){
        // Display redaction must not change the delivery addressed by a click.
        // Local/internal messages use their content as part of their identity.
        let preview=self.notice_preview(target);
        let alert=&preview;
        let width=panel.width()/c.s;let height=panel.height()/c.s;
        if width<180.0||height<4.0{return;}
        let c=c.clipped(0.0,0.0,width,height);let y=height/2.0;
        if let Some((id,turn,kind))=alert.source.agent_notice(){
            let elapsed=self.agent_notice_motion.elapsed(id,turn,kind,alert.source.approval_key(),self.t);
            crate::agent_icons::paint_notice(&c,agent_activity::agent_from_session_id(id),kind,16.0,y,elapsed,self.reduced_motion);
        }else{crate::extras::notice_badge(&c,16.0,y,alert);}
        self.draw_notice_text(&c,y,width-ROW_INSETS,alert);
        let tooltip=format!("{}\n{}\n{}",alert.title,alert.body,self.notice_source(alert));
        let info=c.hit(0.0,0.0,width,height,"notice-summary")
            .on_hover_text(format!("{tooltip}\n点击打开对应内容"))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        info.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,format!("{tooltip}，打开对应内容")));
        if info.has_focus(){c.stroke(2.0,2.0,width-4.0,height-4.0,16.0,1.4,theme::CYAN);}
        if self.notice_close(&c,width-32.0,y-14.0,target,"notice-dismiss"){return;}
        if info.clicked(){
            self.dismiss_notice(target);self.open_notification(target,cmds);
            c.ui.ctx().memory_mut(|memory|for id in ["notice-summary","notice-dismiss"]{memory.surrender_focus(egui::Id::new(("card",true,id)));});
            c.ui.ctx().request_repaint();
        }
    }

    pub(crate) fn notice_surface(&self,win:Rect)->Option<Surface>{
        let panel=self.displayed_island_rect(win);
        let silhouette=if self.side(){
            let (flare,depth,radius)=crate::side_dock::shape_parameters(panel.width(),panel.height(),self.progress());
            Rect::from_points(&crate::shape::outline_docked(panel,self.dock(),flare,depth,radius))
        }else if self.progress()>0.003 {
            self.horizontal_join(win,panel,self.progress()).bounds
        }else{self.resident_rect(win)};
        let camera=match self.desktop_placement{crate::app::DesktopPlacement::Positioned(plan)=>plan.camera.map(|rect|rect.translate(win.min.to_vec2())),_=>None};
        surface_avoiding_camera(silhouette,win,self.screen,self.dock(),self.notice_motion.width(),self.notice_motion.progress(),camera)
    }

    pub(crate) fn draw_floating_notice(&mut self,ui:&egui::Ui,win:Rect,opaque:bool,cmds:&mut Vec<Cmd>)->Option<Surface>{
        let surface=self.notice_surface(win)?;
        let mut painter=ui.painter_at(surface.rect);
        painter.multiply_opacity(surface.alpha);
        let fill=self.glass_fill();
        let fill=if opaque{egui::Color32::from_rgb(fill.r(),fill.g(),fill.b())}else{fill};
        // egui's rounded rectangles quantize radii to whole points. Paint the
        // same continuous outline supplied to native input/blur so a shallow
        // emerging capsule cannot gain square or stepped corners.
        let outline=crate::app::GlassShape::Round(surface.rect,surface.radius).outline();
        painter.add(egui::Shape::convex_polygon(outline,fill,egui::Stroke::NONE));
        let rim=crate::app::GlassShape::Round(surface.rect.shrink(0.35_f32.min(surface.rect.height()*0.25)),(surface.radius-0.35).max(0.0)).outline();
        painter.add(egui::Shape::closed_line(rim,egui::Stroke::new(0.7_f32,self.glass_rim())));
        let Some(alert)=self.hud_notice.clone()else{return Some(surface);};
        let live=self.notice_is_presented(&alert)&&self.display_notice().is_some_and(|active|active.same_delivery(&alert));
        let c=Card::new(ui,surface.content_rect.min,surface.rect.intersect(ui.max_rect())).faded(surface.content_alpha);
        let c=if live{c}else{c.ghost()};
        let ctx=ui.ctx();let layer=c.painter.layer_id();let from=crate::skin_paint::shape_count(ctx,layer);
        self.draw_agent_notice(&c,surface.content_rect,&alert,cmds);
        if let Some(palette)=self.skin_palette(){
            crate::skin_paint::recolor_layer(ctx,layer,&[(from,crate::skin_paint::shape_count(ctx,layer))],&palette);
        }
        Some(surface)
    }
}
