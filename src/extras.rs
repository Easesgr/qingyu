//! 设置与待办、邮箱提醒。
use crate::settings_theme::color as settings_color;
use crate::app::{App, Cmd, PAGE_NAMES};
use crate::paint::{Card, H};
use crate::theme::{self, size, W};
use eframe::egui::{self, Color32, Rect};

pub(crate) const NOTICE_DURATION:f64=15.0;
pub(crate) const ATTENTION_NOTICE_DURATION:f64=20.0;

pub(crate) fn bar_tooltip(response:egui::Response,text:String)->egui::Response{
    let ctx=&response.ctx;let id=response.id.with("bar-tooltip");
    if response.hovered()&&!ctx.input(|i|i.pointer.any_down()){
        let now=ctx.input(|i|i.time);
        let since=ctx.data_mut(|data|*data.get_temp_mut_or_insert_with(id,||now));
        if now-since>=0.5{
            egui::Area::new(id).order(egui::Order::Tooltip).fixed_pos(response.rect.left_bottom()+egui::vec2(0.0,4.0)).interactable(false).show(ctx,|ui|{
                egui::Frame::popup(ui.style()).show(ui,|ui|{ui.label(&text);});
            });
        }else{ctx.request_repaint_after_secs((0.5-(now-since))as f32);}
    }else{ctx.data_mut(|data|data.remove::<f64>(id));}
    response
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AlertSource {
    Internal,
    Mail {account:String,uid:u32},
    Desktop {bundle_id:String,name:String,activation:String,key:String},
    Agent {session_id:String,turn_key:u64},
    AgentActivity {session_id:String,turn_key:u64},
    AgentEvent {session_id:String,turn_key:u64,kind:crate::agent_activity::NoticeKind},
    /// Approval can outlive the monitor's bounded journal. Retain only the
    /// verified activation metadata needed to reopen its original owner.
    AgentApproval {session_id:String,turn_key:u64,request_key:u64,pid:Option<u32>,observed_at_ms:Option<u64>},
    LocalTest,
}

impl AlertSource {
    pub(crate) fn approval_key(&self)->u64{if let Self::AgentApproval{request_key,..}=self{*request_key}else{0}}
    pub(crate) fn agent_notice(&self)->Option<(&str,u64,crate::agent_activity::NoticeKind)>{
        use crate::agent_activity::NoticeKind;
        match self{
            Self::AgentEvent{session_id,turn_key,kind}=>Some((session_id,*turn_key,*kind)),
            Self::AgentApproval{session_id,turn_key,..}=>Some((session_id,*turn_key,NoticeKind::Approval)),
            Self::Agent{session_id,turn_key}=>Some((session_id,*turn_key,NoticeKind::Completed)),
            Self::AgentActivity{session_id,turn_key}=>Some((session_id,*turn_key,NoticeKind::Started)),
            _=>None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Alert {
    pub title: String,
    pub source: AlertSource,
    pub body: String,
    pub page: usize,
    pub at: f64,
    pub duration: f64,
}

impl Alert {
    pub(crate) fn is_message_delivery(&self)->bool{matches!(self.source,AlertSource::Desktop{..}|AlertSource::Mail{..})}
    pub(crate) fn requires_acknowledgement(&self)->bool{
        self.source.agent_notice().is_some_and(|(_,_,kind)|kind==crate::agent_activity::NoticeKind::Approval)
    }
    pub(crate) fn delivery_key(&self)->Option<(String,String)>{
        match &self.source{
            AlertSource::Desktop{bundle_id,key,..}=>Some((format!("app:{}",bundle_id.to_ascii_lowercase()),key.clone())),
            AlertSource::Mail{account,uid}=>Some((format!("mail:{}",account.to_ascii_lowercase()),uid.to_string())),
            _=>None,
        }
    }
    pub(crate) fn same_delivery(&self,other:&Alert)->bool{
        if let (Some(old),Some(new))=(self.delivery_key(),other.delivery_key()){return old==new;}
        if let (Some(old),Some(new))=(self.source.agent_notice(),other.source.agent_notice()){
            return old==new&&self.source.approval_key()==other.source.approval_key();
        }
        self.source==other.source&&self.page==other.page&&self.title==other.title&&self.body==other.body
    }
}

pub(crate) fn notice_badge(c:&Card,x:f32,y:f32,a:&Alert){notice_icon(&c.scaled(x,y,0.85),x,y,a);}

fn notice_icon(c:&Card,x:f32,y:f32,a:&Alert){
    if let Some((session_id,_,_))=a.source.agent_notice() {
        if let Some(agent)=crate::agent_activity::agent_from_session_id(session_id){crate::agent_icons::paint(c,agent,x,y,22.0);}
        else{crate::paint::glyph(c,"agent",x,y,20.0,theme::CYAN);}
    }
    else if a.page==crate::app::TODO_PAGE{crate::paint::checklist(c,x,y,23.0,Color32::from_rgb(190,180,255),Color32::from_rgb(43,35,66));}
    else{crate::paint::bell(c,x,y,theme::WHITE,0.0);}
}

impl App {

    pub fn alert(&mut self,title:impl Into<String>,body:impl Into<String>,page:usize,system:bool){
        if !matches!(page,crate::app::TODO_PAGE|crate::app::MAIL_PAGE){return;}
        self.enqueue_alert(Alert{source:AlertSource::Internal,title:title.into(),body:body.into(),page,at:self.t,duration:NOTICE_DURATION},system);
    }
    pub(crate) fn enqueue_alert(&mut self, mut a: Alert, system: bool) {
        if !self.notification_allowed(&a){return;}
        if a.delivery_key().is_some_and(|key|self.notification_consumed.contains(&key)){return;}
        self.redact_notification(&mut a);
        if a.page==crate::app::MAIL_PAGE&&!self.settings.notification_preview {a.title="新邮件".into();a.body="收到一封新邮件，打开邮箱查看".into();}
        if matches!(a.source,AlertSource::Desktop{..}){
            // AX may publish the sender before its body is available. Refresh
            // that exact delivery in place, retaining its FIFO position and
            // reading clock, including in the cached transition surface.
            let mut refreshed=false;
            for old in self.history.iter_mut().chain(self.alerts.iter_mut()).chain(self.notice.iter_mut()).chain(self.hud_notice.iter_mut()){
                if !old.same_delivery(&a){continue;}
                let mut source=a.source.clone();
                if let (AlertSource::Desktop{name,activation,..},AlertSource::Desktop{activation:previous,..})=(&mut source,&old.source){
                    if a.title!=*name||old.title==*name{old.title=a.title.clone();}
                    if activation.is_empty(){*activation=previous.clone();}
                }
                if !a.body.trim().is_empty(){old.body=a.body.clone();}
                old.source=source;refreshed=true;
            }
            if refreshed{return;}
        }
        // Identity includes account + UID / the original AX notice identity;
        // consecutive equal messages from different notices remain distinct.
        let same_event=|old:&Alert|if old.source.agent_notice().is_some()&&a.source.agent_notice().is_some(){old.same_delivery(&a)}else{match (&old.source,&a.source) {
            (AlertSource::Mail{account:old_account,uid:old_uid},AlertSource::Mail{account,uid})=>old_account.eq_ignore_ascii_case(account)&&old_uid==uid,
            (AlertSource::Desktop{bundle_id:old_bundle,key:old_key,..},AlertSource::Desktop{bundle_id,key,..})=>old_bundle.eq_ignore_ascii_case(bundle_id)&&old_key==key,
            (AlertSource::Agent{session_id:old_id,turn_key:old_turn},AlertSource::Agent{session_id,turn_key})=>old_id==session_id&&old_turn==turn_key,
            _=>old.source==a.source&&old.title==a.title&&old.body==a.body&&old.at==a.at,
        }};
        if self.history.iter().chain(self.notice.iter()).chain(&self.alerts).any(same_event){return;}
        if let Some((id,turn,kind))=a.source.agent_notice(){
            use crate::agent_activity::NoticeKind;
            if kind!=NoticeKind::Started{
                // A later state replaces stale feedback from this same turn,
                // including a start popup held open by the pointer.
                let superseded=|old:&Alert|old.source.agent_notice().is_some_and(|(old_id,old_turn,old_kind)|{
                    old_id==id&&old_turn==turn&&(matches!(old_kind,NoticeKind::Started|NoticeKind::Waiting)
                        ||kind==NoticeKind::Completed&&old_kind==NoticeKind::Interrupted)
                });
                self.alerts.retain(|old|!superseded(old));
                if self.notice.as_ref().is_some_and(superseded){self.notice=None;}
            }
        }
        if a.source.agent_notice().is_none()||a.requires_acknowledgement(){
            self.history.push_front(a.clone());
        }
        if system && (a.source==AlertSource::Internal||a.source.agent_notice().is_some_and(|(_,_,kind)|kind==crate::agent_activity::NoticeKind::Completed)) {
            self.pending.push(Cmd::Notify(a.title.clone(), a.body.clone()));
        }
        self.alerts.retain(|old| !same_event(old));
        self.activity_notice=None;
        if !a.requires_acknowledgement()&&self.notice.as_ref().is_some_and(Alert::requires_acknowledgement){
            // Keep the request reachable while another event has its ordinary
            // display time. It returns after those messages have been shown.
            if let Some(approval)=self.notice.take(){self.alerts.push_front(approval);}
        }
        if a.source.agent_notice().is_none(){
            // A chat message takes over a brief start announcement, while
            // queued messages retain their original relative order.
            if self.notice.as_ref().is_some_and(|old|old.source.agent_notice().is_some_and(|(_,_,kind)|kind==crate::agent_activity::NoticeKind::Started)){self.notice=None;}
            let index=self.alerts.iter().position(|old|old.source.agent_notice().is_some_and(|(_,_,kind)|kind==crate::agent_activity::NoticeKind::Started)).unwrap_or(self.alerts.len());
            self.alerts.insert(index,a);
        }else{self.alerts.push_back(a);}
        let evictable=|old:&Alert|!old.requires_acknowledgement()&&!old.is_message_delivery();
        let mut transient=self.alerts.iter().filter(|old|evictable(old)).count();
        while transient>8{
            if let Some(index)=self.alerts.iter().position(evictable){self.alerts.remove(index);transient-=1;}else{break;}
        }
        self.trim_notification_history();
    }

    pub(crate) fn trim_notification_history(&mut self){
        if self.history.len()<=50{return;}
        let pending:std::collections::HashSet<_>=self.alerts.iter().chain(self.notice.iter())
            .filter(|alert|alert.is_message_delivery()).filter_map(Alert::delivery_key).collect();
        let mut recent=0;
        self.history.retain(|alert|{
            if alert.requires_acknowledgement()||alert.delivery_key().is_some_and(|key|pending.contains(&key)){return true;}
            recent+=1;recent<=50
        });
    }

    pub(crate) fn tick_alerts(&mut self){
        self.reconcile_notification_privacy();
        let pages=self.settings.visible_pages;
        let enabled=self.settings.notifications;
        let allowed=|a:&Alert|enabled && pages.get(a.page).copied().unwrap_or(false);
        self.alerts.retain(&allowed);
        // A queued delivery has its full reading interval after its own row
        // becomes readable, including after a delayed frame during handoff.
        if self.notice.as_ref().is_some_and(|a|!self.notice_is_presented(a)){
            if let Some(a)=self.notice.as_mut(){a.at=self.t;}
        }
        if self.activity_notice.as_ref().is_some_and(|a|!self.notice_is_presented(a)){
            if let Some(a)=self.activity_notice.as_mut(){a.at=self.t;}
        }
        let retired=self.notice.as_ref().is_some_and(|a|!allowed(a)||!a.requires_acknowledgement()&&self.t-a.at>a.duration);
        if retired{self.notice=None;}
        if self.activity_notice.as_ref().is_some_and(|a|!allowed(a)||!a.requires_acknowledgement()&&self.t-a.at>a.duration){self.activity_notice=None;}
        if self.hud_notice.as_ref().is_some_and(|a|!allowed(a)){self.hud_notice=None;}
        if self.notice.is_none(){
            let index=self.alerts.iter().position(|a|!a.requires_acknowledgement()).unwrap_or(0);
            if let Some(mut a)=self.alerts.remove(index){a.at=self.t;self.activity_notice=None;self.notice=Some(a);}
        }
        if retired{self.trim_notification_history();}
    }
}

/// 将本地提醒时间规范化，拒绝不存在的日期与超出范围的时分。
pub(crate) fn parse_due(value: &str, today: &str) -> Option<String> {
    let value = value.trim();
    let full = if value.len() == 5 {
        format!("{today} {value}")
    } else {
        value.to_string()
    };
    if !full.is_ascii() || full.len() != 16 {
        return None;
    }
    let b = full.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' {
        return None;
    }
    let year = full[0..4].parse::<u32>().ok()?;
    let month = full[5..7].parse::<usize>().ok()?;
    let day = full[8..10].parse::<u32>().ok()?;
    let hour = full[11..13].parse::<u32>().ok()?;
    let minute = full[14..16].parse::<u32>().ok()?;
    if !(2000..=9999).contains(&year) || !(1..=12).contains(&month) || hour > 23 || minute > 59 {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day == 0 || day > days[month - 1] {
        return None;
    }
    Some(full)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn island_size_drag_commits_only_on_release(){
        let ctx=egui::Context::default();let mut value=100;
        let pos=egui::pos2(30.0,30.0);let end=egui::pos2(110.0,30.0);
        let button=|pos,pressed|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()};
        let frames=[vec![],vec![egui::Event::PointerMoved(pos)],vec![button(pos,true)],vec![egui::Event::PointerMoved(end)],vec![button(end,false)]];
        for (i,events) in frames.into_iter().enumerate(){
            let _=ctx.run(egui::RawInput{events,screen_rect:Some(Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(320.0,80.0))),..Default::default()},|ctx|{
                egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(20)).show(ctx,|ui|{ui.spacing_mut().slider_width=180.0;island_size_slider(ui,&mut value);});
            });
            if i<4{assert_eq!(value,100);}else{assert_ne!(value,100);}
        }
    }
    #[test]
    fn number_stepper_clicks_follow_steps_and_limits() {
        for (initial,step,label,expected) in [(5u32,1,"+",6),(5,1,"−",4),(0,1,"−",0),(10,1,"+",10),(9,3,"+",10),(1,3,"−",0)] {
            let ctx=egui::Context::default();
            let mut value=initial;
            // Use a monotonic clock and an approaching pointer, as in the
            // real settings window, before pressing either button.
            let mut time=1.0;
            let mut frame=|events|{
                time+=1.0/60.0;
                let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(200.0,80.0))),time:Some(time),events,..Default::default()};
                ctx.run(raw,|ctx|{
                    egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(20)).show(ctx,|ui|{
                        ui.horizontal(|ui|{number_stepper(ui,&mut value,0,10,"",step,76.0);});
                    });
                })
            };
            // Follow the rendered controls rather than assuming their vertical
            // alignment and font metrics in the settings row.
            let output=frame(vec![egui::Event::PointerMoved(egui::pos2(199.0,79.0))]);
            frame(vec![egui::Event::PointerMoved(egui::pos2(190.0,79.0))]);
            let pos=output.shapes.iter().find_map(|shape|match &shape.shape{
                egui::Shape::Text(text) if text.galley.text()==label=>Some(text.visual_bounding_rect().center()),_=>None,
            }).expect("stepper button must be visible");
            for events in [vec![egui::Event::PointerMoved(pos)],vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}],vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]] {
                frame(events);
            }
            assert_eq!(value,expected,"初值 {initial}，步长 {step}，点击按钮 {label}");
        }
    }

    #[test]
    fn number_stepper_fits_with_settings_button_padding() {
        let ctx=egui::Context::default();
        let raw=egui::RawInput{screen_rect:Some(Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(340.0,240.0))),..Default::default()};
        let _=ctx.run(raw,|ctx|{
            egui::CentralPanel::default().frame(egui::Frame::new()).show(ctx,|ui|{
                ui.spacing_mut().button_padding=egui::vec2(10.0,6.0);
                for mut value in [0u32,25,180] {
                    let row=ui.horizontal(|ui|{number_stepper(ui,&mut value,0,180," 分钟",1,76.0);});
                    assert!(row.response.rect.width()<=140.1,"数值控件超出预留宽度：{}",row.response.rect.width());
                }
                for mut value in [25u32,5,15] {setting_number(ui,"计时时长",&mut value,1,180," 分钟",1.0);}
                assert!(ui.min_rect().right()<=340.1,"连续数值行不应撑宽卡片");
            });
        });
    }

    #[test]
    fn settings_groups_keep_every_page_reachable(){
        let tabs:Vec<_>=SETTINGS_GROUPS.iter().map(|(_,start,end)|{assert_eq!(start,end);*start}).collect();
        assert_eq!(tabs,vec![0,10,1,5,14,4,7,12,13,15,16]);
        for tab in crate::app::PAGE_SETTINGS_TABS{assert_eq!(tabs.iter().filter(|t|**t==tab).count(),1);}
        for removed in [2,3,6,8,9,11]{assert!(!tabs.contains(&removed));}
    }

    #[test]
    fn due_time_checks_date_and_format() {
        assert_eq!(
            parse_due("09:30", "2026-10-29").as_deref(),
            Some("2026-10-29 09:30")
        );
        assert!(parse_due("2026-02-29 12:00", "").is_none());
        assert!(parse_due("2028-02-29 12:00", "").is_some());
        for value in [
            "24:00",
            "12:60",
            "明天中午",
            "2026-13-01 10:00",
            "2026-00-01 10:00",
        ] {
            assert!(parse_due(value, "2026-10-29").is_none());
        }
    }
}

pub(crate) struct TaskDraft {index:usize,original:String,text:String,due:String,error:String,focused:bool,reminder_only:bool,pub(crate) repeat:u8,pub(crate) picker:crate::task_picker::ReminderPicker}
impl TaskDraft{pub(crate) fn accepts_text(&self)->bool{!self.reminder_only&&!self.picker.open}}
impl App {
    pub(crate) fn open_task_dialog(&mut self,index:usize){
        self.todo_choosing=false;
        if let Some(task)=self.todos.items.get(index){
            let draft=TaskDraft{index,original:task.text.clone(),text:task.text.clone(),due:task.due.clone().unwrap_or_default(),error:String::new(),focused:false,reminder_only:false,repeat:task.repeat,picker:crate::task_picker::ReminderPicker::new(task.due.as_deref())};
            self.show_page(crate::app::TODO_PAGE);self.task_dialog=Some(draft);
            self.library_view.legacy_tasks=true;
        }
    }
    pub(crate) fn open_task_reminder(&mut self,index:usize){
        self.open_task_dialog(index);
        if let Some(d)=self.task_dialog.as_mut(){d.reminder_only=true;d.picker.show();}
    }
    pub(crate) fn draw_task_editor(&mut self,c:&Card){
        if self.task_dialog.as_ref().is_some_and(|draft|draft.picker.open){self.draw_task_reminder(c);return;}
        let reminder_only=self.task_dialog.as_ref().is_some_and(|d|d.reminder_only);
        c.text(15.0,25.0,if reminder_only{"设置提醒"}else{"编辑待办"},size::TITLE,W::Bold,theme::WHITE,H::Left);
        c.text(43.0,48.0,if reminder_only{"待办"}else{"内容"},10.5,W::Regular,theme::GRAY,H::Left);
        c.fill(91.0,33.0,275.0,20.0,6.0,theme::CARD);
        if let Some(d)=self.task_dialog.as_ref().filter(|d|d.reminder_only){
            c.text(97.0,47.0,&c.fit(&d.text,11.5,W::Regular,260.0),11.5,W::Regular,theme::WHITE,H::Left);
        }
        c.text(43.0,72.0,"提醒",10.5,W::Regular,theme::GRAY,H::Left);
        let time=self.task_dialog.as_ref().map(|d|if d.due.is_empty(){"点击选择时间".into()}
            else if d.repeat!=0{format!("{} {}",crate::model::repeat_label(d.repeat),d.due.get(11..16).unwrap_or(&d.due))}else{d.due.clone()}).unwrap_or_default();
        if setting_button(c,91.0,58.0,275.0,&time,false,"task-select-reminder"){if let Some(d)=self.task_dialog.as_mut(){d.picker.show();}}
        // 每周重复：七个星期按钮，全不选即一次性待办。
        c.text(43.0,95.0,"重复",10.5,W::Regular,theme::GRAY,H::Left);
        self.task_repeat_chips(c,91.0,81.0,25.0,19.0,28.0);
        let repeat=self.task_dialog.as_ref().map(|d|d.repeat).unwrap_or(0);
        let quick=if repeat==crate::model::REPEAT_ALL{"不重复"}else{"每天"};
        if setting_button(c,289.0,81.0,77.0,quick,false,"task-repeat-all"){
            if let Some(d)=self.task_dialog.as_mut(){d.repeat=if d.repeat==crate::model::REPEAT_ALL{0}else{crate::model::REPEAT_ALL};}
        }
        if let Some(d)=&self.task_dialog {
            let summary=if d.repeat!=0{crate::model::repeat_label(d.repeat)}else{"只做一次".into()};
            let hint=if d.error.is_empty(){summary.as_str()}else{&d.error};
            c.text(43.0,117.0,&c.fit(hint,10.5,W::Regular,if reminder_only{140.0}else{154.0}),10.5,if d.error.is_empty(){W::Regular}else{W::Semibold},if d.error.is_empty(){theme::GRAY}else{theme::RED},H::Left);
        }
        if reminder_only {
            let has_reminder=self.task_dialog.as_ref().and_then(|d|self.todos.items.get(d.index)).is_some_and(|task|task.due.is_some());
            let skip=if has_reminder{"取消提醒"}else if repeat!=0{"只设重复"}else{"暂不提醒"};
            if setting_button(c,188.0,104.0,88.0,skip,false,"task-reminder-skip"){
                if let Some(d)=self.task_dialog.as_mut(){d.due.clear();}
                self.save_task_editor();
            }
        }else if setting_button(c,202.0,104.0,74.0,"取消",false,"task-cancel"){self.task_dialog=None;}
        if setting_button(c,286.0,104.0,80.0,if reminder_only{"选择时间"}else{"保存"},true,"task-save"){
            if reminder_only{if let Some(d)=self.task_dialog.as_mut(){d.picker.show();}}else{self.save_task_editor();}
        }
    }
    pub(crate) fn task_editor_inputs(&mut self,ui:&mut egui::Ui,origin:egui::Pos2,cmds:&mut Vec<Cmd>){
        let side=self.side();let side_width=self.expanded_width();
        let Some(d)=self.task_dialog.as_mut() else{return};
        if d.reminder_only || d.picker.open{return;}
        let rect=if side{Rect::from_min_size(origin+egui::vec2(32.0,177.0),egui::vec2(side_width-64.0,23.0))}else{Rect::from_min_size(origin+egui::vec2(97.0,35.0),egui::vec2(260.0,17.0))};
        let name=ui.put(rect,egui::TextEdit::singleline(&mut d.text).vertical_align(egui::Align::Center).id(egui::Id::new("task-name-edit")).frame(false).font(theme::font(if side{12.0}else{11.5},W::Regular)));
        let first_focus=!d.focused;
        if first_focus{name.request_focus();d.focused=true;cmds.push(Cmd::Focus);}
        // 新建内容与下一步共用这一帧，避免同一次回车又提交了空提醒。
        if !first_focus && ui.input(|i|i.key_pressed(egui::Key::Enter)){self.save_task_editor();}
    }

    pub(crate) fn draw_portrait_task_editor(&mut self,c:&Card){
        use crate::portrait::button;
        let width=self.expanded_width()-40.0;
        if self.task_dialog.as_ref().is_some_and(|d|d.picker.open){
            use crate::task_picker::PickerAction;
            let Some(d)=self.task_dialog.as_mut() else{return;};
            let has=self.todos.items.get(d.index).is_some_and(|item|item.due.is_some());
            d.picker.repeat_note=Some(if d.repeat!=0{format!("{} · 到点提醒",crate::model::repeat_label(d.repeat))}else{String::new()});
            let action=d.picker.draw_portrait(c,&d.text,has);
            let focus=std::mem::take(&mut d.picker.focus);
            self.task_repeat_chips(c,20.0,397.0,30.0,28.0,36.0);
            if focus{self.pending.push(Cmd::Focus);}
            let Some(d)=self.task_dialog.as_mut() else{return;};
            match action{
                Some(PickerAction::Save)=>{d.due=d.picker.due();d.picker.open=false;if d.reminder_only{self.save_task_editor();}},
                Some(PickerAction::Skip)=>{d.due.clear();d.picker.open=false;if d.reminder_only{self.save_task_editor();}},
                Some(PickerAction::Cancel)=>{d.picker.open=false;if d.reminder_only{self.task_dialog=None;}},
                None=>{}
            }
            return;
        }
        let Some(d)=self.task_dialog.as_ref() else{return;};
        let reminder_only=d.reminder_only;
        c.text(20.0,137.0,if reminder_only{"设置提醒"}else{"编辑待办"},17.0,W::Semibold,theme::WHITE,H::Left);
        c.fill(20.0,169.0,width,39.0,10.0,theme::CARD);
        c.text(20.0,160.0,"内容",11.0,W::Regular,theme::GRAY,H::Left);
        if reminder_only{c.text(32.0,194.0,&c.fit(&d.text,12.0,W::Regular,width-24.0),12.0,W::Regular,theme::WHITE,H::Left);}
        let due=if d.due.is_empty(){"选择提醒时间".to_owned()}else{d.due.clone()};
        let repeat=d.repeat;
        let hint=if d.error.is_empty(){if repeat!=0{crate::model::repeat_label(repeat)}else{"只做一次".to_owned()}}else{d.error.clone()};
        c.text(20.0,242.0,"提醒",11.0,W::Regular,theme::GRAY,H::Left);
        if button(c,20.0,252.0,width,&due,"task-select-reminder"){if let Some(d)=self.task_dialog.as_mut(){d.picker.show();}}
        c.text(20.0,321.0,"每周重复",11.0,W::Regular,theme::GRAY,H::Left);
        self.task_repeat_chips(c,20.0,334.0,30.0,28.0,36.0);
        if button(c,192.0,374.0,80.0,if repeat==crate::model::REPEAT_ALL{"不重复"}else{"每天"},"task-repeat-all"){
            if let Some(d)=self.task_dialog.as_mut(){d.repeat=if repeat==crate::model::REPEAT_ALL{0}else{crate::model::REPEAT_ALL};}
        }
        c.text(20.0,423.0,&c.fit(&hint,11.0,W::Regular,252.0),11.0,W::Regular,theme::GRAY,H::Left);
        if button(c,20.0,470.0,112.0,if reminder_only{"暂不提醒"}else{"取消"},"task-cancel"){
            if reminder_only{if let Some(d)=self.task_dialog.as_mut(){d.due.clear();}self.save_task_editor();}else{self.task_dialog=None;}
        }
        if button(c,160.0,470.0,112.0,if reminder_only{"选择时间"}else{"保存"},"task-save"){
            if reminder_only{if let Some(d)=self.task_dialog.as_mut(){d.picker.show();}}else{self.save_task_editor();}
        }
    }
    /// 七个星期按钮（一…日），点一下切换；全不选即一次性待办。
    fn task_repeat_chips(&mut self,c:&Card,x:f32,y:f32,w:f32,h:f32,step:f32){
        let repeat=self.task_dialog.as_ref().map(|d|d.repeat).unwrap_or(0);
        for (day,name) in crate::model::WEEKDAY_NAMES.iter().enumerate() {
            let on=repeat&(1<<day)!=0;
            let cx=x+day as f32*step;
            let (r,b)=crate::paint::capsule(c,cx,y,w,h,&format!("task-repeat-{day}"),if on{Color32::from_rgb(39,94,150)}else{theme::BTN},theme::BTN_HOVER);
            b.centered_text(cx+w/2.0,y+h/2.0,name,if h<18.0{9.5}else{10.5},W::Semibold,if on{theme::WHITE}else{theme::GRAY},H::Center);
            if r.on_hover_text(format!("每周{name}")).clicked(){
                if let Some(d)=self.task_dialog.as_mut(){d.repeat^=1<<day;d.error.clear();}
            }
        }
    }
    fn draw_task_reminder(&mut self,c:&Card){
        use crate::task_picker::PickerAction;
        let Some(d)=self.task_dialog.as_mut().filter(|d|d.picker.open) else{return};
        let has_reminder=self.todos.items.get(d.index).is_some_and(|task|task.due.is_some());
        c.text(15.0,25.0,"待办",size::TITLE,W::Bold,theme::WHITE,H::Left);
        c.text(44.0,25.0,"清单",size::TITLE,W::Bold,theme::GREEN,H::Left);
        let (done,total)=self.todos.progress(&self.today);
        c.text(42.0,61.0,&format!("{done}/{total}"),29.0,W::Bold,theme::WHITE,H::Left);
        // 左侧：每周重复。选了星期几后，提醒只按时间、在这些日子到点提醒。
        d.picker.repeat_note=Some(if d.repeat!=0{format!("{} · 到点提醒",crate::model::repeat_label(d.repeat))}else{String::new()});
        let action=d.picker.draw(c,&d.text,has_reminder);
        let focus=std::mem::take(&mut d.picker.focus);
        self.task_repeat_chips(c,41.0,84.0,17.0,16.0,19.5);
        let Some(d)=self.task_dialog.as_mut() else{return};
        if focus{self.pending.push(Cmd::Focus);}
        match action{
            Some(PickerAction::Save)=>{d.due=d.picker.due();d.picker.open=false;if d.reminder_only{self.save_task_editor();}},
            Some(PickerAction::Skip)=>{d.due.clear();d.picker.open=false;if d.reminder_only{self.save_task_editor();}},
            Some(PickerAction::Cancel)=>{d.picker.open=false;if d.reminder_only{self.task_dialog=None;}},
            None=>{},
        }
    }
    fn save_task_editor(&mut self){
        let Some(mut d)=self.task_dialog.take() else{return};
        let due=if d.due.trim().is_empty(){Some(None)}else{parse_due(&d.due,&self.today).map(Some)};
        if d.text.trim().is_empty(){d.error="请填写任务内容".into();}
        else if due.is_none(){d.error="时间格式应为 18:30 或完整日期时间".into();}
        else if self.todos.items.get(d.index).is_none_or(|t|t.text!=d.original){d.error="任务已变化，请取消后重新打开".into();}
        else{
            let item=&mut self.todos.items[d.index];
            if item.repeat!=d.repeat{item.done_on=None;}
            item.text=d.text.trim().into();item.due=due.unwrap();item.reminded=false;item.reminded_on=None;item.repeat=d.repeat;
            self.todos.save();self.say("待办已保存");return;
        }
        self.task_dialog=Some(d);
    }
}

fn setting_button(c:&Card,x:f32,y:f32,w:f32,text:&str,on:bool,id:&str)->bool {
    let (r,b)=crate::paint::capsule(c,x,y,w,19.0,id,if on{Color32::from_rgb(39,94,150)}else{theme::BTN},theme::BTN_HOVER);
    b.text(x+w/2.0,y+13.3,text,10.5,W::Semibold,theme::WHITE,H::Center);
    r.clicked()
}

/// 岛内小开关（与健康页右上开关同款）。

/// 加减器：[−] 数值 [+]，右缘对齐到 right。返回 −1 / 0 / +1。

fn switch(ui:&mut egui::Ui,value:&mut bool)->bool {switch_named(ui,value,"")}
fn switch_named(ui:&mut egui::Ui,value:&mut bool,label:&str)->bool {
    let (hit,mut response)=ui.allocate_exact_size(egui::vec2(34.0,if cfg!(target_os="macos"){28.0}else{20.0}),egui::Sense::click());
    let rect=egui::Rect::from_center_size(hit.center(),egui::vec2(32.0,19.0));
    response.widget_info(||egui::WidgetInfo::selected(egui::WidgetType::Checkbox,ui.is_enabled(),*value,label));
    if response.clicked(){*value=!*value;response.mark_changed();}
    let fill=if *value{#[cfg(target_os="macos")]{crate::mac_design::palette(ui.ctx()).accent}#[cfg(not(target_os="macos"))]{Color32::from_rgb(48,184,92)}}else{settings_color(&ui.ctx().clone(),Color32::from_rgb(82,82,87),Color32::from_rgb(203,207,214))};
    ui.painter().rect_filled(rect,10.0,fill);
    if response.has_focus(){ui.painter().rect_stroke(hit.expand(2.0),6.0,egui::Stroke::new(2.0,ui.visuals().hyperlink_color),egui::StrokeKind::Inside);}
    let x=if *value{rect.right()-10.0}else{rect.left()+10.0};
    ui.painter().circle_filled(egui::pos2(x,rect.center().y+0.5),7.5,Color32::from_black_alpha(35));
    ui.painter().circle_filled(egui::pos2(x,rect.center().y),7.0,Color32::WHITE);
    response.clicked()
}
/// 设置页版式：标签列宽、开关尺寸、控件与卡片右边缘之间的固定留白。
pub(crate) const SETTINGS_LABEL_W: f32 = 80.0;
const SETTINGS_SWITCH_W: f32 = 34.0;
const SETTINGS_ROW_GAP: f32 = 16.0;

/// 开关行：左侧标题 + 说明（宽度固定、自动换行），右侧开关，长说明也不会把开关挤出卡片。
pub(crate) fn setting_row(ui:&mut egui::Ui,title:&str,description:&str,value:&mut bool){
    let ctx=&ui.ctx().clone();
    ui.add_space(2.0);
    ui.horizontal(|ui|{
        let text_w=(ui.available_width()-SETTINGS_SWITCH_W-SETTINGS_ROW_GAP).max(120.0);
        ui.vertical(|ui|{
            ui.set_width(text_w);
            ui.spacing_mut().item_spacing.y=2.0;
            ui.add(egui::Label::new(egui::RichText::new(title).size(13.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41)))).wrap());
            if !description.is_empty(){ui.add(egui::Label::new(egui::RichText::new(description).size(11.5).color(settings_color(ctx,Color32::from_gray(175),Color32::from_gray(90)))).wrap());}
        });
        // add_space 之后还会再插入一次 item_spacing.x，这里一并扣掉，开关才不会每行往外挤一点。
        ui.add_space((ui.available_width()-SETTINGS_SWITCH_W-ui.spacing().item_spacing.x).max(0.0));
        switch_named(ui,value,title);
    });
    ui.add_space(2.0);
}

/// 设置页统一的分段选择器（灰色底框 + 选中块）。返回值为是否发生了切换。
pub(crate) fn settings_segmented<T:PartialEq+Copy>(ui:&mut egui::Ui,value:&mut T,options:&[(T,&str)],item_w:f32)->bool{
    let ctx=&ui.ctx().clone();
    let mut changed=false;
    egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(28,28,30),Color32::from_rgb(230,233,238)))
        .corner_radius(8.0).inner_margin(3).show(ui,|ui|{
            ui.horizontal(|ui|{
                ui.spacing_mut().item_spacing.x=2.0;
                for (option,label) in options {
                    let selected=*value==*option;
                    let text=egui::RichText::new(*label).size(12.0).color(if selected{settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))}else{settings_color(ctx,Color32::from_gray(200),Color32::from_gray(70))});
                    let button=egui::Button::new(text)
                        .fill(if selected{settings_color(ctx,Color32::from_rgb(72,72,76),Color32::WHITE)}else{Color32::TRANSPARENT})
                        .stroke(egui::Stroke::NONE).corner_radius(6.0);
                    let width=item_w.max(ctx.fonts(|f|f.layout_no_wrap(label.to_string(),egui::FontId::proportional(12.0),Color32::WHITE).size().x)+20.0);
                    if ui.add_sized([width,26.0],button).clicked() && !selected {*value=*option;changed=true;}
                }
            });
        });
    changed
}

/// 选项行：标签列之后紧跟控件，同一卡片里所有控件左缘对齐成一条竖线。
pub(crate) fn settings_control_row_right(ui:&mut egui::Ui,title:&str,controls:impl FnOnce(&mut egui::Ui))->egui::Response{
    settings_control_row(ui,title,SETTINGS_LABEL_W,|ui|ui.horizontal(|ui|controls(ui)).response)
}

/// 设置项下方的说明文字。
fn island_size_slider(ui:&mut egui::Ui,value:&mut u32)->egui::Response {
    let id=ui.make_persistent_id("island-size-draft");
    let mut draft=ui.data(|d|d.get_temp::<u32>(id)).unwrap_or(*value);
    let response=ui.add(egui::Slider::new(&mut draft,crate::model::ISLAND_SIZE_MIN..=crate::model::ISLAND_SIZE_MAX)
        .step_by(crate::model::ISLAND_SIZE_STEP as f64).suffix("%"));
    if ui.input(|i|i.pointer.primary_down()) {
        if response.changed(){ui.data_mut(|d|d.insert_temp(id,draft));}
    }else{
        *value=crate::model::island_size_value(draft);
        ui.data_mut(|d|d.remove::<u32>(id));
    }
    response
}

pub(crate) fn settings_hint(ui:&mut egui::Ui,text:&str){
    let ctx=&ui.ctx().clone();
    ui.add(egui::Label::new(egui::RichText::new(text).size(11.5).color(settings_color(ctx,Color32::from_gray(160),Color32::from_gray(105)))).wrap());
}
pub(crate) fn settings_card(ui:&mut egui::Ui,add:impl FnOnce(&mut egui::Ui)) {
    let ctx=&ui.ctx().clone();
    egui::Frame::new().fill({#[cfg(target_os="macos")]{crate::mac_design::palette(ctx).group}#[cfg(not(target_os="macos"))]{settings_color(ctx,Color32::from_rgb(35,35,38),Color32::from_rgb(255,255,255))}})
        .corner_radius(10.0).inner_margin(egui::Margin::symmetric(16,12))
        .show(ui,|ui|{ui.set_min_width(ui.available_width());ui.spacing_mut().item_spacing.y=if cfg!(target_os="macos"){6.0}else{8.0};add(ui);});
}
/// 设置分页对应的功能模块；岛屿外观、功能编排不属于任何模块。
pub(crate) fn settings_tab_module(tab:usize)->Option<usize>{
    use crate::app::*;
    match tab {1=>Some(MUSIC_PAGE),4=>Some(TODO_PAGE),5=>Some(SHELF_PAGE),14=>Some(CLIPBOARD_PAGE),7=>Some(MAIL_PAGE),12=>Some(WEATHER_PAGE),13=>Some(AGENT_PAGE),15=>Some(MAC_STATS_PAGE),16=>Some(crate::app::NOTIFICATIONS_PAGE),_=>None}
}

pub(crate) fn settings_label(ui:&mut egui::Ui,text:&str){
    let ctx=&ui.ctx().clone();
    ui.add_space(if cfg!(target_os="macos"){16.0}else{9.0});
    ui.label(egui::RichText::new(text).size(if cfg!(target_os="macos"){12.0}else{11.5}).color(settings_color(ctx,Color32::from_gray(185),Color32::from_gray(70))));
    ui.add_space(2.0);
}

pub(crate) fn settings_control_row(ui:&mut egui::Ui,title:&str,label_width:f32,controls:impl FnOnce(&mut egui::Ui)->egui::Response)->egui::Response{
    ui.horizontal(|ui|{
        // 同一张卡片里的控件左缘对齐：标签列至少 SETTINGS_LABEL_W。
        let (label_rect,response)=ui.allocate_exact_size(egui::vec2(label_width.max(SETTINGS_LABEL_W),30.0),egui::Sense::hover());
        response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Label,ui.is_enabled(),title));
        let content=controls(ui);
        ui.painter().text(egui::pos2(label_rect.left(),content.rect.center().y),egui::Align2::LEFT_CENTER,title,egui::TextStyle::Body.resolve(ui.style()),ui.visuals().text_color());
        content
    }).inner
}
fn setting_number<N:egui::emath::Numeric>(ui:&mut egui::Ui,title:&str,value:&mut N,min:N,max:N,suffix:&str,speed:f64)->bool{
    let mut changed=false;
    settings_control_row(ui,title,80.0,|ui|{
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
            ui.allocate_ui_with_layout(egui::vec2(140.0,30.0),egui::Layout::left_to_right(egui::Align::Center),|ui|{
                changed=number_stepper(ui,value,min,max,suffix,N::from_f64(speed),76.0);
            });
        }).response
    });
    changed
}
fn number_stepper<N:egui::emath::Numeric>(ui:&mut egui::Ui,value:&mut N,min:N,max:N,suffix:&str,step:N,field_width:f32)->bool{
    let ctx=&ui.ctx().clone();
    let mut changed=false;
    ui.scope(|ui|{
        ui.spacing_mut().item_spacing.x=6.0;
        ui.spacing_mut().button_padding=egui::vec2(4.0,2.0);
        let button=|text|egui::Button::new(egui::RichText::new(text).size(16.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))))
            .fill(settings_color(ctx,Color32::from_rgb(48,48,51),Color32::from_rgb(229,232,238))).stroke(egui::Stroke::NONE).corner_radius(13.0);
        let decrease=ui.add_enabled(*value>min,button("−").min_size(egui::vec2(26.0,26.0))).on_hover_text("减少");
        if decrease.clicked(){
            *value=N::from_f64((value.to_f64()-step.to_f64()).max(min.to_f64()));changed=true;
        }
        let field=ui.add_sized([field_width,28.0],egui::DragValue::new(value).range(min..=max).speed(step.to_f64()).suffix(suffix).fixed_decimals(if N::INTEGRAL{0}else{2}));
        changed|=field.changed();
        if ui.add_enabled(*value<max,button("+").min_size(egui::vec2(26.0,26.0))).on_hover_text("增加").clicked(){
            *value=N::from_f64((value.to_f64()+step.to_f64()).min(max.to_f64()));changed=true;
        }
    });
    changed
}
/// 时刻加减器（一天中的分钟数，15 分钟一档，跨午夜循环），中间可直接输入 12:30。

fn setting_choices(ui:&mut egui::Ui,title:&str,value:&mut u32,values:[u32;3],unit:&str){
    let labels=values.map(|v|format!("{v} {unit}"));
    let options:Vec<(u32,&str)>=values.iter().zip(labels.iter()).map(|(v,l)|(*v,l.as_str())).collect();
    settings_control_row_right(ui,title,|ui|{settings_segmented(ui,value,&options,62.0);});
}
pub(crate) const SETTINGS_GROUPS: [(&str,usize,usize);11] = [
    ("岛屿外观",0,0), ("功能编排",10,10), ("聆听",1,1), ("暂存架",5,5), ("剪贴簿",14,14),
    ("拾光",4,4), ("信箱",7,7), ("晴雨",12,12), ("智能体",13,13), ("本机概览",15,15), ("消息",16,16),
];

pub(crate) fn settings_section_name(tab:usize)->&'static str {
    SETTINGS_GROUPS.iter().find(|(_,start,end)|(*start..=*end).contains(&tab))
        .map(|(label,_,_)|*label).unwrap_or(SETTINGS_GROUPS[0].0)
}

impl App {
    pub fn settings_ui(&mut self,ctx:&egui::Context,autostart:bool){
        if !self.settings_open{self.window_layout().close();}
        if !self.settings_open{return;}
        if !SETTINGS_GROUPS.iter().any(|(_,start,end)|(*start..=*end).contains(&self.settings_tab)){self.settings_tab=0;}
        let id=egui::ViewportId::from_hash_of("qingyu-settings");
        let focus_requested=std::mem::take(&mut self.settings_focus_requested);
        let opening=self.window_layout().open();
        ctx.show_viewport_immediate(id,egui::ViewportBuilder::default().with_title("轻屿设置").with_icon(theme::app_icon()).with_inner_size(if cfg!(target_os="macos"){[780.0,620.0]}else{[690.0,560.0]}).with_min_inner_size(if cfg!(target_os="macos"){[650.0,520.0]}else{[600.0,450.0]}).with_visible(false).with_active(false).with_window_level(egui::WindowLevel::Normal),|ctx,_|{
            let restored=opening && self.window_layout().restore(ctx);
            if !restored{self.window_layout().observe(ctx);}
            let dark=if cfg!(target_os="macos"){crate::settings_theme::system_dark(ctx)}else{self.settings.settings_theme.dark(crate::settings_theme::system_dark(ctx))};
            let original_style=ctx.style();
            let mut style=(*original_style).clone();
            style.visuals=if dark{egui::Visuals::dark()}else{egui::Visuals::light()};
            #[cfg(target_os="macos")] crate::mac_design::style(ctx,&mut style);
            ctx.set_style(style);
            ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(if dark{egui::SystemTheme::Dark}else{egui::SystemTheme::Light}));
            if self.settings.settings_theme==crate::settings_theme::SettingsTheme::System {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
            if focus_requested {
                // Glow 在首帧交换缓冲后执行窗口命令，避免先显示白色空窗口。
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            if ctx.input(|i|i.viewport().close_requested()){self.settings_open=false;}
            let before=format!("{:?}",self.settings);
            egui::SidePanel::left("settings-sidebar").resizable(false).exact_width(180.0).frame(egui::Frame::new().fill({#[cfg(target_os="macos")]{crate::mac_design::palette(ctx).sidebar}#[cfg(not(target_os="macos"))]{settings_color(ctx,Color32::from_rgb(29,29,32),Color32::from_rgb(237,239,243))}}).stroke(egui::Stroke::new(1.0,settings_color(ctx,Color32::from_rgb(42,42,45),Color32::from_rgb(216,219,225)))).inner_margin(egui::Margin::symmetric(10,12))).show(ctx,|ui|{
                #[cfg(target_os="macos")]
                crate::mac_design::sidebar(ui,&mut self.settings_tab);
                #[cfg(not(target_os="macos"))]
                {
                ui.add_space(5.0);
                ui.label(egui::RichText::new("设置").size(11.0).color(settings_color(ctx,Color32::from_gray(150),Color32::from_gray(105))));
                ui.add_space(7.0);
                for (name,start,end) in SETTINGS_GROUPS {
                    let selected=(start..=end).contains(&self.settings_tab);
                    let (rect,response)=ui.allocate_exact_size(egui::vec2(ui.available_width(),36.0),egui::Sense::click());
                    if response.clicked() && !selected{self.settings_tab=start;}
                    if selected || response.hovered(){
                        ui.painter().rect_filled(rect,7.0,if selected{settings_color(ctx,Color32::from_rgb(43,43,46),Color32::from_rgb(214,221,232))}else{settings_color(ctx,Color32::from_rgb(30,30,33),Color32::from_rgb(225,230,238))});
                    }
                    let icon=match start {0=>"settings",10=>"pages",1=>"music",5=>"shelf",14=>"clipboard",4=>"link",7=>"mail",12=>"weather",13=>"agent",15=>"mac-stats",16=>"bell",_=>"settings"};
                    let icon_bg=egui::Rect::from_center_size(egui::pos2(rect.left()+21.0,rect.center().y),egui::vec2(25.0,25.0));
                    if icon=="mac-stats"{crate::paint::stats_symbol(ui.painter(),icon_bg.center(),25.0,Color32::WHITE);}
                    else if let Some(icon)=theme::glyph(ctx,icon){ui.painter().image(icon.id(),icon_bg,Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.0,1.0)),Color32::WHITE);}
                    ui.painter().text(egui::pos2(rect.left()+40.0,rect.center().y),egui::Align2::LEFT_CENTER,name,egui::FontId::proportional(13.0),if selected{settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))}else{settings_color(ctx,Color32::from_gray(225),Color32::from_gray(55))});
                    ui.add_space(3.0);
                }
                }

            });
            egui::CentralPanel::default().frame(egui::Frame::new().fill({#[cfg(target_os="macos")]{crate::mac_design::palette(ctx).window}#[cfg(not(target_os="macos"))]{settings_color(ctx,Color32::from_rgb(25,25,27),Color32::from_rgb(246,247,249))}}).inner_margin(egui::Margin::symmetric(22,16))).show(ctx,|ui|{
                ui.spacing_mut().item_spacing=egui::vec2(8.0,5.0);
                ui.spacing_mut().button_padding=egui::vec2(10.0,6.0);
                ui.style_mut().text_styles.insert(egui::TextStyle::Small,egui::FontId::proportional(12.0));
                ui.visuals_mut().override_text_color=Some(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41)));
                ui.visuals_mut().extreme_bg_color=settings_color(ctx,Color32::from_rgb(25,25,28),Color32::from_rgb(244,246,249));
                ui.visuals_mut().widgets.inactive.bg_fill=settings_color(ctx,Color32::from_rgb(50,50,54),Color32::from_rgb(228,231,236));
                ui.visuals_mut().widgets.hovered.bg_fill=settings_color(ctx,Color32::from_rgb(65,65,71),Color32::from_rgb(216,222,231));
                // 统一按钮：高度 28、圆角 7、内边距 12×5，无描边；默认按钮也套用这套样式。
                ui.spacing_mut().button_padding=egui::vec2(12.0,5.0);
                ui.spacing_mut().interact_size.y=28.0;
                {
                    let fill=settings_color(ctx,Color32::from_rgb(50,50,54),Color32::from_rgb(228,231,236));
                    let hover=settings_color(ctx,Color32::from_rgb(65,65,71),Color32::from_rgb(216,222,231));
                    let widgets=&mut ui.visuals_mut().widgets;
                    for (w,f) in [(&mut widgets.inactive,fill),(&mut widgets.hovered,hover),(&mut widgets.active,hover)] {
                        w.corner_radius=egui::CornerRadius::same(7);
                        w.weak_bg_fill=f;
                        w.bg_stroke=egui::Stroke::NONE;
                    }
                }
                ui.visuals_mut().selection.bg_fill=settings_color(ctx,Color32::from_rgb(45,105,165),Color32::from_rgb(209,226,248));
                #[cfg(target_os="macos")] {let mut style=(**ui.style()).clone();crate::mac_design::style(ctx,&mut style);ui.set_style(style);}
                let section=settings_section_name(self.settings_tab);
                #[cfg(target_os="macos")]
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("{section} — 轻屿")));
                ui.label(egui::RichText::new(section).size(if cfg!(target_os="macos"){20.0}else{18.0}).strong().color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))));
                if let Some((_,start,end))=SETTINGS_GROUPS.iter().find(|(_,start,end)|(*start..=*end).contains(&self.settings_tab)) {
                    if start!=end {
                        ui.add_space(7.0);
                        egui::Frame::new().fill(settings_color(ctx,Color32::from_rgb(33,33,36),Color32::from_rgb(232,235,240))).corner_radius(8.0).inner_margin(3).show(ui,|ui|{
                            ui.spacing_mut().item_spacing.x=2.0;
                            ui.horizontal(|ui|{
                                for index in *start..=*end {
                                    let selected=self.settings_tab==index;
                                    let button=egui::Button::new(egui::RichText::new(settings_section_name(index)).size(12.0).color(if selected{settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))}else{settings_color(ctx,Color32::from_gray(210),Color32::from_gray(55))}))
                                        .fill(if selected{settings_color(ctx,Color32::from_rgb(70,70,74),Color32::from_rgb(205,215,229))}else{Color32::TRANSPARENT})
                                        .stroke(egui::Stroke::NONE).corner_radius(6.0);
                                    if ui.add_sized([68.0,26.0],button).clicked(){self.settings_tab=index;}
                                }
                            });
                        });
                    }
                }
                ui.add_space(6.0);
                egui::ScrollArea::vertical().id_salt(("settings-scroll",self.settings_tab)).auto_shrink([false,false]).show(ui,|ui|{
                    // 右侧给滚动条留出空隙，卡片和开关不会被它压住。
                    ui.set_width((ui.available_width()-12.0).max(240.0));
                    // 模块已关闭：顶部说明并提供开启入口，下面的设置置灰。
                    if let Some(page)=settings_tab_module(self.settings_tab).filter(|page|!self.module_on(*page)) {
                        let mut enable=false;
                        settings_card(ui,|ui|{
                            ui.horizontal(|ui|{
                                ui.label(egui::RichText::new(format!("「{}」已关闭",PAGE_NAMES[page])).size(12.5));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{enable=ui.button("开启").clicked();});
                            });
                        });
                        if enable {self.set_page_visible(page,true);}
                        ui.add_space(6.0);
                        if !self.module_on(page) {ui.disable();}
                    }
                    match self.settings_tab {
                    12=>self.weather_settings(ui),
                    15=>self.mac_stats_settings(ui),
                    16=>self.notification_settings(ui),
                    13=>self.agent_settings(ui),
                    10=>{
                        if !self.settings.dock.is_side(){
                            settings_label(ui,"常驻排列");
                            settings_card(ui,|ui|self.resident_order_settings(ui));
                        }
                        settings_label(ui,"功能页面");
                        settings_card(ui,|ui|{
                            settings_hint(ui,"拖动排序；关闭后暂停后台活动，保留已有数据。");
                            #[derive(Clone)]
                            struct PageDrag(usize);
                            let mut movement=None;
                            let row_w=ui.available_width();
                            for (index,page) in self.settings.page_order.into_iter().enumerate() {
                                if index>0 {ui.separator();}
                                let mut visible=self.settings.visible_pages[page];
                                let can_change=!visible || self.settings.visible_pages.iter().filter(|enabled|**enabled).count()>1;
                                let row=ui.push_id(("page-order",page),|ui|ui.horizontal(|ui|{
                                    ui.set_max_width(row_w);
                                    let (rect,handle)=ui.allocate_exact_size(egui::vec2(20.0,40.0),egui::Sense::drag());
                                    for x in [-3.0,3.0] {for y in [-6.0,0.0,6.0] {
                                        ui.painter().circle_filled(rect.center()+egui::vec2(x,y),1.4,settings_color(ctx,Color32::from_gray(155),Color32::from_gray(100)));
                                    }}
                                    handle.on_hover_cursor(egui::CursorIcon::Grab).dnd_set_drag_payload(PageDrag(page));
                                    ui.add_enabled_ui(can_change,|ui|{setting_row(ui,PAGE_NAMES[page],"",&mut visible);});
                                })).inner.response;
                                if let Some(payload)=row.dnd_hover_payload::<PageDrag>() {
                                    if payload.0!=page {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                                        let y=if self.settings.page_order.iter().position(|p|*p==payload.0).unwrap()<index {row.rect.bottom()}else{row.rect.top()};
                                        ui.painter().line_segment([egui::pos2(row.rect.left(),y),egui::pos2(row.rect.right(),y)],egui::Stroke::new(2.0,Color32::from_rgb(70,150,255)));
                                    }
                                }
                                if let Some(payload)=row.dnd_release_payload::<PageDrag>() {movement=Some((payload.0,index));}
                                if visible!=self.settings.visible_pages[page] {self.set_page_visible(page,visible);}
                            }
                            if let Some((page,to))=movement {
                                let from=self.settings.page_order.iter().position(|p|*p==page).unwrap();
                                if from<to {self.settings.page_order[from..=to].rotate_left(1);}
                                else {self.settings.page_order[to..=from].rotate_right(1);}
                            }
                            if self.settings.visible_pages.iter().filter(|enabled|**enabled).count()==1 {
                                ui.add_space(5.0);
                                ui.label(egui::RichText::new("至少保留一个页面。").size(11.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            }
                        });
                    }
                    0=>{
                        #[cfg(target_os="macos")]
                        self.mac_general_settings(ui,autostart);
                        #[cfg(not(target_os="macos"))]
                        {
                        settings_label(ui,"设置页外观");
                        settings_card(ui,|ui|{
                            use crate::settings_theme::SettingsTheme;
                            settings_control_row_right(ui,"主题",|ui|{
                                if settings_segmented(ui,&mut self.settings.settings_theme,&[(SettingsTheme::System,"跟随系统"),(SettingsTheme::Light,"浅色"),(SettingsTheme::Dark,"深色")],52.0) {ctx.request_repaint();}
                            });
                            settings_hint(ui,"仅影响设置页，岛的外观保持不变。");
                        });
                        settings_label(ui,"启动");
                        settings_card(ui,|ui|{
                            let mut a=autostart;
                            setting_row(ui,"开机启动",if cfg!(target_os="macos"){"登录 Mac 时自动打开轻屿"}else{"登录 Windows 时自动打开轻屿"},&mut a);
                            if a!=autostart{self.pending.push(Cmd::ToggleAutostart);}
                        });
                        settings_label(ui,"图形渲染");
                        #[cfg(target_os="macos")]
                        settings_card(ui,|ui|{
                            ui.label("Metal 原生渲染");
                            settings_hint(ui,"使用 Mac 图形处理器与垂直同步；静止时降低刷新频率，交互时恢复流畅动画。");
                        });
                        #[cfg(not(target_os="macos"))]
                        settings_card(ui,|ui|{
                            let before=self.settings.render_compat;
                            setting_row(ui,"兼容渲染（Direct3D 12）","岛周围出现黑框、画面闪烁或错乱，或打开设置后崩溃退出时开启；切换后重启轻屿生效",&mut self.settings.render_compat);
                            // 手动关闭后不再显示自动切换的原因；再次开启视为手动开启。
                            if before!=self.settings.render_compat {self.settings.render_compat_reason.clear();}
                            if !self.settings.render_compat_reason.is_empty() {settings_hint(ui,&self.settings.render_compat_reason);}
                            if self.settings.render_compat!=self.render_compat_running {
                                ui.horizontal(|ui|{
                                    ui.label(egui::RichText::new("重启轻屿后生效").size(12.0).color(Color32::from_rgb(230,150,40)));
                                    if ui.button("立即重启").clicked(){self.pending.push(Cmd::RestartApp);}
                                });
                            }
                        });
                        settings_label(ui,"岛体外观");
                        settings_card(ui,|ui|{
                            use crate::model::{self as model,Skin};
                            let skins:Vec<(Skin,&str)>=Skin::ALL.iter().map(|s|(*s,s.label())).collect();
                            settings_control_row_right(ui,"皮肤",|ui|{settings_segmented(ui,&mut self.settings.skin,&skins,64.0);});
                            settings_hint(ui,self.settings.skin.description());
                            ui.separator();
                            let shapes:Vec<(model::IslandShape,&str)>=model::IslandShape::ALL.iter().map(|s|(*s,s.label())).collect();
                            settings_control_row_right(ui,"形状",|ui|{settings_segmented(ui,&mut self.settings.island_shape,&shapes,64.0);});
                            settings_hint(ui,self.settings.island_shape.description());
                            if self.settings.skin.is_glass() {
                                ui.separator();
                                let mut blur=self.settings.glass_blur;
                                setting_row(ui,"背景模糊",if cfg!(target_os="macos"){"使用 macOS 原生材质模糊；关闭后改为半透明底色"}else{"使用 Windows 合成引擎的实时模糊，严格贴合岛体形状（需开启系统“透明效果”）；关闭后改为烟熏半透明"},&mut blur);
                                self.settings.glass_blur=blur;
                            }
                            ui.separator();
                            settings_control_row(ui,"大小",56.0,|ui|{
                                ui.horizontal(|ui|{
                                    ui.spacing_mut().slider_width=180.0;
                                    island_size_slider(ui,&mut self.settings.island_size);
                                    if ui.add_enabled(self.settings.island_size!=100,egui::Button::new("恢复默认")).clicked(){self.settings.island_size=100;}
                                }).response
                            });
                            settings_hint(ui,"文字和图标按实际像素重新绘制，放大后依旧清晰；只影响岛体，不影响设置窗口。");
                        });
                        settings_label(ui,"岛体显示");
                        settings_card(ui,|ui|{
                            let selected=self.displays.iter().find(|d|d.id==self.settings.display_id);
                            let label=if self.settings.display_id.is_empty(){"跟随主屏"}else{selected.map_or("所选屏幕未连接",|d|d.label.as_str())};
                            settings_control_row(ui,"显示屏",56.0,|ui|{
                                egui::ComboBox::from_id_salt("island-display").selected_text(label).width(ui.available_width().min(300.0)).truncate().show_ui(ui,|ui|{
                                    ui.selectable_value(&mut self.settings.display_id,String::new(),"跟随主屏");
                                    for display in &self.displays {ui.selectable_value(&mut self.settings.display_id,display.id.clone(),&display.label);}
                                }).response
                            });
                            let edge=self.settings.dock.label();
                            ui.label(egui::RichText::new(if !self.settings.display_id.is_empty()&&selected.is_none(){"暂时显示在主屏，重新连接所选屏幕后自动恢复。".to_string()}else{format!("显示在所选屏幕{edge}；断开时暂时回到主屏。")}).size(11.0).color(ui.visuals().weak_text_color()));
                            ui.add_space(4.0);
                            // 停靠边缘：顶部为横向；左右为竖向排列，展开面板从边缘滑出。
                            let docks:Vec<(crate::model::Dock,&str)>=crate::model::Dock::ALL.iter().map(|d|(*d,d.label())).collect();
                            settings_control_row_right(ui,"停靠",|ui|{
                                // 换边后偏移的含义变了（水平 ↔ 上下），回到居中。
                                if settings_segmented(ui,&mut self.settings.dock,&docks,64.0) {self.settings.island_offset=0;}
                            });
                            if self.settings.dock.is_side() {
                                ui.label(egui::RichText::new("收起时竖向排列时间、天气和音乐；展开面板从屏幕边缘滑出。岛上歌词在顶部停靠时显示。").size(11.0).color(ui.visuals().weak_text_color()));
                            }
                            ui.add_space(4.0);
                            let side=self.settings.dock.is_side();
                            settings_control_row(ui,if side{"上下位置"}else{"水平位置"},56.0,|ui|{
                                ui.horizontal(|ui|{
                                    let limit=crate::model::ISLAND_OFFSET_MAX;
                                    let offset=&mut self.settings.island_offset;
                                    let (less,more,less_tip,more_tip)=if side{("↑","↓","向上微调","向下微调")}else{("←","→","向左微调","向右微调")};
                                    if ui.add_enabled(*offset>-limit,egui::Button::new(less).min_size(egui::vec2(26.0,22.0))).on_hover_text(less_tip).clicked(){*offset=(*offset-5).max(-limit);}
                                    ui.spacing_mut().slider_width=150.0;
                                    ui.add(egui::Slider::new(&mut *offset,-limit..=limit).show_value(false));
                                    if ui.add_enabled(*offset<limit,egui::Button::new(more).min_size(egui::vec2(26.0,22.0))).on_hover_text(more_tip).clicked(){*offset=(*offset+5).min(limit);}
                                    if ui.add_enabled(*offset!=0,egui::Button::new("恢复居中")).clicked(){*offset=0;}
                                }).response
                            });
                            let offset=self.settings.island_offset;
                            let (less,more)=if side{("偏上","偏下")}else{("偏左","偏右")};
                            let position=match offset {0=>"居中".to_string(),o if o<0=>format!("{less} {}%",-o),o=>format!("{more} {o}%")};
                            ui.label(egui::RichText::new(format!("当前：{position}。也可以展开后按住日期时间，拖到屏幕顶部、左侧或右侧；展开面板和通知始终留在屏幕内，换屏或调整大小后保持相对位置。")).size(11.0).color(ui.visuals().weak_text_color()));
                            ui.add_space(4.0);
                            setting_row(ui,"OLED 像素偏移","收起静止时让岛体沿屏幕边缘缓慢移动几个像素，帮助减轻 OLED 静态显示留下的痕迹；不会改动你保存的位置",&mut self.settings.oled_shift);
                            if self.settings.oled_shift {
                                settings_hint(ui,"每 45 秒移动 1 像素、单方向最多 3 像素；顶部停靠左右移动，左右停靠上下移动。展开、点击或拖拽时保持当前位置，位置拉满或屏幕太窄时少挪或不挪。");
                            }
                            ui.add_space(4.0);
                            setting_row(ui,"避开顶栏","顶部有菜单栏类软件或任务栏时，岛体放在它下方",&mut self.settings.avoid_top_bar);
                            if self.settings.avoid_top_bar {
                                let reserved=crate::display_layout::selected(&self.displays,&self.settings.display_id).map_or(0,|d|{
                                    let b=crate::display_layout::placement_bounds(d,true);(b.top-d.bounds.top).max(0)
                                });
                                settings_hint(ui,&if reserved>0 {format!("已检测到顶栏，岛体下移 {reserved} 像素。")}else{"未检测到顶栏；未向系统登记占用区域的顶栏无法识别，可改用胶囊形状。".to_string()});
                            }
                            ui.separator();
                            settings_control_row_right(ui,"时间制式",|ui|{settings_segmented(ui,&mut self.settings.clock_12h,&[(false,"24 小时"),(true,"12 小时")],72.0);});
                            settings_control_row_right(ui,"时段文字",|ui|{settings_segmented(ui,&mut self.settings.clock_period,&[(0,"不显示"),(1,"上午 / 下午"),(2,"详细时段")],72.0);});
                            settings_hint(ui,"详细时段：凌晨、上午、中午、下午、傍晚、晚上");
                            ui.separator();
                            let mut minimal=self.settings.minimal;
                            setting_row(ui,"极简刘海","收起时显示一条细线",&mut minimal);
                            if minimal!=self.settings.minimal {self.set_minimal(minimal);}
                            ui.separator();

                        });
                        #[cfg(not(target_os="macos"))]
                        self.avoidance.settings(ui,&mut self.settings);
                        #[cfg(target_os="macos")]
                        settings_hint(ui,"macOS 支持全屏隐藏；按应用自动避让暂不可用。");
                        settings_label(ui,"声音与显示");
                        settings_card(ui,|ui|{
                            setting_row(ui,"提醒声音","待办和邮件提醒",&mut self.settings.sound);
                            ui.separator();
                            setting_row(ui,"全屏时隐藏","游戏、视频和演示全屏时隐藏",&mut self.settings.hide_fullscreen);
                        });
                        }
                    }
                    1=>{
                        #[cfg(target_os="macos")]
                        {
                        settings_label(ui,"QQ 音乐");
                        settings_card(ui,|ui|{
                            ui.horizontal_wrapped(|ui|{
                                if ui.add_sized([124.0,32.0],egui::Button::new("打开我的音乐…")).clicked(){self.pending.push(Cmd::QqMusic(crate::qq_music::Page::Library));}
                                if ui.add_sized([112.0,32.0],egui::Button::new("打开播放器…")).clicked(){self.pending.push(Cmd::QqMusic(crate::qq_music::Page::Player));}
                                if ui.add_sized([112.0,32.0],egui::Button::new("停止 QQ 音乐")).on_hover_text("关闭后台播放页面，保留登录").clicked(){self.pending.push(Cmd::QqMusicStop);}
                            });
                        });
                        settings_label(ui,"其他播放器");
                        settings_card(ui,|ui|{
                            ui.label("支持 Apple Music 与 Spotify");
                            settings_hint(ui,"首次使用需允许自动化控制。");
                        });
                        }
                        settings_label(ui,"歌词");
                        settings_card(ui,|ui|{
                            setting_row(ui,"展开时显示歌词","",&mut self.settings.lyrics_enabled);
                            self.show_lyrics=self.settings.lyrics_enabled;
                            if !self.settings.dock.is_side(){
                                ui.separator();
                                setting_row(ui,"岛上歌词","收起时显示当前句",&mut self.settings.island_lyrics);
                            }
                        });
                        settings_label(ui,"歌词显示时间");
                        settings_card(ui,|ui|{
                            let mut advance=-self.settings.lyric_delay_ms as f64/1000.0;
                            if setting_number(ui,"时间偏移",&mut advance,-5.0,5.0," 秒",0.1){self.settings.lyric_delay_ms=(-advance*1000.0).round()as i32;}
                            ui.add_space(3.0);
                            ui.small("正数提前，负数延后。");
                        });
                    }

                    4=>{
                        self.library_settings_ui(ui);
                    }
                    5=>{
                        settings_label(ui,"暂存架");
                        settings_card(ui,|ui|{
                            settings_hint(ui,"移除条目不会删除原文件。");
                            ui.label(egui::RichText::new(format!("当前 {} 项",self.shelf.items.len())).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                            ui.add_space(5.0);
                            if ui.add_sized([120.0,32.0],egui::Button::new("打开暂存架")).clicked(){self.settings_open=false;self.show_page(crate::app::SHELF_PAGE);}
                        });
                    }
                    14=>{
                        settings_label(ui,"剪贴簿");
                        settings_card(ui,|ui|{
                            setting_row(ui,"记录复制内容","文字、图片和视频保存在本机",&mut self.settings.clipboard_history);
                            settings_hint(ui,&format!("保留最近 {} 条；跳过标为敏感的内容。",crate::clipboard::MAX_RECENT));
                            ui.add_space(5.0);
                            if ui.add_sized([120.0,32.0],egui::Button::new("打开剪贴簿")).clicked(){self.settings_open=false;self.show_page(crate::app::CLIPBOARD_PAGE);}
                        });
                    }

                    7=>{
                        settings_label(ui,"邮件提醒");
                        settings_card(ui,|ui|{
                            setting_row(ui,"新邮件提醒","",&mut self.settings.mail_enabled);
                            ui.separator();
                            let sync_before=self.settings.mail_sync_operations;
                            ui.add_enabled_ui(self.mail.manage.pending.is_empty(),|ui|{
                                setting_row(ui,"同步邮件操作到邮箱","同步已读、删除和恢复；关闭时仅影响轻屿",&mut self.settings.mail_sync_operations);
                            });
                            if sync_before!=self.settings.mail_sync_operations{self.mail_manage_mode_changed();}
                            settings_hint(ui,"不会同步此前的本机操作。");
                            ui.separator();
                            setting_row(ui,"验证码高亮","点击即可复制",&mut self.settings.mail_code);
                            ui.separator();
                            setting_choices(ui,"验证码隐藏",&mut self.settings.mail_code_hide_seconds,[30,60,300],"秒");
                            settings_hint(ui,"隐藏不代表失效；查看后 15 秒再次隐藏。");
                            setting_choices(ui,"检查间隔",&mut self.settings.mail_interval,[60,120,300],"秒");
                            settings_hint(ui,"仅用于不支持推送的邮箱。");
                        });
                        settings_label(ui,"已添加的邮箱");
                        settings_card(ui,|ui|{
                            if self.mail.accounts.is_empty(){settings_hint(ui,"尚未添加邮箱。");}
                            let total=self.mail.accounts.len();
                            let(mut changed,mut remove,mut test)=(false,None,None);
                            for i in 0..total{
                                let(email,provider)=(self.mail.accounts[i].email.clone(),self.mail.accounts[i].provider);
                                let status=self.mail.status_of(&email).label();
                                let mut on=self.mail.accounts[i].enabled;
                                if i>0{ui.separator();}
                                ui.label(egui::RichText::new(&email).size(13.0).color(settings_color(ctx,Color32::WHITE,Color32::from_rgb(30,34,41))));
                                ui.label(egui::RichText::new(format!("{} · {}",provider.label(),status)).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75))));
                                ui.horizontal(|ui|{
                                    if ui.add_sized([66.0,30.0],egui::Button::new("测试")).clicked(){test=Some(i);}
                                    if ui.add_sized([66.0,30.0],egui::Button::new("删除")).clicked(){remove=Some(i);}
                                    ui.add_space(8.0);
                                    ui.label(egui::RichText::new("启用").size(12.0).color(settings_color(ctx,Color32::from_gray(190),Color32::from_gray(65))));
                                    if switch(ui,&mut on){self.mail.accounts[i].enabled=on;changed=true;}
                                });
                            }
                            if changed{self.mail_accounts_changed();}
                            if let Some(i)=remove{
                                let email=self.mail.accounts[i].email.clone();
                                let credential=self.mail.accounts[i].credential_key();
                                self.mail.accounts.remove(i);
                                crate::mail::secrets::delete(&credential);
                                self.mail.status.remove(&email.to_ascii_lowercase());
                                self.mail.note=format!("{email}：已移除，凭据已从系统凭据管理器删除");
                                self.mail_accounts_changed();
                            }
                            if let Some(i)=test{
                                let account=self.mail.accounts[i].clone();
                                if let Err(error)=account.endpoint(){
                                    self.mail.note=error;
                                }else{
                                match crate::mail::secrets::load_interactive(&account.credential_key()){
                                    Ok(pass)=>{self.mail.testing=Some(account.email.clone());self.mail.note=format!("{}：正在测试连接…",account.email);self.pending.push(Cmd::MailTest(account,pass));}
                                    Err(e)=>self.mail.note=format!("{}：{e}",account.email),
                                }
                                }
                            }
                        });
                        settings_label(ui,"添加邮箱");
                        settings_card(ui,|ui|{
                            settings_control_row(ui,"邮箱服务",56.0,|ui|{
                                egui::ComboBox::from_id_salt("mail-provider").width(160.0).selected_text(self.mail.form.provider.label()).show_ui(ui,|ui|{
                                    for p in crate::mail::Provider::ALL{if ui.selectable_label(self.mail.form.provider==p,p.label()).clicked(){self.mail.form.provider=p;}}
                                }).response
                            });
                            ui.add_space(5.0);
                            let microsoft=self.mail.form.provider==crate::mail::Provider::Microsoft;
                            let help=|ui:&mut egui::Ui|{
                                ui.add(egui::Label::new(egui::RichText::new(self.mail.form.provider.hint()).size(12.0).color(settings_color(ctx,Color32::from_gray(200),Color32::from_gray(55)))).wrap());
                            };
                            if microsoft{
                                ui.label(egui::RichText::new("连接微软邮箱").size(12.5).strong());
                                help(ui);
                            }else{
                                ui.collapsing("如何获取授权码？",help);
                            }
                            let (label,url)=self.mail.form.provider.setup_link();
                            if !url.is_empty() && ui.link(label).clicked(){self.pending.push(Cmd::OpenUrl(url.into()));}
                            ui.add_space(8.0);
                            if self.mail.form.provider==crate::mail::Provider::Custom {
                                settings_control_row(ui,"IMAP 主机",80.0,|ui|{ui.text_edit_singleline(&mut self.mail.form.custom.host)});
                                settings_control_row(ui,"TLS 端口",80.0,|ui|{ui.add(egui::DragValue::new(&mut self.mail.form.custom.port).range(1..=65535))});
                                settings_control_row(ui,"登录用户名",80.0,|ui|{ui.add(egui::TextEdit::singleline(&mut self.mail.form.custom.username).vertical_align(egui::Align::Center).hint_text("留空使用邮箱地址"))});
                            }
                            if !microsoft{
                            let input_width=(ui.available_width()-56.0-48.0-2.0*ui.spacing().item_spacing.x).max(100.0);
                            settings_control_row(ui,"邮箱",56.0,|ui|{
                                ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.mail.form.email).vertical_align(egui::Align::Center).hint_text("输入邮箱地址"))
                            });
                            let target=crate::mail::Account{custom:(self.mail.form.provider==crate::mail::Provider::Custom).then(||self.mail.form.custom.clone()),email:self.mail.form.email.clone(),provider:self.mail.form.provider,enabled:true};
                            let unsupported=target.endpoint().err().as_deref()==Some(crate::mail::MICROSOFT_UNSUPPORTED);
                            if unsupported{ui.add(egui::Label::new(crate::mail::MICROSOFT_UNSUPPORTED).wrap());}
                            ui.add_enabled_ui(!unsupported,|ui|{
                            settings_control_row(ui,"授权码",56.0,|ui|{
                                let field=ui.add_sized([input_width,30.0],egui::TextEdit::singleline(&mut self.mail.form.password).vertical_align(egui::Align::Center).password(!self.mail.form.show).hint_text("输入邮箱授权码"));
                                if ui.add_sized([48.0,30.0],egui::Button::new(if self.mail.form.show{"隐藏"}else{"显示"})).clicked(){self.mail.form.show=!self.mail.form.show;}
                                field
                            });
                            ui.horizontal(|ui|{
                                if ui.button("测试连接").clicked(){
                                    let email=self.mail.form.email.trim().to_string();
                                    let target=crate::mail::Account{custom:(self.mail.form.provider==crate::mail::Provider::Custom).then(||self.mail.form.custom.clone()),email:email.clone(),provider:self.mail.form.provider,enabled:true};
                                    if !email.contains('@'){self.mail.note="请先填邮箱地址".into();}
                                    else if let Err(error)=target.endpoint(){self.mail.note=error;}
                                    else if self.mail.form.password.is_empty(){self.mail.note="请先填授权码（不是邮箱登录密码）".into();}
                                    else{
                                        self.mail.testing=Some(email.clone());
                                        self.mail.note=format!("{email}：正在测试连接…");
                                        self.pending.push(Cmd::MailTest(crate::mail::Account {custom: (self.mail.form.provider==crate::mail::Provider::Custom).then(||self.mail.form.custom.clone()), email:email.clone(),provider:self.mail.form.provider,enabled:true},self.mail.form.password.clone()));
                                    }
                                }
                                if ui.button("保存并开始监控").clicked(){
                                    let email=self.mail.form.email.trim().to_string();
                                    let target=crate::mail::Account{custom:(self.mail.form.provider==crate::mail::Provider::Custom).then(||self.mail.form.custom.clone()),email:email.clone(),provider:self.mail.form.provider,enabled:true};
                                    if !email.contains('@'){self.mail.note="请先填邮箱地址".into();}
                                    else if let Err(error)=target.endpoint(){self.mail.note=error;}
                                    else if self.mail.form.password.is_empty(){self.mail.note="请先填授权码（不是邮箱登录密码）".into();}
                                    else{
                                        match crate::mail::secrets::save(&target.credential_key(),&self.mail.form.password){
                                            Ok(())=>{
                                                let account=crate::mail::Account {custom: (self.mail.form.provider==crate::mail::Provider::Custom).then(||self.mail.form.custom.clone()), email:email.clone(),provider:self.mail.form.provider,enabled:true};
                                                if let Some(slot)=self.mail.accounts.iter_mut().find(|a|a.email.eq_ignore_ascii_case(&email)){*slot=account;}else{self.mail.accounts.push(account);}
                                                self.mail.form.password.clear();
                                                self.mail.note=format!("{email}：已保存，正在连接…");
                                                self.mail_accounts_changed();
                                            }
                                            Err(e)=>self.mail.note=e,
                                        }
                                    }
                                }
                            });
                            settings_hint(ui,"授权码保存在系统安全凭据库。");
                            });
                            }
                            if !self.mail.note.is_empty(){ui.add_space(3.0);ui.add(egui::Label::new(egui::RichText::new(&self.mail.note).size(12.0).color(settings_color(ctx,Color32::from_gray(180),Color32::from_gray(75)))).wrap());}
                        });
                    }
                    _=>{},
                    }
                });
            });
            self.avoidance.dialog(ctx,&mut self.settings);
            self.reconcile_notification_privacy();
            if before!=format!("{:?}",self.settings){self.settings.save();}
            ctx.set_style(original_style);
        });
        if !self.settings_open{self.window_layout().close();}
    }
}
