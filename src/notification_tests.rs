use super::*;
use crate::{app::NOTIFICATIONS_PAGE,notifications::{Batch,Preview,Status,Unread}};

pub(super) fn enable(app:&mut App) {
    app.settings.read_notifications=true;app.settings.macos_dock_badges=true;
    app.settings.qq_notifications=true;app.settings.wechat_notifications=true;app.settings.notification_preview=true;
}
pub(super) fn preview(id:&str,bundle:&str,name:&str,sender:&str,body:&str)->Unread {
    Unread{bundle_id:bundle.into(),name:name.into(),count:None,preview:Some(Preview{id:id.into(),bundle_id:bundle.into(),name:name.into(),sender:sender.into(),body:body.into(),activation:format!("action-{id}")})}
}
fn batch(events:Vec<Unread>)->Batch {Batch{status:Status::Watching{apps:2},events}}
fn mail(uid:u32)->crate::mail::Item {
    crate::mail::Item{received_unix:None,account:"test@qq.com".into(),from:"发件人".into(),subject:"测试邮件".into(),time:"09:41".into(),code:None,html:String::new().into(),summary:String::new().into(),links:vec![],uid}
}

#[test]
fn notification_inbox_unifies_chat_and_mail_without_feedback_or_duplicate_uid(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();enable(&mut h.app);
    h.app.mail.accounts=vec![crate::mail::Account{email:"test@qq.com".into(),provider:crate::mail::Provider::Qq,custom:None,enabled:true}];
    h.app.notification_batch(batch(vec![preview("qq-1","com.tencent.qq","QQ","同事","这周四开会"),preview("wx-1","com.tencent.xinWeChat","微信","家人","到家了吗") ]));
    h.app.mail_event(crate::mail::Event::Mail{item:mail(42),new:true});
    h.app.mail_event(crate::mail::Event::Mail{item:mail(42),new:true});
    assert_eq!(h.app.history.len(),3);assert_eq!(h.app.alerts.len(),3);
    assert!(h.app.history.iter().any(|a|a.body.contains("这周四开会")));
    assert!(h.app.history.iter().any(|a|a.body.contains("到家了吗")));
    assert!(!h.app.pending.iter().any(|c|matches!(c,crate::app::Cmd::Notify(..))),"imported notices must never be re-emitted into Notification Center");
    let wx=h.app.history.iter().find(|a|a.title.starts_with("微信")).unwrap().clone();let mut commands=vec![];h.app.open_notification(&wx,&mut commands);
    assert_eq!(commands,vec![crate::app::Cmd::OpenNotification{bundle_id:"com.tencent.xinWeChat".into(),activation:"action-wx-1".into()}]);
    let email=h.app.history.front().unwrap().clone();h.app.open_notification(&email,&mut commands);
    assert_eq!(h.app.page(),crate::app::MAIL_PAGE);assert_eq!(h.app.mail.open_uid,Some(("test@qq.com".into(),42)));
}

#[test]
fn notification_open_removes_only_that_message_and_blocks_late_replay(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();enable(&mut h.app);
    h.app.notification_batch(batch(vec![preview("opened","com.tencent.qq","QQ","朋友","已打开的消息"),preview("other-qq","com.tencent.qq","QQ","同事","另一条 QQ 消息"),preview("other-wx","com.tencent.xinWeChat","微信","家人","微信消息")]));
    h.app.tick_alerts();let opened=h.app.notice.clone().unwrap();h.app.hud_notice=Some(opened.clone());
    h.app.alerts.push_back(opened.clone());let mut commands=vec![];
    h.app.open_notification(&opened,&mut commands);
    assert_eq!(commands,vec![crate::app::Cmd::OpenNotification{bundle_id:"com.tencent.qq".into(),activation:"action-opened".into()}]);
    assert!(h.app.notice.is_none()&&h.app.hud_notice.is_none());assert_eq!(h.app.notice_motion.progress(),0.0);
    assert_eq!(h.app.history.len(),2);assert_eq!(h.app.alerts.len(),2);
    assert!(h.app.history.iter().all(|alert|!alert.same_delivery(&opened)));
    h.app.notification_batch(batch(vec![preview("opened","com.tencent.qq","QQ","朋友","已打开的消息")]));
    assert_eq!(h.app.history.len(),2,"a late copy of the opened event cannot restore it");
    h.app.notification_batch(batch(vec![preview("genuinely-new","com.tencent.qq","QQ","朋友","已打开的消息")]));
    assert_eq!(h.app.history.len(),3,"new notices with identical content must still arrive");
    assert_eq!(h.app.notification_consumed.len(),1);
    h.app.set_notification_session_locked(true);assert!(h.app.notification_consumed.is_empty());
}

#[test]
fn notification_privacy_redacts_storage_and_clears_disabled_sources_and_locked_session(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();enable(&mut h.app);
    h.app.notification_batch(batch(vec![preview("a","com.tencent.qq","QQ","私密发送者","私密正文，验证码 123456"),preview("b","com.tencent.xinWeChat","微信","朋友","另一条消息") ]));
    for a in h.app.history.iter().chain(&h.app.alerts){assert!(!a.body.contains("123456"));}
    h.app.tick_alerts();assert!(h.app.notice.is_some());
    h.app.settings.notification_preview=false;h.app.reconcile_notification_privacy();
    for a in h.app.history.iter().chain(&h.app.alerts).chain(h.app.notice.iter()).chain(h.app.hud_notice.iter()) {
        assert!(!a.title.contains("发送者")&&!a.title.contains("朋友"));assert!(!a.body.contains("私密")&&!a.body.contains("另一条"));
        if let crate::extras::AlertSource::Desktop{activation,..}=&a.source{assert!(activation.is_empty());}
    }
    h.app.settings.qq_notifications=false;h.app.reconcile_notification_privacy();
    assert_eq!(h.app.history.len(),1);assert!(h.app.history.front().unwrap().title.contains("微信"));
    h.app.notification_apps.items.iter_mut().find(|(id,_,_)|id=="com.tencent.xinWeChat").unwrap().2=false;h.app.reconcile_notification_privacy();
    assert!(h.app.history.is_empty());assert!(h.app.alerts.is_empty());assert!(h.app.notice.is_none());
    h.app.settings.qq_notifications=true;h.app.settings.notification_preview=true;
    h.app.notification_batch(batch(vec![preview("c","com.tencent.qq","QQ","新发件人","新正文") ]));
    h.app.set_notification_session_locked(true);assert!(h.app.history.is_empty());assert!(h.app.alerts.is_empty());
    h.app.notification_batch(Batch{status:Status::Locked,events:vec![preview("late","com.tencent.qq","QQ","不能出现","锁屏后延迟事件") ]});assert!(h.app.history.is_empty());
    h.app.set_notification_session_locked(false);h.app.notification_batch(batch(vec![preview("d","com.tencent.qq","QQ","新发件人","解锁后的新事件") ]));assert_eq!(h.app.history.len(),1);
    h.app.settings.notifications=false;h.app.reconcile_notification_privacy();assert!(h.app.history.is_empty());
    h.app.settings.notifications=true;h.app.notification_batch(batch(vec![preview("e","com.tencent.qq","QQ","发件人","关闭模块前") ]));assert_eq!(h.app.history.len(),1);
    h.app.set_page_visible(NOTIFICATIONS_PAGE,false);assert!(h.app.history.is_empty());assert!(h.app.notice.is_none());assert!(!h.app.notification_config().enabled);
}

#[test]
fn notification_preview_replaces_recent_badge_and_preserves_all_pending_messages(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();enable(&mut h.app);
    h.app.notification_batch(batch(vec![Unread{bundle_id:"com.tencent.qq".into(),name:"QQ".into(),count:Some(2),preview:None}]));assert_eq!(h.app.history.len(),1);
    h.app.notification_batch(batch(vec![preview("actual","com.tencent.qq","QQ","朋友","消息原文") ]));assert_eq!(h.app.history.len(),1);assert_eq!(h.app.history.front().unwrap().body,"消息原文");
    h.app.notification_batch(batch(vec![preview("actual","com.tencent.qq","QQ","朋友","消息原文") ]));assert_eq!(h.app.history.len(),1);
    h.app.notification_batch(batch(vec![preview("next-identical","com.tencent.qq","QQ","朋友","消息原文") ]));assert_eq!(h.app.history.len(),2,"identical consecutive real notices remain distinct");
    for i in 0..200 {h.app.notification_batch(batch(vec![preview(&format!("extra-{i}"),"com.tencent.qq","QQ","朋友","消息原文") ]));}
    assert_eq!(h.app.history.len(),202);assert_eq!(h.app.alerts.len(),202,"pending messages must never be silently evicted by the old eight-row limit");
    assert!(matches!(&h.app.alerts.front().unwrap().source,crate::extras::AlertSource::Desktop{key,..}if key=="actual"));
}

#[test]
fn notification_preview_enrichment_updates_the_same_queued_and_visible_message_without_replaying_it(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();enable(&mut h.app);
    h.app.notification_batch(batch(vec![preview("hydrating","com.tencent.xinWeChat","微信","张三",""),preview("next","com.tencent.xinWeChat","微信","李四","下一条消息")]));
    h.app.tick_alerts();h.app.hud_notice=h.app.notice.clone();let at=h.app.notice.as_ref().unwrap().at;
    h.app.t+=4.0;
    h.app.notification_batch(batch(vec![preview("hydrating","com.tencent.xinWeChat","微信","张三","稍后公开的消息正文")]));
    assert_eq!(h.app.alerts.len(),1);assert_eq!(h.app.history.len(),2);
    for alert in h.app.history.iter().chain(h.app.notice.iter()).chain(h.app.hud_notice.iter()).filter(|alert|matches!(&alert.source,crate::extras::AlertSource::Desktop{key,..}if key=="hydrating")){
        assert_eq!(alert.title,"微信 · 张三");assert_eq!(alert.body,"稍后公开的消息正文");assert_eq!(alert.at,at,"a richer preview must not restart its reading clock");
    }
    h.app.notification_batch(batch(vec![preview("next","com.tencent.xinWeChat","微信","李四","下一条消息的完整正文")]));
    assert_eq!(h.app.alerts.front().unwrap().body,"下一条消息的完整正文");assert_eq!(h.app.alerts.len(),1);
    let opened=h.app.notice.clone().unwrap();let mut commands=vec![];h.app.open_notification(&opened,&mut commands);
    h.app.notification_batch(batch(vec![preview("hydrating","com.tencent.xinWeChat","微信","张三","打开之后迟到的预览副本")]));
    assert_eq!(h.app.history.len(),1);assert_eq!(h.app.alerts.len(),1);assert!(h.app.notice.is_none()&&h.app.hud_notice.is_none());
}

#[test]
fn notification_schema_upgrade_preserves_switches_removed_data_accounts_and_no_consent(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());let mut h=H::new();let dir=crate::model::config_dir();
    let old="page_schema=6\nvisible_pages=01000100\npage_order=7,3,5,2,0,6,1,4\nresident_order=5,4,3,2,1,0\nread_notifications=1\nmacos_dock_badges=0\nwechat_notifications=1\nqq_only=0\nnotification_preview=0\ncalendar_auto_sync=0\nfocus_minutes=45\n";
    std::fs::write(dir.join("settings.txt"),old).unwrap();std::fs::write(dir.join("mail-accounts.json"),b"existing-account-fixture").unwrap();std::fs::write(dir.join("old-feature-data.json"),b"preserve-me").unwrap();
    let settings=crate::model::Settings::load();assert_eq!(&settings.visible_pages[..8],&[false,true,false,false,false,true,false,false]);assert!(settings.visible_pages[8]);assert_eq!(settings.page_order,[7,3,5,2,0,6,1,4,8]);
    assert!(settings.read_notifications&&settings.wechat_notifications);assert!(!settings.macos_dock_badges&&!settings.notification_preview);
    settings.save();let saved=std::fs::read_to_string(dir.join("settings.txt")).unwrap();assert!(saved.contains("page_schema=7\n")&&saved.contains("calendar_auto_sync=0\n")&&saved.contains("focus_minutes=45\n"));
    assert_eq!(std::fs::read(dir.join("mail-accounts.json")).unwrap(),b"existing-account-fixture");assert_eq!(std::fs::read(dir.join("old-feature-data.json")).unwrap(),b"preserve-me");
    h.app.settings=crate::model::Settings::load();assert!(!h.app.notification_config().enabled);assert!(!h.app.pending.iter().any(|c|matches!(c,crate::app::Cmd::RequestNotificationAccess)));
}

#[test]
fn notification_settings_fit_minimum_width_in_light_and_dark_appearance(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let folder=std::env::var_os("QINGYU_NOTIFICATION_PREVIEW_DIR").map(std::path::PathBuf::from);if let Some(folder)=&folder{std::fs::create_dir_all(folder).unwrap();}
    for dark in [false,true] {
        let mut h=H::new();h.app.open_settings();h.app.settings_tab=16;h.ctx.set_embed_viewports(false);
        h.app.settings.settings_theme=if dark{crate::settings_theme::SettingsTheme::Dark}else{crate::settings_theme::SettingsTheme::Light};
        let width=650.0;let height=1800.0;let scale=1.5;
        let output=std::sync::Arc::new(std::sync::Mutex::new(None));let capture=output.clone();
        egui::Context::set_immediate_viewport_renderer(move|ctx,mut viewport|{
            let id=viewport.ids.this;
            let raw=egui::RawInput{viewport_id:id,screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(width,height))),viewports:std::iter::once((id,egui::ViewportInfo{native_pixels_per_point:Some(scale),..Default::default()})).collect(),..Default::default()};
            *capture.lock().unwrap()=Some(ctx.run(raw,|ctx|(viewport.viewport_ui_cb)(ctx)));
        });
        let mut raster=folder.as_ref().map(|_|crate::preview_raster::Raster::new((width*scale)as usize,(height*scale)as usize));
        for frame in 0..4 {
            h.ctx.data_mut(|d|d.insert_temp(egui::Id::new("settings-system-theme"),(std::time::Instant::now(),dark)));
            let root=h.ctx.run(Default::default(),|ctx|h.app.settings_ui(ctx,false));let child=output.lock().unwrap().take().unwrap();
            for shape in &child.shapes{if let egui::Shape::Text(text)=&shape.shape {
                assert!(text.visual_bounding_rect().right()<=width+0.5,"notification settings text overflow at minimum width: {}",text.galley.text());
            }}
            assert!(!h.app.pending.iter().any(|c|matches!(c,crate::app::Cmd::RequestNotificationAccess)),"rendering permission setup must never request consent");
            if let Some(raster)=&mut raster {
                raster.update_textures(&root.textures_delta);raster.update_textures(&child.textures_delta);
                if frame==3 {raster.buf.fill(if dark{[0.08,0.08,0.09,1.0]}else{[0.96,0.96,0.97,1.0]});raster.draw(&h.ctx.tessellate(child.shapes,child.pixels_per_point),child.pixels_per_point);let mut rgba=vec![];raster.to_rgba8(&mut rgba);image::save_buffer(folder.as_ref().unwrap().join(format!("notification-settings-{}-650.png",if dark{"dark"}else{"light"})),&rgba,(width*scale)as u32,(height*scale)as u32,image::ColorType::Rgba8).unwrap();}
                raster.free_textures(&root.textures_delta);raster.free_textures(&child.textures_delta);
            }
        }
    }
}
