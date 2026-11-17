//! Unified, memory-only notification inbox and explicit permission setup.
use crate::{app::{App,Cmd,NOTIFICATIONS_PAGE,MAIL_PAGE,TODO_PAGE},extras::{Alert,AlertSource},notifications::{self,Config,Kind,Status},paint::{Card,H},theme::{self,W}};
use eframe::egui;

pub(crate) fn safe_preview(text:&str)->String {
    // Apply the same OTP policy to system previews as to IMAP notifications.
    // Do not expose verification codes in a transient HUD or notification list.
    let mut result=text.chars().filter(|c|!c.is_control()||*c=='\n').take(1200).collect::<String>();
    for _ in 0..8 {let Some(code)=crate::otp::extract(&result,"",&[])else{break;};let masked=crate::mail_privacy::mask(&result,&code.text);if masked==result{break;}result=masked;}
    result
}

impl App {
    pub fn notification_config(&self)->Config {
        let s=&self.settings;
        let kinds=[(Kind::Qq,s.qq_notifications),(Kind::Wechat,s.wechat_notifications&&!s.qq_only),(Kind::Wecom,s.wecom_notifications&&!s.qq_only),(Kind::Feishu,s.feishu_notifications&&!s.qq_only),(Kind::Dingtalk,s.dingtalk_notifications&&!s.qq_only),(Kind::DesktopMail,s.desktop_mail_notifications&&!s.qq_only)].into_iter().filter_map(|(kind,on)|on.then_some(kind)).collect::<Vec<_>>();
        Config {enabled:self.module_on(NOTIFICATIONS_PAGE)&&s.notifications&&s.read_notifications&&s.macos_dock_badges&&!self.notification_locked&&!kinds.is_empty(),kinds,previews:s.notification_preview,disabled:self.notification_apps.items.iter().filter(|(_,_,on)|!*on).map(|(id,_,_)|id.clone()).collect()}
    }
    pub fn set_notification_session_locked(&mut self,locked:bool) {
        if locked {self.agent_feedback.suspend();self.activity_notice=None;}
        self.notification_locked=locked;
        self.reconcile_notification_privacy();
    }
    pub fn notification_batch(&mut self,batch:notifications::Batch) {
        self.notification_status=batch.status;
        if self.notification_status==Status::Locked {self.notification_locked=true;}
        else if self.notification_status.monitoring() {self.notification_locked=false;}
        self.reconcile_notification_privacy();
        let config=self.notification_config();
        for event in batch.events {
            if !config.allows(&event.bundle_id)||!self.notification_apps.observe(&event.bundle_id,&event.name){continue;}
            if event.preview.is_some() {
                let matching=|a:&Alert|matches!(&a.source,AlertSource::Desktop{bundle_id,key,..} if bundle_id.eq_ignore_ascii_case(&event.bundle_id)&&key.starts_with("badge-"))&&self.t-a.at<=6.0;
                self.history.retain(|a|!matching(a));self.alerts.retain(|a|!matching(a));
                if self.notice.as_ref().is_some_and(matching){self.notice=None;}
                if self.hud_notice.as_ref().is_some_and(matching){self.hud_notice=None;}
            }
            let name=if event.name.is_empty(){Kind::of(&event.bundle_id).map_or("应用",Kind::label).into()}else{event.name};
            let generic=event.count.map_or_else(||"有新的未读消息（未提供正文）".into(),|count|format!("{count} 条未读消息（Dock 标记）"));
            let (title,body,activation,key)=if let Some(preview)=event.preview.filter(|_|self.settings.notification_preview) {
                let title=if preview.sender.is_empty(){name.clone()}else{format!("{name} · {}",safe_preview(&preview.sender))};
                (title,safe_preview(&preview.body),preview.activation,preview.id)
            }else{(name.clone(),generic,String::new(),format!("badge-{}-{:?}",self.t,event.count))};
            self.enqueue_alert(Alert{source:AlertSource::Desktop{bundle_id:event.bundle_id,name,activation,key},title,body,page:NOTIFICATIONS_PAGE,at:self.t,duration:crate::extras::NOTICE_DURATION},false);
        }
    }
    pub(crate) fn mail_notification(&mut self,item:crate::mail::Item) {
        let body=if self.settings.notification_preview {
            let body=format!("{} · {}",item.from,item.subject);
            let masked=item.code.as_ref().map(|code|crate::mail_privacy::mask(&body,code)).unwrap_or(body);
            safe_preview(&masked)
        }else{"收到一封新邮件，打开邮箱查看".into()};
        self.enqueue_alert(Alert{source:AlertSource::Mail{account:item.account.to_ascii_lowercase(),uid:item.uid},title:"新邮件".into(),body,page:MAIL_PAGE,at:self.t,duration:crate::extras::NOTICE_DURATION},false);
    }
    pub(crate) fn notification_allowed(&self,a:&Alert)->bool {
        if self.notification_locked||!self.settings.notifications||!self.module_on(NOTIFICATIONS_PAGE)||!self.module_on(a.page){return false;}
        if let Some((_,_,kind))=a.source.agent_notice(){
            return self.module_on(crate::app::AGENT_PAGE)&&if kind==crate::agent_activity::NoticeKind::Completed{self.settings.agent_completion}else{self.settings.agent_activity};
        }
        match &a.source {
            AlertSource::Desktop{bundle_id,..}=>self.notification_config().allows(bundle_id)&&!matches!(self.notification_status,Status::PermissionNeeded|Status::Locked),
            AlertSource::Mail{account,..}=>self.settings.mail_notifications&&self.mail_active()&&self.mail.accounts.iter().any(|a|a.enabled&&a.email.eq_ignore_ascii_case(account)),
            AlertSource::Agent{..}=>self.settings.agent_completion&&self.module_on(crate::app::AGENT_PAGE),
            AlertSource::AgentActivity{..}=>self.settings.agent_activity&&self.module_on(crate::app::AGENT_PAGE),
            _=>a.page!=MAIL_PAGE||self.settings.mail_notifications&&self.mail_active(),
        }
    }
    pub(crate) fn reconcile_notification_privacy(&mut self) {
        // Remove content rather than merely hiding it in the view. This also
        // applies when settings are changed through module management.
        let filtered=|items:&std::collections::VecDeque<Alert>|items.iter().filter(|a|self.notification_allowed(a)).cloned().collect();
        let history=filtered(&self.history);let alerts=filtered(&self.alerts);
        self.history=history;self.alerts=alerts;
        if self.notice.as_ref().is_some_and(|a|!self.notification_allowed(a)){self.notice=None;}
        if self.hud_notice.as_ref().is_some_and(|a|!self.notification_allowed(a)){self.hud_notice=None;}
        if self.activity_notice.as_ref().is_some_and(|a|!self.notification_allowed(a)){self.activity_notice=None;}
        if !self.settings.notification_preview {
            let redact=|a:&mut Alert|match &mut a.source {
                AlertSource::Desktop{name,activation,..}=>{a.title=name.clone();a.body="有新的未读消息，打开应用查看".into();activation.clear();},
                AlertSource::Agent{session_id,..}|AlertSource::AgentActivity{session_id,..}|AlertSource::AgentEvent{session_id,..}|AlertSource::AgentApproval{session_id,..}=>a.title=crate::agent_activity::conversation_label(session_id),
                _ if a.page==MAIL_PAGE=>{a.title="新邮件".into();a.body="收到一封新邮件，打开邮箱查看".into();},_=>{},
            };
            for a in self.history.iter_mut().chain(&mut self.alerts).chain(self.notice.iter_mut()).chain(self.hud_notice.iter_mut()).chain(self.activity_notice.iter_mut()){redact(a);}
        }
        if self.notification_locked||!self.settings.notifications||!self.module_on(NOTIFICATIONS_PAGE) {
            self.pending.retain(|cmd|!matches!(cmd,Cmd::Notify(..)|Cmd::OpenNotification{..}|Cmd::OpenAgent{..}));
            self.notification_consumed.clear();
            self.notification_bell_until=0.0;
        }
        if self.notification_locked||!self.settings.notifications||!self.module_on(NOTIFICATIONS_PAGE)||!self.module_on(crate::app::AGENT_PAGE){self.agent_feedback.suspend();}
    }
    pub(crate) fn redact_notification(&self,a:&mut Alert){
        if let Some((session_id,_,_))=a.source.agent_notice() {
            a.title=if self.settings.notification_preview{safe_preview(&a.title)}else{crate::agent_activity::conversation_label(session_id)};
            a.body=safe_preview(&a.body);
        }
    }
    pub(crate) fn open_notification(&mut self,a:&Alert,cmds:&mut Vec<Cmd>) {
        if !self.notification_allowed(a){return;}
        match &a.source {
            AlertSource::Desktop{bundle_id,activation,..}=>cmds.push(Cmd::OpenNotification{bundle_id:bundle_id.clone(),activation:activation.clone()}),
            AlertSource::Mail{account,uid}=>{self.show_page(MAIL_PAGE);if let Some(item)=self.mail.recent.iter().find(|m|m.account.eq_ignore_ascii_case(account)&&m.uid==*uid).cloned(){self.mail_open(&item);}},
            AlertSource::Agent{session_id,..}|AlertSource::AgentActivity{session_id,..}|AlertSource::AgentEvent{session_id,..}|AlertSource::AgentApproval{session_id,..}=>{
                let session=self.agent_snapshot.sessions.iter().find(|session|session.id==*session_id)
                    .or_else(||self.agent_snapshot.notices.iter().rev().find(|event|event.session.id==*session_id).map(|event|&event.session))
                    .or_else(||self.agent_snapshot.completions.iter().rev().find(|event|event.session.id==*session_id).map(|event|&event.session));
                if let Some(agent)=session.map(|session|session.agent).or_else(||crate::agent_activity::agent_from_session_id(session_id)){
                    let (pid,observed_at_ms)=if let AlertSource::AgentApproval{pid,observed_at_ms,..}=&a.source{(*pid,*observed_at_ms)}
                        else{(session.and_then(|session|session.pid),session.filter(|session|session.updated_at>0).map(|session|session.updated_at.saturating_mul(1000)))};
                    cmds.push(Cmd::OpenAgent{agent,session_id:session_id.clone(),pid,observed_at_ms});
                }else{self.say("无法确认这条通知对应的 Agent 应用");return;}
            },
            _=>{self.show_page(a.page);if a.page==TODO_PAGE{
                self.library_view.legacy_tasks=true;
                let order=self.todos.display_order(&self.today);
                if let Some(position)=order.iter().position(|&index|self.todos.items[index].text==a.body){
                    let capacity=((self.content_rect().bottom()-51.0)/22.0).floor()as usize;
                    self.todo_first=position.saturating_sub(capacity.saturating_sub(1)).min(order.len().saturating_sub(capacity));
                }
            }},
        }
        self.consume_notification(a);
    }
    pub(crate) fn consume_notification(&mut self,a:&Alert){
        if let Some(key)=a.delivery_key(){
            if !self.notification_consumed.contains(&key){self.notification_consumed.push_back(key);}
            while self.notification_consumed.len()>256{self.notification_consumed.pop_front();}
        }
        self.history.retain(|old|!old.same_delivery(a));
        self.alerts.retain(|old|!old.same_delivery(a));
        if self.notice.as_ref().is_some_and(|old|old.same_delivery(a)){self.notice=None;}
        if self.activity_notice.as_ref().is_some_and(|old|old.same_delivery(a)){self.activity_notice=None;}
        if self.hud_notice.as_ref().is_some_and(|old|old.same_delivery(a)){
            self.hud_notice=None;self.notice_motion.hide();
        }
        if self.display_notice().is_none()&&self.alerts.is_empty(){self.notification_bell_until=0.0;}
    }
    pub(crate) fn clear_notification_history(&mut self) {
        self.history.clear();self.alerts.clear();self.notice=None;self.hud_notice=None;self.activity_notice=None;self.notification_bell_until=0.0;
    }
    pub(crate) fn page_notifications(&mut self,c:&Card,side:bool,cmds:&mut Vec<Cmd>) {
        let inset=crate::portrait::INSET;
        let (left,width,top,height)=if side{(inset,self.expanded_width()-inset*2.0,110.0,self.expanded_height())}else{(44.0,331.0,43.0,self.content_rect().bottom())};
        if side{c.centered_text(left,124.0,"通知",15.0,W::Semibold,theme::WHITE,H::Left);}
        let status=if !self.settings.notifications{"通知已关闭".into()}else if !self.settings.read_notifications||!self.settings.macos_dock_badges{"聊天监测未启用 · 邮箱可独立接收".into()}else{self.notification_status.label()};
        c.centered_text(left,if side{150.0}else{56.0},&c.fit(&status,11.0,W::Regular,width-36.0),11.0,W::Regular,theme::GRAY,H::Left);
        let clear=c.hit(left+width-28.0,top,28.0,28.0,"notifications-clear").on_hover_text("清空本次通知记录");
        clear.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,"清空本次通知记录"));
        if clear.hovered()||clear.has_focus(){c.fill(left+width-28.0,top,28.0,28.0,7.0,theme::BTN_HOVER);}
        if clear.has_focus(){c.stroke(left+width-28.0,top,28.0,28.0,7.0,1.4,theme::CYAN);}
        crate::paint::glyph(c,"trash",left+width-14.0,top+14.0,16.0,theme::GRAY);
        if clear.clicked(){self.clear_notification_history();}
        let list_top=if side{171.0}else{77.0};let action_y=if side{height-46.0}else{height-28.0};let list_height=action_y-list_top-9.0;
        if self.history.is_empty() {
            crate::portrait::empty(c,egui::Rect::from_min_size(egui::pos2(left,list_top),egui::vec2(width,list_height)),"bell","还没有新通知","连接 QQ、微信或邮箱后，在这里汇总");
        }else {
            let history=self.history.clone();let mut open=None;let mut remove=None;
            let mut ui=crate::portrait::scroll_ui(c,left,list_top,width,list_height,"notifications");
            egui::ScrollArea::vertical().id_salt(("notification-list",side)).auto_shrink([false,false]).show(&mut ui,|ui|{
                let row_width=(ui.available_width()/c.s).max(1.0);
                for (index,alert) in history.iter().enumerate() {
                    let row_h=if side{90.0}else{58.0};
                    let (rect,_)=ui.allocate_exact_size(egui::vec2(row_width*c.s,row_h*c.s),egui::Sense::hover());
                    let mut row=Card::new(ui,rect.min,ui.clip_rect());row.s=c.s;row.live=c.live;
                    let description=if alert.requires_acknowledgement(){"打开对应应用查看审批请求".into()}else{match &alert.source {AlertSource::Desktop{name,activation,..}=>if activation.is_empty(){format!("打开 {name}；系统未提供聊天直达")}else{format!("打开 {name} 的原通知；失效时打开应用")},AlertSource::Mail{..}=>"打开这封邮件".into(),AlertSource::Agent{..}=>"查看这次 Codex 任务".into(),_ if alert.page==TODO_PAGE=>"查看待办".into(),_=>"查看通知".into()}};
                    let response=row.ui.interact(row.rect(0.0,0.0,row_width-30.0,row_h),egui::Id::new(("card",row.live,("notification-row",index))),if row.live{egui::Sense::click()}else{egui::Sense::hover()}).on_hover_text(&description);
                    response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,format!("{}，{}，{description}",alert.title,alert.body)));
                    if response.hovered(){row.fill(0.0,0.0,row_width,row_h,7.0,egui::Color32::from_rgb(31,65,71));}
                    if response.has_focus(){row.stroke(0.5,0.5,row_width-30.5,row_h-1.0,8.0,1.4,theme::CYAN);}
                    row.centered_text(10.0,17.0,&row.fit(&alert.title,12.0,W::Semibold,row_width-52.0),12.0,W::Semibold,theme::WHITE,H::Left);
                    row.centered_text(10.0,39.0,&row.fit(&alert.body.replace('\n'," "),11.0,W::Regular,row_width-26.0),11.0,W::Regular,theme::GRAY,H::Left);
                    if side{row.centered_text(10.0,69.0,&row.fit(&description,10.5,W::Regular,row_width-20.0),10.5,W::Regular,theme::CYAN,H::Left);}
                    let dismiss=row.hit(row_width-29.0,3.0,28.0,28.0,("notification-dismiss",index)).on_hover_text("移除此条记录");
                    dismiss.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,c.live,"移除此条记录"));
                    if dismiss.hovered()||dismiss.has_focus(){row.fill(row_width-29.0,3.0,28.0,28.0,7.0,theme::BTN_HOVER);}
                    if dismiss.has_focus(){row.stroke(row_width-29.0,3.0,28.0,28.0,7.0,1.4,theme::CYAN);}
                    crate::paint::glyph(&row,"close",row_width-15.0,17.0,12.0,theme::GRAY);
                    row.seg((0.0,row_h-0.5),(row_width,row_h-0.5),0.5,egui::Color32::from_white_alpha(22));
                    if dismiss.clicked(){remove=Some(index);}else if response.clicked(){open=Some(alert.clone());}
                }
            });
            if let Some(index)=remove{if let Some(alert)=self.history.get(index).cloned(){self.consume_notification(&alert);}}
            if let Some(alert)=open{self.open_notification(&alert,cmds);}
        }
        c.centered_text(left+width/2.0,action_y+14.0,"点击右上角齿轮管理来源与隐私",10.5,W::Regular,theme::GRAY,H::Center);
    }
    pub(crate) fn notification_settings(&mut self,ui:&mut egui::Ui) {
        use crate::extras::{setting_row,settings_card,settings_hint,settings_label};
        settings_label(ui,"通知与隐私");
        settings_card(ui,|ui|{
            setting_row(ui,"接收通知","关闭后清空通知记录，并暂停聊天监测",&mut self.settings.notifications);
            setting_row(ui,"提示音","",&mut self.settings.sound);
            setting_row(ui,"显示内容预览","显示发送者、正文与任务标题；关闭后清除已存预览",&mut self.settings.notification_preview);
            settings_hint(ui,"普通通知停留 15 秒，等待或中断停留 20 秒；审批提醒保留到点击或关闭。悬停或键盘焦点停留时暂停计时。");
            settings_hint(ui,"点击审批提醒打开对应应用，由你在原应用中处理审批。");
            settings_hint(ui,"记录仅留本次运行，锁屏清空；验证码始终遮挡。");
        });
        settings_label(ui,"智能体");
        settings_card(ui,|ui|{
            if ui.button("打开智能体提醒设置…").clicked(){self.settings_tab=13;}
        });
        settings_label(ui,"来源应用");
        settings_card(ui,|ui|{
            let mut monitoring=self.settings.read_notifications&&self.settings.macos_dock_badges;
            let previous=monitoring;
            setting_row(ui,"启用系统通知监测","需要辅助功能权限",&mut monitoring);
            if monitoring!=previous{self.settings.read_notifications=monitoring;self.settings.macos_dock_badges=monitoring;}
            let status=if !self.notification_config().enabled{Status::Off.label()}else{self.notification_status.label()};
            ui.label(status);
            #[cfg(target_os="macos")] ui.label(self.notification_status.accessibility_label());
            ui.horizontal_wrapped(|ui|{
                if ui.add_enabled(cfg!(target_os="macos"),egui::Button::new("打开辅助功能设置…")).clicked(){self.pending.push(Cmd::RequestNotificationAccess);}
                if ui.button("打开系统通知设置…").clicked(){self.pending.push(Cmd::OpenUrl("x-apple.systempreferences:com.apple.Notifications-Settings.extension".into()));}
            });
            if self.notification_status==Status::PermissionNeeded {
                settings_hint(ui,"系统已显示允许但仍无法读取时，请在辅助功能列表移除旧的轻屿条目，再添加当前使用的轻屿。权限变更后会自动检查。");
            }
            settings_hint(ui,"需允许来源应用的通知与预览。读不到正文时显示未读标记；点击打开原应用。");
            setting_row(ui,"QQ","",&mut self.settings.qq_notifications);
            setting_row(ui,"微信","多开预览可能无法区分账号",&mut self.settings.wechat_notifications);
            setting_row(ui,"企业微信","",&mut self.settings.wecom_notifications);
            setting_row(ui,"飞书 / Lark","",&mut self.settings.feishu_notifications);
            setting_row(ui,"钉钉","",&mut self.settings.dingtalk_notifications);
            setting_row(ui,"桌面邮箱应用","可能与已连接的信箱重复提醒",&mut self.settings.desktop_mail_notifications);
            if self.settings.qq_only{setting_row(ui,"兼容旧版：仅 QQ","关闭后使用上方各来源开关",&mut self.settings.qq_only);}
        });
        settings_label(ui,"信箱");
        settings_card(ui,|ui|{
            setting_row(ui,"新邮件加入通知","使用已配置的 IMAP 邮箱；关闭不影响收信列表",&mut self.settings.mail_notifications);
            ui.label(format!("已配置 {} 个邮箱 · {} 个启用",self.mail.accounts.len(),self.mail.accounts.iter().filter(|a|a.enabled).count()));
            if ui.button("管理邮箱账号…").clicked(){self.open_mail_settings();}
        });
        if !self.notification_apps.items.is_empty() {
            settings_label(ui,"已接收的应用");
            let mut changed=false;
            settings_card(ui,|ui|{for (_,name,on) in &mut self.notification_apps.items {let before=*on;setting_row(ui,name,"",on);changed|=before!=*on;}});
            if changed{self.notification_apps.save();}
        }
        settings_label(ui,"本地预览");
        settings_card(ui,|ui|{
            ui.horizontal_wrapped(|ui|{
                if ui.add_enabled(self.settings.notifications,egui::Button::new("测试岛上通知")).clicked(){
                    self.enqueue_alert(Alert{source:AlertSource::LocalTest,title:"本地通知测试".into(),body:"这是轻屿的本地预览，可在通知页清除".into(),page:NOTIFICATIONS_PAGE,at:self.t,duration:crate::extras::NOTICE_DURATION},false);
                }
                if ui.button("清空通知记录").clicked(){self.clear_notification_history();}
            });
        });
        self.reconcile_notification_privacy();
    }
}
