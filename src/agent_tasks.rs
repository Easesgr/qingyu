//! The current task page's bounded projection. The monitor snapshot and completion
//! journal remain untouched so notifications can consume them independently.
use crate::agent_monitor::{Session, Snapshot, Status};
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) const COMPLETED_HOLD_SECONDS: f64 = 60.0;
const MAX_ROWS: usize = 64;
const MAX_TRACKED: usize = 128;
const MAX_SEEN: usize = 256;
const MAX_JOURNAL: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Turn { Known(u64), Started(u64), Anonymous(u64) }

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key { id: String, turn: Turn }

impl Key {
    fn reliable(session: &Session) -> Option<Self> {
        let turn = if session.turn_key != 0 { Turn::Known(session.turn_key) }
            else if session.started_at != 0 { Turn::Started(session.started_at) }
            else { return None };
        Some(Self { id: session.id.clone(), turn })
    }
}

struct Track { key: Key, active: bool, touched: u64 }
struct Row { key: Key, session: Session, expires: Option<f64>, order: u64 }

/// Time is supplied by the application; construction and updates do no I/O.
#[derive(Default)]
pub(crate) struct List {
    generation: Option<u64>,
    cursor: u64,
    after_ms: u64,
    clock: f64,
    tick: u64,
    serial: u64,
    tracks: HashMap<String, Track>,
    seen: HashSet<Key>,
    seen_order: VecDeque<Key>,
    retired_known_through: u64,
    retired_started_through: u64,
    rows: Vec<Row>,
    sessions: Vec<Session>,
}

impl List {
    pub(crate) fn sessions(&self) -> &[Session] { &self.sessions }
    pub(crate) fn reset(&mut self) { *self = Self::default(); }
    /// Absolute deadline on the application's `t` clock, suitable for repaint scheduling.
    pub(crate) fn next_expiry(&self) -> Option<f64> {
        self.rows.iter().filter_map(|row| row.expires).min_by(f64::total_cmp)
    }

    pub(crate) fn update(&mut self, snapshot: &Snapshot, t: f64, now_ms: u64) {
        let clock = if t.is_finite() { t.max(0.0).max(self.clock) } else { self.clock };
        let baseline = self.generation != Some(snapshot.generation);
        if baseline {
            self.reset(); self.clock = clock;
            // Wait for the worker's initial tail before setting the journal baseline.
            if snapshot.scanning { return; }
            self.generation = Some(snapshot.generation);
            self.cursor = snapshot.completions.iter().map(|event| event.sequence).max().unwrap_or(0);
            self.after_ms = now_ms;
        }
        self.clock = clock;
        self.tick = self.tick.wrapping_add(1);
        self.rows.retain(|row| row.expires.is_none_or(|expires| self.clock < expires));

        let mut current = Vec::<&Session>::new();
        for session in &snapshot.sessions {
            if let Some(index) = current.iter().position(|other| other.id == session.id) {
                if prefer(session, current[index]) { current[index] = session; }
            } else if current.len() < MAX_TRACKED { current.push(session); }
            else {
                let worst = current.iter().enumerate().max_by_key(|(_, value)| input_rank(value)).map(|(index, _)| index).unwrap();
                if input_rank(session) < input_rank(current[worst]) { current[worst] = session; }
            }
        }
        // Existing rows keep their order; new arrivals use deterministic identities.
        current.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.id.cmp(&b.id)));
        let mut active = HashSet::new();
        let mut blocked = HashSet::new();
        let mut finished = Vec::<(Key, Session)>::new();
        for session in &current {
            let previous = self.tracks.get(&session.id).map(|track| (track.key.clone(), track.active));
            let key = Key::reliable(session).unwrap_or_else(|| {
                if let Some((key, was_active)) = &previous {
                    if !session.status.is_active() || *was_active { return key.clone(); }
                }
                self.serial = self.serial.wrapping_add(1);
                Key { id: session.id.clone(), turn: Turn::Anonymous(self.serial) }
            });
            if previous.as_ref().is_some_and(|(old, _)| older(&key, old)) {
                if previous.as_ref().is_some_and(|(_, was_active)| *was_active) { active.insert(session.id.clone()); }
                continue;
            }
            let is_active = !session.process_only && session.status.is_active();
            let was_active = previous.as_ref().is_some_and(|(old, was_active)| *was_active && *old == key);
            self.track(key.clone(), is_active);
            if is_active {
                active.insert(session.id.clone());
                // A terminal turn's repeated start/tool record is not a new task.
                if self.seen.contains(&key) && !matches!(key.turn, Turn::Anonymous(_)) { continue; }
                self.put(key, (*session).clone(), None);
            } else if !session.process_only && session.status == Status::Completed {
                if self.rows.iter().any(|row| row.key.id == session.id && row.key != key) { self.rows.retain(|row| row.key.id != session.id); }
                if let Some(row) = self.rows.iter_mut().find(|row| row.key == key && row.expires.is_some()) { row.session = (*session).clone(); }
                if !baseline && was_active { finished.push((key, (*session).clone())); }
                else if baseline { self.remember(key); }
            } else {
                blocked.insert(session.id.clone());
                // Process shutdown or an idle/unknown snapshot does not revoke
                // an already verified successful turn or shorten its hold.
                if let Some(row) = self.rows.iter_mut().find(|row| row.key == key && row.expires.is_some()) {
                    row.session.title = session.title.clone();
                } else { self.rows.retain(|row| row.key.id != session.id); }
            }
        }
        // Missing/lost tasks cannot remain busy indefinitely. Completed rows retain
        // their deadline when a short task exists only in the journal.
        self.rows.retain(|row| row.expires.is_some() || active.contains(&row.key.id));
        for track in self.tracks.values_mut() { if !active.contains(&track.key.id) { track.active = false; } }

        if baseline {
            for event in snapshot.completions.iter().take(MAX_JOURNAL) {
                if let Some(key) = Key::reliable(&event.session) {
                    if !active.contains(&key.id) { self.remember(key); }
                }
            }
        } else {
            let mut events: Vec<_> = snapshot.completions.iter().filter(|event| event.sequence > self.cursor).take(MAX_JOURNAL).collect();
            events.sort_by_key(|event| event.sequence);
            for event in events {
                if event.sequence <= self.cursor { continue; }
                self.cursor = event.sequence;
                if event.session.process_only || event.session.status != Status::Completed { continue; }
                let key = Key::reliable(&event.session).or_else(|| finished.iter().find(|(key, _)| key.id == event.session.id).map(|(key, _)| key.clone()));
                let Some(key) = key else { continue };
                if active.contains(&key.id) { continue; }
                if blocked.contains(&key.id) || self.tracks.get(&key.id).is_some_and(|track| older(&key, &track.key)) { self.remember(key); continue; }
                let observed = finished.iter().any(|(old, _)| *old == key);
                if self.seen.contains(&key) || (!observed && self.retired(&key)) { continue; }
                let age_ms = now_ms.saturating_sub(event.at_ms);
                let fresh = now_ms == 0 || ((observed || event.at_ms >= self.after_ms) && event.at_ms <= now_ms.saturating_add(5_000)
                    && age_ms < (COMPLETED_HOLD_SECONDS * 1000.0) as u64);
                if !fresh { self.remember(key); continue; }
                let remaining = COMPLETED_HOLD_SECONDS - if now_ms == 0 { 0.0 } else { age_ms as f64 / 1000.0 };
                self.complete(key, event.session.clone(), self.clock + remaining);
            }
        }
        for (key, session) in finished {
            if !self.seen.contains(&key) { self.complete(key, session, self.clock + COMPLETED_HOLD_SECONDS); }
        }
        self.publish();
    }

    fn retired(&self, key: &Key) -> bool {
        match key.turn { Turn::Known(turn) => turn <= self.retired_known_through,
            Turn::Started(start) => start <= self.retired_started_through, Turn::Anonymous(_) => true }
    }

    fn remember(&mut self, key: Key) {
        if !self.seen.insert(key.clone()) { return; }
        self.seen_order.push_back(key);
        while self.seen_order.len() > MAX_SEEN {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen.remove(&old);
                match old.turn { Turn::Known(turn) => self.retired_known_through = self.retired_known_through.max(turn),
                    Turn::Started(start) => self.retired_started_through = self.retired_started_through.max(start), Turn::Anonymous(_) => {} }
            }
        }
    }

    fn track(&mut self, key: Key, active: bool) {
        if self.tracks.len() >= MAX_TRACKED && !self.tracks.contains_key(&key.id) {
            let oldest = self.tracks.iter().min_by_key(|(_, track)| (track.active, track.touched)).map(|(id, _)| id.clone());
            if let Some(id) = oldest { self.tracks.remove(&id); }
        }
        self.tracks.insert(key.id.clone(), Track { key, active, touched: self.tick });
    }

    fn complete(&mut self, key: Key, mut session: Session, expires: f64) {
        self.remember(key.clone()); self.track(key.clone(), false);
        session.status = Status::Completed;
        self.put(key, session, Some(expires));
    }

    fn put(&mut self, key: Key, session: Session, expires: Option<f64>) {
        if let Some(row) = self.rows.iter_mut().find(|row| row.key.id == key.id) {
            row.key = key; row.session = session; row.expires = expires; return;
        }
        self.serial = self.serial.wrapping_add(1);
        self.rows.push(Row { key, session, expires, order: self.serial });
        self.rows.sort_by(|a, b| rank(&a.session).cmp(&rank(&b.session)).then_with(|| a.order.cmp(&b.order)));
        self.rows.truncate(MAX_ROWS);
    }

    fn publish(&mut self) {
        self.rows.sort_by(|a, b| rank(&a.session).cmp(&rank(&b.session)).then_with(|| a.order.cmp(&b.order)));
        self.sessions.clear(); self.sessions.extend(self.rows.iter().map(|row| row.session.clone()));
    }
}

fn rank(session: &Session) -> u8 {
    if session.process_only { return 5; }
    match session.status { Status::Approval => 0, Status::Waiting => 1, Status::Working => 2, Status::Thinking => 3, Status::Completed => 4, _ => 5 }
}

fn input_rank(session: &Session) -> (u8, &str) { (rank(session), session.id.as_str()) }

fn older(first: &Key, second: &Key) -> bool {
    match (first.turn, second.turn) { (Turn::Known(a), Turn::Known(b)) | (Turn::Started(a), Turn::Started(b)) => a < b, _ => false }
}

fn prefer(first: &Session, second: &Session) -> bool {
    if let (Some(a), Some(b)) = (Key::reliable(first), Key::reliable(second)) {
        if older(&a, &b) { return false; }
        if older(&b, &a) { return true; }
    }
    rank(first) < rank(second) || (rank(first) == rank(second) && first.updated_at > second.updated_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_monitor::{AgentKind, Completion};

    fn task(id: &str, turn: u64, status: Status) -> Session {
        Session { id: id.into(), agent: AgentKind::Codex, status, turn_key: turn, title: format!("Named conversation {id}"),
            project: "/project".into(), detail: "exec_command".into(), pid: Some(42), started_at: 100,
            ..Default::default() }
    }
    fn snapshot(sessions: Vec<Session>) -> Snapshot { Snapshot { generation: 1, sessions, ..Default::default() } }
    fn completion(sequence: u64, at_ms: u64, session: Session) -> Completion { Completion { sequence, at_ms, session } }
    fn finish(list: &mut List, source: &mut Snapshot, t: f64) {
        source.sessions[0].status = Status::Completed;
        source.sessions[0].finished_at = Some(150);
        list.update(source, t, 150_000);
    }

    #[test]
    fn completion_holds_exactly_one_minute_without_refreshing_or_reappearing() {
        let mut list = List::default(); let mut source = snapshot(vec![task("a", 1, Status::Working)]);
        list.update(&source, 0.0, 100_000); finish(&mut list, &mut source, 1.0);
        assert_eq!(list.next_expiry(), Some(61.0)); assert_eq!(list.sessions()[0].status, Status::Completed);
        source.sessions[0].title = "Renamed during completion".into();
        source.completions.push(completion(1, 150_000, source.sessions[0].clone()));
        list.update(&source, 60.9, 150_000); assert_eq!(list.sessions()[0].title, "Renamed during completion"); assert_eq!(list.next_expiry(), Some(61.0));
        list.update(&source, 61.0, 150_000); assert!(list.sessions().is_empty()); assert_eq!(list.next_expiry(), None);
        source.completions.push(completion(2, 150_000, source.sessions[0].clone()));
        list.update(&source, 62.0, 150_000); assert!(list.sessions().is_empty());
        list.update(&source, 1000.0, 150_000); assert!(list.sessions().is_empty());
    }

    #[test]
    fn a_completed_turn_keeps_its_deadline_and_final_duration_after_process_exit() {
        for status in [Status::Stopped, Status::Unknown, Status::Idle] {
            let mut list = List::default(); let mut source = snapshot(vec![task("a", 1, Status::Working)]);
            list.update(&source, 0.0, 100_000); finish(&mut list, &mut source, 1.0);
            let verified = list.sessions()[0].clone(); assert_eq!(verified.elapsed_seconds(999), Some(50));
            source.sessions[0].status = status; source.sessions[0].finished_at = Some(151); source.sessions[0].updated_at = 151;
            source.sessions[0].title = "Renamed after exit".into();
            source.completions = vec![completion(1, 150_000, verified.clone())];
            list.update(&source, 2.0, 151_000);
            assert_eq!(list.sessions()[0].status, Status::Completed); assert_eq!(list.sessions()[0].finished_at, verified.finished_at);
            assert_eq!(list.sessions()[0].elapsed_seconds(999), Some(50)); assert_eq!(list.sessions()[0].title, "Renamed after exit"); assert_eq!(list.next_expiry(), Some(61.0));
            list.update(&source, 60.9, 210_900); assert_eq!(list.sessions()[0].status, Status::Completed);
            list.update(&source, 61.0, 211_000); assert!(list.sessions().is_empty());
            source.completions.push(completion(2, 211_001, verified)); list.update(&source, 62.0, 212_000); assert!(list.sessions().is_empty());
        }
    }

    #[test]
    fn waiting_and_real_active_tasks_preserve_all_metadata_without_start_or_title_requirements() {
        let mut source = snapshot(vec![task("a", 1, Status::Thinking), task("b", 2, Status::Working), task("c", 3, Status::Waiting)]);
        source.sessions[0].title.clear(); source.sessions[0].started_at = 0;
        source.sessions[1].detail = "真实工具".into();
        let mut list = List::default(); list.update(&source, 0.0, 100_000);
        assert_eq!(list.sessions().iter().map(|session| session.id.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
        assert_eq!(list.sessions()[0], source.sessions[2]); assert_eq!(list.sessions()[1], source.sessions[1]); assert_eq!(list.sessions()[2], source.sessions[0]);
        list.update(&source, 7.0 * 3600.0, 100_000); assert_eq!(list.sessions().len(), 3); assert_eq!(list.next_expiry(), None);
    }

    #[test]
    fn idle_presence_errors_stops_and_missing_states_never_imply_success() {
        let mut list = List::default();
        for status in [Status::Idle, Status::Running, Status::Stopped, Status::Unknown, Status::Error] {
            let source = snapshot(vec![task("a", 1, status)]); list.update(&source, 0.0, 100_000); assert!(list.sessions().is_empty());
        }
        let mut source = snapshot(vec![Session { process_only: true, ..task("process", 2, Status::Working) }]);
        list.update(&source, 0.0, 100_000); assert!(list.sessions().is_empty());
        for status in [Status::Error, Status::Stopped, Status::Unknown, Status::Idle, Status::Running] {
            list.reset(); source = snapshot(vec![task("a", 3, Status::Working)]); list.update(&source, 0.0, 100_000);
            source.sessions[0].status = status; source.sessions[0].finished_at = Some(101);
            source.completions = vec![completion(1, 101_000, source.sessions[0].clone())];
            list.update(&source, 1.0, 101_000); assert!(list.sessions().is_empty(), "{status:?}");
        }
        list.reset(); source = snapshot(vec![task("a", 4, Status::Working)]); list.update(&source, 0.0, 100_000);
        source.sessions.clear(); list.update(&source, 1.0, 101_000); assert!(list.sessions().is_empty());
    }

    #[test]
    fn unknown_or_error_can_recover_real_activity_in_the_same_turn() {
        for status in [Status::Unknown, Status::Error, Status::Stopped] {
            let mut list = List::default(); let mut source = snapshot(vec![task("a", 1, Status::Working)]);
            list.update(&source, 0.0, 100_000); source.sessions[0].status = status; list.update(&source, 1.0, 101_000); assert!(list.sessions().is_empty());
            source.sessions[0].status = Status::Working; list.update(&source, 2.0, 102_000); assert_eq!(list.sessions().len(), 1);
        }
    }

    #[test]
    fn cold_history_worker_restart_reset_and_initial_scanning_are_quiet() {
        let mut list = List::default(); let done = task("old", 1, Status::Completed);
        let mut source = Snapshot { generation: 1, sessions: vec![done.clone(), task("live", 2, Status::Working)],
            completions: vec![completion(5, 100_000, done)], ..Default::default() };
        list.update(&source, 0.0, 100_000); assert_eq!(list.sessions().len(), 1); assert_eq!(list.sessions()[0].id, "live");
        source.sessions[1].status = Status::Completed; source.completions.push(completion(6, 101_000, source.sessions[1].clone()));
        list.update(&source, 1.0, 101_000); assert_eq!(list.sessions()[0].id, "live");
        source.generation = 2; source.completions = vec![completion(1, 102_000, task("restart-old", 1, Status::Completed))];
        list.update(&source, 2.0, 102_000); assert!(list.sessions().is_empty());
        list.reset(); list.update(&source, 3.0, 103_000); assert!(list.sessions().is_empty());
        list.reset(); source.generation = 3; source.scanning = true; source.completions.clear(); list.update(&source, 4.0, 104_000);
        source.scanning = false; source.completions.push(completion(1, 104_500, task("startup-tail", 1, Status::Completed)));
        list.update(&source, 5.0, 105_000); assert!(list.sessions().is_empty());
    }

    #[test]
    fn journal_recovers_short_tasks_between_frames_and_keeps_only_the_latest_turn_per_conversation() {
        let mut list = List::default(); let mut source = snapshot(Vec::new()); list.update(&source, 0.0, 100_000);
        source.completions = vec![completion(3, 101_000, task("a", 3, Status::Completed)), completion(1, 101_000, task("a", 1, Status::Completed)),
            completion(2, 101_000, task("b", 2, Status::Completed)), completion(2, 101_000, task("b", 2, Status::Completed))];
        list.update(&source, 1.0, 101_000); assert_eq!(list.sessions().len(), 2);
        assert_eq!(list.sessions().iter().find(|session| session.id == "a").unwrap().turn_key, 3);
        assert_eq!(list.next_expiry(), Some(61.0));
        list.update(&source, 60.9, 160_900); assert_eq!(list.sessions().len(), 2);
        list.update(&source, 61.0, 161_000); assert!(list.sessions().is_empty());
        list.update(&source, 62.0, 162_000); assert!(list.sessions().is_empty());
    }

    #[test]
    fn late_completions_use_their_remaining_minute_and_pre_baseline_or_old_records_stay_quiet() {
        let mut list = List::default(); let mut source = snapshot(Vec::new()); list.update(&source, 0.0, 100_000);
        source.completions = vec![completion(1, 110_000, task("a", 1, Status::Completed))];
        list.update(&source, 20.0, 120_000); assert_eq!(list.next_expiry(), Some(70.0));
        source.completions.push(completion(2, 99_000, task("before-baseline", 2, Status::Completed)));
        source.completions.push(completion(3, 110_000, task("too-old", 3, Status::Completed)));
        list.update(&source, 70.0, 170_000); assert!(list.sessions().is_empty());
        source.completions.push(completion(4, 110_000, task("a", 1, Status::Completed))); list.update(&source, 71.0, 171_000); assert!(list.sessions().is_empty());
    }

    #[test]
    fn new_active_turn_replaces_completed_and_old_completion_cannot_hide_or_extend_it() {
        let mut list = List::default(); let mut source = snapshot(vec![task("a", 1, Status::Working)]); list.update(&source, 0.0, 100_000); finish(&mut list, &mut source, 1.0);
        source.sessions = vec![task("a", 2, Status::Thinking), task("a", 1, Status::Completed)];
        source.completions = vec![completion(1, 151_000, task("a", 1, Status::Completed))];
        list.update(&source, 2.0, 151_000); assert_eq!(list.sessions().len(), 1); assert_eq!(list.sessions()[0].turn_key, 2); assert_eq!(list.sessions()[0].status, Status::Thinking); assert_eq!(list.next_expiry(), None);
        source.sessions = vec![task("a", 2, Status::Completed)]; source.completions.push(completion(2, 152_000, source.sessions[0].clone()));
        list.update(&source, 3.0, 152_000); assert_eq!(list.sessions()[0].turn_key, 2); assert_eq!(list.next_expiry(), Some(63.0));
        source.completions.push(completion(3, 153_000, task("a", 1, Status::Completed))); list.update(&source, 4.0, 153_000); assert_eq!(list.next_expiry(), Some(63.0));
    }

    #[test]
    fn old_turn_key_zero_uses_a_reliable_start_and_unknown_active_still_completes_once() {
        let mut list = List::default(); let mut source = snapshot(vec![task("a", 0, Status::Working)]); list.update(&source, 0.0, 100_000); finish(&mut list, &mut source, 1.0);
        source.sessions[0].status = Status::Working; source.sessions[0].started_at = 200; source.sessions[0].finished_at = None;
        list.update(&source, 2.0, 200_000); assert_eq!(list.sessions()[0].started_at, 200); assert_eq!(list.next_expiry(), None);
        list.reset(); source.sessions[0].started_at = 0; source.sessions[0].title.clear(); list.update(&source, 0.0, 0);
        finish(&mut list, &mut source, 1.0); assert_eq!(list.sessions().len(), 1); list.update(&source, 61.0, 0); assert!(list.sessions().is_empty());
        list.update(&source, 62.0, 0); assert!(list.sessions().is_empty());
    }

    #[test]
    fn time_rollback_and_nonfinite_clock_values_do_not_extend_deadlines() {
        let mut list = List::default(); let mut source = snapshot(vec![task("a", 1, Status::Working)]); list.update(&source, 10.0, 100_000);
        source.sessions[0].status = Status::Completed; source.completions.push(completion(1, 90_000, source.sessions[0].clone()));
        list.update(&source, 11.0, 90_000); assert_eq!(list.next_expiry(), Some(71.0));
        for t in [3.0, -1.0, f64::NAN, f64::INFINITY] { list.update(&source, t, 80_000); assert_eq!(list.next_expiry(), Some(71.0)); }
        list.update(&source, 70.9, 80_000); assert_eq!(list.sessions().len(), 1); list.update(&source, 71.0, 80_000); assert!(list.sessions().is_empty());
    }

    #[test]
    fn sorting_prioritizes_waiting_then_execution_and_keeps_equal_rows_stable() {
        let mut list = List::default(); let mut source = snapshot(vec![task("b", 2, Status::Working), task("a", 1, Status::Working), task("c", 3, Status::Thinking)]);
        list.update(&source, 0.0, 100_000); assert_eq!(list.sessions().iter().map(|session| session.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        source.sessions.reverse(); source.sessions[0].updated_at = 999999; source.sessions[1].title = "Renamed".into(); list.update(&source, 1.0, 101_000);
        assert_eq!(list.sessions().iter().map(|session| session.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        source.sessions[0].status = Status::Waiting; list.update(&source, 2.0, 102_000);
        assert_eq!(list.sessions().iter().map(|session| session.id.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
        source.sessions.iter_mut().find(|session|session.id=="a").unwrap().status=Status::Approval;
        list.update(&source,3.0,103_000);
        assert_eq!(list.sessions().iter().map(|session|session.id.as_str()).collect::<Vec<_>>(),["a","c","b"]);
    }

    #[test]
    fn output_tracking_and_deduplication_memory_are_bounded_without_history_replay() {
        let mut list = List::default(); let mut source = snapshot(Vec::new()); list.update(&source, 0.0, 100_000);
        for i in 1..1000 {
            let session = task(&format!("task-{i}"), i, Status::Completed);
            source.completions = vec![completion(i, 100_000, session)]; list.update(&source, i as f64 / 100.0, 100_000);
            assert!(list.sessions().len() <= MAX_ROWS); assert!(list.rows.len() <= MAX_ROWS); assert!(list.tracks.len() <= MAX_TRACKED);
            assert!(list.seen.len() <= MAX_SEEN && list.seen_order.len() <= MAX_SEEN);
        }
        source.completions = vec![completion(1001, 100_000, task("task-1", 1, Status::Completed))]; list.update(&source, 1000.0, 100_000); assert!(list.sessions().is_empty());
        source.completions.clear(); source.sessions = (1..1000).map(|i| task(&format!("active-{i}"), i, Status::Working)).collect();
        source.sessions.push(task("waiting", 2000, Status::Waiting)); list.update(&source, 1001.0, 100_000);
        assert_eq!(list.sessions().len(), MAX_ROWS); assert_eq!(list.sessions()[0].id, "waiting"); assert!(list.tracks.len() <= MAX_TRACKED);
    }
}
