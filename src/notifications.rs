//! Opt-in, passive monitoring of publicly exposed macOS notification previews
//! and Dock unread badges. App storage and notification databases are never
//! read. All Accessibility sampling runs on one cancellable worker.
use std::collections::HashMap;

#[cfg(target_os="macos")]
#[path="notification_diagnostics.rs"]
pub mod diagnostics;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Qq, Wechat, Wecom, Feishu, Dingtalk, DesktopMail }
impl Kind {
    pub fn label(self)->&'static str {match self {Self::Qq=>"QQ",Self::Wechat=>"微信",Self::Wecom=>"企业微信",Self::Feishu=>"飞书 / Lark",Self::Dingtalk=>"钉钉",Self::DesktopMail=>"桌面邮箱"}}
    fn rules(self)->&'static [(&'static str,bool)] {match self {
        Self::Qq=>&[("com.tencent.qq",false)],
        Self::Wechat=>&[("com.tencent.xinwechat",false),("com.tencent.wechat",false),("com.wechatmulti.clone",true)],
        Self::Wecom=>&[("com.tencent.weworkmac",false)],
        Self::Feishu=>&[("com.electron.lark",false),("com.bytedance.feishu",false),("com.larksuite.lark",false)],
        Self::Dingtalk=>&[("com.alibaba.dingtalkmac",false)],
        Self::DesktopMail=>&[("com.apple.mail",false),("com.microsoft.outlook",false),("com.readdle.smartemail-mac",false),("com.tencent.qqmail",false),("com.netease.mailmaster",false)],
    }}
    pub fn of(bundle:&str)->Option<Self> {
        let id=bundle.to_ascii_lowercase();
        [Self::Qq,Self::Wechat,Self::Wecom,Self::Feishu,Self::Dingtalk,Self::DesktopMail].into_iter().find(|kind|kind.rules().iter().any(|(rule,prefix)|if *prefix{id.strip_prefix(rule).is_some_and(|suffix|!suffix.is_empty()&&suffix.bytes().all(|v|v.is_ascii_digit()))}else{id==*rule}))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Config {
    pub enabled:bool,
    pub kinds:Vec<Kind>,
    pub disabled:Vec<String>,
    pub previews:bool,
}
impl Config {
    pub fn allows(&self,id:&str)->bool {self.enabled && Kind::of(id).is_some_and(|kind|self.kinds.contains(&kind)) && !self.disabled.iter().any(|blocked|blocked.eq_ignore_ascii_case(id))}
    #[cfg(target_os="macos")]
    fn rules_json(&self)->String {
        serde_json::json!({"rules":self.kinds.iter().flat_map(|kind|kind.rules().iter().map(|(id,prefix)|serde_json::json!({"id":id,"prefix":prefix}))).collect::<Vec<_>>(),"disabled":self.disabled,"previews":self.previews}).to_string()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Status {
    #[default] Off, Starting, PermissionNeeded, Locked, DockUnavailable, Unsupported,
    Watching { apps:usize }, PreviewOnly, UnreadOnly { apps:usize },
    WaitingForPreview { apps:Option<usize> },
}
impl Status {
    pub fn monitoring(&self)->bool {
        matches!(self,Self::Watching{..}|Self::PreviewOnly|Self::UnreadOnly{..}|Self::WaitingForPreview{..})
    }
    pub fn accessibility_label(&self)->&'static str {match self {
        Self::PermissionNeeded=>"当前运行未获辅助功能权限",
        Self::Watching{..}|Self::PreviewOnly|Self::UnreadOnly{..}|Self::WaitingForPreview{..}=>"当前采样已获辅助功能权限",
        Self::Starting=>"正在检查辅助功能权限…",
        Self::Locked=>"锁屏期间暂停权限检查",
        Self::Off=>"开启系统通知监测后检查权限",
        Self::DockUnavailable=>"辅助功能权限：等待采样确认",
        Self::Unsupported=>"此平台不支持辅助功能监测",
    }}
    pub fn label(&self)->String {match self {
        Self::Off=>"未读监测已关闭".into(),Self::Starting=>"正在检查未读监测…".into(),
        Self::PermissionNeeded=>"需要辅助功能权限".into(),Self::Locked=>"锁屏期间已暂停".into(),
        Self::DockUnavailable=>"暂时无法读取系统通知或 Dock".into(),Self::Unsupported=>"此平台不支持 Dock 监测".into(),
        Self::Watching{apps:0}=>"预览监测中 · 暂无支持的 Dock 应用".into(),Self::Watching{apps}=>format!("预览与未读监测中 · {apps} 个应用"),
        Self::PreviewOnly=>"预览监测中 · Dock 暂不可用".into(),
        Self::UnreadOnly{apps}=>format!("Dock 未读监测中 · {apps} 个应用"),
        Self::WaitingForPreview{apps:Some(apps)}=>format!("Dock 未读监测中 · {apps} 个应用 · 尚未捕获预览"),
        Self::WaitingForPreview{apps:None}=>"等待系统公开通知预览 · Dock 暂不可用".into(),
    }}
}
#[derive(Clone, Debug)]
pub struct Unread {pub bundle_id:String,pub name:String,pub count:Option<u32>,pub preview:Option<Preview>}
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Preview {pub id:String,pub bundle_id:String,pub name:String,pub sender:String,pub body:String,#[serde(default)]pub activation:String}
#[derive(Clone, Debug, Default)]
pub struct Batch {pub status:Status,pub events:Vec<Unread>}
impl Batch {
    fn append(&mut self,events:Vec<Unread>) {
        for event in events {
            if let Some(preview)=&event.preview {
                if let Some(pending)=self.events.iter_mut().find(|pending|pending.bundle_id.eq_ignore_ascii_case(&event.bundle_id)&&pending.preview.as_ref().is_some_and(|old|old.id==preview.id)){*pending=event;continue;}
            }
            self.events.push(event);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Badge { count:Option<u32>, present:bool }
impl Badge {
    fn parse(value:&str)->Self {
        let value=value.trim();
        let digits=value.chars().filter(char::is_ascii_digit).collect::<String>();
        let count=digits.parse::<u32>().ok();
        Self {count,present:!value.is_empty() && count!=Some(0)}
    }
    fn increased(&self,old:&Self)->bool {
        self.present && (!old.present || matches!((old.count,self.count),(Some(a),Some(b)) if b>a))
    }
}
#[derive(Clone, Debug, serde::Deserialize)]
struct DockItem {id:String,name:String,badge:String}
#[derive(Clone,Copy)]
struct TextStamp {hash:u64,bytes:usize}
impl TextStamp {
    fn of(text:&str)->Self {
        use std::hash::{Hash,Hasher};
        let mut hasher=std::collections::hash_map::DefaultHasher::new();text.hash(&mut hasher);
        Self{hash:hasher.finish(),bytes:text.len()}
    }
    fn matches(self,text:&str)->bool {self.bytes==text.len()&&self.hash==Self::of(text).hash}
}
struct SeenPreview {id:String,bundle:String,node:Option<String>,sender:TextStamp,body:TextStamp,baseline:bool}
impl SeenPreview {
    fn enrichment(&self,preview:&Preview)->bool {
        // With content-derived native IDs, only filling a missing field is
        // unambiguous hydration. A changed nonempty body (even an extension)
        // can be a genuinely new message on a reused native node.
        (self.body.bytes==0&&preview.body.len()>0&&self.sender.matches(&preview.sender))
            ||(self.sender.bytes==0&&preview.sender.len()>0&&self.body.matches(&preview.body))
    }
}
fn native_node(id:&str)->Option<String> {
    let mut parts=id.split(':');
    if parts.next()!=Some("ax"){return None;}
    let pid=parts.next()?;let node=parts.next()?;
    if pid.parse::<u32>().is_err()||node.parse::<u64>().is_err()||parts.clone().count()!=2{return None;}
    if !parts.all(|part|part.parse::<u64>().is_ok()){return None;}
    Some(format!("ax:{pid}:{node}"))
}
#[derive(Default)]
struct Detector {previous:HashMap<String,Badge>,primed:bool,preview_seen:std::collections::VecDeque<SeenPreview>,preview_primed:bool}
impl Detector {
    fn reset(&mut self){self.previous.clear();self.primed=false;self.preview_seen.clear();self.preview_primed=false;}
    fn observe(&mut self,items:Vec<DockItem>,config:&Config)->Vec<Unread> {
        let mut next=HashMap::new();let mut events=Vec::new();
        for item in items.into_iter().take(128) {
            if !config.allows(&item.id){continue;}
            let id=item.id.to_ascii_lowercase();let badge=Badge::parse(&item.badge);
            // A newly appearing app is a baseline, just like startup/unlock.
            if self.primed && self.previous.get(&id).is_some_and(|old|badge.increased(old)) {
                events.push(Unread{bundle_id:item.id,name:item.name.chars().filter(|v|!v.is_control()).take(60).collect(),count:badge.count,preview:None});
            }
            next.insert(id,badge);
        }
        self.previous=next;self.primed=true;events
    }
    fn previews(&mut self,previews:Vec<Preview>,config:&Config)->Vec<Unread> {
        let mut events=Vec::new();
        for mut preview in previews.into_iter().take(480) {
            if !config.previews||!config.allows(&preview.bundle_id)||preview.id.is_empty(){continue;}
            // The detector retains only identities and content fingerprints,
            // including its quiet startup baseline, never sender/body strings.
            preview.id=preview.id.chars().filter(|v|!v.is_control()).take(240).collect();
            preview.activation=preview.activation.chars().filter(|v|!v.is_control()).take(240).collect();
            preview.name=preview.name.chars().filter(|v|!v.is_control()).take(60).collect();
            preview.sender=preview.sender.chars().filter(|v|!v.is_control()).take(160).collect();
            preview.body=preview.body.chars().filter(|v|!v.is_control()||*v=='\n').take(1000).collect();
            if preview.id.is_empty(){continue;}
            let bundle=preview.bundle_id.to_ascii_lowercase();let node=native_node(&preview.id);
            let existing=self.preview_seen.iter().rposition(|old|old.bundle==bundle&&(old.id==preview.id
                ||node.is_some()&&old.node==node&&(old.sender.matches(&preview.sender)&&old.body.matches(&preview.body)||old.enrichment(&preview))));
            let baseline=if let Some(index)=existing {
                let old=&mut self.preview_seen[index];
                if old.sender.matches(&preview.sender)&&old.body.matches(&preview.body){continue;}
                if preview.sender.len()<=old.sender.bytes&&preview.body.len()<=old.body.bytes
                    &&(preview.sender.len()<old.sender.bytes||preview.body.len()<old.body.bytes){continue;}
                // A content change under a stable external identity can only
                // update that delivery. Native reused nodes with different
                // content retain distinct IDs and become distinct messages.
                let baseline=old.baseline;preview.id=old.id.clone();
                old.sender=TextStamp::of(&preview.sender);old.body=TextStamp::of(&preview.body);baseline
            }else{
                let baseline=!self.preview_primed;
                self.preview_seen.push_back(SeenPreview{id:preview.id.clone(),bundle,node,sender:TextStamp::of(&preview.sender),body:TextStamp::of(&preview.body),baseline});
                while self.preview_seen.len()>256{self.preview_seen.pop_front();}
                baseline
            };
            if baseline||preview.sender.trim().is_empty()&&preview.body.trim().is_empty(){continue;}
            events.push(Unread{bundle_id:preview.bundle_id.clone(),name:preview.name.clone(),count:None,preview:Some(preview)});
        }
        self.preview_primed=true;events
    }

}

#[derive(serde::Deserialize)]
struct Sample {permission:bool,locked:bool,ready:bool,#[serde(default)]preview_accessible:bool,#[serde(default)]preview_ready:bool,items:Vec<DockItem>,#[serde(default)]previews:Vec<Preview>}

#[derive(Default)]
struct SamplingSession {detector:Detector,captured_preview:bool}
impl SamplingSession {
    fn reset(&mut self){self.detector.reset();self.captured_preview=false;}
    fn accept(&mut self,sample:Option<Sample>,config:&Config)->Batch {
        let Some(sample)=sample else{return Batch{status:Status::DockUnavailable,events:Vec::new()};};
        if sample.locked||!sample.permission {
            self.reset();
            return Batch{status:if sample.locked{Status::Locked}else{Status::PermissionNeeded},events:Vec::new()};
        }
        self.captured_preview|=sample.preview_ready;
        let status=match &sample {
            s if s.ready&&self.captured_preview=>Status::Watching{apps:s.items.len()},
            s if self.captured_preview&&s.preview_accessible=>Status::PreviewOnly,
            s if config.previews&&(s.ready||s.preview_accessible)=>Status::WaitingForPreview{apps:s.ready.then_some(s.items.len())},
            s if s.ready=>Status::UnreadOnly{apps:s.items.len()},_=>Status::DockUnavailable,
        };
        // A missing channel is not an empty channel. Keep each baseline until
        // that channel supplies a successful sample; otherwise a temporary AX
        // failure silently turns the next new message into a startup baseline.
        let mut events=if config.previews&&(sample.preview_accessible||!sample.previews.is_empty()) {
            self.detector.previews(sample.previews,config)
        }else{Vec::new()};
        if sample.ready {
            for badge in self.detector.observe(sample.items,config){
                if !events.iter().any(|preview|preview.bundle_id.eq_ignore_ascii_case(&badge.bundle_id)){events.push(badge);}
            }
        }
        Batch{status,events}
    }
}

#[cfg(target_os="macos")]
mod platform {
    use super::*;
    use std::{ffi::{c_char,c_void,CStr,CString},sync::{Arc,Mutex,atomic::{AtomicBool,AtomicU64,Ordering}},thread::{self,JoinHandle},time::Duration};
    extern "C" {
        fn qy_notifications_sample(rules:*const c_char,active:extern "C" fn(*mut c_void)->bool,context:*mut c_void)->*mut c_char;
        fn qy_notifications_free(value:*mut c_char);
        fn qy_notifications_request_access();
        fn qy_notifications_access_granted()->bool;
        fn qy_notifications_open(bundle:*const c_char,activation:*const c_char)->bool;
        fn qy_notifications_clear();
    }
    struct State {config:Config,batch:Batch}
    struct Shared {state:Mutex<State>,generation:AtomicU64,stop:AtomicBool}
    struct Ticket {shared:Arc<Shared>,generation:u64}
    extern "C" fn active(context:*mut c_void)->bool {
        let ticket=unsafe{&*(context as *const Ticket)};
        !ticket.shared.stop.load(Ordering::Acquire) && ticket.shared.generation.load(Ordering::Acquire)==ticket.generation
    }
    pub struct Monitor {shared:Arc<Shared>,thread:Option<JoinHandle<()>>}
    impl Monitor {
        pub fn start(ctx:eframe::egui::Context)->Self {
            let shared=Arc::new(Shared{state:Mutex::new(State{config:Config::default(),batch:Batch::default()}),generation:AtomicU64::new(1),stop:AtomicBool::new(false)});
            let worker=shared.clone();
            let thread=thread::Builder::new().name("qingyu-unread".into()).spawn(move||{
                let mut session=SamplingSession::default();let mut seen=0;
                while !worker.stop.load(Ordering::Acquire) {
                    let (generation,config)={let state=worker.state.lock().unwrap_or_else(|e|e.into_inner());(worker.generation.load(Ordering::Acquire),state.config.clone())};
                    if generation!=seen{session.reset();seen=generation;}
                    if config.enabled {
                        let rules=CString::new(config.rules_json()).unwrap();
                        let mut ticket=Ticket{shared:worker.clone(),generation};
                        let raw=unsafe{qy_notifications_sample(rules.as_ptr(),active,(&mut ticket as *mut Ticket).cast())};
                        let sample=if raw.is_null(){None}else{let data=unsafe{CStr::from_ptr(raw)}.to_bytes();let sample=serde_json::from_slice::<Sample>(data).ok();unsafe{qy_notifications_free(raw)};sample};
                        let Batch{status,events}=session.accept(sample,&config);
                        let mut state=worker.state.lock().unwrap_or_else(|e|e.into_inner());
                        if worker.generation.load(Ordering::Acquire)==generation && !worker.stop.load(Ordering::Acquire) {
                            let changed=state.batch.status!=status||!events.is_empty();
                            if matches!(status,Status::PermissionNeeded|Status::Locked){state.batch.events.clear();}
                            state.batch.status=status;
                            state.batch.append(events);
                            if changed{ctx.request_repaint();}
                        }
                    } else {session.reset();}
                    thread::park_timeout(Duration::from_millis(750));
                }
            }).ok();
            Self{shared,thread}
        }
        pub fn configure(&self,config:Config){
            let mut state=self.shared.state.lock().unwrap_or_else(|e|e.into_inner());
            if state.config==config{return;}
            self.shared.generation.fetch_add(1,Ordering::AcqRel);
            unsafe{qy_notifications_clear()};
            state.batch=Batch{status:if config.enabled{Status::Starting}else{Status::Off},events:Vec::new()};state.config=config;
            if let Some(thread)=&self.thread{thread.thread().unpark();}
        }
        pub fn take(&self)->Batch {
            let mut state=self.shared.state.lock().unwrap_or_else(|e|e.into_inner());
            Batch{status:if self.thread.is_none(){Status::DockUnavailable}else{state.batch.status.clone()},events:std::mem::take(&mut state.batch.events)}
        }
    }
    impl Drop for Monitor {
        fn drop(&mut self){self.shared.stop.store(true,Ordering::Release);self.shared.generation.fetch_add(1,Ordering::AcqRel);unsafe{qy_notifications_clear()};if let Some(thread)=self.thread.take(){thread.thread().unpark();/* No GUI-thread join; worker owns all its state and cancels AX reads. */}}
    }
    pub fn request_access(){unsafe{qy_notifications_request_access()}}
    pub fn access_granted()->bool {unsafe{qy_notifications_access_granted()}}
    pub fn open(bundle:&str,activation:&str)->bool {let (Ok(id),Ok(action))=(CString::new(bundle),CString::new(activation))else{return false;};unsafe{qy_notifications_open(id.as_ptr(),action.as_ptr())}}
}
#[cfg(target_os="macos")]
pub use platform::{Monitor,request_access,access_granted,open};

#[cfg(test)]
mod tests {
    use super::*;
    fn config()->Config {Config{enabled:true,kinds:vec![Kind::Qq,Kind::Wechat],..Default::default()}}
    fn item(id:&str,badge:&str)->DockItem {DockItem{id:id.into(),name:"测试应用".into(),badge:badge.into()}}
    fn sample(permission:bool,ready:bool,preview_accessible:bool,badge:&str,body:Option<&str>)->Sample {
        Sample{permission,locked:false,ready,preview_accessible,preview_ready:body.is_some(),
            items:if ready{vec![item("com.tencent.qq",badge)]}else{Vec::new()},
            previews:body.map(|body|vec![Preview{id:body.into(),bundle_id:"com.tencent.qq".into(),name:"QQ".into(),sender:"朋友".into(),body:body.into(),activation:String::new()}]).unwrap_or_default()}
    }
    #[test]
    fn notification_permission_grant_revoke_and_regrant_recover_without_replaying_history(){
        let mut config=config();config.previews=true;let mut session=SamplingSession::default();
        for _ in 0..3 {
            let denied=session.accept(Some(sample(false,false,false,"",None)),&config);
            assert_eq!(denied.status,Status::PermissionNeeded);assert!(denied.events.is_empty());
        }
        let baseline=session.accept(Some(sample(true,true,true,"4",Some("授权前的旧通知"))),&config);
        assert_eq!(baseline.status,Status::Watching{apps:1});assert!(baseline.events.is_empty());
        assert_eq!(baseline.status.accessibility_label(),"当前采样已获辅助功能权限");
        let new=session.accept(Some(sample(true,true,true,"5",Some("授权后的新通知"))),&config);
        assert_eq!(new.events.len(),1);assert_eq!(new.events[0].preview.as_ref().unwrap().body,"授权后的新通知");
        let revoked=session.accept(Some(sample(false,true,true,"6",Some("撤销时的延迟结果"))),&config);
        assert_eq!(revoked.status,Status::PermissionNeeded);assert!(revoked.events.is_empty());
        assert!(session.detector.previous.is_empty()&&session.detector.preview_seen.is_empty()&&!session.captured_preview);
        assert!(session.accept(Some(sample(true,true,true,"8",Some("重新授权前的通知"))),&config).events.is_empty());
        let recovered=session.accept(Some(sample(true,true,true,"9",Some("重新授权后的通知"))),&config);
        assert_eq!(recovered.events.len(),1);assert_eq!(recovered.events[0].preview.as_ref().unwrap().body,"重新授权后的通知");
        let mut locked=sample(true,true,true,"10",Some("锁屏时的延迟结果"));locked.locked=true;
        let locked=session.accept(Some(locked),&config);assert_eq!(locked.status,Status::Locked);assert!(locked.events.is_empty());
        assert!(session.detector.preview_seen.is_empty());
        assert!(session.accept(Some(sample(true,true,true,"10",Some("解锁前的通知"))),&config).events.is_empty());
        assert_eq!(session.accept(Some(sample(true,true,true,"11",Some("解锁后的新通知"))),&config).events.len(),1);
        session.reset();
        assert!(session.accept(Some(sample(true,true,true,"11",Some("解锁后的新通知"))),&config).events.is_empty(),"restart/configuration changes establish a quiet baseline");
    }
    #[test]
    fn notification_transient_channel_failures_do_not_erase_independent_baselines(){
        let mut config=config();config.previews=true;let mut session=SamplingSession::default();
        assert!(session.accept(Some(sample(true,true,true,"1",None)),&config).events.is_empty());
        let failed=session.accept(None,&config);assert_eq!(failed.status,Status::DockUnavailable);
        assert!(failed.events.is_empty());
        assert_eq!(session.accept(Some(sample(true,false,false,"",None)),&config).status,Status::DockUnavailable);
        let recovered=session.accept(Some(sample(true,true,true,"2",Some("短暂失联后的新消息"))),&config);
        assert_eq!(recovered.events.len(),1);assert!(recovered.events[0].preview.is_some(),"reconnection must not make a new preview a fresh startup baseline");
        // Losing just Dock must not overwrite its last successful unread count.
        assert!(session.accept(Some(sample(true,false,true,"",None)),&config).events.is_empty());
        let badge=session.accept(Some(sample(true,true,false,"3",None)),&config);
        assert_eq!(badge.events.len(),1);assert_eq!(badge.events[0].count,Some(3));
        // An unavailable preview channel cannot prime it with a fake empty list.
        let mut unprimed=SamplingSession::default();
        unprimed.accept(Some(sample(true,true,false,"1",None)),&config);
        assert!(unprimed.accept(Some(sample(true,true,true,"1",Some("首次公开的已有通知"))),&config).events.is_empty());
        assert_eq!(unprimed.accept(Some(sample(true,true,true,"1",Some("首次公开后的新通知"))),&config).events.len(),1);
    }
    #[test]
    fn notification_baselines_do_not_replay_on_start_restart_unlock_or_app_reappearance(){
        let mut detector=Detector::default();let config=config();
        assert!(detector.observe(vec![item("com.tencent.qq","9")],&config).is_empty());
        assert_eq!(detector.observe(vec![item("com.tencent.qq","10")],&config)[0].count,Some(10));
        assert!(detector.observe(vec![item("com.tencent.qq","10")],&config).is_empty());
        assert!(detector.observe(vec![item("com.tencent.qq","2")],&config).is_empty());
        detector.reset();assert!(detector.observe(vec![item("com.tencent.qq","30")],&config).is_empty());
        detector.observe(vec![],&config);assert!(detector.observe(vec![item("com.tencent.qq","40")],&config).is_empty());
        assert!(detector.observe(vec![item("com.tencent.qq","")],&config).is_empty());
        assert_eq!(detector.observe(vec![item("com.tencent.qq","•")],&config).len(),1);
        assert!(detector.observe(vec![item("com.tencent.qq","••")],&config).is_empty());
    }
    #[test]
    fn notification_preview_identity_keeps_equal_messages_and_resets_privacy_baselines(){
        let mut config=config();config.previews=true;let mut detector=Detector::default();
        let preview=|id:&str,body:&str|Preview{id:id.into(),bundle_id:"com.tencent.qq".into(),name:"QQ".into(),sender:"朋友".into(),body:body.into(),activation:id.into()};
        assert!(detector.previews(vec![preview("existing","旧消息")],&config).is_empty());
        assert!(detector.previews(vec![preview("existing","旧消息")],&config).is_empty());
        assert_eq!(detector.previews(vec![preview("new-1","相同消息"),preview("new-2","相同消息")],&config).len(),2);
        detector.reset();assert!(detector.previews(vec![preview("new-2","相同消息")],&config).is_empty());
        for i in 0..400{detector.previews(vec![preview(&format!("item-{i}"),"消息")],&config);}
        assert_eq!(detector.preview_seen.len(),256);
        config.previews=false;assert!(detector.previews(vec![preview("disabled","不能保留")],&config).is_empty());
        assert!(!detector.preview_seen.iter().any(|seen|seen.id=="disabled"));
    }
    #[test]
    fn notification_hydration_refreshes_delivery_while_startup_stays_quiet() {
        let mut config=config();config.previews=true;
        let preview=|id:&str,sender:&str,body:&str|Preview{id:id.into(),bundle_id:"com.tencent.qq".into(),name:"QQ".into(),sender:sender.into(),body:body.into(),activation:id.into()};
        let mut detector=Detector::default();
        assert!(detector.previews(vec![preview("ax:42:7:1:0","朋友","")],&config).is_empty());
        assert!(detector.previews(vec![preview("ax:42:7:1:2","朋友","启动时的旧消息")],&config).is_empty());
        assert!(detector.previews(vec![],&config).is_empty());
        let initial=detector.previews(vec![preview("ax:42:8:1:0","朋友","")],&config);
        assert_eq!(initial.len(),1);
        let hydrated=detector.previews(vec![preview("ax:42:8:1:3","朋友","稍后系统补齐正文")],&config);
        assert_eq!(hydrated.len(),1);
        assert_eq!(hydrated[0].preview.as_ref().unwrap().id,"ax:42:8:1:0");
        assert_eq!(hydrated[0].preview.as_ref().unwrap().activation,"ax:42:8:1:3");
        assert!(detector.previews(vec![preview("ax:42:8:1:3","朋友","稍后系统补齐正文")],&config).is_empty());
        assert!(detector.previews(vec![preview("ax:42:8:1:0","朋友","")],&config).is_empty(),"late placeholders must not erase hydrated text");
        assert_eq!(detector.previews(vec![preview("ax:42:8:1:33","朋友","稍后系统补齐正文，下一条")],&config).len(),1,"a nonempty changed body is a new native message");
        assert_eq!(detector.previews(vec![preview("ax:42:8:1:4","朋友","同节点新消息")],&config).len(),1);
        assert_eq!(detector.previews(vec![preview("ax:42:9:1:4","朋友","同节点新消息")],&config).len(),1);
        detector.reset();assert!(detector.preview_seen.is_empty());
        assert!(detector.previews(vec![preview("ax:42:8:1:4","朋友","同节点新消息")],&config).is_empty());
    }
    #[test]
    fn notification_detector_rejects_disabled_and_empty_identity_and_bounds_content() {
        let mut config=config();config.previews=true;let mut detector=Detector::default();detector.previews(vec![],&config);
        let preview=|id:&str,bundle:&str|Preview{id:id.into(),bundle_id:bundle.into(),name:"QQ".into(),sender:"发送者".repeat(300),body:format!("{}\0", "正文".repeat(1000)),activation:"action".into()};
        assert!(detector.previews(vec![preview("","com.tencent.qq"),preview("evil","com.other.app")],&config).is_empty());
        let events=detector.previews(vec![preview("bounded","com.tencent.qq")],&config);
        let parsed=events[0].preview.as_ref().unwrap();assert_eq!(parsed.sender.chars().count(),160);assert_eq!(parsed.body.chars().count(),1000);assert!(!parsed.body.contains('\0'));
        config.previews=false;assert!(detector.previews(vec![preview("disabled","com.tencent.qq")],&config).is_empty());
    }
    #[test]
    fn notification_pending_batch_keeps_all_distinct_deliveries_and_refreshes_in_place() {
        let preview=|id:usize,body:&str|Unread{bundle_id:"com.tencent.qq".into(),name:"QQ".into(),count:None,preview:Some(Preview{id:format!("delivery-{id}"),bundle_id:"com.tencent.qq".into(),name:"QQ".into(),sender:"朋友".into(),body:body.into(),activation:body.into()})};
        let mut batch=Batch::default();batch.append((0..80).map(|i|preview(i,"原文")).collect());
        assert_eq!(batch.events.len(),80);
        batch.append(vec![preview(1,"补齐正文"),preview(80,"下一条")]);assert_eq!(batch.events.len(),81);
        assert_eq!(batch.events[1].preview.as_ref().unwrap().body,"补齐正文");assert_eq!(batch.events[80].preview.as_ref().unwrap().id,"delivery-80");
        batch.events.clear();assert!(batch.events.is_empty());
    }
    #[test]
    fn notification_source_selection_is_case_insensitive_and_clones_are_bounded(){
        assert_eq!(Kind::of("com.tencent.xinWeChat"),Some(Kind::Wechat));
        assert_eq!(Kind::of("com.wechatmulti.clone2"),Some(Kind::Wechat));
        assert_eq!(Kind::of("com.wechatmulti.clone2.evil"),None);
        assert_eq!(Kind::of("com.tencent.QQMusic"),None);
        let mut config=config();config.disabled.push("COM.TENCENT.QQ".into());
        let mut detector=Detector::default();detector.observe(vec![item("com.tencent.qq","")],&config);
        assert!(detector.observe(vec![item("com.tencent.qq","3"),item("com.apple.mail","8")],&config).is_empty());
        assert!(detector.previous.is_empty());
    }
}

#[cfg(all(test,target_os="macos"))]
mod native_tests {
    use std::ffi::{c_char,CStr,CString};
    extern "C" {fn qy_notifications_fixture(input:*const c_char)->*mut c_char;fn qy_notifications_free(value:*mut c_char);}
    fn fixture(mut fields:serde_json::Value)->serde_json::Value {
        fields["config"]=serde_json::json!({"rules":[{"id":"com.tencent.qq","prefix":false},{"id":"com.tencent.xinwechat","prefix":false},{"id":"com.wechatmulti.clone","prefix":true}],"disabled":[]});
        fields["identifiers"]=serde_json::json!(["com.tencent.qq","com.tencent.xinWeChat","com.wechatmulti.clone2"]);
        let input=CString::new(fields.to_string()).unwrap();let value=unsafe{qy_notifications_fixture(input.as_ptr())};assert!(!value.is_null());
        let result=serde_json::from_slice(unsafe{CStr::from_ptr(value)}.to_bytes()).unwrap();unsafe{qy_notifications_free(value)};result
    }
    #[test]
    fn notification_native_sequoia_aggregates_and_attributed_text_preserve_commas(){
        let aggregate="朋友, 群聊, 明天见，带上苹果, 香蕉";
        let source=serde_json::json!({"stacking":"notification-app:com.tencent.qq","source":"QQ","description":aggregate,"element":7,"pid":42});
        let parsed=fixture(source.clone());assert_eq!(parsed["bundle_id"],"com.tencent.qq");assert_eq!(parsed["sender"],"");assert_eq!(parsed["body"],aggregate);
        let mut attributed=source.clone();attributed["description"]=serde_json::json!("");attributed["attributed"]=serde_json::json!(aggregate);
        assert_eq!(fixture(attributed)["body"],aggregate);
        let mut changed=source.clone();changed["description"]=serde_json::json!("同一原生节点，下一条消息");assert_ne!(fixture(changed)["id"],parsed["id"],"a reused AX node must not hide changed content");
        let mut other=source;other["element"]=serde_json::json!(8);assert_ne!(fixture(other)["id"],parsed["id"],"identical consecutive messages have distinct native identity");
    }
    #[test]
    fn notification_native_source_gate_rejects_unknown_apps_and_exact_suffix_spoofs(){
        for id in ["notification-app:com.tencent.qq.bad","notification-app:evilcom.tencent.qq","notification-app:com.other.app"] {
            assert_eq!(fixture(serde_json::json!({"stacking":id,"description":"不应读取或保留正文"})),serde_json::json!({}));
        }
        let clone=fixture(serde_json::json!({"stacking":"notification-app:com.wechatmulti.clone2","texts":["微信 2","发送者","原始正文，含逗号"],"source":"微信 2","element":5,"pid":42}));
        assert_eq!(clone["bundle_id"],"com.wechatmulti.clone2");assert_eq!(clone["sender"],"发送者");assert_eq!(clone["body"],"原始正文，含逗号");
    }
    fn text(value:&str)->serde_json::Value {serde_json::json!({"role":"AXStaticText","value":value})}
    fn notice(element:u32,body:&str)->serde_json::Value {serde_json::json!({"role":"AXButton","element":element,"children":[text("QQ"),text("朋友"),text(body)]})}
    #[test]
    fn notification_native_tree_unknown_wrapper_and_inherited_stack_keep_separate_notices() {
        let tree=serde_json::json!({"role":"AXWindow","stacking":"opaque-system-wrapper","children":[
            {"role":"AXGroup","stacking":"notification-app:com.tencent.qq","children":[notice(1,"第一条"),notice(2,"第二条")]}]});
        let parsed=fixture(serde_json::json!({"tree":tree,"pid":42}));let previews=parsed["previews"].as_array().unwrap();
        assert_eq!(previews.len(),2);assert_eq!(previews[0]["sender"],"朋友");assert_eq!(previews[0]["body"],"第一条");assert_eq!(previews[1]["body"],"第二条");assert_ne!(previews[0]["id"],previews[1]["id"]);
        assert_eq!(parsed["diagnostics"]["captures"][0]["source_mode"],"inherited");
    }
    #[test]
    fn notification_native_tree_app_time_children_do_not_block_aggregate_or_attributed_fallback() {
        let aggregate="QQ, 现在, 朋友, 群聊, 苹果, 香蕉";
        let mut tree=serde_json::json!({"role":"AXButton","stacking":"notification-app:com.tencent.qq","element":4,"children":[text("QQ"),text("2分钟前")],"description":aggregate});
        let parsed=fixture(serde_json::json!({"tree":tree,"pid":42}));assert_eq!(parsed["previews"][0]["sender"],"");assert_eq!(parsed["previews"][0]["body"],aggregate);assert_eq!(parsed["diagnostics"]["captures"][0]["parser"],"description");
        tree["description"]=serde_json::json!("notification");tree["attributed"]=serde_json::json!(aggregate);
        let attributed=fixture(serde_json::json!({"tree":tree,"pid":42}));assert_eq!(attributed["previews"][0]["body"],aggregate);assert_eq!(attributed["diagnostics"]["captures"][0]["parser"],"attributed-description");
    }
    #[test]
    fn notification_native_tree_source_spoofs_never_read_message_fields() {
        for tree in [
            serde_json::json!({"role":"AXGroup","children":[text("QQ"),text("微信"),text("伪造正文")]}),
            serde_json::json!({"role":"AXButton","subrole":"AXNotification","title":"QQ","description":"伪造正文"}),
            serde_json::json!({"role":"AXButton","subrole":"AXNotification","stacking":"notification-app:com.other.app","title":"QQ","children":[text("QQ"),text("伪造正文")]}),
            serde_json::json!({"role":"AXGroup","stacking":"notification-app:com.tencent.qq","children":[{"role":"AXButton","stacking":"notification-app:evilcom.tencent.qq","description":"伪造正文"}]}),
        ] {
            let parsed=fixture(serde_json::json!({"tree":tree,"pid":42}));assert_eq!(parsed["previews"],serde_json::json!([]));assert!(parsed["diagnostics"]["statistics"]["content_nodes"].as_u64().unwrap_or(0)==0);
        }
    }
    #[test]
    fn notification_native_tree_bounded_cancelled_and_metadata_only_diagnostics() {
        let private="私密发送者，验证码123456，私密正文";
        let tree=serde_json::json!({"role":"AXButton","stacking":"notification-app:com.tencent.qq","description":private,"element":88});
        let parsed=fixture(serde_json::json!({"tree":tree,"pid":42}));assert_eq!(parsed["previews"][0]["body"],private);
        assert!(!parsed["diagnostics"].to_string().contains(private));assert!(!parsed["diagnostics"].to_string().contains("123456"));
        let cancelled=fixture(serde_json::json!({"tree":tree,"pid":42,"cancel_after":5}));assert_eq!(cancelled["previews"],serde_json::json!([]));assert_eq!(cancelled["diagnostics"]["cancelled"],true);
        let mut nested=notice(1,"不可到达的正文");for _ in 0..25{nested=serde_json::json!({"role":"AXGroup","children":[nested]});}
        let limited=fixture(serde_json::json!({"tree":nested,"pid":42}));assert_eq!(limited["previews"],serde_json::json!([]));assert_eq!(limited["diagnostics"]["bounded"],true);
        let limited=fixture(serde_json::json!({"tree":{"role":"AXGroup","stacking":"notification-app:com.tencent.qq","children":[notice(1,"一"),notice(2,"二")]},"pid":42,"budget":1}));assert_eq!(limited["previews"],serde_json::json!([]));assert_eq!(limited["diagnostics"]["bounded"],true);
        let menu=fixture(serde_json::json!({"tree":{"role":"AXMenuBar","children":[tree]},"pid":42}));assert_eq!(menu["previews"],serde_json::json!([]));assert_eq!(menu["diagnostics"]["statistics"]["skipped_menus"],1);
    }
    #[test]
    fn notification_native_tree_static_descriptions_and_unicode_bounds_work() {
        let parsed=fixture(serde_json::json!({"tree":{"role":"AXGroup","stacking":"notification-app:com.tencent.qq","children":[{"role":"AXButton","element":1,"children":[text("QQ"),{"role":"AXStaticText","description":"发送者"},{"role":"AXStaticText","attributed":"正文，保留逗号"}]}]},"pid":42}));
        assert_eq!(parsed["previews"][0]["sender"],"发送者");assert_eq!(parsed["previews"][0]["body"],"正文，保留逗号");
        let parsed=fixture(serde_json::json!({"tree":{"role":"AXButton","stacking":"notification-app:com.tencent.qq","description":"🙂".repeat(2000),"element":2},"pid":42}));
        let body=parsed["previews"][0]["body"].as_str().unwrap();assert!(body.encode_utf16().count()<=1200);assert!(!body.contains('�'));
    }

    #[test]
    fn notification_native_tree_more_than_32_notices_are_individually_returned() {
        let notices=(0..40).map(|i|notice(i,&format!("消息{i}"))).collect::<Vec<_>>();
        let parsed=fixture(serde_json::json!({"tree":{"role":"AXGroup","stacking":"notification-app:com.tencent.qq","children":notices},"pid":42}));
        let previews=parsed["previews"].as_array().unwrap();assert_eq!(previews.len(),40);assert_eq!(previews[39]["body"],"消息39");
    }
    #[test]
    fn notification_native_tree_controls_do_not_hide_parent_aggregate() {
        let parsed=fixture(serde_json::json!({"tree":{"role":"AXButton","stacking":"notification-app:com.tencent.qq","description":"发送者, 真实合并正文","element":9,"children":[text("QQ"),text("现在"),{"role":"AXButton","description":"关闭通知"}]},"pid":42}));
        assert_eq!(parsed["previews"].as_array().unwrap().len(),1);assert_eq!(parsed["previews"][0]["body"],"发送者, 真实合并正文");
    }

    #[test]
    fn notification_native_real_wechat_banner_ignores_unknown_inherited_controls() {
        // Observed macOS 15 structure: exact-source banner, sender/body static
        // fields, then two inherited action buttons appearing on hover.
        let banner=serde_json::json!({"role":"AXGroup","subrole":"AXNotificationCenterBanner","stacking":"notification-app:com.tencent.xinWeChat","identifier":"banner-instance","element":704,"actions":["AXPress"],"description":"微信, 设计组, 这是测试文","children":[text("设计组"),text("这是测试文")]});
        let baseline=fixture(serde_json::json!({"tree":banner,"pid":698}));
        let mut hovered=banner.clone();
        hovered["children"].as_array_mut().unwrap().extend([
            serde_json::json!({"role":"AXButton","identifier":"opaque-action-1","description":"某个未知操作","attributed":"某个未知操作","actions":["AXPress"],"element":705,"children":[text("操作按钮文字")]}),
            serde_json::json!({"role":"AXButton","description":"回复","actions":["AXPress"],"element":706}),
        ]);
        let parsed=fixture(serde_json::json!({"tree":{"role":"AXWindow","subrole":"AXSystemDialog","children":[{"role":"AXGroup","subrole":"AXHostingView","children":[{"role":"AXScrollArea","children":[hovered]}]}]},"pid":698}));
        let previews=parsed["previews"].as_array().unwrap();assert_eq!(previews.len(),1);
        assert_eq!(previews[0]["sender"],"设计组");assert_eq!(previews[0]["body"],"这是测试文");
        assert_eq!(previews[0]["id"],baseline["previews"][0]["id"],"hover controls cannot change a message's identity or content");
        assert_eq!(previews[0]["activation"],previews[0]["id"],"the parent notification keeps its AXPress action");
        assert!(previews[0]["activation"].as_str().unwrap().starts_with("ax:698:704:"));
        assert_eq!(parsed["diagnostics"]["statistics"]["candidates"],1);
        assert_eq!(parsed["diagnostics"]["captures"][0]["parser"],"static-text");
        for node in parsed["diagnostics"]["nodes"].as_array().unwrap().iter().filter(|node|node["role"]=="AXButton") {
            assert_eq!(node["candidate"],false);assert!(node.get("description_length").is_none(),"control descriptions are never read as message fields");
        }
    }

    #[test]
    fn notification_native_button_stack_keeps_its_separate_child_notices() {
        let parsed=fixture(serde_json::json!({"tree":{"role":"AXButton","subrole":"AXNotificationCenterStack","stacking":"notification-app:com.tencent.qq","description":"不能把整组当成一条消息","children":[notice(1,"第一条"),notice(2,"第二条")]},"pid":42}));
        let previews=parsed["previews"].as_array().unwrap();assert_eq!(previews.len(),2);assert_eq!(previews[0]["body"],"第一条");assert_eq!(previews[1]["body"],"第二条");
    }

}
