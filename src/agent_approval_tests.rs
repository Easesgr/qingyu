//! Real structured source fixtures through the journal, persistent HUD, inbox,
//! privacy cleanup and external activation commands. No agent is approved here.
use super::*;
use crate::{agent_monitor::{AgentKind,Notice,NoticeKind,Snapshot,Status},app::{AGENT_PAGE,NOTIFICATIONS_PAGE},extras::{Alert,AlertSource}};

fn island(dock:Dock)->Side{
    let mut h=H::new();h.app.settings.dock=dock;h.app.settings.macos_side_handle=false;
    h.app.settings.island_shape=crate::model::IslandShape::Pill;h.app.settings.notification_preview=true;
    h.app.settings.skin=crate::model::Skin::GlassDark;
    Side{h,out:None}
}
fn start(s:&mut Side,agent:AgentKind)->Vec<Snapshot>{
    let snapshots=crate::agent_monitor::approval_fixture(agent);
    s.h.app.agent_snapshot=snapshots[0].clone();s.frame(vec![]);
    s.h.t=1.0;s.h.app.agent_snapshot=snapshots[1].clone();s.frame(vec![]);
    s.h.t=2.0;s.h.app.agent_snapshot=snapshots[2].clone();s.frame(vec![]);s.settle();
    snapshots
}
fn approvals(s:&Side)->usize{
    s.h.app.alerts.iter().chain(s.h.app.notice.iter()).filter(|alert|alert.requires_acknowledgement()).count()
}
fn message(app:&mut App,index:usize){
    app.enqueue_alert(Alert{source:AlertSource::LocalTest,title:format!("消息 {index}"),body:"普通消息".into(),page:NOTIFICATIONS_PAGE,
        at:app.t,duration:crate::extras::NOTICE_DURATION},false);
}
fn add_approval(s:&mut Side,id:&str,turn:u64){
    let mut session=s.h.app.agent_snapshot.sessions[0].clone();session.id=id.into();session.turn_key=turn;session.status=Status::Approval;
    let sequence=s.h.app.agent_snapshot.notices.iter().map(|event|event.sequence).max().unwrap_or(0)+1;
    let at_ms=1_794_700_000_000+(s.h.t*1000.0)as u64;
    s.h.app.agent_snapshot.sessions.push(session.clone());
    s.h.app.agent_snapshot.notices.push(Notice{sequence,at_ms,kind:NoticeKind::Approval,session});
    s.frame(vec![]);
}

#[test]
fn approval_source_to_hud_stays_until_row_click_and_only_opens_the_original_app(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for agent in [AgentKind::Codex,AgentKind::Claude,AgentKind::Hermes]{for control in ["notice-summary"]{
        let mut s=island(Dock::Right);start(&mut s,agent);
        let alert=s.h.app.notice.as_ref().unwrap();assert!(alert.requires_acknowledgement());
        assert_eq!(alert.title,"审批测试会话");assert!(alert.body.contains("需要审批"));
        assert_eq!(s.h.app.history.iter().filter(|alert|alert.requires_acknowledgement()).count(),1);
        let previous_page=s.h.app.page();s.h.t+=12.0*3600.0;s.frame(vec![egui::Event::PointerGone]);
        assert!(s.h.app.notice.as_ref().unwrap().requires_acknowledgement());
        assert!(!s.h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenAgent{..})),"approval must wait for an explicit click");
        s.click(control);
        assert_eq!(s.h.app.page(),previous_page);assert!(s.h.app.notice.is_none());assert_eq!(approvals(&s),0);assert!(s.h.app.history.is_empty());
        assert!(s.h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenAgent{agent:actual,pid:Some(42),observed_at_ms:Some(1_794_700_002_000),..}if *actual==agent)));
        s.frame(vec![]);assert_eq!(approvals(&s),0,"the same journal request cannot replay after acknowledgement");
    }}
}

#[test]
fn unclicked_approval_survives_completion_interruption_and_new_turn_as_a_past_request(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for terminal in [Status::Completed,Status::Error,Status::Idle,Status::Stopped]{
        let mut s=island(Dock::Right);let snapshots=start(&mut s,AgentKind::Claude);
        s.h.t=4.0;
        let mut source=snapshots[3].clone();
        if terminal!=Status::Completed{
            source.sessions[0].status=terminal;source.notices[2].kind=NoticeKind::Interrupted;source.notices[2].session.status=terminal;source.completions.clear();
        }
        s.h.app.agent_snapshot=source;s.frame(vec![]);
        assert_eq!(approvals(&s),1);let past=s.h.app.history.iter().find(|alert|alert.requires_acknowledgement()).unwrap();
        assert!(past.body.contains("曾需审批"));assert!(past.body.contains(if terminal==Status::Completed{"本轮已完成"}else{"本轮已结束"}));
        s.settle(); // The finite result's reading interval begins after handoff.
        s.h.t+=4.0*3600.0;s.frame(vec![egui::Event::PointerGone]);assert!(s.h.app.notice.as_ref().unwrap().requires_acknowledgement());
        s.h.app.agent_snapshot.sessions[0].turn_key+=1;s.h.app.agent_snapshot.sessions[0].status=Status::Thinking;s.frame(vec![]);
        assert!(s.h.app.notice.as_ref().unwrap().body.contains("已进入下一轮"));assert_eq!(approvals(&s),1);
        // The monitor may eventually discard the old process and journals;
        // the retained notice still targets the originally observed owner.
        s.h.app.agent_snapshot=Snapshot{generation:193,..Default::default()};s.frame(vec![]);s.settle();s.click("notice-summary");
        assert!(s.h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenAgent{agent:AgentKind::Claude,pid:Some(42),observed_at_ms:Some(1_794_700_002_000),..})));
        assert!(s.h.app.history.is_empty());assert_eq!(approvals(&s),0);
    }
}

#[test]
fn multiple_approvals_are_not_evicted_by_messages_queue_limits_or_history_limits(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut s=island(Dock::Right);start(&mut s,AgentKind::Codex);
    for index in 1..=12{add_approval(&mut s,&format!("codex:approval-{index}"),index+100);}
    assert_eq!(approvals(&s),13);
    for index in 0..80{message(&mut s.h.app,index);}
    s.frame(vec![]);
    assert!(!s.h.app.notice.as_ref().unwrap().requires_acknowledgement(),"ordinary messages can be read while approval remains pending");
    assert_eq!(approvals(&s),13);assert_eq!(s.h.app.history.iter().filter(|alert|alert.requires_acknowledgement()).count(),13);
    assert_eq!(s.h.app.history.iter().filter(|alert|!alert.requires_acknowledgement()).count(),50);
    assert!(s.h.app.history.iter().any(|alert|alert.title=="消息 79"));
    for _ in 0..9{s.settle();s.h.t+=16.0;s.frame(vec![egui::Event::PointerGone]);}
    assert!(s.h.app.notice.as_ref().unwrap().requires_acknowledgement());assert_eq!(approvals(&s),13);
    let first=s.h.app.notice.clone().unwrap();s.h.app.dismiss_notice(&first);s.frame(vec![]);
    assert_eq!(approvals(&s),12);assert_eq!(s.h.app.history.iter().filter(|alert|alert.requires_acknowledgement()).count(),12);
    s.h.t+=24.0*3600.0;s.frame(vec![]);assert_eq!(approvals(&s),12);
}

#[test]
fn ordinary_and_waiting_notices_keep_their_finite_lifetimes_while_approval_is_preserved(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut s=island(Dock::Right);start(&mut s,AgentKind::Codex);message(&mut s.h.app,1);s.frame(vec![]);
    s.settle();let at=s.h.app.notice.as_ref().unwrap().at;s.h.t=at+14.0;s.frame(vec![]);assert_eq!(s.h.app.notice.as_ref().unwrap().title,"消息 1");
    s.h.t=at+15.1;s.frame(vec![]);assert!(s.h.app.notice.as_ref().unwrap().requires_acknowledgement());
    let mut session=s.h.app.agent_snapshot.sessions[0].clone();session.status=Status::Waiting;
    let sequence=s.h.app.agent_snapshot.notices.last().unwrap().sequence+1;
    s.h.app.agent_snapshot.notices.push(Notice{sequence,at_ms:1_794_700_000_000+(s.h.t*1000.0)as u64,kind:NoticeKind::Waiting,session:session.clone()});
    s.h.app.agent_snapshot.sessions[0]=session;s.frame(vec![]);
    assert_eq!(s.h.app.notice.as_ref().unwrap().duration,20.0);assert!(!s.h.app.notice.as_ref().unwrap().requires_acknowledgement());
    s.settle();let at=s.h.app.notice.as_ref().unwrap().at;s.h.t=at+19.0;s.frame(vec![]);assert!(!s.h.app.notice.as_ref().unwrap().requires_acknowledgement());
    s.h.t=at+20.1;s.frame(vec![]);assert!(s.h.app.notice.as_ref().unwrap().requires_acknowledgement());assert_eq!(approvals(&s),1);
}

#[test]
fn closing_the_approval_is_acknowledgement_and_does_not_grant_or_launch_anything(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut s=island(Dock::Right);start(&mut s,AgentKind::Codex);s.click("notice-dismiss");s.frame(vec![]);
    assert!(s.h.app.notice.is_none());assert!(s.h.app.history.is_empty());assert_eq!(approvals(&s),0);
    assert_eq!(s.h.app.agent_snapshot.sessions[0].status,Status::Approval,"closing Qingyu's reminder cannot change the source approval decision");
    assert!(!s.h.cmds.iter().any(|cmd|matches!(cmd,crate::app::Cmd::OpenAgent{..}|crate::app::Cmd::Notify(..))));
}

#[test]
fn approval_duplicates_and_repeated_permission_events_never_create_periodic_spam(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let mut s=island(Dock::Right);start(&mut s,AgentKind::Codex);
    for index in 0..10{
        let event=s.h.app.agent_snapshot.notices[1].clone();
        s.h.app.agent_snapshot.notices.push(Notice{sequence:3+index,at_ms:1_794_700_003_000,..event});
        s.h.t+=45.0;s.frame(vec![]);
        assert_eq!(approvals(&s),1);assert_eq!(s.h.app.history.len(),1);
    }
}

#[test]
fn acknowledging_one_request_then_resuming_allows_a_new_sticky_approval_in_the_same_turn(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let snapshots=crate::agent_monitor::approval_episode_fixture();let mut s=island(Dock::Right);
    for (phase,snapshot) in snapshots.iter().enumerate().take(3){s.h.t=phase as f64;s.h.app.agent_snapshot=snapshot.clone();s.frame(vec![]);}
    s.settle();let first=s.h.app.notice.as_ref().unwrap().clone();assert!(first.requires_acknowledgement());
    let first_key=first.source.approval_key();assert_ne!(first_key,0);s.click("notice-summary");assert_eq!(approvals(&s),0);
    s.h.t=4.0;s.h.app.agent_snapshot=snapshots[4].clone();s.frame(vec![]);assert_eq!(approvals(&s),0);
    s.h.t=5.0;s.h.app.agent_snapshot=snapshots[5].clone();s.frame(vec![]);s.settle();
    let second=s.h.app.notice.as_ref().unwrap();assert!(second.requires_acknowledgement());
    assert_ne!(second.source.approval_key(),first_key);assert_eq!(second.source.agent_notice(),first.source.agent_notice());
    assert_eq!(approvals(&s),1);assert_eq!(s.h.app.history.len(),1);
    s.h.t=7.0;s.h.app.agent_snapshot=snapshots[6].clone();s.frame(vec![]);assert_eq!(approvals(&s),1);
    s.h.t+=10.0*3600.0;s.frame(vec![]);assert!(s.h.app.notice.as_ref().unwrap().requires_acknowledgement());
    s.click("notice-dismiss");s.frame(vec![]);assert_eq!(approvals(&s),0);assert!(s.h.app.history.is_empty());
    assert_eq!(s.h.cmds.iter().filter(|cmd|matches!(cmd,crate::app::Cmd::OpenAgent{..})).count(),1);
}

#[test]
fn two_unacknowledged_requests_in_one_turn_remain_individually_reachable(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let snapshots=crate::agent_monitor::approval_episode_fixture();let mut s=island(Dock::Right);
    s.h.app.agent_snapshot=snapshots[0].clone();s.frame(vec![]);
    s.h.t=5.0;s.h.app.agent_snapshot=snapshots[5].clone();s.frame(vec![]);s.settle();
    assert_eq!(approvals(&s),2);assert_eq!(s.h.app.history.len(),2);
    assert!(s.h.app.notice.as_ref().unwrap().body.contains("新的审批请求"));
    s.click("notice-dismiss");s.frame(vec![]);s.settle();assert_eq!(approvals(&s),1);assert_eq!(s.h.app.history.len(),1);
    assert!(s.h.app.notice.as_ref().unwrap().body.contains("需要审批"));s.click("notice-summary");s.frame(vec![]);
    assert_eq!(approvals(&s),0);assert!(s.h.app.history.is_empty());
}

#[test]
fn approval_preview_redaction_privacy_lock_optout_and_restart_remain_quiet(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for off in ["lock","notifications","activity","module"]{
        let mut s=island(Dock::Right);let snapshots=start(&mut s,AgentKind::Claude);
        s.h.app.settings.notification_preview=false;s.h.app.reconcile_notification_privacy();
        for alert in s.h.app.history.iter().chain(s.h.app.notice.iter()).chain(s.h.app.alerts.iter()){
            assert!(!alert.title.contains("审批测试会话"));assert!(alert.body.contains("需要审批"));
        }
        match off{
            "lock"=>s.h.app.set_notification_session_locked(true),
            "notifications"=>s.h.app.settings.notifications=false,
            "activity"=>s.h.app.settings.agent_activity=false,
            _=>s.h.app.settings.visible_pages[AGENT_PAGE]=false,
        }
        s.h.app.reconcile_notification_privacy();assert!(s.h.app.history.is_empty());assert!(s.h.app.notice.is_none());assert!(s.h.app.hud_notice.is_none());assert_eq!(approvals(&s),0);
        s.frame(vec![]);
        s.h.app.settings.notifications=true;s.h.app.settings.agent_activity=true;s.h.app.settings.visible_pages[AGENT_PAGE]=true;s.h.app.set_notification_session_locked(false);
        s.h.t+=1.0;s.frame(vec![]);assert_eq!(approvals(&s),0);
        let mut restarted=snapshots[2].clone();restarted.generation+=1;s.h.app.agent_snapshot=restarted;s.frame(vec![]);
        assert_eq!(approvals(&s),0);assert!(s.h.app.history.is_empty());
    }
}

#[test]
fn startup_approval_history_and_delayed_permission_events_do_not_replay(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let snapshots=crate::agent_monitor::approval_fixture(AgentKind::Codex);let mut s=island(Dock::Right);
    s.h.t=8.0*3600.0;s.h.app.agent_snapshot=snapshots[2].clone();s.frame(vec![]);assert_eq!(approvals(&s),0);
    let mut delayed=snapshots[2].notices[1].clone();delayed.sequence=3;
    s.h.app.agent_snapshot.notices.push(delayed);s.frame(vec![]);assert_eq!(approvals(&s),0);assert!(s.h.app.history.is_empty());
}

#[test]
fn a_live_approval_journal_event_survives_a_long_ui_pause_after_the_quiet_baseline(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    let snapshots=crate::agent_monitor::approval_fixture(AgentKind::Codex);let mut s=island(Dock::Right);
    s.h.app.agent_snapshot=snapshots[0].clone();s.frame(vec![]);
    s.h.t=10.0*3600.0;s.h.app.agent_snapshot=snapshots[3].clone();s.frame(vec![]);s.settle();
    assert_eq!(approvals(&s),1);assert!(s.h.app.notice.as_ref().unwrap().body.contains("曾需审批 · 本轮已完成"));
    s.h.t+=10.0*3600.0;s.frame(vec![]);assert_eq!(approvals(&s),1);
}

#[test]
fn approval_single_row_and_distinct_task_state_fit_all_docks(){
    let _guard=ENV_LOCK.lock().unwrap_or_else(|e|e.into_inner());
    for dock in [Dock::Top,Dock::Left,Dock::Right]{
        let mut s=island(dock);start(&mut s,AgentKind::Codex);
        let surface=s.h.app.notice_surface(Rect::from_min_size(Pos2::ZERO,s.h.app.canvas_size())).unwrap().rect;
        let texts:Vec<_>=s.h.shapes.iter().filter_map(|shape|if let egui::Shape::Text(text)=&shape.shape{surface.intersects(text.visual_bounding_rect()).then_some(text)}else{None}).collect();
        let title=texts.iter().find(|text|text.galley.text().contains("审批测试")).unwrap();
        let state=texts.iter().find(|text|text.galley.text().contains("需要审批")).unwrap();
        assert!((title.visual_bounding_rect().center().y-state.visual_bounding_rect().center().y).abs()<3.0);
        assert!(surface.contains_rect(title.visual_bounding_rect()));assert!(surface.contains_rect(state.visual_bounding_rect()));
        assert!(!texts.iter().any(|text|["批准","同意","拒绝"].contains(&text.galley.text())));
        assert_eq!(s.h.app.agent_tasks.sessions()[0].status,Status::Approval);
        s.h.app.dismiss_notice(&s.h.app.notice.clone().unwrap());s.h.app.show_page(AGENT_PAGE);s.settle();
        assert!(s.h.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.text().contains("需要审批"))));
    }
}
