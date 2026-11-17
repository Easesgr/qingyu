//! Local execution status, using the same list in the top and side panels.
use crate::{agent_monitor::{Session, Status}, app::{App,Env}, paint::{self, Card, H}, theme::{self, W}};
use eframe::egui::{self, Color32, Pos2, Rect};

pub(crate) const TOP_LIST_BOUNDS: Rect = Rect::from_min_max(Pos2::new(44.0,76.0),Pos2::new(375.0,238.0));

fn status_color(status:Status)->Color32 {
    match status {
        Status::Thinking|Status::Working=>theme::CYAN,
        Status::Waiting|Status::Approval=>theme::YELLOW,
        Status::Completed=>theme::SOFT_GREEN,
        Status::Error=>theme::RED_TOP,
        _=>theme::GRAY,
    }
}

fn one_line(text:&str)->String {text.split_whitespace().collect::<Vec<_>>().join(" ")}

fn session_title(session:&Session)->String {
    crate::agent_activity::title(session)
}

fn elapsed_clock(session:&Session,now:u64)->String {
    let Some(seconds)=session.elapsed_seconds(now) else{return "—".into();};
    if seconds<3600{format!("{:02}:{:02}",seconds/60,seconds%60)}
    else{format!("{}:{:02}:{:02}",seconds/3600,seconds/60%60,seconds%60)}
}

fn elapsed_description(session:&Session,now:u64)->String {
    let Some(seconds)=session.elapsed_seconds(now) else{return "尚无可靠的开始时间".into();};
    let prefix=if session.process_only{if session.finished_at.is_some(){"进程运行了"}else{"已启动"}}
        else if session.finished_at.is_some(){"本轮用时"}else if session.status==Status::Unknown{"距本轮开始"}else{"已运行"};
    let duration=if seconds<60{format!("{seconds} 秒")}
        else if seconds<3600{format!("{} 分 {} 秒",seconds/60,seconds%60)}
        else{format!("{} 小时 {} 分 {} 秒",seconds/3600,seconds/60%60,seconds%60)};
    format!("{prefix} {duration}")
}

fn compact_status(status:Status)->&'static str{
    match status{Status::Thinking=>"思考",Status::Working=>"执行",Status::Waiting=>"等待",Status::Approval=>"需要审批",Status::Completed=>"完成",_=>status.label()}
}

fn header_button(c:&Card,x:f32,y:f32,glyph:&str,name:&str,id:&str)->bool{
    let response=c.hit(x,y,28.0,28.0,id).on_hover_text(name);
    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,name));
    if response.hovered()||response.has_focus(){c.fill(x,y,28.0,28.0,7.0,theme::BTN_HOVER);}
    if response.has_focus(){c.stroke(x,y,28.0,28.0,7.0,1.4,theme::CYAN);}
    paint::glyph(c,glyph,x+14.0,y+14.0,16.0,theme::GRAY);
    response.clicked()
}

fn expanded_title(c:&Card,session:&Session,width:f32)->std::sync::Arc<egui::Galley>{
    c.painter.layout(session_title(session),c.font(12.0,W::Semibold),theme::WHITE,(width-20.0)*c.s)
}

fn session_height(c:&Card,session:&Session,width:f32,expanded:bool)->f32{
    if expanded{107.0+expanded_title(c,session,width).size().y/c.s}else{44.0}
}

fn session_row(c:&Card,session:&Session,width:f32,expanded:bool,now:u64)->bool{
    let height=session_height(c,session,width,expanded);
    let title=session_title(session);
    let elapsed=elapsed_description(session,now);
    let pid=session.pid.map_or_else(||"未关联进程".into(),|pid|format!("进程 {pid}"));
    let description=format!("{}\n{} · {}\n{elapsed}\n项目：{}\n{}\n{pid}\n会话 {}\n{}",title,session.agent.label(),session.status.label(),session.project,session.detail,session.id,if expanded{"点击收起为一行"}else{"点击展开详情"});
    let response=c.ui.interact(c.rect(0.0,0.0,width,height),egui::Id::new(("agent-session",c.live,&session.id)),egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if expanded{"收起为一行"}else{"展开对话详情"});
    response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::Button,c.live,expanded,&description));
    if expanded||response.hovered()||response.has_focus(){
        c.fill(0.0,0.0,width,height,8.0,if expanded{Color32::from_rgb(25,53,58)}else{Color32::from_rgb(31,65,71)});
    }
    if response.has_focus(){c.stroke(0.5,0.5,width-1.0,height-1.0,8.0,1.4,theme::CYAN);}
    crate::agent_icons::paint(c,session.agent,16.0,22.0,22.0);
    let badge=compact_status(session.status);
    let badge_width=c.text_w(badge,11.0,W::Semibold).max(25.0);
    let badge_x=width-badge_width-8.0;
    c.centered_text(width-8.0,22.0,badge,11.0,W::Semibold,status_color(session.status),H::Right);
    let clock=elapsed_clock(session,now);
    let clock_width=c.text_w(&clock,11.0,W::Regular).max(28.0);
    let clock_right=badge_x-12.0;
    c.centered_text(clock_right,22.0,&clock,11.0,W::Regular,theme::GRAY,H::Right);
    let title_width=(clock_right-clock_width-12.0-32.0).max(10.0);
    c.centered_text(32.0,22.0,&c.fit(&title,13.0,W::Semibold,title_width),13.0,W::Semibold,theme::WHITE,H::Left);
    if !expanded{c.seg((32.0,height-0.5),(width-8.0,height-0.5),0.5,Color32::from_white_alpha(20));}
    if expanded {
        c.seg((10.0,43.0),(width-10.0,43.0),0.5,Color32::from_white_alpha(24));
        let full_title=expanded_title(c,session,width);
        let project_y=61.0+full_title.size().y/c.s;
        c.painter.galley(c.pos(10.0,49.0),full_title,theme::WHITE);
        let project=format!("项目 · {}",if session.project.is_empty(){"未提供项目路径".into()}else{one_line(&session.project)});
        c.centered_text(10.0,project_y,&c.fit(&project,11.0,W::Regular,width-20.0),11.0,W::Regular,theme::GRAY,H::Left);
        let detail=format!("{} · {}",session.agent.label(),if session.detail.is_empty(){pid}else{one_line(&session.detail)});
        c.centered_text(10.0,project_y+18.0,&c.fit(&detail,11.0,W::Regular,width-20.0),11.0,W::Regular,theme::GRAY,H::Left);
        c.centered_text(10.0,project_y+36.0,&c.fit(&elapsed,11.0,W::Regular,width-20.0),11.0,W::Regular,theme::CYAN,H::Left);
    }
    response.clicked()
}

impl App {
    pub(crate) fn page_agents(&mut self,c:&Card,env:&Env,side:bool){
        let mut visible=self.agent_tasks.sessions().to_vec();
        // An interruption is no longer a running task, but its explicit View
        // action still opens that exact conversation's observed details.
        if let Some(id)=&self.agent_expanded{
            if !visible.iter().any(|session|&session.id==id){
                if let Some(session)=self.agent_snapshot.sessions.iter().find(|session|&session.id==id&&!session.process_only&&session.turn_key!=0&&matches!(session.status,Status::Error|Status::Idle|Status::Stopped)){visible.insert(0,session.clone());}
            }
        }
        let sessions=visible.as_slice();
        let snapshot=&self.agent_snapshot;
        if self.agent_expanded.as_ref().is_some_and(|id|!sessions.iter().any(|session|&session.id==id)){self.agent_expanded=None;}
        if self.agent_focus.as_ref().is_some_and(|id|!sessions.iter().any(|session|&session.id==id)){self.agent_focus=None;}
        if c.live&&!sessions.is_empty(){
            c.ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
        }
        let width=self.expanded_width();let height=self.expanded_height();
        let inset=crate::portrait::INSET;
        let left=if side{inset}else{44.0};
        let body_width=if side{width-inset*2.0}else{331.0};
        if side {c.centered_text(left,122.0,"当前任务",17.0,W::Semibold,theme::WHITE,H::Left);}
        let active=sessions.iter().filter(|session|matches!(session.status,Status::Thinking|Status::Working)).count();
        let waiting=sessions.iter().filter(|session|session.status==Status::Waiting).count();
        let approval=sessions.iter().filter(|session|session.status==Status::Approval).count();
        let completed=sessions.iter().filter(|session|session.status==Status::Completed).count();
        let summary=if snapshot.scanning&&sessions.is_empty(){"正在查看任务…".into()}
            else if approval>0{format!("{active} 个执行中 · {approval} 个需要审批")}
            else if waiting>0{format!("{active} 个执行中 · {waiting} 个等待处理")}
            else if active>0{format!("{active} 个任务执行中")}
            else if completed>0{"任务已完成".into()}
            else{"当前没有执行中的任务".into()};
        let summary_y=if side{148.0}else{55.0};
        c.centered_text(left,summary_y,&c.fit(&summary,11.5,W::Regular,body_width-if side{0.0}else{36.0}),11.5,W::Regular,theme::GRAY,H::Left);
        let refresh_y=if side{109.0}else{41.0};
        let refresh_clicked=header_button(c,left+body_width-28.0,refresh_y,"refresh","刷新本机任务状态","agents-refresh");
        let list_y=if side{169.0}else{76.0};
        let list_h=self.content_rect().bottom()-list_y;
        let mut clicked_session=None;
        let mut focused=false;
        if sessions.is_empty(){
            let (title,hint)=if let Some(error)=&snapshot.error{("暂时无法读取状态",one_line(error))}
                else if snapshot.scanning{("正在查看任务状态","状态会自动更新".into())}
                else{("暂无运行任务","开始新任务后会自动显示".into())};
            crate::portrait::empty(c,Rect::from_min_size(egui::pos2(left,list_y),egui::vec2(body_width,list_h)),"agent",title,&hint);
        }else{
            let mut ui=crate::portrait::scroll_ui(c,left,list_y,body_width,list_h,"agent-sessions");
            egui::ScrollArea::vertical().id_salt(("agent-sessions",side)).auto_shrink([false,false]).show(&mut ui,|ui|{
                let row_width=(ui.available_width()/c.s).max(1.0);
                    for session in sessions{
                        let expanded=self.agent_expanded.as_deref()==Some(session.id.as_str());
                        let (_,rect)=ui.allocate_space(egui::vec2(row_width*c.s,session_height(c,session,row_width,expanded)*c.s));
                        if c.live&&self.agent_focus.as_deref()==Some(session.id.as_str()) {
                            ui.scroll_to_rect(rect,Some(egui::Align::TOP));focused=true;
                        }
                        let mut row=Card::new(ui,rect.min,ui.clip_rect());row.s=c.s;row.live=c.live;
                        if session_row(&row,session,row_width,expanded,env.unix_time){clicked_session=Some(session.id.clone());}
                    }
            });
        }
        if focused{self.agent_focus=None;c.ui.ctx().request_repaint();}
        if let Some(id)=clicked_session{
            self.agent_expanded=if self.agent_expanded.as_ref()==Some(&id){None}else{Some(id)};
            c.ui.ctx().request_repaint();
        }
        if refresh_clicked{self.agents.refresh();}
    }

    pub(crate) fn agent_settings(&mut self,ui:&mut egui::Ui){
        use crate::extras::{setting_row,settings_card,settings_hint,settings_label};
        let snapshot=self.agent_snapshot.clone();
        settings_label(ui,"本机监测");
        settings_card(ui,|ui|{
            ui.horizontal(|ui|{
                let active=self.agent_tasks.sessions().iter().filter(|session|session.status.is_active()).count();
                ui.label(format!("当前 {active} 个运行任务"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    if ui.button("刷新状态").clicked(){self.agents.refresh();}
                });
            });
            if let Some(error)=&snapshot.error{ui.label(error);}
        });
        settings_label(ui,"岛内提醒");
        settings_card(ui,|ui|{
            setting_row(ui,"开始与中断提醒","包含等待与需要审批",&mut self.settings.agent_activity);
            setting_row(ui,"任务完成提醒","显示本轮总用时",&mut self.settings.agent_completion);
        });
        settings_label(ui,"宠物搭档");
        settings_card(ui,|ui|self.pet_binding_settings(ui));
        settings_label(ui,"执行状态集成");
        settings_card(ui,|ui|{
            for (index,integration) in snapshot.integrations.iter().enumerate(){
                if index>0{ui.separator();}
                let row=ui.horizontal(|ui|{
                    crate::agent_icons::ui(ui,integration.agent,22.0);
                    ui.label(integration.agent.label());
                    let status=if integration.ready{"已收到任务状态"}else if integration.installed{"集成已安装"}else if integration.detected{"待接入"}else{"未发现应用"};
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{ui.label(egui::RichText::new(status).size(11.0));});
                });
                row.response.on_hover_text(&integration.message);
            }
            ui.add_space(4.0);
            settings_hint(ui,"安装或移除后，重启对应智能体生效。");
            ui.horizontal_wrapped(|ui|{
                if ui.add_enabled(self.opts.monitor_agents&&!snapshot.integration_busy,egui::Button::new(if snapshot.integration_busy{"正在更新状态集成…"}else if snapshot.integrations_installed{"修复状态集成"}else{"安装状态集成"})).clicked(){self.agents.install_integrations();}
                if ui.add_enabled(self.opts.monitor_agents&&!snapshot.integration_busy&&snapshot.integrations_installed,egui::Button::new("移除状态集成")).clicked(){self.agents.uninstall_integrations();}
            });
            if let Some(message)=&snapshot.integration_message{ui.label(message);}
        });
        settings_label(ui,"隐私与后台运行");
        settings_card(ui,|ui|{
            settings_hint(ui,"状态在本机处理，不展示对话正文或工具参数。");
        });
    }
}
