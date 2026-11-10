//! Where the agent keeps its token, and what the strip remembers between runs.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::layout::Edge;
use crate::protocol::PANEL_PORT;

/// `%LOCALAPPDATA%\winsill`, the folder the agent uses (agent/src/platform/windows.ts).
pub fn data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share")))?;
    Some(base.join("winsill"))
}

/// The pairing token the agent wrote into its config.json; None before the agent's first run.
pub fn read_token(data_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(data_dir.join("config.json")).ok()?;
    token_from_json(&text)
}

pub fn token_from_json(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    let token = v.get("token")?.as_str()?;
    (!token.is_empty()).then(|| token.to_owned())
}

pub fn websocket_url(token: &str) -> String {
    let mut query = String::new();
    for byte in token.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => query.push(byte as char),
            _ => query.push_str(&format!("%{byte:02X}")),
        }
    }
    format!("ws://127.0.0.1:{PANEL_PORT}/ws?token={query}")
}

/// What the user chose, kept in `strip.json` next to the agent's files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub edge: Edge,
    pub collapsed: bool,
    pub hidden: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { edge: Edge::Right, collapsed: false, hidden: false }
    }
}

impl Prefs {
    pub fn parse(text: &str) -> Prefs {
        let default = Prefs::default();
        let Ok(v) = serde_json::from_str::<Value>(text) else { return default };
        Prefs {
            edge: v.get("edge").and_then(Value::as_str).and_then(Edge::parse).unwrap_or(default.edge),
            collapsed: v.get("collapsed").and_then(Value::as_bool).unwrap_or(default.collapsed),
            hidden: v.get("hidden").and_then(Value::as_bool).unwrap_or(default.hidden),
        }
    }

    pub fn load(data_dir: &Path) -> Prefs {
        std::fs::read_to_string(data_dir.join("strip.json")).map(|text| Prefs::parse(&text)).unwrap_or_default()
    }

    /// Best effort: losing a preference is not worth a crash.
    pub fn save(&self, data_dir: &Path) {
        let text = json!({ "edge": self.edge.name(), "collapsed": self.collapsed, "hidden": self.hidden }).to_string();
        let _ = std::fs::create_dir_all(data_dir);
        let _ = std::fs::write(data_dir.join("strip.json"), text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_token_the_agent_wrote() {
        assert_eq!(token_from_json(r#"{"token":"abc-_9","hookToken":"other"}"#).as_deref(), Some("abc-_9"));
        assert_eq!(token_from_json(r#"{"token":""}"#), None);
        assert_eq!(token_from_json(r#"{"hookToken":"x"}"#), None);
        assert_eq!(token_from_json("{ broken"), None);
    }

    #[test]
    fn the_url_keeps_a_plain_token_and_escapes_the_rest() {
        assert_eq!(websocket_url("abc-_9"), "ws://127.0.0.1:7778/ws?token=abc-_9");
        assert_eq!(websocket_url("a+b=c d"), "ws://127.0.0.1:7778/ws?token=a%2Bb%3Dc%20d");
    }

    #[test]
    fn preferences_survive_a_round_trip_and_forgive_a_bad_file() {
        let dir = std::env::temp_dir().join(format!("winsill-prefs-{}", std::process::id()));
        let prefs = Prefs { edge: Edge::Left, collapsed: true, hidden: false };
        prefs.save(&dir);
        assert_eq!(Prefs::load(&dir), prefs);
        std::fs::write(dir.join("strip.json"), "{ not json").unwrap();
        assert_eq!(Prefs::load(&dir), Prefs::default());
        assert_eq!(Prefs::parse(r#"{"edge":"up"}"#), Prefs::default());
        assert_eq!(Prefs::parse(r#"{"hidden":true}"#), Prefs { hidden: true, ..Prefs::default() });
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
