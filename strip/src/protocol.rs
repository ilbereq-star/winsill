//! Mirrors shared/protocol.ts: keep the two in step.

use serde_json::{Value, json};

pub const PANEL_PORT: u16 = 7778;
/// The protocol this strip understands; the agent says its own in `hello`.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Started, no prompt yet.
    Idle,
    Working,
    NeedsPermission,
    NeedsInput,
    Compacting,
    Done,
    Error,
    /// Process gone; the session itself can be reopened.
    Ended,
}

impl State {
    /// Unknown states from a newer agent read as idle instead of failing the whole message.
    pub fn parse(text: &str) -> State {
        match text {
            "working" => State::Working,
            "needs_permission" => State::NeedsPermission,
            "needs_input" => State::NeedsInput,
            "compacting" => State::Compacting,
            "done" => State::Done,
            "error" => State::Error,
            "ended" => State::Ended,
            _ => State::Idle,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Working => "working",
            State::NeedsPermission => "needs_permission",
            State::NeedsInput => "needs_input",
            State::Compacting => "compacting",
            State::Done => "done",
            State::Error => "error",
            State::Ended => "ended",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    pub character: String,
    /// "#rrggbb"
    pub color: String,
}

impl Default for Icon {
    fn default() -> Self {
        Icon { character: "default".into(), color: "#d97757".into() }
    }
}

/// The colors the other panels offer, so a choice made on one face exists on the others.
pub const PALETTE: [(&str, &str); 9] = [
    ("Orange", "#ff6b1a"),
    ("Clay", "#d97757"),
    ("Yellow", "#ffd23f"),
    ("Green", "#4cd964"),
    ("Cyan", "#3ccfe6"),
    ("Blue", "#4f7cff"),
    ("Purple", "#a66cff"),
    ("Pink", "#ff5fa2"),
    ("White", "#e8e8e8"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub state: State,
    pub activity: Option<String>,
    /// The Claude process is running.
    pub live: bool,
    /// Epoch ms of the last state change.
    pub state_since: i64,
    pub pinned: bool,
    pub position: Option<i64>,
    pub icon: Icon,
}

/// A desktop-app session that can be pinned, running or not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryEntry {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub last_activity_at: i64,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMessage {
    /// First on every connection.
    Hello { protocol: u32, agent: String },
    Snapshot { sessions: Vec<Session>, focused_id: Option<String> },
    Session(Session),
    Removed(String),
    /// The session in front in the desktop app.
    Focus(Option<String>),
    Error(String),
    /// Reply to a library request, most recent first.
    Library(Vec<LibraryEntry>),
}

impl ServerMessage {
    /// None for malformed or unknown messages: a newer agent may send types this strip doesn't know.
    pub fn parse(text: &str) -> Option<ServerMessage> {
        let v: Value = serde_json::from_str(text).ok()?;
        let string = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
        match v.get("type")?.as_str()? {
            "hello" => Some(ServerMessage::Hello {
                protocol: u32::try_from(v.get("protocol")?.as_u64()?).ok()?,
                agent: string("agent").unwrap_or_default(),
            }),
            "snapshot" => Some(ServerMessage::Snapshot {
                sessions: v.get("sessions")?.as_array()?.iter().filter_map(session_from).collect(),
                focused_id: string("focusedId"),
            }),
            "session" => session_from(v.get("session")?).map(ServerMessage::Session),
            "removed" => Some(ServerMessage::Removed(string("id")?)),
            "focus" => Some(ServerMessage::Focus(string("id"))),
            "error" => Some(ServerMessage::Error(string("message")?)),
            "library" => Some(ServerMessage::Library(
                v.get("sessions")?.as_array()?.iter().filter_map(entry_from).collect(),
            )),
            _ => None,
        }
    }
}

// An unreadable session is skipped instead of failing the whole snapshot.
fn session_from(v: &Value) -> Option<Session> {
    let text = |key: &str| v.get(key).and_then(Value::as_str);
    let icon = v.get("icon");
    let icon_text = |key: &str, fallback: &str| {
        icon.and_then(|i| i.get(key)).and_then(Value::as_str).unwrap_or(fallback).to_owned()
    };
    let default = Icon::default();
    Some(Session {
        id: text("id")?.to_owned(),
        name: text("name")?.to_owned(),
        cwd: text("cwd")?.to_owned(),
        state: State::parse(text("state")?),
        activity: text("activity").map(str::to_owned),
        live: v.get("live")?.as_bool()?,
        state_since: v.get("stateSince")?.as_f64()? as i64,
        pinned: v.get("pinned")?.as_bool()?,
        position: v.get("position").and_then(Value::as_i64),
        icon: Icon { character: icon_text("character", &default.character), color: icon_text("color", &default.color) },
    })
}

fn entry_from(v: &Value) -> Option<LibraryEntry> {
    let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_owned);
    Some(LibraryEntry {
        id: text("id")?,
        name: text("name")?,
        cwd: text("cwd")?,
        last_activity_at: v.get("lastActivityAt")?.as_f64()? as i64,
        pinned: v.get("pinned")?.as_bool()?,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientMessage {
    Open(String),
    Pin(String, bool),
    /// The agent stores icons with pins, so this also pins the session, and unpinning resets its icon.
    Icon(String, Icon),
    /// Asks for the desktop sessions that can be pinned; answered with `ServerMessage::Library`.
    Library,
}

impl ClientMessage {
    pub fn session_id(&self) -> Option<&str> {
        match self {
            ClientMessage::Open(id) | ClientMessage::Pin(id, _) | ClientMessage::Icon(id, _) => Some(id),
            ClientMessage::Library => None,
        }
    }

    pub fn json(&self) -> String {
        let value = match self {
            ClientMessage::Open(id) => json!({ "type": "open", "id": id }),
            ClientMessage::Pin(id, pinned) => json!({ "type": "pin", "id": id, "pinned": pinned }),
            ClientMessage::Icon(id, icon) => {
                json!({ "type": "icon", "id": id, "icon": { "character": icon.character, "color": icon.color } })
            }
            ClientMessage::Library => json!({ "type": "library" }),
        };
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = r##"{"id":"a1","name":"api","cwd":"C:\\work\\api","state":"needs_permission","activity":"Bash: npm test","live":true,"stateSince":1700000000000,"pinned":true,"position":2,"icon":{"character":"robot","color":"#4f7cff"}}"##;

    #[test]
    fn reads_a_session() {
        let message = ServerMessage::parse(&format!(r#"{{"type":"session","session":{SESSION}}}"#)).unwrap();
        let ServerMessage::Session(s) = message else { panic!("not a session") };
        assert_eq!((s.id.as_str(), s.state, s.live, s.pinned, s.position), ("a1", State::NeedsPermission, true, true, Some(2)));
        assert_eq!(s.cwd, r"C:\work\api");
        assert_eq!(s.activity.as_deref(), Some("Bash: npm test"));
        assert_eq!(s.state_since, 1_700_000_000_000);
        assert_eq!(s.icon, Icon { character: "robot".into(), color: "#4f7cff".into() });
    }

    #[test]
    fn a_snapshot_skips_the_session_it_cannot_read() {
        let text = format!(r#"{{"type":"snapshot","sessions":[{SESSION},{{"id":"broken"}}],"focusedId":"a1"}}"#);
        let ServerMessage::Snapshot { sessions, focused_id } = ServerMessage::parse(&text).unwrap() else { panic!() };
        assert_eq!(sessions.len(), 1);
        assert_eq!(focused_id.as_deref(), Some("a1"));
    }

    #[test]
    fn an_unknown_state_reads_as_idle_and_a_missing_icon_as_the_default() {
        let text = r#"{"type":"session","session":{"id":"x","name":"n","cwd":"c","state":"dreaming","activity":null,"live":false,"stateSince":5,"pinned":false,"position":null}}"#;
        let ServerMessage::Session(s) = ServerMessage::parse(text).unwrap() else { panic!() };
        assert_eq!(s.state, State::Idle);
        assert_eq!(s.activity, None);
        assert_eq!(s.icon, Icon::default());
    }

    #[test]
    fn reads_the_other_messages() {
        assert_eq!(
            ServerMessage::parse(r#"{"type":"hello","protocol":1,"agent":"0.4.0"}"#),
            Some(ServerMessage::Hello { protocol: 1, agent: "0.4.0".into() })
        );
        assert_eq!(ServerMessage::parse(r#"{"type":"removed","id":"a1"}"#), Some(ServerMessage::Removed("a1".into())));
        assert_eq!(ServerMessage::parse(r#"{"type":"focus","id":null}"#), Some(ServerMessage::Focus(None)));
        assert_eq!(ServerMessage::parse(r#"{"type":"error","message":"no"}"#), Some(ServerMessage::Error("no".into())));
        let library = r#"{"type":"library","sessions":[{"id":"l","name":"n","cwd":"c","lastActivityAt":9,"pinned":false},{"x":1}]}"#;
        let ServerMessage::Library(entries) = ServerMessage::parse(library).unwrap() else { panic!() };
        assert_eq!(entries, vec![LibraryEntry { id: "l".into(), name: "n".into(), cwd: "c".into(), last_activity_at: 9, pinned: false }]);
    }

    #[test]
    fn ignores_what_it_does_not_know() {
        assert_eq!(ServerMessage::parse(r#"{"type":"from_the_future"}"#), None);
        assert_eq!(ServerMessage::parse("not json"), None);
        assert_eq!(ServerMessage::parse(r#"{"no":"type"}"#), None);
    }

    #[test]
    fn writes_what_the_agent_expects() {
        assert_eq!(ClientMessage::Open("a".into()).json(), r#"{"id":"a","type":"open"}"#);
        assert_eq!(ClientMessage::Pin("a".into(), true).json(), r#"{"id":"a","pinned":true,"type":"pin"}"#);
        assert_eq!(ClientMessage::Library.json(), r#"{"type":"library"}"#);
        let icon = Icon { character: "cat".into(), color: "#ffd23f".into() };
        assert_eq!(
            ClientMessage::Icon("a".into(), icon).json(),
            r##"{"icon":{"character":"cat","color":"#ffd23f"},"id":"a","type":"icon"}"##
        );
    }
}
