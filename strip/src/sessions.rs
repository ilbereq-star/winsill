//! What the strip knows about its sessions, and what each one is up to at a given moment.
//! Time is always passed in (epoch ms), never read, so everything here is testable.

use std::collections::HashMap;

use crate::protocol::{ServerMessage, Session, State};
use crate::sprite::Mood;

/// A session that finished or sits idle for this long falls asleep, and leaves the strip unless pinned.
pub const DOZE_AFTER_MS: i64 = 5 * 60 * 1000;

#[derive(Default)]
pub struct SessionList {
    sessions: HashMap<String, Session>,
    pub focused_id: Option<String>,
}

impl SessionList {
    pub fn apply(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Snapshot { sessions, focused_id } => {
                self.sessions = sessions.into_iter().map(|s| (s.id.clone(), s)).collect();
                self.focused_id = focused_id;
            }
            ServerMessage::Session(session) => {
                self.sessions.insert(session.id.clone(), session);
            }
            ServerMessage::Removed(id) => {
                self.sessions.remove(&id);
            }
            ServerMessage::Focus(id) => self.focused_id = id,
            ServerMessage::Hello { .. } | ServerMessage::Error(_) | ServerMessage::Library(_) => {}
        }
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Pinned sessions, then the active unpinned ones by name rather than activity, so creatures
    /// don't jump around as their states change.
    pub fn shown(&self, now: i64) -> Vec<&Session> {
        let mut pinned: Vec<&Session> = self.sessions.values().filter(|s| s.pinned).collect();
        pinned.sort_by(|a, b| (a.position.unwrap_or(0), &a.id).cmp(&(b.position.unwrap_or(0), &b.id)));
        let mut others: Vec<&Session> = self.sessions.values().filter(|s| !s.pinned && is_active(s, now)).collect();
        others.sort_by(|a, b| {
            a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id))
        });
        pinned.extend(others);
        pinned
    }
}

pub fn needs_attention(session: &Session) -> bool {
    matches!(session.state, State::NeedsPermission | State::NeedsInput)
}

/// Doing something or waiting on the user, or finished recently: the strip shows these even unpinned.
pub fn is_active(session: &Session, now: i64) -> bool {
    if !session.live {
        return false;
    }
    match session.state {
        State::Working | State::Compacting | State::NeedsPermission | State::NeedsInput => true,
        State::Done | State::Error => now - session.state_since <= DOZE_AFTER_MS,
        State::Idle | State::Ended => false,
    }
}

pub fn mood(session: &Session, now: i64) -> Mood {
    let dozing = now - session.state_since > DOZE_AFTER_MS;
    match session.state {
        State::Working => Mood::Working,
        State::NeedsPermission => Mood::Alert,
        State::NeedsInput => Mood::Asking,
        State::Compacting => Mood::Compacting,
        State::Error => Mood::Error,
        State::Ended => Mood::Sleeping,
        State::Idle => if dozing { Mood::Sleeping } else { Mood::Idle },
        State::Done => if dozing { Mood::Sleeping } else { Mood::Finished },
    }
}

pub fn status_line(session: &Session, now: i64) -> String {
    let since = ago(session.state_since, now);
    match session.state {
        State::Working => session.activity.clone().unwrap_or_else(|| "working".into()),
        State::NeedsPermission => "needs permission".into(),
        State::NeedsInput => session.activity.clone().unwrap_or_else(|| "waiting for you".into()),
        State::Compacting => "compacting".into(),
        State::Error => match &session.activity {
            Some(activity) => format!("error · {activity}"),
            None => "error".into(),
        },
        State::Idle | State::Done => format!("{} · {since}", session.state.name()),
        State::Ended => format!("closed · {since}"),
    }
}

/// What the strip holds, as the tab's status line.
pub fn summary(sessions: &[&Session]) -> String {
    let needy = sessions.iter().filter(|s| needs_attention(s)).count();
    if needy > 0 {
        return format!("{needy} need{} you", if needy == 1 { "s" } else { "" });
    }
    match sessions.len() {
        0 => "no sessions".into(),
        1 => "1 session".into(),
        n => format!("{n} sessions"),
    }
}

pub fn ago(since: i64, now: i64) -> String {
    let s = ((now - since) / 1000).max(0);
    match s {
        0..=4 => "now".into(),
        5..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::protocol::Icon;

    pub fn session(id: &str, name: &str, state: State) -> Session {
        Session {
            id: id.into(),
            name: name.into(),
            cwd: String::new(),
            state,
            activity: None,
            live: true,
            state_since: 0,
            pinned: false,
            position: None,
            icon: Icon::default(),
        }
    }

    fn list(sessions: Vec<Session>) -> SessionList {
        let mut list = SessionList::default();
        list.apply(ServerMessage::Snapshot { sessions, focused_id: None });
        list
    }

    #[test]
    fn pinned_first_by_position_then_the_active_ones_by_name() {
        let mut a = session("a", "zeta", State::Working);
        let mut b = session("b", "alpha", State::Working);
        let c = session("c", "Beta", State::NeedsInput);
        a.pinned = true;
        a.position = Some(1);
        b.pinned = true;
        b.position = Some(0);
        let ids: Vec<_> = list(vec![a, b, c, session("d", "quiet", State::Idle)]).shown(1000).iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids, ["b", "a", "c"]);
    }

    #[test]
    fn a_finished_session_leaves_the_strip_after_five_minutes_unless_pinned() {
        let mut done = session("a", "api", State::Done);
        done.state_since = 1000;
        let unpinned = list(vec![done.clone()]);
        assert_eq!(unpinned.shown(1000 + DOZE_AFTER_MS).len(), 1);
        assert_eq!(unpinned.shown(1001 + DOZE_AFTER_MS).len(), 0);
        done.pinned = true;
        assert_eq!(list(vec![done]).shown(1001 + DOZE_AFTER_MS).len(), 1);
    }

    #[test]
    fn a_session_that_is_not_running_is_not_active() {
        let mut s = session("a", "api", State::Working);
        s.live = false;
        assert!(!is_active(&s, 0));
    }

    #[test]
    fn messages_update_the_list() {
        let mut list = list(vec![session("a", "api", State::Working)]);
        list.apply(ServerMessage::Session(session("b", "web", State::Working)));
        list.apply(ServerMessage::Focus(Some("b".into())));
        assert_eq!((list.len(), list.focused_id.as_deref()), (2, Some("b")));
        list.apply(ServerMessage::Removed("a".into()));
        assert_eq!(list.shown(0).iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["b"]);
        list.apply(ServerMessage::Snapshot { sessions: vec![], focused_id: None });
        assert!(list.is_empty());
    }

    #[test]
    fn moods() {
        assert_eq!(mood(&session("a", "n", State::Working), 0), Mood::Working);
        assert_eq!(mood(&session("a", "n", State::NeedsPermission), 0), Mood::Alert);
        assert_eq!(mood(&session("a", "n", State::NeedsInput), 0), Mood::Asking);
        assert_eq!(mood(&session("a", "n", State::Done), 0), Mood::Finished);
        assert_eq!(mood(&session("a", "n", State::Done), DOZE_AFTER_MS + 1), Mood::Sleeping);
        assert_eq!(mood(&session("a", "n", State::Idle), DOZE_AFTER_MS + 1), Mood::Sleeping);
        assert_eq!(mood(&session("a", "n", State::Ended), 0), Mood::Sleeping);
    }

    #[test]
    fn status_lines_and_summary() {
        let mut working = session("a", "n", State::Working);
        assert_eq!(status_line(&working, 0), "working");
        working.activity = Some("Bash: npm test".into());
        assert_eq!(status_line(&working, 0), "Bash: npm test");
        assert_eq!(status_line(&session("a", "n", State::Done), 90_000), "done · 1m");
        assert_eq!(status_line(&session("a", "n", State::Ended), 7_200_000), "closed · 2h");
        let (a, b) = (session("a", "n", State::NeedsInput), session("b", "n", State::Working));
        assert_eq!(summary(&[&a, &b]), "1 needs you");
        assert_eq!(summary(&[&b, &b]), "2 sessions");
        assert_eq!(summary(&[]), "no sessions");
        assert_eq!(summary(&[&a, &a]), "2 need you");
    }

    #[test]
    fn ago_steps() {
        assert_eq!(ago(0, 4_999), "now");
        assert_eq!(ago(0, 5_000), "5s");
        assert_eq!(ago(0, 59_999), "59s");
        assert_eq!(ago(0, 86_400_000 * 3), "3d");
        assert_eq!(ago(10_000, 0), "now");
    }
}
