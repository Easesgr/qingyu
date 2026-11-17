use super::*;
use crate::app::{MUSIC_PAGE,SHELF_PAGE,CLIPBOARD_PAGE,TODO_PAGE,MAIL_PAGE,WEATHER_PAGE};
use crate::model::Dock;
use crate::side_dock::{SIDE_W,SIDE_H,PANEL_W,PANEL_H};

#[path="island_notice_tests.rs"]
mod island_notice_tests;
#[path="agent_approval_tests.rs"]
mod agent_approval_tests;

struct Side{h:H,out:Option<crate::app::FrameOut>}

fn seed_agents(h:&mut H,count:usize){
    use crate::agent_monitor::{AgentKind,Session,Snapshot,Status};
    let kinds=[AgentKind::Codex,AgentKind::Claude,AgentKind::Hermes];
    let statuses=[Status::Working,Status::Waiting,Status::Thinking];
    h.app.agent_snapshot=Snapshot{sessions:(0..count).map(|i|Session{
        id:format!("agent-fixture-{i}"),agent:kinds[i%3],status:statuses[i%3],title:["监测集成","登录修复","文档整理"][i%3].into(),
        project:format!("/Users/demo/Code/agent-project-{i}"),detail:if i%3==0{"exec_command"}else if i%3==1{"等待确认工具调用"}else{"正在处理任务"}.into(),
        pid:Some(8000+i as u32),updated_at:1_794_700_000,started_at:1_794_699_868,..Default::default()
    }).collect(),..Default::default()};
}

#[test]
fn agent_monitor_multiple_agents_show_brand_icons_instead_of_letter_labels(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::agent_monitor::{AgentKind,Status};
    let preview=std::env::var_os("QINGYU_AGENT_ICONS_PREVIEW_DIR").map(std::path::PathBuf::from);
    if let Some(path)=&preview{std::fs::create_dir_all(path).unwrap();}
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::fresh(dock,crate::app::AGENT_PAGE);seed_agents(&mut s.h,3);
        for session in &mut s.h.app.agent_snapshot.sessions{session.status=Status::Working;}
        let size=s.h.app.canvas_size();s.h.ctx.set_pixels_per_point(2.0);
        let mut raster=preview.as_ref().map(|_|crate::preview_raster::Raster::new((size.x*2.0)as usize,(size.y*2.0)as usize));
        for frame in 0..70{
            let output=s.frame(vec![]);
            if let Some(raster)=&mut raster{
                raster.update_textures(&output.textures_delta);
                if frame==69{raster.buf.fill([0.13,0.18,0.20,1.0]);raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=vec![];raster.to_rgba8(&mut rgba);image::save_buffer(preview.as_ref().unwrap().join(format!("agent-icons-{}.png",dock.key())),&rgba,(size.x*2.0)as u32,(size.y*2.0)as u32,image::ColorType::Rgba8).unwrap();}
                raster.free_textures(&output.textures_delta);
            }
        }
        for kind in [AgentKind::Codex,AgentKind::Claude,AgentKind::Hermes]{
            let id=crate::agent_icons::texture(&s.h.ctx,kind).unwrap().id();
            assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Mesh(mesh)if mesh.texture_id==id)),"{dock:?} must show {kind:?}'s own brand icon");
        }
        assert!(!s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if ["Cx","Cl","H"].contains(&text.galley.text()))));
    }
}

#[test]
fn agent_monitor_settings_show_each_tools_actual_integration_status(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::agent_monitor::{AgentKind,Integration};
    let mut h=H::new();
    h.app.agent_snapshot.integrations=vec![
        Integration{agent:AgentKind::Codex,detected:true,installed:false,ready:true,message:"已收到真实任务事件".into()},
        Integration{agent:AgentKind::Claude,detected:true,installed:true,ready:false,message:"等待启动 Claude Code 任务".into()},
        Integration{agent:AgentKind::Hermes,detected:true,installed:false,ready:false,message:"安装插件后接收 Hermes 执行状态".into()},
    ];
    let out=h.ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(650.0,1300.0))),..Default::default()},|ctx|{
        egui::CentralPanel::default().show(ctx,|ui|h.app.agent_settings(ui));
    });
    let texts:Vec<_>=out.shapes.iter().filter_map(|shape|match &shape.shape{egui::Shape::Text(value)=>Some(value.galley.text()),_=>None}).collect();
    for expected in ["Codex","Claude","Hermes","已收到任务状态","集成已安装","待接入","安装状态集成"]{assert!(texts.contains(&expected),"missing {expected}");}
    for kind in [AgentKind::Codex,AgentKind::Claude,AgentKind::Hermes]{
        let id=crate::agent_icons::texture(&h.ctx,kind).unwrap().id();
        assert!(out.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Mesh(mesh)if mesh.texture_id==id)
            ||matches!(&shape.shape,egui::Shape::Rect(rect)if rect.fill_texture_id()==id)));
    }
}

#[test]
fn agent_monitor_panels_show_live_states_scroll_and_open_settings(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let preview=std::env::var_os("QINGYU_AGENT_PREVIEW_DIR").map(std::path::PathBuf::from);
    if let Some(folder)=&preview{std::fs::create_dir_all(folder).unwrap();}
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::fresh(dock,crate::app::AGENT_PAGE);seed_agents(&mut s.h,12);
        let size=s.h.app.canvas_size();s.h.ctx.set_pixels_per_point(2.0);
        let mut raster=preview.as_ref().map(|_|crate::preview_raster::Raster::new((size.x*2.0) as usize,(size.y*2.0) as usize));
        for frame in 0..70{
            let output=s.frame(vec![]);
            if let Some(raster)=&mut raster{
                raster.update_textures(&output.textures_delta);
                if frame==69{
                    raster.buf.fill([0.13,0.18,0.20,1.0]);
                    raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                    let mut rgba=vec![];raster.to_rgba8(&mut rgba);
                    image::save_buffer(preview.as_ref().unwrap().join(format!("agents-{}.png",dock.key())),&rgba,(size.x*2.0) as u32,(size.y*2.0) as u32,image::ColorType::Rgba8).unwrap();
                }
                raster.free_textures(&output.textures_delta);
            }
        }
        let row_id=egui::Id::new(("agent-session",true,"agent-fixture-0"));
        let before=s.h.ctx.read_response(row_id).unwrap().rect.top();
        assert!((s.h.ctx.read_response(row_id).unwrap().rect.height()-44.0).abs()<1.0,"default row must stay on one line at 2x scale");
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="监测集成")),"show the conversation title instead of the project/tool summary");
        let body=s.h.app.island_rect(Rect::from_min_size(Pos2::ZERO,size),s.h.app.expanded_width(),s.h.app.expanded_height());
        let settings=if dock==Dock::Top{"page-settings"}else{"portrait-settings"};
        for id in ["agents-refresh",settings]{
            let response=s.h.ctx.read_response(egui::Id::new(("card",true,id))).unwrap();
            assert!(body.expand(1.0).contains_rect(response.rect),"{dock:?} {id} must stay inside the panel");
            assert!(response.rect.height()>=28.0);
        }
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="等待")));
        s.wheel(180.0,if dock==Dock::Top{130.0}else{260.0},-220.0);s.settle();
        assert_eq!(s.h.app.page(),crate::app::AGENT_PAGE,"list scrolling must retain the Agent page");
        assert!(s.h.ctx.read_response(row_id).unwrap().rect.top()<before,"{dock:?} list must scroll");
        s.click(settings);assert!(s.h.app.settings_open);assert_eq!(s.h.app.settings_tab,13);
    }
}

#[test]
fn agent_monitor_elapsed_time_ticks_freezes_and_details_collapse_to_one_line(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,crate::app::AGENT_PAGE);seed_agents(&mut s.h,1);
        s.h.cursor=Some(s.point(100.0,if dock==Dock::Top{120.0}else{200.0}));
        s.h.t=10.0;s.frame(vec![]);
        let has=|s:&Side,expected:&str|s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()==expected));
        assert!(has(&s,"02:22"));
        s.h.t+=60.0;s.frame(vec![]);assert!(has(&s,"03:22"),"elapsed time must advance with the supplied clock");
        let id=egui::Id::new(("agent-session",true,"agent-fixture-0"));
        let compact=s.h.ctx.read_response(id).unwrap().rect;
        s.click_at(compact.center());for _ in 0..3{s.frame(vec![]);}
        assert_eq!(s.h.app.agent_expanded.as_deref(),Some("agent-fixture-0"),"{dock:?}: row={compact:?}, hit={:?}, open={}, progress={}",s.h.hit,s.h.app.open,s.h.app.progress());
        let expanded=s.h.ctx.read_response(id).unwrap().rect;assert!(expanded.height()>compact.height()+40.0,"{dock:?}: compact={compact:?}, expanded={expanded:?}, selected={:?}",s.h.app.agent_expanded);
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text().contains("exec_command"))),"expanded row must retain tool details");
        s.click_at(egui::pos2(expanded.center().x,expanded.top()+22.0));for _ in 0..3{s.frame(vec![]);}
        assert!(s.h.app.agent_expanded.is_none());assert!((s.h.ctx.read_response(id).unwrap().rect.height()-compact.height()).abs()<0.1);
        s.h.app.agent_snapshot.sessions[0].status=crate::agent_monitor::Status::Completed;
        s.h.app.agent_snapshot.sessions[0].finished_at=Some(1_794_700_013);
        s.frame(vec![]);assert!(has(&s,"02:25"));
        s.h.t+=1.0;s.frame(vec![]);assert!(has(&s,"02:25"),"completed turns must preserve their final duration");
        s.h.t+=60.0;s.frame(vec![]);assert!(!has(&s,"02:25"),"completed turns must disappear after one minute");
        s.h.app.agent_snapshot.sessions[0].status=crate::agent_monitor::Status::Working;
        s.h.app.agent_snapshot.sessions[0].finished_at=None;s.h.app.agent_snapshot.sessions[0].turn_key+=1;
        s.h.app.agent_snapshot.sessions[0].title.clear();s.h.app.agent_snapshot.sessions[0].project="Window".into();
        s.frame(vec![]);assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().starts_with("对话 fixture0"))));assert!(!has(&s,"Window"),"a project name must never substitute for the conversation title");
    }
}

#[test]
fn agent_monitor_replaces_process_rows_with_named_conversations_and_refreshes_renames(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::agent_monitor::{AgentKind,Session,Status};
    let mut s=Side::new(Dock::Right,crate::app::AGENT_PAGE);
    let has=|s:&Side,text:&str|s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(value) if value.galley.text()==text));
    s.h.app.agent_snapshot.sessions=vec![Session{id:"process:codex:42".into(),agent:AgentKind::Codex,status:Status::Idle,pid:Some(42),process_only:true,..Default::default()}];
    s.frame(vec![]);assert!(has(&s,"暂无运行任务"));assert!(!has(&s,"Codex 进程")&&!has(&s,"运行中的应用"));
    seed_agents(&mut s.h,2);
    for session in &mut s.h.app.agent_snapshot.sessions{session.agent=AgentKind::Codex;session.pid=Some(42);session.project="Window".into();}
    s.h.app.agent_snapshot.sessions[0].title="已命名对话".into();
    s.h.app.agent_snapshot.sessions[1].title="另一个对话".into();
    s.frame(vec![]);
    assert!(has(&s,"已命名对话")&&has(&s,"另一个对话"));assert!(has(&s,"1 个执行中 · 1 个等待处理"));
    assert!(!has(&s,"Codex 进程")&&!has(&s,"Window"));
    let started=s.h.app.agent_snapshot.sessions[0].started_at;
    s.h.app.agent_snapshot.sessions[0].title="运行时改名".into();s.frame(vec![]);
    assert!(has(&s,"运行时改名")&&!has(&s,"已命名对话"));assert!(has(&s,"另一个对话"));
    assert_eq!(s.h.app.agent_snapshot.sessions[0].started_at,started);
}

#[test]
fn agent_monitor_completed_tasks_leave_after_one_minute_without_returning_on_refresh(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::agent_monitor::{Session,Status};
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,crate::app::AGENT_PAGE);seed_agents(&mut s.h,1);
        s.h.cursor=Some(s.point(100.0,if dock==Dock::Top{120.0}else{220.0}));
        s.h.app.agent_snapshot.sessions.extend([
            Session{id:"idle-dialog".into(),title:"闲置对话".into(),status:Status::Idle,..Default::default()},
            Session{id:"old-completed".into(),title:"旧任务记录".into(),status:Status::Completed,finished_at:Some(1),..Default::default()},
            Session{id:"process".into(),process_only:true,status:Status::Running,..Default::default()},
        ]);
        s.frame(vec![]);
        let has=|s:&Side,text:&str|s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(value) if value.galley.text()==text));
        assert!(has(&s,"监测集成"));assert!(!has(&s,"闲置对话")&&!has(&s,"旧任务记录")&&!has(&s,"Codex 进程"));
        s.h.t=20.0;
        s.h.app.agent_snapshot.sessions[0].status=Status::Completed;
        s.h.app.agent_snapshot.sessions[0].finished_at=Some(1_794_700_020);
        s.frame(vec![]);let completed_at=s.h.t;assert!(has(&s,"完成"));
        s.h.t=completed_at+59.0;s.frame(vec![]);assert!(has(&s,"监测集成"));
        s.h.t=completed_at+60.1;s.frame(vec![]);assert!(has(&s,"暂无运行任务"));assert!(!has(&s,"监测集成"));
        s.h.app.agents.refresh();s.frame(vec![]);assert!(!has(&s,"监测集成"));
        assert!(s.h.app.history.iter().all(|alert|!matches!(alert.source,crate::extras::AlertSource::Agent{..})));
    }
}

#[test]
fn agent_monitor_compact_rows_keep_title_duration_and_status_in_separate_columns(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for (dock,width) in [(Dock::Top,292),(Dock::Left,292),(Dock::Right,292),(Dock::Right,420),(Dock::Left,520)]{
        let mut s=Side::fresh(dock,crate::app::AGENT_PAGE);s.h.app.settings.side_panel_width=width;
        seed_agents(&mut s.h,1);
        let session=&mut s.h.app.agent_snapshot.sessions[0];
        session.title="持续处理中的长对话名称与清晰的任务布局".into();session.status=crate::agent_monitor::Status::Waiting;
        session.started_at=1_794_600_000;
        s.settle();
        let row=s.h.ctx.read_response(egui::Id::new(("agent-session",true,"agent-fixture-0"))).unwrap().rect;
        let mut text:Vec<_>=s.h.shapes.iter().filter_map(|shape|match &shape.shape{
            egui::Shape::Text(value) if row.contains(value.visual_bounding_rect().center())=>Some((value.galley.text(),value.visual_bounding_rect())),_=>None,
        }).collect();
        text.sort_by(|a,b|a.1.left().total_cmp(&b.1.left()));
        assert_eq!(text.len(),3,"title, elapsed time and status each have one column beside the agent icon");
        for pair in text.windows(2){assert!(pair[0].1.right()+2.0<=pair[1].1.left(),"{dock:?}/{width}: overlap {} and {}",pair[0].0,pair[1].0);}
        assert!(text.iter().all(|(_,rect)|row.expand(1.0).contains_rect(*rect)));
        assert_eq!(text.last().unwrap().0,"等待");
    }
}

#[test]
fn agent_monitor_empty_errors_and_hidden_entry_are_actionable(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut s=Side::new(Dock::Right,crate::app::AGENT_PAGE);
    assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="暂无运行任务")));
    s.h.app.agent_snapshot.error=Some("无法读取会话目录".into());s.frame(vec![]);
    assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="暂时无法读取状态")));
    s.h.app.set_page_visible(crate::app::AGENT_PAGE,false);s.h.app.show_page(crate::app::AGENT_PAGE);
    assert_ne!(s.h.app.page(),crate::app::AGENT_PAGE);assert!(!s.h.app.settings_open);assert_eq!(s.h.app.settings_tab,0);
    assert_eq!(crate::extras::settings_tab_module(13),Some(crate::app::AGENT_PAGE));
}
impl Side{
    fn new(dock:Dock,page:usize)->Self{
        let mut s=Self::fresh(dock,page);s.settle();s
    }
    fn fresh(dock:Dock,page:usize)->Self{
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.macos_side_handle=true;h.app.settings.island_shape=crate::model::IslandShape::Pill;
        h.app.show_lyrics=false;h.app.settings.lyrics_enabled=false;h.app.settings.clipboard_history=false;
        h.media=crate::smtc::NowPlaying{title:"晴天".into(),artist:"周杰伦".into(),source:"cloudmusic.exe".into(),duration:240.0,position:60.0,playing:true,volume:Some((65,false)),..Default::default()};
        h.app.show_page(page);
        Self{h,out:None}
    }
    fn frame(&mut self,events:Vec<egui::Event>)->egui::FullOutput{
        self.frame_dt(events,1.0/60.0)
    }
    fn frame_dt(&mut self,events:Vec<egui::Event>,dt:f64)->egui::FullOutput{
        self.h.t+=dt;
        let size=self.h.app.canvas_size();let win=Rect::from_min_size(Pos2::ZERO,size);
        let mut raw=egui::RawInput{screen_rect:Some(win),time:Some(self.h.t),events,focused:true,..Default::default()};
        self.h.app.filter_todo_input(&mut raw);
        let env=Env{t:self.h.t,unix_time:1_794_700_000+self.h.t as u64,clock:"09:41:00".into(),today:"2026-11-14".into(),date:"11月14日 周六".into(),cursor:self.h.cursor,media:self.h.media.clone(),..Default::default()};
        let mut out=None;
        let output=self.h.ctx.run(raw,|ctx|{egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx,|ui|{out=Some(self.h.app.frame(ui,win,&env));});});
        let mut frame=out.unwrap();self.h.cmds.append(&mut frame.cmds);self.h.hit=frame.hit;self.h.region=frame.region;self.h.shapes=output.shapes.clone();self.out=Some(frame);output
    }
    fn settle(&mut self){for _ in 0..65{self.frame(vec![]);}}
    fn point(&self,x:f32,y:f32)->Pos2{self.h.app.content_origin(Rect::from_min_size(Pos2::ZERO,self.h.app.canvas_size()))+egui::vec2(x,y)}
    fn click_at(&mut self,point:Pos2){
        self.h.cursor=Some(point);self.frame(vec![egui::Event::PointerMoved(point)]);
        for pressed in [true,false]{self.frame(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    }
    #[track_caller]
    fn click(&mut self,id:impl std::hash::Hash){
        let response=self.h.ctx.read_response(egui::Id::new(("card",true,id))).expect("portrait control must be present");self.click_at(response.rect.center());
    }
    fn wheel(&mut self,x:f32,y:f32,delta:f32){let point=self.point(x,y);self.h.cursor=Some(point);self.frame(vec![egui::Event::PointerMoved(point),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,delta),modifiers:Default::default()}]);}
    fn key(&mut self,key:egui::Key){self.frame(vec![egui::Event::Key{key,physical_key:None,pressed:true,repeat:false,modifiers:Default::default()}]);self.frame(vec![egui::Event::Key{key,physical_key:None,pressed:false,repeat:false,modifiers:Default::default()}]);}
}

#[test]
fn portrait_tabs_have_labels_mirrored_geometry_and_transparent_desktop(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut hits=Vec::new();
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);
        for page in [SHELF_PAGE,CLIPBOARD_PAGE,TODO_PAGE,MAIL_PAGE,WEATHER_PAGE,crate::app::AGENT_PAGE,MUSIC_PAGE]{
            let id=egui::Id::new(("card",true,("portrait-tab",page)));s.h.ctx.memory_mut(|memory|memory.request_focus(id));s.settle();
            let tab=s.h.ctx.read_response(id).unwrap();assert!(tab.rect.width()>=28.0,"{dock:?}/{page}: focused tab must scroll into view, rect={:?}, page={}, focus={:?}",tab.rect,s.h.app.page(),s.h.ctx.memory(|memory|memory.focused()));
            s.click(("portrait-tab",page));s.settle();assert_eq!(s.h.app.page(),page);
            let out=s.out.as_ref().unwrap();assert!(out.hit.height()>out.hit.width());assert!((out.hit.height()-PANEL_H-44.0).abs()<8.0);
            assert!(!out.hit.contains(egui::pos2(SIDE_W/2.0,SIDE_H-5.0)),"transparent desktop below the body must pass clicks");
            let body=s.h.app.island_rect(Rect::from_min_size(Pos2::ZERO,egui::vec2(SIDE_W,SIDE_H)),PANEL_W,PANEL_H);
            assert!(out.file_drop_zone.unwrap().contains_rect(body));
            for shape in &s.h.shapes{if let egui::Shape::Text(text)=&shape.shape{let visible=text.visual_bounding_rect().intersect(shape.clip_rect);if visible.is_positive(){assert!(visible.left()>=body.left()-1.0&&visible.right()<=body.right()+1.0,"visible portrait text leaves narrow body: {}",text.galley.text());}}}
        }
        hits.push(s.h.hit);
        s.h.app.settings.skin=crate::model::Skin::GlassDark;s.frame(vec![]);
        let shapes=s.out.as_ref().unwrap().glass.as_ref().unwrap();
        let rect=match &shapes[0]{crate::app::GlassShape::Docked{rect,..}=>*rect,_=>panic!("side should use continuous docked native mask")};
        assert!((rect.width()-PANEL_W).abs()<1.0&&(rect.height()-PANEL_H).abs()<1.0);
        assert_eq!(shapes[0].outline(),s.out.as_ref().unwrap().hit_outline.as_ref().unwrap().clone(),"input and blur must share the rendered silhouette");
    }
    assert!((hits[0].left()-(SIDE_W-hits[1].right())).abs()<0.01);assert!((hits[0].right()-(SIDE_W-hits[1].left())).abs()<0.01);
}

#[test]
fn portrait_music_uses_existing_transport_seek_and_source_bound_volume(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);s.h.cmds.clear();
        s.click("portrait-play");s.click("portrait-previous");s.click("portrait-next");
        for command in [crate::smtc::Transport::Toggle,crate::smtc::Transport::Prev,crate::smtc::Transport::Next]{assert!(s.h.cmds.contains(&crate::app::Cmd::Media(command)));}
        s.click_at(s.point(146.0,330.0));
        assert!(s.h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::Media(crate::smtc::Transport::Seek(value))if (*value-120.0).abs()<1.0)));
        s.click_at(s.point(180.0,435.0));
        assert!(s.h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetVolume(75),source)if source=="cloudmusic.exe")));
        s.click("portrait-mute");assert!(s.h.cmds.iter().any(|c|matches!(c,crate::app::Cmd::MusicVolume(crate::smtc::Transport::SetMute(true),_))));
        s.click("portrait-lyrics");assert!(s.h.app.show_lyrics);
    }
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_portrait_entries_work_before_playback_and_with_another_player_active(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);
        for empty in [true,false]{
            s.h.media=if empty{Default::default()}else{crate::smtc::NowPlaying{title:"正在听的歌".into(),artist:"歌手".into(),source:"com.apple.Music".into(),..Default::default()}};
            s.frame(vec![]);
            for (id,page) in [("portrait-qq-library",crate::qq_music::Page::Library),("portrait-qq-player",crate::qq_music::Page::Player)]{
                s.h.app.qq_library_open=false;s.h.app.set_qq_music(Default::default(),None);s.frame(vec![]);
                let response=s.h.ctx.read_response(egui::Id::new(("card",true,id))).expect("QQ Music must be reachable without a track");
                assert!(s.h.hit.contains_rect(response.rect));
                assert!(response.rect.height()>=28.0);
                s.h.cmds.clear();s.click(id);
                let expected=if page==crate::qq_music::Page::Library{crate::app::Cmd::QqMusicRefresh}else{crate::app::Cmd::QqMusic(page)};
                assert_eq!(s.h.cmds,vec![expected.clone()],"opening QQ Music must not control the other player");
                s.h.app.qq_library_open=false;s.h.app.set_qq_music(Default::default(),None);s.frame(vec![]);
                s.h.cmds.clear();s.h.ctx.memory_mut(|memory|memory.request_focus(response.id));
                s.key(egui::Key::Enter);
                assert_eq!(s.h.cmds,vec![expected]);
            }
        }
    }
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_playback_clicks_are_immediate_and_absolute_in_both_side_panels(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);s.h.media.source=crate::qq_music::SOURCE.into();s.frame(vec![]);s.h.cmds.clear();
        let mut requests=Vec::new();
        for desired in [false,true,false]{
            s.click("portrait-play");
            assert_eq!(s.h.app.music_playing(),desired,"the icon must follow the click before the media sample changes");
            match s.h.cmds.pop().unwrap(){crate::app::Cmd::QqMusicSetPlaying{request,playing}=>{assert_eq!(playing,desired);requests.push(request);},command=>panic!("expected source-bound playing target, got {command:?}")}
            assert!(s.h.cmds.is_empty());
        }
        assert!(requests.windows(2).all(|pair|pair[0]<pair[1]));
        s.h.app.complete_qq_playback(requests[0],false);s.frame(vec![]);assert!(!s.h.app.music_playing(),"an older failure cannot reverse the newest intent");
        s.h.app.complete_qq_playback(requests[2],false);s.frame(vec![]);assert!(s.h.app.music_playing(),"a real failure restores the actual player state");
    }
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_island_playlists_play_selected_identity_and_lyrics_follow_current_source(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);
        s.h.app.set_qq_music(crate::qq_music::Library{session:crate::qq_music::SessionStatus::Connected,logged_in:true,playlists:vec![crate::qq_music::Playlist{id:"123456".into(),title:"我的收藏".into()}],..Default::default()},Some(crate::qq_music::NativeLyrics{title:"晴天".into(),lines:vec!["前一句".into(),"正在唱的这一句".into()],current:Some(1)}));
        s.h.media.source=crate::qq_music::SOURCE.into();s.h.app.show_lyrics=true;s.frame(vec![]);
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="正在唱的这一句")));
        s.h.cmds.clear();s.click("portrait-qq-stop");assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicStop]);
        s.click("portrait-qq-library");s.frame(vec![]);assert!(s.h.app.qq_library_open);
        s.h.cmds.clear();s.click(("qq-playlist",&String::from("123456")));
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicPlayPlaylist("123456".into())]);assert!(!s.h.app.qq_library_open);
        s.h.media.source="com.apple.Music".into();s.frame(vec![]);assert!(s.h.app.native_qq_lyrics().is_none(),"another player must never display QQ lyrics");
    }
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_island_recovers_existing_login_and_only_prompts_after_confirmed_logout(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::qq_music::{Library,SessionStatus};
    let has=|s:&Side,value:&str|s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()==value));
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);s.h.media=Default::default();
        for (session,title) in [(SessionStatus::Unknown,"QQ 音乐"),(SessionStatus::Restoring,"正在恢复 QQ 音乐…"),(SessionStatus::Connected,"QQ 音乐已连接"),(SessionStatus::Expired,"登录 QQ 音乐")]{
            s.h.app.set_qq_music(Library{session,..Default::default()},None);s.frame(vec![]);
            assert!(has(&s,title),"{dock:?}: empty player must reflect the account state");
            assert!(!has(&s,"登录后，听你的收藏与歌单"));
        }
        s.h.app.set_qq_music(Library::default(),None);s.h.cmds.clear();
        s.click(if dock==Dock::Top{"m-qq-library"}else{"portrait-qq-library"});s.frame(vec![]);
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicRefresh]);
        assert_eq!(s.h.app.qq_library.session,SessionStatus::Restoring);
        assert!(s.h.app.qq_library.loading);
        assert!(has(&s,"正在恢复 QQ 音乐…"));assert!(!has(&s,"登录 QQ 音乐"));

        s.h.app.set_qq_music(Library{message:"网络暂不可用，请重试".into(),..Default::default()},None);s.frame(vec![]);
        assert!(has(&s,"重试读取"));assert!(!has(&s,"登录 QQ 音乐…"));
        s.h.cmds.clear();s.click("qq-library-retry");s.frame(vec![]);
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicRefresh]);
        assert!(has(&s,"正在恢复 QQ 音乐…"));

        s.h.app.set_qq_music(Library{playlists:vec![crate::qq_music::Playlist{id:"123456".into(),title:"已保存的歌单".into()}],..Default::default()},None);s.frame(vec![]);
        s.h.cmds.clear();s.click(("qq-playlist",&String::from("123456")));s.frame(vec![]);
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicPlayPlaylist("123456".into())]);
        assert!(!s.h.app.qq_library_open,"cached playlists must play without another library refresh");

        s.h.app.qq_library_open=true;s.h.app.set_qq_music(Library{session:SessionStatus::Connected,logged_in:true,..Default::default()},None);s.frame(vec![]);
        assert!(has(&s,"暂未读取到歌单"));assert!(has(&s,"打开我的音乐…"));

        s.h.app.set_qq_music(Library{session:SessionStatus::Expired,..Default::default()},None);s.frame(vec![]);
        assert!(has(&s,"登录 QQ 音乐"));assert!(has(&s,"登录 QQ 音乐…"));
        s.h.cmds.clear();s.click("qq-library-login");
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusic(crate::qq_music::Page::Library)]);
        s.h.cmds.clear();s.click("qq-library-refresh");s.frame(vec![]);
        assert_eq!(s.h.app.qq_library.session,SessionStatus::Restoring);
        assert!(has(&s,"正在恢复 QQ 音乐…"));assert!(!has(&s,"登录 QQ 音乐"));
    }
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_cached_lists_open_immediately_and_refresh_only_when_requested(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);
        s.h.app.set_qq_music(crate::qq_music::Library{playlists:vec![crate::qq_music::Playlist{id:"123456".into(),title:"已保存的歌单".into()}],..Default::default()},None);
        for _ in 0..2{
            s.frame(vec![]);s.h.cmds.clear();s.click(if dock==Dock::Top{"m-qq-library"}else{"portrait-qq-library"});s.frame(vec![]);
            assert!(s.h.app.qq_library_open);assert!(!s.h.app.qq_library.loading);assert!(s.h.cmds.is_empty(),"opening a cached list must not create a WebView or fetch the profile");
            assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="已保存的歌单")));
            s.click("qq-library-back");s.frame(vec![]);
        }
        s.click(if dock==Dock::Top{"m-qq-library"}else{"portrait-qq-library"});s.frame(vec![]);s.h.cmds.clear();s.click("qq-library-refresh");
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicRefresh]);assert!(s.h.app.qq_library.loading);
    }
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_live_track_and_lyrics_replace_saved_music_before_the_next_media_poll(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    use crate::{qq_music::{Library,NativeLyrics,Resume},smtc::{MediaKind,NowPlaying}};
    let track=|title:&str,at|NowPlaying{source:crate::qq_music::SOURCE.into(),title:title.into(),artist:"歌手".into(),playing:true,
        duration:240.0,position:25.0,sampled_at:Some(at),playback_type:MediaKind::Music,..Default::default()};
    let lyric=|title:&str,line:&str|NativeLyrics{title:title.into(),lines:vec![line.into()],current:Some(0)};
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);s.h.app.show_lyrics=true;s.h.app.settings.island_lyrics=true;
        let old_at=std::time::Instant::now()-std::time::Duration::from_secs(2);
        s.h.media=track("以前的歌",old_at);
        for(title,line)in [("风吹麦浪","远处蔚蓝天空下"),("南方姑娘","在北方的村庄")]{
            let live=track(title,std::time::Instant::now());
            s.h.app.set_qq_music(Library{player_ready:true,resume:Some(Resume{title:"以前的歌".into(),..Default::default()}),
                live:Some(live.clone()),live_lyrics:Some(lyric(title,line)),..Default::default()},Some(lyric("以前的歌","过期歌词")));
            s.frame(vec![]);
            assert_eq!(s.h.app.media.title,title,"{dock:?}: use the current WebKit snapshot before the old worker poll catches up");
            assert_eq!(s.h.app.music_display().title,title);assert!(s.h.app.music_playing());assert!(!s.h.app.can_resume_qq_music());
            assert_eq!(s.h.app.native_qq_lyrics().unwrap().lines[0],line);
            if dock==Dock::Top{assert_eq!(s.h.app.island_lyric().unwrap().1,line);}
        }
        let mut delayed=s.h.app.qq_library.clone();delayed.player_ready=false;delayed.live=None;delayed.live_lyrics=None;
        s.h.media=track("南方姑娘",std::time::Instant::now());s.h.app.set_qq_music(delayed,None);s.frame(vec![]);
        assert_eq!(s.h.app.music_display().title,"南方姑娘","a temporarily unready bridge must not hide a fresh playing track behind old memory");
        assert!(s.h.app.music_playing());

        s.h.app.stop_qq_playback();s.frame(vec![]);
        assert!(s.h.app.media.title.is_empty(),"stop must reject the stale pre-stop worker observation");assert!(!s.h.app.music_playing());
        let mut resumed=s.h.app.qq_library.clone();resumed.live=Some(track("南方姑娘",std::time::Instant::now()));resumed.player_ready=true;
        s.h.app.set_qq_music(resumed,None);s.frame(vec![]);assert_eq!(s.h.app.media.title,"南方姑娘");

        s.h.media=NowPlaying{source:"com.spotify.client".into(),title:"Spotify 当前歌曲".into(),artist:"歌手".into(),playing:true,
            sampled_at:Some(std::time::Instant::now()),playback_type:MediaKind::Music,..Default::default()};s.frame(vec![]);
        assert_eq!(s.h.app.media.source,"com.spotify.client");assert!(s.h.app.native_qq_lyrics().is_none());
    }
}

#[cfg(target_os="macos")]
#[test]
fn qq_music_main_play_resumes_memory_and_can_cancel_without_selecting_a_playlist(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);s.h.media=Default::default();
        s.h.app.set_qq_music(crate::qq_music::Library{resume:Some(crate::qq_music::Resume{title:"上次听的歌".into(),artist:"歌手".into(),position:42.0,duration:240.0,..Default::default()}),..Default::default()},None);s.frame(vec![]);
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="上次听的歌")));
        assert!(s.h.app.media.title.is_empty(),"remembered metadata must not become live media");
        assert_eq!(s.h.app.music_display().position,42.0);assert!(!s.h.app.music_playing());
        let play=if dock==Dock::Top{"m-play"}else{"portrait-play"};
        s.h.cmds.clear();s.click(play);s.frame(vec![]);
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicResume]);assert!(s.h.app.qq_library.resuming);assert!(!s.h.app.qq_library_open);
        assert!(!s.h.app.music_playing(),"loading does not claim audio has started");
        s.h.cmds.clear();s.click(play);s.frame(vec![]);
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicStop]);assert!(!s.h.app.qq_library.resuming);assert!(s.h.app.qq_library.resume.is_some());

        s.h.media=crate::smtc::NowPlaying{source:crate::qq_music::SOURCE.into(),title:"上次听的歌".into(),artist:"歌手".into(),playing:true,playback_type:crate::smtc::MediaKind::Music,..Default::default()};s.frame(vec![]);s.h.cmds.clear();s.click(play);
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicResume],"a stale media sample after stop must restore the stopped engine directly");
        s.h.app.cancel_qq_playback();
        s.h.media=crate::smtc::NowPlaying{source:"com.apple.Music".into(),title:"其他播放器歌曲".into(),artist:"其他歌手".into(),playback_type:crate::smtc::MediaKind::Music,..Default::default()};s.frame(vec![]);s.h.cmds.clear();s.click(play);
        assert_eq!(s.h.cmds,vec![crate::app::Cmd::Media(crate::smtc::Transport::Toggle)],"QQ memory must not replace the active external transport");
        s.h.media=Default::default();s.h.app.qq_library.resuming=true;
        let mut failed=s.h.app.qq_library.clone();failed.resuming=false;failed.playback_error="播放失败，请重试".into();s.h.app.set_qq_music(failed,None);s.frame(vec![]);
        assert!(s.h.app.qq_library.resume.is_some());assert!(!s.h.app.music_playing());
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()=="播放失败，请重试")));
        s.h.cmds.clear();s.click(play);assert_eq!(s.h.cmds,vec![crate::app::Cmd::QqMusicResume]);assert!(s.h.app.qq_library.playback_error.is_empty());
        s.h.app.cancel_qq_playback();
        s.h.media=Default::default();s.h.app.qq_library.session=crate::qq_music::SessionStatus::Expired;s.frame(vec![]);s.h.cmds.clear();s.click(play);
        assert!(s.h.cmds.is_empty(),"confirmed logout must not launch remembered playback");
    }
}

#[test]
fn portrait_todo_add_ime_complete_edit_and_reminder_work_on_both_edges(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,TODO_PAGE);s.click("portrait-todo-add");s.frame(vec![]);assert!(s.h.app.adding);
        let input=s.h.ctx.read_response(egui::Id::new("todo-input")).unwrap();assert!(s.h.hit.contains_rect(input.rect));
        s.frame(vec![egui::Event::Ime(egui::ImeEvent::Enabled),egui::Event::Ime(egui::ImeEvent::Preedit("轻屿事项".into()))]);
        s.key(egui::Key::Enter);assert!(s.h.app.adding&&s.h.app.todos.items.is_empty(),"IME Enter must not submit");
        s.frame(vec![egui::Event::Ime(egui::ImeEvent::Commit("轻屿事项".into())),egui::Event::Ime(egui::ImeEvent::Disabled)]);s.frame(vec![]);s.key(egui::Key::Enter);
        assert_eq!(s.h.app.todos.items[0].text,"轻屿事项");assert!(s.h.app.task_dialog.is_some());
        s.frame(vec![]);s.click("task-reminder-skip");s.frame(vec![]);assert!(s.h.app.task_dialog.is_none());
        s.click(("portrait-todo",0usize));assert!(s.h.app.todos.items[0].done_for("2026-11-14"));
        s.click(("portrait-todo-edit",0usize));s.frame(vec![]);
        let edit=s.h.ctx.read_response(egui::Id::new("task-name-edit")).unwrap();assert!(s.h.hit.contains_rect(edit.rect));
        s.frame(vec![egui::Event::Ime(egui::ImeEvent::Enabled),egui::Event::Ime(egui::ImeEvent::Preedit("更新".into()))]);s.key(egui::Key::Enter);assert!(s.h.app.task_dialog.is_some(),"IME Enter must not save the edit dialog");
        s.frame(vec![egui::Event::Ime(egui::ImeEvent::Commit("更新".into())),egui::Event::Ime(egui::ImeEvent::Disabled)]);s.frame(vec![]);
        s.h.app.task_dialog.as_mut().unwrap().repeat=crate::model::REPEAT_ALL;
        s.click("task-save");assert_eq!(s.h.app.todos.items[0].repeat,crate::model::REPEAT_ALL);
    }
}

#[test]
fn portrait_lists_use_the_full_content_lane_without_detached_row_insets(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut shelf=Side::new(dock,SHELF_PAGE);
        let path=std::path::PathBuf::from("/synthetic/field-notes.pdf");shelf.h.app.shelf.items=vec![path.clone()];shelf.frame(vec![]);
        let file=shelf.h.ctx.read_response(egui::Id::new(("card-drag",true,("portrait-file",&path)))).unwrap();
        let remove=shelf.h.ctx.read_response(egui::Id::new(("card",true,("portrait-file-remove",&path)))).unwrap();
        assert!((remove.rect.right()-(file.rect.left()+shelf.h.app.expanded_width()-24.0)).abs()<0.1,"{dock:?}: shelf controls reach the content edge");

        let mut clips=Side::new(dock,CLIPBOARD_PAGE);clips.h.app.settings.clipboard_history=true;
        clips.h.app.clipboard.lock().items=vec![crate::clipboard::Clip{text:"一条连续列表记录".into(),at:"09:41".into(),pinned:false,media:None}];clips.frame(vec![]);
        let row=clips.h.ctx.read_response(egui::Id::new(("card",true,("portrait-clip",0usize)))).unwrap();
        let remove=clips.h.ctx.read_response(egui::Id::new(("card",true,("portrait-clip-delete",0usize)))).unwrap();
        assert!((remove.rect.right()-(row.rect.left()+clips.h.app.expanded_width()-26.0)).abs()<0.1,"{dock:?}: clipboard controls reach the content edge");

        let mut todos=Side::new(dock,TODO_PAGE);todos.h.app.todos.items=vec![crate::model::TodoItem::new("留给今天的一件事")];todos.frame(vec![]);
        let row=todos.h.ctx.read_response(egui::Id::new(("card",true,("portrait-todo",0usize)))).unwrap();
        let remove=todos.h.ctx.read_response(egui::Id::new(("card",true,("portrait-todo-delete",0usize)))).unwrap();
        assert!((remove.rect.right()-(row.rect.left()+todos.h.app.expanded_width()-25.0)).abs()<0.1,"{dock:?}: todo controls reach the content edge");

        let mut mail=Side::new(dock,MAIL_PAGE);seed_mail(&mut mail);mail.frame(vec![]);
        let row=mail.h.ctx.read_response(egui::Id::new(("card",true,("portrait-mail-row","demo@example.com",1u32)))).unwrap();
        assert!(row.rect.width()>=mail.h.app.expanded_width()-32.0,"{dock:?}: mail rows leave only the native scrollbar lane");
    }
}

#[test]
fn portrait_shelf_lists_scroll_delete_without_touching_files_and_clipboard_works(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,SHELF_PAGE);
        let folder=crate::model::config_dir().join("portrait-files");std::fs::create_dir_all(&folder).unwrap();
        let paths:Vec<_>=(0..12).map(|i|{let p=folder.join(format!("第{i:02}个文件.txt"));std::fs::write(&p,"x").unwrap();p}).collect();s.h.app.shelf.items=paths.clone();s.frame(vec![]);
        s.click("portrait-shelf-add");assert!(s.h.cmds.contains(&crate::app::Cmd::AddFiles));
        s.click(("portrait-file-remove",&paths[0]));assert!(!s.h.app.shelf.items.contains(&paths[0]));assert!(paths[0].exists());
        let before=s.h.ctx.read_response(egui::Id::new(("card-drag",true,("portrait-file",&paths[1])))).unwrap().rect;
        s.wheel(125.0,310.0,-300.0);s.settle();assert_eq!(s.h.app.page(),SHELF_PAGE);
        let after=s.h.ctx.read_response(egui::Id::new(("card-drag",true,("portrait-file",&paths[1])))).unwrap().rect;assert!(after.top()<before.top());
        s.click(("portrait-tab",CLIPBOARD_PAGE));s.frame(vec![]);s.click("portrait-clipboard-enable");assert!(s.h.app.settings.clipboard_history);
    }
}

fn seed_mail(s:&mut Side){
    s.h.app.mail.accounts=vec![crate::mail::Account{email:"demo@example.com".into(),enabled:true,provider:crate::mail::Provider::Qq,custom:None}];
    s.h.app.mail.recent=(1..=12).map(|uid|crate::mail::Item{account:"demo@example.com".into(),uid,from:format!("发件人 {uid}"),subject:format!("第 {uid} 封邮件"),summary:(0..30).map(|i|format!("第 {i} 段正文，竖向阅读可以完整滚动。\n\n")).collect::<String>().into(),html:"".into(),time:"09:41".into(),code:if uid==1{Some("123456".into())}else{None},links:vec![],received_unix:None}).collect();
    let item=s.h.app.mail.recent[0].clone();s.h.app.mail.privacy.observe(&item,true,s.h.t,chrono::Utc::now().timestamp());
}

#[test]
fn portrait_mail_code_list_and_body_scroll_without_navigating_pages(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MAIL_PAGE);seed_mail(&mut s);s.frame(vec![]);
        for shape in &s.h.shapes{if let egui::Shape::Text(text)=&shape.shape{assert!(!text.galley.text().contains('\n'),"mail list previews must stay within one row");}}
        s.click("portrait-mail-code");assert!(s.h.cmds.contains(&crate::app::Cmd::Copy("123456".into())));
        s.frame(vec![]);
        s.click(("portrait-mail-row","demo@example.com",1u32));s.frame(vec![]);assert!(s.h.app.mail.open_uid.is_some());
        let before=s.h.shapes.iter().filter_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text().contains("第 0 段正文")=>Some(text.pos.y),_=>None}).next().unwrap();
        s.wheel(130.0,330.0,-150.0);s.settle();assert_eq!(s.h.app.page(),MAIL_PAGE);
        let after=s.h.shapes.iter().filter_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text().contains("第 0 段正文")=>Some(text.pos.y),_=>None}).next();assert!(after.is_none_or(|y|y<before),"scroll must move the first paragraph upward or outside the clipped viewport");
        s.click("portrait-mail-reader");assert!(s.h.app.mail.reading.is_some());
        s.click("portrait-mail-back");s.frame(vec![]);assert!(s.h.app.mail.open_uid.is_none());
        s.wheel(130.0,340.0,-300.0);s.settle();assert_eq!(s.h.app.page(),MAIL_PAGE);
    }
}

fn seed_weather(s:&mut Side){
    s.h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:19.0,feels:Some(18.0),humidity:Some(58),wind:Some(12.0),code:2,day:true,high:Some(23.0),low:Some(15.0),sunrise:Some("06:52".into()),sunset:Some("17:21".into()),at:chrono::Utc::now().timestamp(),days:(0..7).map(|i|crate::weather::Day{label:if i==0{"今天".into()}else{format!("周{}",["日","一","二","三","四","五"][i-1])},date:format!("2026-11-{:02}",14+i),high:23.0,low:15.0,rain:20,code:2}).collect(),..Default::default()});
}

#[test]
fn portrait_weather_seven_day_forecast_city_and_solar_controls_are_reachable(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,WEATHER_PAGE);seed_weather(&mut s);s.frame(vec![]);
        for date in ["11/14","11/15","11/16","11/17","11/18","11/19","11/20"]{assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()==date)));}
        s.click("portrait-weather-solar");assert!(s.h.app.weather.solar_open);s.frame(vec![]);
        s.click("portrait-weather-back");assert!(!s.h.app.weather.solar_open);s.frame(vec![]);
        s.click("portrait-weather-place");s.settle();assert!(s.h.app.weather.picker_open);
        let hint=s.h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text()=="在右上角齿轮中管理城市"=>Some(text.visual_bounding_rect().center()),_=>None}).unwrap();
        s.click_at(hint);assert!(!s.h.app.settings_open);
        s.click("portrait-settings");assert!(s.h.app.settings_open);assert_eq!(s.h.app.settings_tab,12);
        s.click("portrait-weather-source");assert!(s.h.cmds.contains(&crate::app::Cmd::OpenUrl("https://open-meteo.com/".into())));
    }
}

#[test]
fn side_interaction_visible_default_and_saved_handle_choice_ignore_top_minimal(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut h=H::new();let path=crate::model::config_dir().join("settings.txt");
    for (stored,handle) in [("dock=left\nminimal=1\n",false),("dock=right\nminimal=1\nmacos_side_handle=1\n",true),("dock=left\nmacos_side_handle=0\n",false)]{
        std::fs::write(&path,stored).unwrap();h.app.settings=crate::model::Settings::load();assert_eq!(h.app.settings.macos_side_handle,handle);
        for _ in 0..100{side_frame(&mut h,None);}
        assert_eq!(h.app.side_handle(),handle);assert_eq!(h.app.w.pos,if handle{6.0}else{crate::side_dock::SIDE_BAR_W});
        if !handle{assert!(h.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(text)if text.galley.text()=="09")),"visible island must keep its clock even when top minimal was saved");}
        h.app.settings.save();assert_eq!(crate::model::Settings::load().macos_side_handle,handle);
    }
}

#[test]
fn side_interaction_collapsed_body_moves_both_modes_in_points_and_saves_on_release(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let bounds=crate::display_layout::Bounds{left:-3840,top:-400,right:0,bottom:1760};
    for dock in [Dock::Left,Dock::Right]{for handle in [false,true]{for scale in [0.8,1.0,2.0]{
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.macos_side_handle=handle;h.app.settings.island_offset=-40;h.app.settings.skin=crate::model::Skin::GlassDark;h.app.settings.save();
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:11.0,code:61,day:true,..Default::default()});
        let mut screen=Rect::NOTHING;for _ in 0..100{screen=island_drag_frame(&mut h,bounds,scale,None,None);}
        let grip=h.ctx.read_response(egui::Id::new(("card-drag",true,"side-position"))).expect("resident body is draggable");
        let start=grip.rect.center()-screen.min.to_vec2();island_drag_frame(&mut h,bounds,scale,Some(start),Some(true));
        for _ in 0..38{island_drag_frame(&mut h,bounds,scale,Some(start),None);assert!(!h.app.open,"holding the grip must not trigger hover expansion");}
        let before=island_drag_frame(&mut h,bounds,scale,Some(start),None);let body_before=h.hit.center()-before.min.to_vec2();
        let end=start+egui::vec2(0.0,96.0/scale);for _ in 0..50{island_drag_frame(&mut h,bounds,scale,Some(end),None);}
        let after=island_drag_frame(&mut h,bounds,scale,Some(end),None);
        let body_after=h.hit.center()-after.min.to_vec2();
        assert!(((body_after.y-body_before.y)*scale-96.0).abs()<6.0,"{dock:?} / {handle} / {scale}");
        assert_eq!(crate::model::Settings::load().island_offset,-40,"drag must not write every frame");
        let offset=h.app.settings.island_offset;assert!(offset>-40);
        island_drag_frame(&mut h,bounds,scale,Some(end),Some(false));assert_eq!(crate::model::Settings::load().island_offset,offset);
        for _ in 0..80{island_drag_frame(&mut h,bounds,scale,Some(end),None);}assert!(!h.app.open);
        let outline=h.app.side_hit_outline(Rect::from_min_size(Pos2::ZERO,h.app.canvas_size()));assert!(outline.iter().all(|p|p.is_finite()));
    }}}
}

#[test]
fn side_interaction_resident_body_drag_keeps_weather_clickable(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let bounds=crate::display_layout::Bounds{left:0,top:0,right:1920,bottom:1080};
    for dock in [Dock::Left,Dock::Right]{
        let mut h=H::new();h.app.settings.dock=dock;h.app.settings.macos_side_handle=false;
        h.app.weather.snapshot=Some(crate::weather::Snapshot{place:"杭州".into(),temp:11.0,code:61,day:true,..Default::default()});
        let mut screen=Rect::NOTHING;for _ in 0..100{screen=island_drag_frame(&mut h,bounds,1.0,None,None);}
        let weather=h.ctx.read_response(egui::Id::new(("card",true,"weather-side"))).unwrap();
        let point=weather.rect.center()-screen.min.to_vec2();
        island_drag_frame(&mut h,bounds,1.0,Some(point),None);
        for pressed in [true,false]{island_drag_frame(&mut h,bounds,1.0,Some(point),Some(pressed));}
        assert_eq!(h.app.page(),WEATHER_PAGE,"a short click must still open the weather");
        assert_eq!(h.app.settings.island_offset,0,"a short click must not move the island");
    }
}

#[test]
fn side_interaction_interrupted_motion_keeps_content_continuous_and_stops_repainting_when_idle(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{for hz in [60.0,120.0]{
        let mut s=Side::fresh(dock,MUSIC_PAGE);s.h.media.playing=false;s.h.app.settings.skin=crate::model::Skin::GlassDark;
        let mut previous_alpha=0.0;let mut previous_p=0.0;let mut saw_middle=false;
        for frame in 0..(hz as usize*7){
            // Force a close and return while the contour is still moving.
            if frame==(hz*0.15) as usize{s.h.app.foreground_changed();s.h.t+=5.0;}
            if frame==(hz*0.33) as usize{s.h.app.show_page(MUSIC_PAGE);}
            let output=s.frame_dt(vec![],1.0/hz);
            let p=s.h.app.progress();let alpha=s.h.app.reveal;
            if frame>0{assert!((alpha-previous_alpha).abs()<=((p-previous_p) as f32).abs()*2.0+0.001,"opening direction must not change opacity by itself");}
            if (0.1..0.8).contains(&p){saw_middle=true;}
            let out=s.out.as_ref().unwrap();assert_eq!(out.hit_outline.as_ref().unwrap(),&out.glass.as_ref().unwrap()[0].outline());
            if frame==(hz*0.1) as usize{assert!(out.repaint_after.is_zero(),"transitions use display-paced repaint");}
            previous_alpha=alpha;previous_p=p;
            let _=output;
        }
        assert!(saw_middle);assert!(!s.h.app.open);assert_eq!(s.out.as_ref().unwrap().repaint_after,std::time::Duration::from_secs(1));
    }}
}

#[test]
fn side_interaction_larger_viewports_show_more_rows_and_keep_fonts_and_controls(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut counts=Vec::new();
        for (width,height) in [(292,532),(480,820)]{
            let mut s=Side::new(dock,TODO_PAGE);s.h.app.settings.side_panel_width=width;s.h.app.settings.side_panel_height=height;
            s.h.app.todos.items=(0..20).map(|i|crate::model::TodoItem::new(format!("事项编号 {i:02}，检查面板容量"))).collect();s.settle();
            let visible=s.h.shapes.iter().filter(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().starts_with("事项编号")&&shape.clip_rect.contains_rect(text.visual_bounding_rect()))).count();
            let font_height=s.h.shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text().starts_with("事项编号")=>Some(text.galley.rows[0].height()),_=>None}).unwrap();
            counts.push((visible,font_height));
            let bounds=s.h.app.island_rect(Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size()),width as f32,height as f32);
            assert!(bounds.contains_rect(s.h.ctx.read_response(egui::Id::new(("card",true,"portrait-todo-add"))).unwrap().rect));
            s.click(("portrait-tab",MAIL_PAGE));seed_mail(&mut s);s.settle();
            let before=s.h.shapes.iter().filter(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().starts_with("发件人")&&shape.clip_rect.contains_rect(text.visual_bounding_rect()))).count();
            assert!(before>=if height>532{6}else{3});
            s.click(("portrait-mail-row","demo@example.com",1u32));s.frame(vec![]);
            let content=s.h.shapes.iter().filter_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text().contains("段正文")=>Some(shape.clip_rect.height()),_=>None}).fold(0.0f32,f32::max);
            assert!(content>=if height>532{470.0}else{190.0});
            s.click(("portrait-tab",WEATHER_PAGE));seed_weather(&mut s);s.settle();
            for date in ["11/14","11/15","11/16","11/17","11/18","11/19","11/20"]{assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text()==date&&shape.clip_rect.contains_rect(text.visual_bounding_rect()))));}
            s.h.app.settings.save();let loaded=crate::model::Settings::load();assert_eq!((loaded.side_panel_width,loaded.side_panel_height),(width,height));
        }
        assert!(counts[1].0>counts[0].0+3,"larger viewport must show more real todo rows: {counts:?}");assert_eq!(counts[0].1,counts[1].1,"font metrics must stay unchanged");
    }
}

#[test]
fn side_interaction_header_resize_drag_changes_real_width_height_and_persists(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,TODO_PAGE);let start=s.point(72.0,24.0);
        s.h.cursor=Some(start);s.frame(vec![egui::Event::PointerMoved(start)]);s.frame(vec![egui::Event::PointerButton{pos:start,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        let delta=egui::vec2(if dock==Dock::Left{120.0}else{-120.0},190.0);let end=start+delta;
        s.h.cursor=Some(end);for _ in 0..3{s.frame(vec![egui::Event::PointerMoved(end)]);}
        s.frame(vec![egui::Event::PointerButton{pos:end,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        assert_eq!((s.h.app.settings.side_panel_width,s.h.app.settings.side_panel_height),(412,722));
        let loaded=crate::model::Settings::load();assert_eq!((loaded.side_panel_width,loaded.side_panel_height),(412,722));
        s.settle();assert!((s.h.app.w.pos-412.0).abs()<0.1&&(s.h.app.h.pos-722.0).abs()<0.1);
    }
}

#[cfg(target_os="macos")]
#[test]
fn side_interaction_settings_switch_and_position_buttons_save_the_visible_choice(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut s=Side::new(Dock::Right,TODO_PAGE);s.h.app.settings.minimal=true;s.h.app.settings.macos_side_handle=false;s.h.app.settings_tab=0;s.h.app.open_settings();s.h.ctx.set_embed_viewports(false);
    fn draw(s:&mut Side,events:Vec<egui::Event>)->Vec<egui::epaint::ClippedShape>{
        let captured=std::sync::Arc::new(Mutex::new(None));let capture=captured.clone();let incoming=std::sync::Arc::new(Mutex::new(events));
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;let events=std::mem::take(&mut *incoming.lock().unwrap());
            *capture.lock().unwrap()=Some(ctx.run(egui::RawInput{viewport_id:id,viewports:std::iter::once((id,egui::ViewportInfo::default())).collect(),events,focused:true,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(800.0,1500.0))),..Default::default()},|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let _=s.h.ctx.run(Default::default(),|ctx|s.h.app.settings_ui(ctx,false));let shapes=captured.lock().unwrap().take().unwrap().shapes;shapes
    }
    fn click(s:&mut Side,label:&str){
        let shapes=draw(s,vec![]);let point=shapes.iter().find_map(|shape|match &shape.shape{egui::Shape::Text(text)if text.galley.text()==label&&shape.clip_rect.contains(text.visual_bounding_rect().center())=>Some(text.visual_bounding_rect().center()),_=>None}).unwrap_or_else(||panic!("settings control missing: {label}"));
        draw(s,vec![egui::Event::PointerMoved(point)]);for pressed in [true,false]{draw(s,vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()}]);}
    }
    click(&mut s,"细把手");assert!(crate::model::Settings::load().macos_side_handle);
    click(&mut s,"灵动岛");assert!(!crate::model::Settings::load().macos_side_handle);assert!(s.h.app.settings.minimal,"top preference is preserved");
    let before=s.h.app.settings.island_offset;click(&mut s,"↑");assert_eq!(crate::model::Settings::load().island_offset,before-5);
    click(&mut s,"居中");assert_eq!(crate::model::Settings::load().island_offset,0);assert!(!s.h.app.settings.macos_side_handle,"position preset must not overwrite appearance");
    s.h.app.settings.side_panel_width=480;s.h.app.settings.side_panel_height=820;
    click(&mut s,"恢复默认面板大小");let saved=crate::model::Settings::load();assert_eq!((saved.side_panel_width,saved.side_panel_height),(292,532));
}

#[test]
#[ignore="QINGYU_SIDE_REVIEW=output/review/side-interaction cargo test side_interaction_gallery -- --ignored --test-threads=1"]
fn side_interaction_gallery(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_SIDE_REVIEW").expect("review directory"));std::fs::create_dir_all(&dest).unwrap();
    for (name,handle,panel,page) in [("visible-island",false,(292,532),TODO_PAGE),("thin-handle",true,(292,532),TODO_PAGE),("larger-todo",false,(480,820),TODO_PAGE),("larger-mail",false,(480,820),MAIL_PAGE),("larger-weather",false,(480,820),WEATHER_PAGE),("larger-music",false,(480,820),MUSIC_PAGE)]{
        let mut s=Side::fresh(Dock::Right,page);s.h.app.settings.macos_side_handle=handle;s.h.app.settings.side_panel_width=panel.0;s.h.app.settings.side_panel_height=panel.1;s.h.media.playing=false;
        seed_mail(&mut s);seed_weather(&mut s);s.h.app.todos.items=(0..18).map(|i|crate::model::TodoItem::new(format!("事项 {i:02} · 更多内容，字号保持不变"))).collect();
        if name=="larger-mail"{let item=s.h.app.mail.recent[0].clone();s.h.app.mail_open(&item);}
        let size=s.h.app.canvas_size();let ppp=2.0;s.h.ctx.set_pixels_per_point(ppp);let mut raster=crate::preview_raster::Raster::new((size.x*ppp) as usize,(size.y*ppp) as usize);
        if name=="visible-island"||name=="thin-handle"{s.h.t+=6.0;}
        for frame in 0..100{
            let output=s.frame(vec![]);raster.update_textures(&output.textures_delta);
            if frame==99{raster.buf.fill([0.64,0.70,0.71,1.0]);raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=vec![];raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("{name}.png")),&rgba,(size.x*ppp) as u32,(size.y*ppp) as u32,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&output.textures_delta);
        }
    }
}

#[test]
fn portrait_small_root_keeps_real_settings_and_reader_at_their_native_scale(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut s=Side::new(Dock::Right,MAIL_PAGE);seed_mail(&mut s);let item=s.h.app.mail.recent[0].clone();s.h.app.mail_open_reader(&item);s.h.app.open_settings();s.h.ctx.set_embed_viewports(false);
    let captured=std::sync::Arc::new(Mutex::new(Vec::new()));let capture=captured.clone();
    egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
        let id=viewport.ids.this;
        let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(800.0,650.0))),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        capture.lock().unwrap().push(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
    });
    for scale in [0.2,0.5,0.8,1.0]{
        let mut raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(SIDE_W*scale,SIDE_H*scale))),viewports:std::iter::once((egui::ViewportId::ROOT,egui::ViewportInfo{native_pixels_per_point:Some(2.0),..Default::default()})).collect(),..Default::default()};
        eframe::scale_root_input(&mut raw,scale);
        assert!((raw.screen_rect.unwrap().size()-egui::vec2(SIDE_W,SIDE_H)).length()<0.001);
        let root=s.h.ctx.run(raw,|ctx|{s.h.app.settings_ui(ctx,false);s.h.app.mail_reader_ui(ctx);});
        assert!((root.pixels_per_point-2.0*scale).abs()<0.001);
        let children:Vec<_>=captured.lock().unwrap().drain(..).collect();assert_eq!(children.len(),2,"both real child windows must render");
        for child in children{assert_eq!(child.pixels_per_point,2.0);for shape in child.shapes{if let egui::Shape::Text(text)=shape.shape{assert_eq!(text.galley.pixels_per_point,2.0);}}}
    }
}

#[test]
fn portrait_fluid_outline_matches_input_blur_and_reduce_motion_at_every_stage(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::new(dock,MUSIC_PAGE);s.h.app.settings.skin=crate::model::Skin::GlassDark;
        s.h.cursor=None;for _ in 0..260{s.frame(vec![]);}
        assert!(!s.h.app.open);assert!((s.h.app.w.pos-6.0).abs()<0.01&&(s.h.app.h.pos-52.0).abs()<0.01);
        let resident=s.h.app.displayed_island_rect(Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size()));
        let edge=if dock==Dock::Left{resident.left()+0.5}else{resident.right()-0.5};let resident_y=resident.center().y;
        s.h.cursor=Some(egui::pos2(edge,resident_y));let mut middle=false;
        for frame in 0..100{
            s.frame(vec![egui::Event::PointerMoved(s.h.cursor.unwrap())]);
            let out=s.out.as_ref().unwrap();let input=out.hit_outline.as_ref().unwrap();let mask=out.glass.as_ref().unwrap()[0].outline();assert_eq!(*input,mask);
            assert!(input.iter().all(|p|p.is_finite()));
            if frame<26{assert!(!s.h.app.open,"0.45s hover intent must be preserved");}
            if (0.2..0.8).contains(&s.h.app.progress()){middle=true;}
            let body=s.h.app.displayed_island_rect(Rect::from_min_size(Pos2::ZERO,egui::vec2(SIDE_W,SIDE_H)));
            let corner=if dock==Dock::Left{egui::pos2(body.right()-1.0,body.top()+1.0)}else{egui::pos2(body.left()+1.0,body.top()+1.0)};
            if body.width()>50.0{assert!(!crate::shape::contains_outline(input,corner),"transparent outer corner must pass clicks");}
        }
        assert!(middle&&s.h.app.open);
        #[cfg(target_os="macos")]
        {
            s.h.ctx.data_mut(|data|data.insert_temp(egui::Id::new("macos-accessibility-preferences"),(std::time::Instant::now(),2u32)));
            s.h.cursor=None;s.h.t+=5.0;s.frame(vec![]);s.frame(vec![]);s.h.t+=0.5;s.frame(vec![]);
            assert!(!s.h.app.open);assert_eq!(s.h.app.w.pos,6.0);assert_eq!(s.h.app.h.pos,52.0);
            s.h.cursor=Some(egui::pos2(edge,resident_y));s.frame(vec![]);s.h.t+=0.5;s.frame(vec![]);
            assert!(s.h.app.open);assert_eq!(s.h.app.w.pos,PANEL_W);assert_eq!(s.h.app.h.pos,PANEL_H);
            assert_eq!(s.out.as_ref().unwrap().hit_outline.as_ref().unwrap(),&s.out.as_ref().unwrap().glass.as_ref().unwrap()[0].outline());
        }
    }
}

#[test]
#[ignore="Synthetic actual-egui portrait gallery: QINGYU_PORTRAIT_GALLERY=/tmp/qingyu-portrait cargo test portrait_gallery -- --ignored --test-threads=1"]
fn portrait_gallery(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let dest=std::path::PathBuf::from(std::env::var_os("QINGYU_PORTRAIT_GALLERY").expect("gallery output directory"));std::fs::create_dir_all(&dest).unwrap();
    for dock in [Dock::Left,Dock::Right]{for page in 0..5{
        let mut s=Side::fresh(dock,page);seed_mail(&mut s);seed_weather(&mut s);
        s.h.app.todos.items=["整理项目文档","给绿植浇水","确认周末出行安排","阅读二十分钟"].into_iter().map(crate::model::TodoItem::new).collect();
        let folder=crate::model::config_dir().join("preview");std::fs::create_dir_all(&folder).unwrap();
        s.h.app.shelf.items=["设计稿.png","会议记录.md","轻屿使用指南.txt","灵感收集.pdf"].into_iter().map(|name|{let p=folder.join(name);std::fs::write(&p,"preview").unwrap();p}).collect();
        s.h.ctx.set_pixels_per_point(2.0);let mut raster=crate::preview_raster::Raster::new((SIDE_W*2.0) as usize,(SIDE_H*2.0) as usize);
        for frame in 0..65{
            let output=s.frame(vec![]);raster.update_textures(&output.textures_delta);
            if frame==64{raster.buf.fill([0.15,0.20,0.22,1.0]);raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=vec![];raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("portrait-{}-{page}.png",if dock==Dock::Left{"left"}else{"right"})),&rgba,(SIDE_W*2.0) as u32,(SIDE_H*2.0) as u32,image::ColorType::Rgba8).unwrap();}
            raster.free_textures(&output.textures_delta);
        }
    }}
    for dock in [Dock::Left,Dock::Right]{
        let mut s=Side::fresh(dock,MUSIC_PAGE);s.h.media.playing=false;s.h.ctx.set_pixels_per_point(2.0);
        let mut raster=crate::preview_raster::Raster::new((SIDE_W*2.0) as usize,(SIDE_H*2.0) as usize);
        let mut stage=0;
        for frame in 0..480{
            if frame==360{s.h.cursor=Some(egui::pos2(if dock==Dock::Left{0.5}else{SIDE_W-0.5},300.0));}
            let output=s.frame(vec![]);raster.update_textures(&output.textures_delta);
            let name=if frame==359{Some("closed")}else if stage==1&&(0.35..0.7).contains(&s.h.app.progress()){Some("middle")}else if stage==2&&s.h.app.progress()>0.99{Some("open")}else{None};
            if let Some(name)=name{
                raster.buf.fill([0.78,0.82,0.83,1.0]);raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);
                let mut rgba=vec![];raster.to_rgba8(&mut rgba);image::save_buffer(dest.join(format!("connection-{}-{name}.png",if dock==Dock::Left{"left"}else{"right"})),&rgba,(SIDE_W*2.0) as u32,(SIDE_H*2.0) as u32,image::ColorType::Rgba8).unwrap();stage+=1;
            }
            raster.free_textures(&output.textures_delta);if stage==3{break;}
        }
        assert_eq!(stage,3,"export all actual collapse/animation/expanded silhouettes");
    }
}

#[path="clipboard_ui_tests.rs"]
mod clipboard_tests;

#[test]
fn notification_panels_unify_sources_scroll_open_and_keep_controls_inside_all_docks(){
    use super::notification_tests::{enable,preview};
    use crate::notifications::{Batch,Status};
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let folder=std::env::var_os("QINGYU_NOTIFICATION_PREVIEW_DIR").map(std::path::PathBuf::from);if let Some(folder)=&folder{std::fs::create_dir_all(folder).unwrap();}
    for dock in [Dock::Top,Dock::Left,Dock::Right] {
        let mut s=Side::fresh(dock,crate::app::NOTIFICATIONS_PAGE);enable(&mut s.h.app);
        let mut notices=vec![];
        for index in 0..10{notices.push(preview(&format!("fixture-{index}"),"com.tencent.qq","QQ","项目群","文档已更新，有空时看一下"));}
        notices.extend([preview("qq","com.tencent.qq","QQ","陈同学","周四的评审改到下午三点"),preview("wx","com.tencent.xinWeChat","微信","家人","晚饭等你回来，一起吃吧")]);
        s.h.app.mail.accounts=vec![crate::mail::Account{email:"demo@qq.com".into(),provider:crate::mail::Provider::Qq,enabled:true,custom:None}];
        s.h.app.notification_batch(Batch{status:Status::Watching{apps:2},events:notices});
        s.h.app.mail_event(crate::mail::Event::Mail{new:true,item:crate::mail::Item{received_unix:None,account:"demo@qq.com".into(),from:"设计团队".into(),subject:"周五的评审资料已准备好".into(),time:"09:41".into(),code:None,summary:String::new().into(),html:String::new().into(),links:vec![],uid:42}});
        s.h.app.alerts.clear();s.h.app.notice=None;
        let size=s.h.app.canvas_size();s.h.ctx.set_pixels_per_point(2.0);
        let mut raster=folder.as_ref().map(|_|crate::preview_raster::Raster::new((size.x*2.0)as usize,(size.y*2.0)as usize));
        for index in 0..70 {
            let output=s.frame(vec![]);
            if let Some(raster)=&mut raster {
                raster.update_textures(&output.textures_delta);
                if index==69 {raster.buf.fill([0.13,0.18,0.20,1.0]);raster.draw(&s.h.ctx.tessellate(output.shapes,output.pixels_per_point),output.pixels_per_point);let mut rgba=vec![];raster.to_rgba8(&mut rgba);image::save_buffer(folder.as_ref().unwrap().join(format!("notifications-{}.png",dock.key())),&rgba,(size.x*2.0)as u32,(size.y*2.0)as u32,image::ColorType::Rgba8).unwrap();}
                raster.free_textures(&output.textures_delta);
            }
        }
        let body=s.h.app.island_rect(Rect::from_min_size(Pos2::ZERO,size),s.h.app.expanded_width(),s.h.app.expanded_height());
        let settings=if dock.is_side(){"portrait-settings"}else{"page-settings"};
        for id in [settings,"notifications-clear"] {
            let response=s.h.ctx.read_response(egui::Id::new(("card",true,id))).unwrap();assert!(body.expand(1.0).contains_rect(response.rect),"{dock:?} {id}");assert!(response.rect.height()>=28.0);
        }
        if dock.is_side(){
            let selected=s.h.ctx.read_response(egui::Id::new(("card",true,("portrait-tab",s.h.app.page())))).unwrap();
            assert!(selected.rect.width()>=28.0-0.01&&body.contains_rect(selected.rect),"the selected notification tab must be visible in the scrollable navigation");
            assert!(s.h.ctx.read_response(egui::Id::new(("card",true,"control-center-tab"))).is_some(),"the main overview retains its own navigation entry");
        }
        let row=egui::Id::new(("card",true,("notification-row",0usize)));let before=s.h.ctx.read_response(row).unwrap().rect.top();
        s.wheel(180.0,if dock==Dock::Top{130.0}else{260.0},-220.0);s.settle();assert_eq!(s.h.app.page(),crate::app::NOTIFICATIONS_PAGE);assert!(s.h.ctx.read_response(row).unwrap().rect.top()<before,"{dock:?} notification list must scroll without changing pages; before {before}, after {}",s.h.ctx.read_response(row).unwrap().rect.top());
        s.click(settings);assert_eq!(s.h.app.settings_tab,16);assert!(s.h.app.settings_open);
        s.h.app.settings_open=false;s.click("notifications-clear");assert!(s.h.app.history.is_empty());
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="还没有新通知")));
    }
}

#[test]
fn notification_empty_page_and_settings_are_reachable_without_permissions(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut s=Side::new(Dock::Right,crate::app::NOTIFICATIONS_PAGE);
    assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="还没有新通知")));
    assert!(s.h.app.pending.iter().all(|cmd|!matches!(cmd,crate::app::Cmd::RequestNotificationAccess)));
    s.click("portrait-settings");assert_eq!(s.h.app.settings_tab,16);assert!(s.h.app.settings_open);s.h.app.settings_open=false;
    assert_eq!(crate::extras::settings_tab_module(16),Some(crate::app::NOTIFICATIONS_PAGE));
    s.h.app.set_page_visible(crate::app::NOTIFICATIONS_PAGE,false);s.h.app.show_page(crate::app::NOTIFICATIONS_PAGE);assert_ne!(s.h.app.page(),crate::app::NOTIFICATIONS_PAGE);assert!(!s.h.app.settings_open);assert_eq!(s.h.app.settings_tab,16);
}
