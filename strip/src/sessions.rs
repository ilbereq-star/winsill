//! What the strip knows about its sessions, and what each one is up to at a given moment.
//! Time is always passed in (epoch ms), never read, so everything here is testable.

use std::collections::HashMap;

use crate::lang::{Lang, Text};
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
    /// Takes in what the agent said, and returns the news worth telling the user about. A snapshot
    /// brings none: whatever it holds was already true when the strip connected.
    pub fn apply(&mut self, message: ServerMessage) -> Vec<Alert> {
        match message {
            ServerMessage::Snapshot { sessions, focused_id } => {
                self.sessions = sessions.into_iter().map(|s| (s.id.clone(), s)).collect();
                self.focused_id = focused_id;
            }
            ServerMessage::Session(session) => {
                let previous = self.sessions.insert(session.id.clone(), session.clone());
                return alert_for(previous.as_ref(), &session).into_iter().collect();
            }
            ServerMessage::Removed(id) => {
                self.sessions.remove(&id);
            }
            ServerMessage::Focus(id) => self.focused_id = id,
            ServerMessage::Hello { .. } | ServerMessage::Error(_) | ServerMessage::Library(_) => {}
        }
        Vec::new()
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertKind {
    NeedsPermission,
    NeedsInput,
    Done,
    Error,
}

/// A session that just changed into a state the user may want to hear about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub session_id: String,
    pub name: String,
    pub kind: AlertKind,
    pub activity: Option<String>,
}

impl Alert {
    pub fn text(&self) -> Text {
        match self.kind {
            AlertKind::NeedsPermission => Text::AlertPermission,
            AlertKind::NeedsInput => Text::AlertAsking,
            AlertKind::Done => Text::AlertDone,
            AlertKind::Error => Text::AlertError,
        }
    }
}

/// Which alerts the user wants to be told about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Notify {
    Off,
    /// A session needs a permission or an answer, or failed.
    #[default]
    Attention,
    /// And when one finishes.
    All,
}

impl Notify {
    pub fn name(self) -> &'static str {
        match self {
            Notify::Off => "off",
            Notify::Attention => "attention",
            Notify::All => "all",
        }
    }

    pub fn parse(text: &str) -> Option<Notify> {
        match text {
            "off" => Some(Notify::Off),
            "attention" => Some(Notify::Attention),
            "all" => Some(Notify::All),
            _ => None,
        }
    }

    pub fn allows(self, kind: AlertKind) -> bool {
        match self {
            Notify::Off => false,
            Notify::Attention => kind != AlertKind::Done,
            Notify::All => true,
        }
    }
}

/// A session that is not running says nothing, and neither does one that stays in its state while
/// its activity changes. Finishing counts only after the session was busy: a session that turns up
/// already done is old news.
fn alert_for(previous: Option<&Session>, session: &Session) -> Option<Alert> {
    if !session.live || previous.is_some_and(|p| p.state == session.state) {
        return None;
    }
    let was_busy = previous.is_some_and(|p| {
        matches!(p.state, State::Working | State::Compacting | State::NeedsPermission | State::NeedsInput)
    });
    let kind = match session.state {
        State::NeedsPermission => AlertKind::NeedsPermission,
        State::NeedsInput => AlertKind::NeedsInput,
        State::Error => AlertKind::Error,
        State::Done if was_busy => AlertKind::Done,
        _ => return None,
    };
    Some(Alert { session_id: session.id.clone(), name: session.name.clone(), kind, activity: session.activity.clone() })
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

/// What the label says under a session's name. `private` keeps what the agent reports it is doing
/// (a shell command, a file name) off the screen.
pub fn status_line(session: &Session, now: i64, lang: Lang, private: bool) -> String {
    let since = ago(session.state_since, now, lang);
    let activity = session.activity.as_deref().filter(|_| !private);
    match session.state {
        State::Working => activity.unwrap_or(lang.say(Text::Working)).to_owned(),
        State::NeedsPermission => lang.say(Text::NeedsPermission).to_owned(),
        State::NeedsInput => activity.unwrap_or(lang.say(Text::WaitingForYou)).to_owned(),
        State::Compacting => lang.say(Text::Compacting).to_owned(),
        State::Error => match activity {
            Some(activity) => format!("{} · {activity}", lang.say(Text::Error)),
            None => lang.say(Text::Error).to_owned(),
        },
        State::Idle => format!("{} · {since}", lang.say(Text::Idle)),
        State::Done => format!("{} · {since}", lang.say(Text::Done)),
        State::Ended => format!("{} · {since}", lang.say(Text::Closed)),
    }
}

/// What the strip holds, as the tab's status line.
pub fn summary(sessions: &[&Session], lang: Lang) -> String {
    let needy = sessions.iter().filter(|s| needs_attention(s)).count();
    if needy > 0 { lang.need_you(needy) } else { lang.sessions(sessions.len()) }
}

pub fn ago(since: i64, now: i64, lang: Lang) -> String {
    lang.ago((now - since) / 1000)
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
        let en = Lang::En;
        let mut working = session("a", "n", State::Working);
        assert_eq!(status_line(&working, 0, en, false), "working");
        working.activity = Some("Bash: npm test".into());
        assert_eq!(status_line(&working, 0, en, false), "Bash: npm test");
        assert_eq!(status_line(&session("a", "n", State::Done), 90_000, en, false), "done · 1m");
        assert_eq!(status_line(&session("a", "n", State::Ended), 7_200_000, en, false), "closed · 2h");
        let (a, b) = (session("a", "n", State::NeedsInput), session("b", "n", State::Working));
        assert_eq!(summary(&[&a, &b], en), "1 needs you");
        assert_eq!(summary(&[&b, &b], en), "2 sessions");
        assert_eq!(summary(&[], en), "no sessions");
        assert_eq!(summary(&[&a, &a], en), "2 need you");
    }

    #[test]
    fn status_lines_in_spanish() {
        let es = Lang::Es;
        assert_eq!(status_line(&session("a", "n", State::Working), 0, es, false), "trabajando");
        assert_eq!(status_line(&session("a", "n", State::Done), 90_000, es, false), "terminada · 1m");
        assert_eq!(status_line(&session("a", "n", State::NeedsPermission), 0, es, false), "necesita permiso");
        assert_eq!(summary(&[], es), "sin sesiones");
    }

    #[test]
    fn private_labels_keep_what_the_session_is_doing_off_the_screen() {
        let mut s = session("a", "n", State::Working);
        s.activity = Some("Bash: rm -rf build".into());
        assert_eq!(status_line(&s, 0, Lang::En, true), "working");
        s.state = State::NeedsInput;
        assert_eq!(status_line(&s, 0, Lang::En, true), "waiting for you");
        assert_eq!(status_line(&s, 0, Lang::En, false), "Bash: rm -rf build");
        s.state = State::Error;
        assert_eq!(status_line(&s, 0, Lang::En, true), "error");
        assert_eq!(status_line(&s, 0, Lang::En, false), "error · Bash: rm -rf build");
    }

    fn news(list: &mut SessionList, id: &str, state: State) -> Vec<AlertKind> {
        list.apply(ServerMessage::Session(session(id, "api", state))).into_iter().map(|a| a.kind).collect()
    }

    #[test]
    fn a_session_that_needs_you_or_fails_is_news() {
        let mut list = list(vec![session("a", "api", State::Working)]);
        assert_eq!(news(&mut list, "a", State::NeedsPermission), [AlertKind::NeedsPermission]);
        assert_eq!(news(&mut list, "a", State::Working), []);
        assert_eq!(news(&mut list, "a", State::NeedsInput), [AlertKind::NeedsInput]);
        assert_eq!(news(&mut list, "a", State::Error), [AlertKind::Error]);
    }

    #[test]
    fn the_same_state_again_is_not_news() {
        let mut list = list(vec![session("a", "api", State::Working)]);
        assert_eq!(news(&mut list, "a", State::NeedsPermission).len(), 1);
        let mut again = session("a", "api", State::NeedsPermission);
        again.activity = Some("Bash".into());
        assert!(list.apply(ServerMessage::Session(again)).is_empty());
    }

    #[test]
    fn finishing_counts_only_after_work() {
        let mut list = list(vec![session("a", "api", State::Working), session("b", "web", State::Idle)]);
        assert_eq!(news(&mut list, "a", State::Done), [AlertKind::Done]);
        assert_eq!(news(&mut list, "b", State::Done), []);
        assert_eq!(news(&mut list, "new", State::Done), []);
    }

    #[test]
    fn a_new_session_that_already_needs_you_is_news() {
        let mut list = SessionList::default();
        assert_eq!(news(&mut list, "a", State::NeedsPermission), [AlertKind::NeedsPermission]);
    }

    #[test]
    fn a_snapshot_and_a_closed_session_say_nothing() {
        let mut list = SessionList::default();
        let waiting = session("a", "api", State::NeedsPermission);
        assert!(list.apply(ServerMessage::Snapshot { sessions: vec![waiting], focused_id: None }).is_empty());
        let mut gone = session("b", "web", State::NeedsInput);
        gone.live = false;
        assert!(list.apply(ServerMessage::Session(gone)).is_empty());
    }

    #[test]
    fn the_alert_carries_the_name_and_what_the_session_was_doing() {
        let mut list = SessionList::default();
        let mut s = session("a", "api", State::NeedsPermission);
        s.activity = Some("Bash: ls".into());
        let alerts = list.apply(ServerMessage::Session(s));
        assert_eq!(alerts[0].name, "api");
        assert_eq!(alerts[0].activity.as_deref(), Some("Bash: ls"));
        assert_eq!(alerts[0].session_id, "a");
    }

    #[test]
    fn notify_levels() {
        assert!(!Notify::Off.allows(AlertKind::Error));
        assert!(Notify::Attention.allows(AlertKind::NeedsPermission));
        assert!(Notify::Attention.allows(AlertKind::Error));
        assert!(!Notify::Attention.allows(AlertKind::Done));
        assert!(Notify::All.allows(AlertKind::Done));
        for level in [Notify::Off, Notify::Attention, Notify::All] {
            assert_eq!(Notify::parse(level.name()), Some(level));
        }
        assert_eq!(Notify::parse("loud"), None);
    }

    #[test]
    fn ago_reads_from_a_moment_in_time() {
        assert_eq!(ago(0, 4_999, Lang::En), "now");
        assert_eq!(ago(0, 5_000, Lang::En), "5s");
        assert_eq!(ago(10_000, 0, Lang::En), "now");
    }
}
