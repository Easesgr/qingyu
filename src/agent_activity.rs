//! Glanceable execution feedback. It consumes the monitor's turn journal rather
//! than guessing task completion from process presence or a UI status change.
use crate::agent_monitor::{AgentKind, Session, Snapshot, Status};
pub(crate) use crate::agent_monitor::NoticeKind;
use std::collections::{HashSet, VecDeque};

const MAX_NOTICE_AGE_MS: u64 = 90_000;
const MAX_SEEN: usize = 1024;
type NoticeKey = (String, u64, NoticeKind, u64);

#[derive(Clone, Copy)]
pub(crate) struct Policy {
    pub enabled: bool,
    pub activity: bool,
    pub completion: bool,
    pub interval: u32,
}

#[derive(Default)]
pub(crate) struct Feedback {
    generation: Option<u64>,
    cursor: u64,
    activity_after_ms: u64,
    completion_after_ms: u64,
    activity_on: bool,
    completion_on: bool,
    seen: HashSet<NoticeKey>,
    seen_order: VecDeque<NoticeKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoticeEvent {
    pub kind: NoticeKind,
    pub session: Session,
}

#[derive(Default)]
pub(crate) struct Update {
    pub events: Vec<NoticeEvent>,
}

impl Feedback {
    pub fn suspend(&mut self) {
        self.generation = None;
    }

    /// Every transition is consumed once. Closing a start popup must not mute
    /// a later wait, interruption or completion of the same turn.
    pub fn dismiss(&mut self, _id: &str, _turn: u64) {}

    pub fn update(&mut self, snapshot: &Snapshot, policy: Policy, _t: f64, now_ms: u64, _busy: bool) -> Update {
        // Retained for settings compatibility. Activity is driven by lifecycle
        // events and never by an interval or a rotation through active rows.
        let _ = policy.interval;
        let mut result = Update::default();
        if !policy.enabled {
            self.suspend();
            return result;
        }
        if snapshot.scanning { return result; }
        if self.generation != Some(snapshot.generation) {
            self.generation = Some(snapshot.generation);
            self.cursor = snapshot.notices.iter().map(|event| event.sequence).max().unwrap_or(0);
            self.activity_after_ms = now_ms;
            self.completion_after_ms = now_ms;
            self.activity_on = policy.activity;
            self.completion_on = policy.completion;
            self.seen.clear();
            self.seen_order.clear();
            return result;
        }
        let activity_baseline = policy.activity && !self.activity_on;
        let completion_baseline = policy.completion && !self.completion_on;
        if activity_baseline { self.activity_after_ms = now_ms; }
        if completion_baseline { self.completion_after_ms = now_ms; }
        self.activity_on = policy.activity;
        self.completion_on = policy.completion;
        let mut events: Vec<_> = snapshot.notices.iter().filter(|event| event.sequence > self.cursor).collect();
        events.sort_by_key(|event| event.sequence);
        for event in events {
            if event.sequence <= self.cursor { continue; }
            self.cursor = event.sequence;
            let (enabled, baseline, after_ms) = if event.kind == NoticeKind::Completed {
                (policy.completion, completion_baseline, self.completion_after_ms)
            } else { (policy.activity, activity_baseline, self.activity_after_ms) };
            let fresh = event.at_ms >= after_ms && (now_ms == 0 || event.at_ms <= now_ms.saturating_add(5_000)
                && (event.kind==NoticeKind::Approval||now_ms.saturating_sub(event.at_ms) <= MAX_NOTICE_AGE_MS));
            let valid = match event.kind {
                NoticeKind::Started => event.session.status.is_active(),
                NoticeKind::Waiting => event.session.status == Status::Waiting,
                NoticeKind::Approval => event.session.status == Status::Approval,
                NoticeKind::Interrupted => matches!(event.session.status, Status::Error | Status::Idle | Status::Stopped),
                NoticeKind::Completed => event.session.status == Status::Completed,
            };
            if !enabled || baseline || !fresh || !valid || event.session.process_only || event.session.turn_key == 0 { continue; }
            let key = (event.session.id.clone(), event.session.turn_key, event.kind,
                if event.kind==NoticeKind::Approval{event.session.approval_key}else{0});
            if !self.seen.insert(key.clone()) { continue; }
            self.seen_order.push_back(key);
            while self.seen_order.len() > MAX_SEEN {
                if let Some(key) = self.seen_order.pop_front() { self.seen.remove(&key); }
            }
            result.events.push(NoticeEvent { kind: event.kind, session: event.session.clone() });
        }
        result
    }
}

pub(crate) fn title(session: &Session) -> String {
    let title = session.title.split_whitespace().collect::<Vec<_>>().join(" ");
    if !title.is_empty() { return title; }
    format!("{} · 名称待同步", conversation_label(&session.id))
}

pub(crate) fn agent_from_session_id(id:&str)->Option<AgentKind>{
    match id.split_once(':')?.0{
        "codex"=>Some(AgentKind::Codex),"claude"|"claude-code"=>Some(AgentKind::Claude),"hermes"=>Some(AgentKind::Hermes),
        "gemini"|"gemini-cli"=>Some(AgentKind::Gemini),"opencode"=>Some(AgentKind::OpenCode),
        "cursor-agent"=>Some(AgentKind::Cursor),"aider"=>Some(AgentKind::Aider),_=>None,
    }
}

/// Keep unnamed or privacy-hidden conversations distinguishable without using
/// their project, prompts or replies as a substitute for the actual title.
pub(crate) fn conversation_label(id: &str) -> String {
    let id = id.split_once(':').map_or(id, |(_, id)| id);
    let suffix: Vec<_> = id.chars().filter(char::is_ascii_alphanumeric).rev().take(8).collect();
    if suffix.is_empty() { return "未命名对话".into(); }
    format!("对话 {}", suffix.into_iter().rev().collect::<String>())
}

pub(crate) fn duration(session: &Session, now: u64) -> String {
    match session.elapsed_seconds(now) {
        Some(seconds) if seconds < 60 => format!("用时 {seconds} 秒"),
        Some(seconds) if seconds < 3600 => format!("用时 {} 分 {} 秒", seconds/60,seconds%60),
        Some(seconds) => format!("用时 {} 小时 {} 分", seconds/3600,seconds/60%60),
        None => "用时待同步".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_monitor::{Completion, Notice};

    fn policy() -> Policy { Policy { enabled: true, activity: true, completion: true, interval: 45 } }
    fn session(id: &str, turn: u64, status: Status) -> Session {
        Session { id: id.into(), agent: AgentKind::Codex, status, turn_key: turn, started_at: 1, ..Default::default() }
    }
    fn notice(sequence: u64, id: &str, turn: u64, kind: NoticeKind) -> Notice {
        let status = match kind {
            NoticeKind::Started => Status::Thinking, NoticeKind::Waiting => Status::Waiting,
            NoticeKind::Approval => Status::Approval,
            NoticeKind::Interrupted => Status::Idle, NoticeKind::Completed => Status::Completed,
        };
        let mut session = session(id, turn, status);
        if matches!(kind, NoticeKind::Interrupted | NoticeKind::Completed) { session.finished_at = Some(10); }
        Notice { sequence, at_ms: 10_000 + sequence, kind, session }
    }
    fn baseline() -> (Feedback, Snapshot) {
        let mut feedback = Feedback::default();
        let snapshot = Snapshot { generation: 1, ..Default::default() };
        assert!(feedback.update(&snapshot, policy(), 0.0, 10_000, false).events.is_empty());
        (feedback, snapshot)
    }

    #[test]
    fn every_agent_gets_each_real_transition_once_without_snapshot_inference() {
        for agent in [AgentKind::Codex, AgentKind::Claude, AgentKind::Hermes, AgentKind::Gemini, AgentKind::OpenCode, AgentKind::Cursor, AgentKind::Aider] {
            let (mut feedback, mut snapshot) = baseline();
            snapshot.sessions = vec![Session { agent, ..session("session", 1, Status::Working) }];
            assert!(feedback.update(&snapshot, policy(), 1.0, 10_001, false).events.is_empty());
            for (index, kind) in [NoticeKind::Started, NoticeKind::Waiting, NoticeKind::Approval, NoticeKind::Interrupted, NoticeKind::Completed].into_iter().enumerate() {
                let mut event = notice(index as u64 + 1, "session", 1, kind); event.session.agent = agent;
                snapshot.notices.push(event);
                let update = feedback.update(&snapshot, policy(), index as f64, 10_010, false);
                assert_eq!(update.events.len(), 1); assert_eq!(update.events[0].kind, kind); assert_eq!(update.events[0].session.agent, agent);
                assert!(feedback.update(&snapshot, policy(), 900.0, 10_010, false).events.is_empty());
            }
        }
    }

    #[test]
    fn short_turns_between_frames_keep_order_identity_and_frozen_completion() {
        let (mut feedback, mut snapshot) = baseline();
        snapshot.notices = vec![notice(5, "b", 3, NoticeKind::Started), notice(1, "a", 1, NoticeKind::Started),
            notice(4, "a", 2, NoticeKind::Completed), notice(2, "a", 1, NoticeKind::Interrupted),
            notice(3, "a", 2, NoticeKind::Started), notice(4, "a", 2, NoticeKind::Completed),
            notice(6, "a", 2, NoticeKind::Completed)];
        let update = feedback.update(&snapshot, policy(), 1.0, 10_010, true);
        assert_eq!(update.events.iter().map(|event| (event.session.id.as_str(), event.session.turn_key, event.kind)).collect::<Vec<_>>(), [
            ("a", 1, NoticeKind::Started), ("a", 1, NoticeKind::Interrupted), ("a", 2, NoticeKind::Started),
            ("a", 2, NoticeKind::Completed), ("b", 3, NoticeKind::Started)]);
        assert_eq!(update.events[3].session.elapsed_seconds(999), Some(9));
        assert!(feedback.update(&snapshot, policy(), 120.0, 10_010, false).events.is_empty());
    }

    #[test]
    fn baseline_lock_disable_restart_and_scanning_do_not_replay_history() {
        for disable in [true, false] {
            let (mut feedback, mut snapshot) = baseline();
            if disable { feedback.update(&snapshot, Policy { enabled: false, ..policy() }, 1.0, 10_001, false); }
            else { feedback.suspend(); }
            snapshot.notices.push(notice(1, "a", 1, NoticeKind::Started));
            snapshot.scanning = true;
            assert!(feedback.update(&snapshot, policy(), 2.0, 20_000, false).events.is_empty());
            snapshot.scanning = false;
            assert!(feedback.update(&snapshot, policy(), 3.0, 20_000, false).events.is_empty());
            // Discovery may return an event later; its old event time still
            // cannot cross the privacy baseline.
            snapshot.notices.push(notice(2, "a", 1, NoticeKind::Interrupted));
            assert!(feedback.update(&snapshot, policy(), 4.0, 20_001, false).events.is_empty());
            snapshot.generation += 1;
            snapshot.notices = vec![notice(1, "b", 2, NoticeKind::Completed)];
            assert!(feedback.update(&snapshot, policy(), 5.0, 20_002, false).events.is_empty());
        }
        let mut feedback = Feedback::default();
        let snapshot = Snapshot { generation: 4, notices: vec![notice(1, "old", 1, NoticeKind::Started)],
            sessions: vec![session("old", 1, Status::Working)], ..Default::default() };
        assert!(feedback.update(&snapshot, policy(), 0.0, 10_000, false).events.is_empty());
    }

    #[test]
    fn activity_and_completion_toggles_establish_independent_quiet_baselines() {
        let (mut feedback, mut snapshot) = baseline();
        let mut activity_off = policy(); activity_off.activity = false;
        snapshot.notices.push(notice(1, "a", 1, NoticeKind::Started));
        assert!(feedback.update(&snapshot, activity_off, 1.0, 10_001, false).events.is_empty());
        snapshot.notices.push(notice(2, "a", 1, NoticeKind::Completed));
        let update = feedback.update(&snapshot, policy(), 2.0, 10_002, false);
        assert_eq!(update.events.iter().map(|event| event.kind).collect::<Vec<_>>(), [NoticeKind::Completed]);
        let mut completion_off = policy(); completion_off.completion = false;
        snapshot.notices.push(notice(3, "a", 2, NoticeKind::Completed));
        assert!(feedback.update(&snapshot, completion_off, 3.0, 10_003, false).events.is_empty());
        snapshot.notices.push(notice(4, "b", 3, NoticeKind::Started));
        let update = feedback.update(&snapshot, policy(), 4.0, 10_004, false);
        assert_eq!(update.events.iter().map(|event| event.kind).collect::<Vec<_>>(), [NoticeKind::Started]);
        snapshot.notices.push(Notice { at_ms: 10_003, ..notice(5, "a", 4, NoticeKind::Completed) });
        assert!(feedback.update(&snapshot, policy(), 5.0, 10_005, false).events.is_empty());
    }

    #[test]
    fn closing_a_start_does_not_mute_later_events_and_time_never_creates_reminders() {
        let (mut feedback, mut snapshot) = baseline();
        snapshot.notices.push(notice(1, "a", 1, NoticeKind::Started));
        assert_eq!(feedback.update(&snapshot, policy(), 1.0, 10_010, false).events.len(), 1);
        feedback.dismiss("a", 1);
        for t in [45.0, 90.0, 3600.0] { assert!(feedback.update(&snapshot, policy(), t, 10_010, false).events.is_empty()); }
        for (index, kind) in [NoticeKind::Waiting, NoticeKind::Interrupted, NoticeKind::Completed].into_iter().enumerate() {
            snapshot.notices.push(notice(index as u64 + 2, "a", 1, kind));
            assert_eq!(feedback.update(&snapshot, policy(), 3601.0, 10_010, false).events[0].kind, kind);
        }
    }

    #[test]
    fn process_presence_unknown_rows_legacy_completions_and_stale_events_stay_quiet() {
        let (mut feedback, mut snapshot) = baseline();
        for status in [Status::Running, Status::Thinking, Status::Working, Status::Waiting, Status::Error, Status::Stopped, Status::Idle, Status::Unknown, Status::Completed] {
            snapshot.sessions = vec![session("a", 1, status)];
            assert!(feedback.update(&snapshot, policy(), 1.0, 10_001, false).events.is_empty());
        }
        snapshot.completions.push(Completion { sequence: 1, at_ms: 10_001, session: session("a", 1, Status::Completed) });
        let mut process = notice(1, "process", 1, NoticeKind::Started); process.session.process_only = true;
        let mut unknown = notice(2, "unknown", 2, NoticeKind::Started); unknown.session.status = Status::Unknown;
        let mut no_turn = notice(3, "loaded", 0, NoticeKind::Started); no_turn.session.status = Status::Idle;
        snapshot.notices = vec![process, unknown, no_turn];
        assert!(feedback.update(&snapshot, policy(), 2.0, 10_010, false).events.is_empty());
        snapshot.notices.push(notice(4, "old", 4, NoticeKind::Completed));
        assert!(feedback.update(&snapshot, policy(), 3.0, 200_000, false).events.is_empty());
        snapshot.notices.push(Notice { at_ms: 210_001, ..notice(5, "future", 5, NoticeKind::Started) });
        assert!(feedback.update(&snapshot, policy(), 4.0, 200_000, false).events.is_empty());
    }

    #[test]
    fn dedup_memory_is_bounded_and_notices_use_refreshed_metadata() {
        let (mut feedback, mut snapshot) = baseline();
        for index in 1..=1200 {
            let mut event = notice(index, "session", index, NoticeKind::Started);
            event.at_ms = 10_010; event.session.title = "Renamed conversation".into();
            snapshot.notices = vec![event];
            let update = feedback.update(&snapshot, policy(), 1.0, 10_010, false);
            assert_eq!(update.events[0].session.title, "Renamed conversation");
        }
        assert_eq!(feedback.seen.len(), MAX_SEEN); assert_eq!(feedback.seen_order.len(), MAX_SEEN);
    }
}
