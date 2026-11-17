//! Small resident companions. Artwork is cached once; poses are bounded and
//! deterministic, and task activity comes only from observed execution turns.
use crate::{agent_monitor::{AgentKind,Snapshot,Status},anim,app::{App,AGENT_PAGE},model::PetKind,paint::Card,theme};
use eframe::egui::{self,Color32,Pos2,Rect,Vec2};

pub(crate) const AGENTS:[AgentKind;7]=[AgentKind::Codex,AgentKind::Claude,AgentKind::Hermes,AgentKind::Gemini,AgentKind::OpenCode,AgentKind::Cursor,AgentKind::Aider];

#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]
pub(crate) struct Team {pub running:usize,pub waiting:usize,counts:[[usize;2];7]}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
struct Companion {agent:AgentKind,running:bool,waiting:bool}
impl Team {
    pub fn from_snapshot(snapshot:&Snapshot)->Self{
        let mut team=Self::default();
        for session in &snapshot.sessions{
            if session.process_only||session.turn_key==0{continue;}
            let state=match session.status{
                Status::Thinking|Status::Working=>{team.running+=1;0},
                Status::Waiting|Status::Approval=>{team.waiting+=1;1},
                _=>continue,
            };
            team.counts[agent_index(session.agent)][state]+=1;
        }
        team
    }
    fn label(self)->String{
        let agents=AGENTS.into_iter().enumerate().filter(|(i,_)|self.counts[*i].iter().sum::<usize>()>0).map(|(_,agent)|agent.label()).collect::<Vec<_>>().join("、");
        let status=match (self.running,self.waiting){
            (0,0)=>return "Agent · 暂无执行中的任务".into(),
            (running,0)=>format!("{running} 个任务执行中"),
            (0,waiting)=>format!("{waiting} 个任务等待操作"),
            (running,waiting)=>format!("{running} 个任务执行中，{waiting} 个等待操作"),
        };format!("{agents} · {status}")
    }
    fn figures(self,t:f64,reduced:bool)->[Option<Companion>;3]{
        let mut figures=[None;3];
        let total=self.running+self.waiting;
        if total==0{figures[0]=Some(Companion{agent:AgentKind::Codex,running:false,waiting:false});return figures;}
        if total>3{
            // Keep the original compact overflow count. Mixed teams take
            // turns displaying their own pet and matching application mark.
            let active=AGENTS.into_iter().filter(|agent|self.counts[agent_index(*agent)].iter().sum::<usize>()>0).collect::<Vec<_>>();
            let index=if reduced{0}else{(t.max(0.0)/4.0).floor() as usize%active.len()};
            let agent=active[index];let counts=self.counts[agent_index(agent)];
            figures[0]=Some(Companion{agent,running:counts[0]>0&&counts[1]==0,waiting:counts[1]>0});
        }else{
            let mut index=0;
            for (agent,counts) in AGENTS.into_iter().zip(self.counts){
                for waiting in [true,false]{for _ in 0..counts[usize::from(waiting)]{
                    figures[index]=Some(Companion{agent,running:!waiting,waiting});index+=1;
                }}
            }
        }
        figures
    }
}

fn agent_index(agent:AgentKind)->usize{AGENTS.iter().position(|kind|*kind==agent).expect("known companion agent")}

pub(crate) fn binding_key(agent:AgentKind)->&'static str{match agent{
    AgentKind::Codex=>"codex",AgentKind::Claude=>"claude",AgentKind::Hermes=>"hermes",AgentKind::Gemini=>"gemini",AgentKind::OpenCode=>"opencode",AgentKind::Cursor=>"cursor",AgentKind::Aider=>"aider",
}}

impl crate::model::Settings{
    pub(crate) fn agent_pet(&self,agent:AgentKind)->PetKind{
        self.agent_pets.get(binding_key(agent)).copied().unwrap_or(PetKind::ALL[agent_index(agent)])
    }
    pub(crate) fn resident_pet_kind(&self)->PetKind{
        self.resident_pet_agent.as_deref().and_then(|key|AGENTS.into_iter().find(|agent|binding_key(*agent)==key))
            .map(|agent|self.agent_pet(agent)).unwrap_or(self.resident_pet)
    }
}

/// Preserve the generated sheets on disk. Normalize their transparent frame
/// bounds only when uploading small, bounded UI textures; every pose in a
/// sheet uses one shared crop and scale so expressions never change head size.
fn frames(ctx:&egui::Context,name:&'static str,bytes:&[u8],columns:u32,rows:u32)->Vec<egui::TextureHandle>{
    let key=egui::Id::new(("island-companion-art",name));
    if let Some(textures)=ctx.data(|data|data.get_temp::<Vec<egui::TextureHandle>>(key)){return textures;}
    let sheet=image::load_from_memory(bytes).expect("bundled companion artwork").into_rgba8();
    let (cell_w,cell_h)=(sheet.width()/columns,sheet.height()/rows);
    let mut bounds=(cell_w,cell_h,0,0);
    for (x,y,pixel) in sheet.enumerate_pixels(){if pixel.0[3]>32{
        let (x,y)=(x%cell_w,y%cell_h);
        bounds=(bounds.0.min(x),bounds.1.min(y),bounds.2.max(x+1),bounds.3.max(y+1));
    }}
    let (w,h)=(bounds.2-bounds.0,bounds.3-bounds.1);
    let factor=128.0/w.max(h) as f32;
    let textures=(0..columns*rows).map(|index|{
        let frame=image::imageops::crop_imm(&sheet,(index%columns)*cell_w+bounds.0,(index/columns)*cell_h+bounds.1,w,h).to_image();
        let frame=image::imageops::resize(&frame,(w as f32*factor).round() as u32,(h as f32*factor).round() as u32,image::imageops::FilterType::Lanczos3);
        let texture=ctx.load_texture(format!("{name}-{index}"),egui::ColorImage::from_rgba_unmultiplied([frame.width() as usize,frame.height() as usize],frame.as_raw()),egui::TextureOptions::LINEAR);
        crate::skin_paint::mark_colored(texture.id());texture
    }).collect::<Vec<_>>();
    ctx.data_mut(|data|data.insert_temp(key,textures.clone()));textures
}

fn image(c:&Card,texture:&egui::TextureHandle,center:Pos2,size:Vec2,pivot:Pos2,angle:f32){
    let (sin,cos)=angle.sin_cos();
    let rotate=|p:Pos2|{let d=p-pivot;let p=pivot+Vec2::new(d.x*cos-d.y*sin,d.x*sin+d.y*cos);c.pos(p.x,p.y)};
    let r=Rect::from_center_size(center,size);
    let mut mesh=egui::Mesh::with_texture(texture.id());
    for (pos,uv) in [(r.left_top(),Pos2::new(0.0,0.0)),(r.right_top(),Pos2::new(1.0,0.0)),(r.right_bottom(),Pos2::new(1.0,1.0)),(r.left_bottom(),Pos2::new(0.0,1.0))]{
        mesh.vertices.push(egui::epaint::Vertex{pos:rotate(pos),uv,color:Color32::WHITE});
    }
    mesh.indices.extend_from_slice(&[0,1,2,0,2,3]);c.painter.add(egui::Shape::mesh(mesh));
}

fn head(ctx:&egui::Context,kind:PetKind)->egui::TextureHandle{
    if kind==PetKind::Tan{return frames(ctx,"island-pet-expressions",include_bytes!("../assets/qingyu/companion/pet-expressions.png"),2,2)[0].clone();}
    let index=PetKind::ALL.iter().position(|pet|*pet==kind).unwrap()-1;
    frames(ctx,"island-agent-pets",include_bytes!("../assets/qingyu/companion/agent-pet-heads.png"),3,2)[index].clone()
}

pub(crate) fn pet_icon(c:&Card,kind:PetKind,x:f32,y:f32,size:f32){
    let texture=head(c.ui.ctx(),kind);
    let ratio=texture.size()[0] as f32/texture.size()[1] as f32;
    let dimensions=if ratio>=1.0{Vec2::new(size,size/ratio)}else{Vec2::new(size*ratio,size)};
    let center=Pos2::new(x,y);image(c,&texture,center,dimensions,center,0.0);
}

/// No task state, sound or celebration is implied by a pet greeting.
pub(crate) fn pet(c:&Card,kind:PetKind,x:f32,y:f32,size:f32,t:f64,reduced:bool){
    let key=egui::Id::new("island-pet-greeting");
    // A crossfade can paint multiple copies of this avatar. Ghost copies
    // share its expression state but must not register another hit target.
    if c.live{
        let response=c.hit(x-14.0,y-14.0,28.0,28.0,"island-pet").on_hover_text(format!("{} · 轻点打招呼 · 可在设置中换伙伴",kind.label()))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,true,"和轻屿的小伙伴打招呼"));
        if response.has_focus(){c.stroke(x-14.0,y-14.0,28.0,28.0,9.0,1.2,theme::CYAN);}
        if response.clicked(){c.ui.ctx().data_mut(|data|data.insert_temp(key,t));}
    }
    let greeting=c.ui.ctx().data(|data|data.get_temp::<f64>(key)).map(|at|t-at).filter(|age|*age>=0.0&&*age<1.5);
    let block=(t.max(0.0)/8.5).floor() as u64;
    let local=(t.max(0.0)%8.5) as f32;
    let action=anim::smoothstep(0.0,0.35,local)*(1.0-anim::smoothstep(1.1,1.8,local));
    let hello=greeting.map(|age|age as f32).or_else(||(block%3==1&&local<1.5).then_some(local));
    let nod=if reduced{0.0}else{hello.map_or(0.0,|age|(age*8.0).sin()*anim::smoothstep(0.0,0.15,age)*(1.0-anim::smoothstep(0.8,1.5,age)))};
    let breath=if reduced{0.0}else{(t*1.65).sin() as f32};
    let tilt=if reduced{0.0}else{action*(anim::hash01(block+41)-0.5)*0.20+nod*0.075};
    let center=Pos2::new(x,y+breath*0.28+nod*0.8);
    if kind!=PetKind::Tan{
        let texture=head(c.ui.ctx(),kind);let ratio=texture.size()[0] as f32/texture.size()[1] as f32;
        let dimensions=if ratio>=1.0{Vec2::new(size,size/ratio)}else{Vec2::new(size*ratio,size)};
        image(c,&texture,center,dimensions,center,tilt);return;
    }
    let heads=frames(c.ui.ctx(),"island-pet-expressions",include_bytes!("../assets/qingyu/companion/pet-expressions.png"),2,2);
    let blink_cycle=(t.max(0.0)/4.8).floor() as u64;
    let blink_phase=(t.max(0.0)%4.8) as f32-2.4-anim::hash01(blink_cycle+9)*0.7;
    let blink=if reduced{0.0}else{anim::smoothstep(0.0,0.065,blink_phase)*(1.0-anim::smoothstep(0.13,0.25,blink_phase))};
    let greeting_alpha=if reduced{0.0}else{greeting.map_or(0.0,|age|anim::smoothstep(0.0,0.12,age as f32)*(1.0-anim::smoothstep(1.0,1.5,age as f32)))};
    let expression=if reduced{0.0}else{action.max(greeting_alpha)};
    let pose=if greeting_alpha>0.0||block%2==0{2}else{3};
    let head_size=Vec2::new(size,size*heads[0].size()[1] as f32/heads[0].size()[0] as f32*(1.0+breath*0.012));
    image(c,&heads[0],center,head_size,center,tilt);
    if expression>0.002{image(&c.faded(expression),&heads[pose],center,head_size,center,tilt);}
    if blink>0.002{image(&c.faded(blink*(1.0-expression)),&heads[1],center,head_size,center,tilt);}
    // FrameOut owns cadence, so hiding the native window also stops this loop.
}

fn agent_head(c:&Card,kind:PetKind,x:f32,y:f32,t:f64,index:usize,running:bool,waiting:bool,reduced:bool){
    let moving=running&&!reduced;
    let phase=(t.rem_euclid(10_000.0)*std::f64::consts::TAU*1.35+index as f64*1.4) as f32;
    let float_phase=(t.rem_euclid(10_000.0)*std::f64::consts::TAU*0.58+index as f64*0.8) as f32;
    let (hop,tilt,squash)=if reduced{(0.0,0.0,0.0)}else if moving{
        (((phase.sin()+1.0)*0.5).powi(2)*2.7,phase.sin()*0.13,phase.cos()*0.035)
    }else{(0.65*float_phase.sin(),float_phase.sin()*0.055,float_phase.cos()*0.012)};
    let c=c.faded(if running||waiting{1.0}else{0.48});
    let texture=head(c.ui.ctx(),kind);let base=16.5;
    let ratio=texture.size()[0] as f32/texture.size()[1] as f32;
    let size=if ratio>=1.0{Vec2::new(base*(1.0+squash),base/ratio*(1.0-squash))}
        else{Vec2::new(base*ratio*(1.0+squash),base*(1.0-squash))};
    let center=Pos2::new(x,y-hop);image(&c,&texture,center,size,center,tilt);
    if waiting{let twinkle=if reduced{0.5}else{(float_phase.cos()+1.0)*0.5};c.circle(x+6.3,y-hop-6.0,1.5+twinkle*0.6,theme::YELLOW);}
}

impl App {
    pub(crate) fn draw_resident_music(&self,c:&Card,x:f32,y:f32,width:f32){
        let left=x-width/2.0;
        crate::paint::playback_bars(c,left+7.0,y,self.t,self.music_playing()&&!self.reduced_motion,theme::WHITE);
        let text_x=left+22.0;let text_width=(width-24.0).max(0.0);
        let title=c.fit(&self.media.title,11.0,theme::W::Semibold,text_width);
        if width>=148.0&&!self.media.artist.is_empty(){
            c.centered_text(text_x,y-5.5,&title,11.0,theme::W::Semibold,theme::WHITE,crate::paint::H::Left);
            c.centered_text(text_x,y+7.0,&c.fit(&self.media.artist,10.0,theme::W::Regular,text_width),10.0,theme::W::Regular,theme::GRAY,crate::paint::H::Left);
        }else{c.centered_text(text_x,y,&title,11.0,theme::W::Semibold,theme::WHITE,crate::paint::H::Left);}
    }
    pub(crate) fn resident_team(&self)->Team{
        if self.notification_locked||!self.module_on(AGENT_PAGE){Team::default()}else{Team::from_snapshot(&self.agent_snapshot)}
    }
    pub(crate) fn draw_resident_pet(&self,c:&Card,x:f32,y:f32,size:f32){pet(c,self.settings.resident_pet_kind(),x,y,size,self.t,self.reduced_motion);}
    pub(crate) fn draw_resident_team(&mut self,c:&Card,x:f32,y:f32,vertical:bool,width:f32){
        let team=self.resident_team();
        let (width,height)=if vertical{(44.0,46.0)}else{(width.max(70.0),28.0)};
        let hit=c.live.then(||{
            let hit=c.hit(x-width/2.0,y-height/2.0,width,height,"island-codex-team").on_hover_text(team.label())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            hit.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,true,format!("{}，查看 Agent",team.label())));
            if hit.has_focus(){c.stroke(x-width/2.0,y-height/2.0,width,height,8.0,1.2,theme::CYAN);}
            hit
        });
        let group_x=if vertical{x}else{x-width/2.0+35.0};
        let (icon_x,icon_y,start_x,avatar_y)=if vertical{(x,y-10.0,x-13.0,y+11.0)}else{(group_x-23.0,y,group_x-1.0,y)};
        let figures=team.figures(self.t,self.reduced_motion);
        crate::agent_icons::paint(c,figures[0].unwrap().agent,icon_x,icon_y,21.0);
        let total=team.running+team.waiting;
        for (index,figure) in figures.into_iter().enumerate(){if let Some(figure)=figure{
            agent_head(c,self.settings.agent_pet(figure.agent),start_x+index as f32*13.0,avatar_y,self.t,index,figure.running,figure.waiting,self.reduced_motion);
        }
        }
        if total>3{
            let hidden=total-1;
            let count=if hidden>99{"99+".into()}else{format!("+{hidden}")};
            let (badge_x,badge_y,badge_w,badge_h)=if vertical{(x+13.0,y+17.0,18.0,12.0)}else{(group_x+18.0,y,21.0,14.0)};
            c.fill(badge_x-badge_w/2.0,badge_y-badge_h/2.0,badge_w,badge_h,badge_h/2.0,theme::BTN_HOVER);
            c.centered_text(badge_x,badge_y,&count,if vertical{8.5}else{10.0},theme::W::Semibold,if team.waiting>0{theme::YELLOW}else{theme::WHITE},crate::paint::H::Center);
        }
        if !vertical&&width>=120.0{
            let status=if team.waiting>0{format!("等待 {}",team.waiting)}else if team.running>0{format!("执行 {}",team.running)}else{"空闲".into()};
            c.centered_text(group_x+38.0,y,&c.fit(&status,10.5,theme::W::Semibold,width-74.0),10.5,theme::W::Semibold,if team.waiting>0{theme::YELLOW}else{theme::GRAY},crate::paint::H::Left);
        }
        if hit.is_some_and(|hit|hit.clicked()){self.show_page(AGENT_PAGE);}
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::agent_monitor::Session;
    #[test]
    fn agent_heads_stay_valid_at_cycle_boundaries_and_after_long_uptime(){
        let ctx=egui::Context::default();
        for t in [0.8-1e-9,86400.8-1e-9,1_000_000.8-1e-9]{
            let output=ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::splat(80.0))),time:Some(t),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                    let c=Card::new(ui,Pos2::ZERO,ui.clip_rect());
                    for (index,kind) in PetKind::ALL.into_iter().enumerate(){agent_head(&c,kind,10.0+index as f32*10.0,40.0,t,index,true,false,false);}
                });
            });
            let meshes=output.shapes.iter().filter_map(|shape|if let egui::Shape::Mesh(mesh)=&shape.shape{Some(mesh)}else{None}).collect::<Vec<_>>();
            assert!(meshes.len()>=3);
            assert!(meshes.iter().all(|mesh|mesh.is_valid()&&mesh.calc_bounds().is_finite()));
        }
    }
    #[test]
    fn agent_head_only_motion_bobs_in_place_and_reduced_motion_is_static(){
        let ctx=egui::Context::default();
        let render=|kind,time,running,waiting,reduced|ctx.run(egui::RawInput{
            screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(60.0,44.0))),time:Some(time),..Default::default()
        },|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
            let c=Card::new(ui,Pos2::ZERO,ui.clip_rect());agent_head(&c,kind,30.0,24.0,time,2,running,waiting,reduced);
        });}).shapes;
        let bounds=|shapes:&Vec<egui::epaint::ClippedShape>|{
            let meshes=shapes.iter().filter_map(|shape|if let egui::Shape::Mesh(mesh)=&shape.shape{Some(mesh)}else{None}).collect::<Vec<_>>();
            assert_eq!(meshes.len(),1,"只绘制一个透明宠物头像；没有身体、手脚或底座");
            assert!(meshes[0].is_valid());meshes[0].calc_bounds()
        };
        for kind in PetKind::ALL{
            let first=render(kind,0.0,true,false,false);let next=render(kind,0.3,true,false,false);
            let a=bounds(&first);let b=bounds(&next);
            assert!((a.center()-b.center()).length()>0.5,"running {kind:?} bounces and tilts");
            let waiting_a=render(kind,0.0,false,true,false);let waiting_b=render(kind,0.35,false,true,false);
            assert!((bounds(&waiting_a).center()-bounds(&waiting_b).center()).length()>0.15,"waiting {kind:?} softly floats");
            let still_a=render(kind,0.0,true,false,true);let still_b=render(kind,19.0,true,false,true);
            assert_eq!(bounds(&still_a),bounds(&still_b),"reduced motion fixes the pet head in place");
            let waiting_still_a=render(kind,0.0,false,true,true);let waiting_still_b=render(kind,19.0,false,true,true);
            assert_eq!(format!("{waiting_still_a:?}"),format!("{waiting_still_b:?}"),"reduced motion keeps the attention dot static, too");
        }
    }
    #[test]
    fn team_runs_only_for_observed_agent_turns_and_keeps_attention_separate(){
        let mut snapshot=Snapshot::default();
        for (agent,status,process_only,turn_key) in [
            (AgentKind::Codex,Status::Working,false,1),(AgentKind::Codex,Status::Thinking,false,2),
            (AgentKind::Codex,Status::Approval,false,3),(AgentKind::Codex,Status::Waiting,false,4),
            (AgentKind::Codex,Status::Running,true,0),(AgentKind::Codex,Status::Working,true,5),
            (AgentKind::Codex,Status::Working,false,0),(AgentKind::Codex,Status::Completed,false,6),
            (AgentKind::Claude,Status::Working,false,7),
        ]{snapshot.sessions.push(Session{agent,status,process_only,turn_key,..Default::default()});}
        let team=Team::from_snapshot(&snapshot);
        assert_eq!((team.running,team.waiting),(3,2));
        assert_eq!(team.counts[agent_index(AgentKind::Codex)],[2,2]);
        assert_eq!(team.counts[agent_index(AgentKind::Claude)],[1,0]);
        assert_eq!(team.figures(0.0,false)[0].unwrap(),Companion{agent:AgentKind::Codex,running:false,waiting:true});
        assert_eq!(team.figures(4.1,false)[0].unwrap(),Companion{agent:AgentKind::Claude,running:true,waiting:false});
        assert_eq!(team.figures(0.0,true),team.figures(123.0,true));
        snapshot.sessions.iter_mut().for_each(|session|session.status=Status::Stopped);
        assert_eq!(Team::from_snapshot(&snapshot),Team::default());
    }
    #[test]
    fn small_mixed_teams_keep_the_correct_agent_for_each_real_figure(){
        let mut snapshot=Snapshot::default();
        for (agent,status) in [(AgentKind::Gemini,Status::Working),(AgentKind::Claude,Status::Approval),(AgentKind::Codex,Status::Thinking)]{
            snapshot.sessions.push(Session{agent,status,turn_key:1,..Default::default()});
        }
        let team=Team::from_snapshot(&snapshot);let figures=team.figures(0.0,false);
        assert_eq!(figures,[Some(Companion{agent:AgentKind::Codex,running:true,waiting:false}),Some(Companion{agent:AgentKind::Claude,running:false,waiting:true}),Some(Companion{agent:AgentKind::Gemini,running:true,waiting:false})]);
        snapshot.sessions.reverse();assert_eq!(team,Team::from_snapshot(&snapshot),"refresh ordering must not shuffle the pets");
        assert!(team.label().contains("Claude"));assert!(team.label().contains("Gemini"));
    }

    #[test]
    #[ignore="Export actual pet-head poses for visual review"]
    fn pet_crew_gallery(){
        let path=std::path::PathBuf::from(std::env::var_os("QINGYU_PET_CREW_REVIEW").expect("review output folder"));std::fs::create_dir_all(&path).unwrap();
        let ctx=egui::Context::default();crate::theme::load_fonts(&ctx);ctx.set_pixels_per_point(2.0);
        let mut raster=crate::preview_raster::Raster::new(1260,420);
        for frame in 0..12{
            let t=frame as f64/(12.0*1.55);
            let output=ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,Vec2::new(630.0,210.0))),time:Some(t),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{
                    let c=Card::new(ui,Pos2::ZERO,ui.clip_rect());
                    for (i,kind) in PetKind::ALL.into_iter().enumerate(){
                        let x=45.0+i as f32*90.0;
                        pet_icon(&c,kind,x,35.0,44.0);
                        agent_head(&c.scaled(x,108.0,3.2),kind,x,108.0,t,i,true,false,false);
                        agent_head(&c,kind,x,169.0,t,i,true,false,false);
                        c.centered_text(x,196.0,kind.label(),12.0,theme::W::Semibold,theme::WHITE,crate::paint::H::Center);
                    }
                });
            });
            raster.update_textures(&output.textures_delta);raster.buf.fill([0.08,0.12,0.14,1.0]);
            raster.draw(&ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
            let mut rgba=Vec::new();raster.to_rgba8(&mut rgba);
            image::RgbaImage::from_raw(1260,420,rgba).unwrap().save(path.join(format!("crew-{frame:02}.png"))).unwrap();
            raster.free_textures(&output.textures_delta);
        }
    }
}
