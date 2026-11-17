use super::*;

fn process() -> Process {
    Process { pid:42,parent:1,agent:AgentKind::Codex,birth:"fixture".into(),started_ms:1,
        service:false,role:ProcessRole::Cli,launcher:false,project:"/demo/qingyu".into(),
        sessions:HashSet::new(),rollouts:HashMap::new(),session_scan_complete:false }
}

fn event(status:Status,at_ms:u64,turn:&str)->Event {
    Event{id:"demo-session".into(),agent:AgentKind::Codex,status,at_ms,source:Source::Log,
        turn:turn.into(),project:"/demo/qingyu".into(),tool:String::new(),pid:Some(42),
        heartbeat:false,starts_turn:status==Status::Thinking,ends_turn:matches!(status,Status::Completed|Status::Error|Status::Idle|Status::Stopped),title:String::new(),start_ms:None}
}

#[test]
fn completion_journal_keeps_short_turns_and_deduplicates_hook_log_reordering(){
    let mut core=Core::default();let found=[process()];let parents=HashMap::new();
    core.apply(event(Status::Thinking,1000,"one"),&found,&parents,1000);
    let first=core.states["codex:demo-session"].session.turn_key;
    core.apply(event(Status::Completed,1100,"one"),&found,&parents,1100);
    let mut duplicate=event(Status::Completed,1101,"one");duplicate.source=Source::Hook;
    core.apply(duplicate,&found,&parents,1101);
    core.apply(event(Status::Working,1200,"one"),&found,&parents,1200);
    core.apply(event(Status::Thinking,1201,"one"),&found,&parents,1201);
    assert_eq!(core.completions.len(),1);assert_eq!(core.states["codex:demo-session"].session.status,Status::Completed);
    core.apply(event(Status::Thinking,1300,"two"),&found,&parents,1300);
    core.apply(event(Status::Completed,1400,"two"),&found,&parents,1400);
    core.apply(event(Status::Completed,1500,"one"),&found,&parents,1500);
    assert_eq!(core.completions.len(),2);assert_ne!(first,core.completions[1].session.turn_key);
    assert_eq!(core.completions[0].session.started_at,core.completions[1].session.started_at,"same-second turns still have independent identities");
    core.reconcile(&[],1600);assert_eq!(core.completions.len(),2,"a following process exit cannot erase completion evidence");
}

#[test]
fn completion_journal_is_bounded_and_errors_cancellation_and_process_presence_are_quiet(){
    let mut core=Core::default();let found=[process()];let parents=HashMap::new();
    for (i,status) in [Status::Running,Status::Error,Status::Idle,Status::Stopped].into_iter().enumerate(){core.apply(event(status,1000+i as u64,""),&found,&parents,1000+i as u64);}
    assert!(core.completions.is_empty());
    for i in 0..200 {
        let at=2000+i*2;let turn=format!("turn-{i}");
        core.apply(event(Status::Thinking,at,&turn),&found,&parents,at);
        core.apply(event(Status::Completed,at+1,&turn),&found,&parents,at+1);
    }
    assert_eq!(core.completions.len(),MAX_SESSIONS);assert_eq!(core.sequence,200);
}

/// Isolated real JSONL tail -> parser -> lifecycle core -> public snapshots.
/// No live accounts, process discovery, hooks, permissions or OS notifications.
pub(crate) fn fixture() -> Vec<Snapshot> {
    static SERIAL: AtomicU64=AtomicU64::new(0);
    let folder=std::env::temp_dir().join(format!("qingyu-notice-log-fixture-{}-{}",std::process::id(),SERIAL.fetch_add(1,Ordering::Relaxed)));
    let paths=Paths{root:folder.join("data"),codex:folder.join("codex"),claude:folder.join("claude"),hermes:folder.join("hermes")};
    let directory=paths.codex.join("sessions/2026/11/15");fs::create_dir_all(&directory).unwrap();
    let path=directory.join("rollout-2026-11-15T09-00-00-11111111-1111-1111-1111-111111111111.jsonl");
    let base=1_794_700_000_000u64;
    let line=|kind:&str,payload:Value,delta:u64|{
        let mut bytes=serde_json::to_vec(&json!({"type":kind,"timestamp":DateTime::<Utc>::from_timestamp_millis((base+delta)as i64).unwrap().to_rfc3339(),"payload":payload})).unwrap();bytes.push(b'\n');bytes
    };
    fs::write(&path,line("session_meta",json!({"id":"11111111-1111-1111-1111-111111111111","cwd":"/demo/qingyu"}),0)).unwrap();
    let mut core=Core::default();let mut logs=Logs::default();let found=[process()];let parents=HashMap::new();let mut snapshots=Vec::new();
    for phase in 0..3 {
        let now=base+phase*1000;
        if phase==1 {
            let mut file=OpenOptions::new().append(true).open(&path).unwrap();
            file.write_all(&line("event_msg",json!({"type":"task_started","turn_id":"first"}),900)).unwrap();
            file.write_all(&line("response_item",json!({"type":"function_call","turn_id":"first","name":"exec_command","arguments":"SECRET TOOL INPUT"}),950)).unwrap();
        }
        if phase==2 {
            let mut file=OpenOptions::new().append(true).open(&path).unwrap();
            for (kind,turn,delta) in [("task_complete","first",1800),("task_started","second",1850),("task_complete","second",1900)] {
                file.write_all(&line("event_msg",json!({"type":kind,"turn_id":turn,"message":"SECRET PROMPT OR RESPONSE"}),delta)).unwrap();
            }
        }
        // The log scanner checks file freshness against the simulated event clock.
        // Keep filesystem metadata on that clock as well as the JSONL records.
        File::open(&path).unwrap().set_modified(UNIX_EPOCH+Duration::from_millis(now)).unwrap();
        let mut events=logs.poll(&paths,now);events.sort_by_key(|event|(event.at_ms,event.source));
        for event in events{core.apply(event,&found,&parents,now);}
        core.update_title(AgentKind::Codex,"11111111-1111-1111-1111-111111111111","完善岛内通知");
        snapshots.push(Snapshot{generation:99,sessions:core.reconcile(&found,now),completions:core.completions.iter().cloned().collect(),notices:core.notices.iter().cloned().collect(),..Default::default()});
    }
    fs::remove_dir_all(folder).unwrap();
    snapshots
}

/// Structured, privacy-safe source -> parser -> core journal snapshots used by
/// the approval HUD interaction tests. No installed agents or permissions.
pub(crate) fn approval_fixture(agent:AgentKind)->Vec<Snapshot>{
    let base=1_794_700_000_000;let mut core=Core::default();let mut owner=process();owner.agent=agent;
    let found=[owner];let parents=HashMap::new();let mut snapshots=vec![Snapshot{generation:193,..Default::default()}];
    let mut metadata=LogMeta{id:"approval-session".into(),..Default::default()};
    for phase in 1..=3{
        let at=base+phase*1000;
        let parsed=if agent==AgentKind::Codex{
            let kind=match phase{1=>"task_started",2=>"exec_approval_request",_=>"task_complete"};
            log_event(&serde_json::to_vec(&json!({"type":"event_msg","timestamp":DateTime::<Utc>::from_timestamp_millis(at as i64).unwrap().to_rfc3339(),
                "payload":{"type":kind,"turn_id":"approval-turn","tool_name":"exec_command","prompt":"PRIVATE PROMPT","command":"PRIVATE INPUT"}})).unwrap(),agent,&mut metadata).unwrap()
        }else{
            let kind=match phase{1=>"UserPromptSubmit",2=>"PermissionRequest",_=>"Stop"};
            decode_spool(&serde_json::to_vec(&json!({"version":1,"agent":agent.key(),"event":kind,"at_ms":at,
                "session_id":"approval-session","turn_id":"approval-turn","pid":42,"tool":"terminal","prompt":"PRIVATE PROMPT"})).unwrap(),at).unwrap()
        };
        core.apply(parsed,&found,&parents,at);
        core.update_title(agent,"approval-session","审批测试会话");
        snapshots.push(Snapshot{generation:193,sessions:core.reconcile(&found,at),notices:core.notices.iter().cloned().collect(),
            completions:core.completions.iter().cloned().collect(),..Default::default()});
    }
    snapshots
}

pub(crate) fn approval_episode_fixture()->Vec<Snapshot>{
    let agent=AgentKind::Claude;let base=1_794_700_000_000;
    let mut core=Core::default();let mut owner=process();owner.agent=agent;let found=[owner];let parents=HashMap::new();
    let mut snapshots=vec![Snapshot{generation:194,..Default::default()}];
    for (index,kind) in ["UserPromptSubmit","PermissionRequest","Notification","PreToolUse","PermissionRequest","Notification","Stop"].into_iter().enumerate(){
        let at=base+(index as u64+1)*1000;
        let body=json!({"version":1,"agent":"claude","event":kind,"notification_type":"permission_prompt","at_ms":at,
            "session_id":"approval-session","turn_id":"same-turn","pid":42,"tool":"terminal"});
        let parsed=decode_spool(&serde_json::to_vec(&body).unwrap(),at).unwrap();
        core.apply(parsed.clone(),&found,&parents,at);
        if kind=="PermissionRequest"{
            let mut heartbeat=parsed;heartbeat.heartbeat=true;heartbeat.at_ms+=100;
            core.apply(heartbeat,&found,&parents,at+100);
        }
        core.update_title(agent,"approval-session","一轮内两次审批");
        snapshots.push(Snapshot{generation:194,sessions:core.reconcile(&found,at),notices:core.notices.iter().cloned().collect(),
            completions:core.completions.iter().cloned().collect(),..Default::default()});
    }
    snapshots
}

#[test]
fn approval_episodes_keep_request_identity_and_collapse_hook_notification_duplicates(){
    let snapshots=approval_episode_fixture();
    assert_eq!(snapshots[2].notices.len(),2);assert_eq!(snapshots[3].notices.len(),2);
    assert_eq!(snapshots[5].notices.len(),3);assert_eq!(snapshots[6].notices.len(),3);
    let first=&snapshots[2].notices[1].session;let second=&snapshots[5].notices[2].session;
    assert_eq!(first.turn_key,second.turn_key);assert_ne!(first.approval_key,second.approval_key);
    assert_eq!(snapshots[7].notices.iter().map(|event|event.kind).collect::<Vec<_>>(),[NoticeKind::Started,NoticeKind::Approval,NoticeKind::Approval,NoticeKind::Completed]);
    assert_eq!(snapshots[7].completions.len(),1);assert_eq!(snapshots[7].completions[0].session.elapsed_seconds(9_999_999_999),Some(6));
}

#[test]
fn approval_sources_keep_their_distinct_status_identity_and_success_journal(){
    for agent in [AgentKind::Codex,AgentKind::Claude,AgentKind::Hermes]{
        let snapshots=approval_fixture(agent);
        assert_eq!(snapshots[2].sessions[0].status,Status::Approval);assert!(snapshots[2].sessions[0].status.is_active());
        assert_eq!(snapshots[2].notices.iter().map(|event|event.kind).collect::<Vec<_>>(),[NoticeKind::Started,NoticeKind::Approval]);
        assert_eq!(snapshots[3].notices.len(),3);assert_eq!(snapshots[3].notices[2].kind,NoticeKind::Completed);
        assert_eq!(snapshots[3].completions.len(),1);assert_eq!(snapshots[3].completions[0].session.elapsed_seconds(9_999_999_999),Some(2));
        assert!(snapshots[3].notices.iter().all(|event|event.session.id==format!("{}:approval-session",agent.key())&&event.session.title=="审批测试会话"));
        assert!(!format!("{snapshots:?}").contains("PRIVATE"));
    }
}

#[test]
fn only_structured_permission_and_elicitation_signals_create_approval_notices(){
    let now=10_000;
    for notification_type in ["permission_prompt","elicitation_dialog"]{
        let value=json!({"version":1,"agent":"claude","event":"Notification","notification_type":notification_type,"session_id":"one","at_ms":now,"pid":42});
        assert_eq!(decode_spool(&serde_json::to_vec(&value).unwrap(),now).unwrap().status,Status::Approval);
    }
    let unrelated=json!({"version":1,"agent":"claude","event":"Notification","notification_type":"idle_prompt","message":"permission_prompt needs approval","at_ms":now,"pid":42});
    assert!(decode_spool(&serde_json::to_vec(&unrelated).unwrap(),now).is_none());
    let mut metadata=LogMeta{id:"one".into(),..Default::default()};
    for kind in ["exec_approval_request","apply_patch_approval_request","mcp_elicitation_request","request_user_input","guardian_assessment"]{
        let value=json!({"type":"event_msg","timestamp":"1970-01-01T00:00:10Z","payload":{"type":kind,"message":"PRIVATE needs approval"}});
        let parsed=log_event(&serde_json::to_vec(&value).unwrap(),AgentKind::Codex,&mut metadata).unwrap();
        assert_eq!(parsed.status,if kind=="request_user_input"{Status::Waiting}else if kind=="guardian_assessment"{Status::Working}else{Status::Approval});
    }
    let unrelated=json!({"type":"response_item","timestamp":"1970-01-01T00:00:10Z","payload":{"type":"message","message":"needs approval"}});
    assert!(log_event(&serde_json::to_vec(&unrelated).unwrap(),AgentKind::Codex,&mut metadata).is_none());
    for notification_type in ["permission_prompt","elicitation_dialog"]{
        let value=json!({"type":"hook","timestamp":"1970-01-01T00:00:10Z","sessionId":"one","hook_event_name":"Notification","notification_type":notification_type});
        assert_eq!(log_event(&serde_json::to_vec(&value).unwrap(),AgentKind::Claude,&mut metadata).unwrap().status,Status::Approval);
    }
}

#[test]
fn isolated_log_fixture_drives_short_turns_without_retaining_conversation_content(){
    let snapshots=fixture();assert!(!snapshots[0].sessions[0].status.is_active());
    assert_eq!(snapshots[1].sessions[0].status,Status::Working);assert_ne!(snapshots[1].sessions[0].turn_key,0);
    assert_eq!(snapshots[2].completions.len(),2);assert_eq!(snapshots[2].sessions[0].status,Status::Completed);
    assert_eq!(snapshots[2].notices.iter().map(|event|event.kind).collect::<Vec<_>>(),[NoticeKind::Started,NoticeKind::Completed,NoticeKind::Started,NoticeKind::Completed]);
    assert!(snapshots[2].notices.iter().all(|event|event.session.id=="codex:11111111-1111-1111-1111-111111111111"&&event.session.title=="完善岛内通知"));
    assert!(!format!("{snapshots:?}").contains("SECRET"));
}

#[test]
#[ignore="Read-only aggregate local Codex metadata probe, without hooks or notification delivery"]
fn actual_local_execution_metadata_probe(){
    let paths=Paths::environment();let now=now_ms();let (found,parents)=processes().unwrap();
    let mut logs=Logs::default();let mut core=Core::default();let mut events=logs.poll(&paths,now);
    let fresh=events.iter().filter(|event|event.agent==AgentKind::Codex&&now.saturating_sub(event.at_ms)<=FRESH_LOG_MS).count();
    events.sort_by_key(|event|(event.at_ms,event.source));for event in events{core.apply(event,&found,&parents,now);}
    logs.apply_uncertainty(&mut core);logs.backfill_timers(&mut core,now);
    let sessions=core.reconcile(&found,now);
    let active=sessions.iter().filter(|session|session.agent==AgentKind::Codex&&!session.process_only&&session.status.is_active()).count();
    eprintln!("read_only_codex_processes={} fresh_metadata_events={fresh} genuine_active_tasks={active} explicit_completion_events={} process_only_active={}",found.iter().filter(|process|process.agent==AgentKind::Codex).count(),core.completions.len(),sessions.iter().filter(|session|session.process_only&&session.status.is_active()).count());
    assert!(sessions.iter().filter(|session|session.process_only).all(|session|!session.status.is_active()));
}
