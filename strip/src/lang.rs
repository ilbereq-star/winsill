//! The words the strip says: English and Spanish. What the agent sends (a session's name, the tool
//! it is running) is shown as it comes; everything else is here, in one table, so a missing
//! translation is a compile error rather than a blank menu.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    En,
    Es,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn parse(text: &str) -> Option<Lang> {
        match text {
            "en" => Some(Lang::En),
            "es" => Some(Lang::Es),
            _ => None,
        }
    }

    /// From a Windows language id (`GetUserDefaultUILanguage`): the low ten bits are the language,
    /// whatever the region, so Spanish from Spain, Mexico or Colombia is all the same here.
    pub fn from_langid(id: u16) -> Lang {
        const LANG_SPANISH: u16 = 0x0a;
        if id & 0x3ff == LANG_SPANISH { Lang::Es } else { Lang::En }
    }

    pub fn color_name(self, english: &str) -> String {
        if self == Lang::En {
            return english.to_owned();
        }
        match english {
            "Orange" => "Naranja",
            "Clay" => "Arcilla",
            "Yellow" => "Amarillo",
            "Green" => "Verde",
            "Cyan" => "Cian",
            "Blue" => "Azul",
            "Purple" => "Morado",
            "Pink" => "Rosa",
            "White" => "Blanco",
            other => other,
        }
        .to_owned()
    }

    pub fn sessions(self, count: usize) -> String {
        match (self, count) {
            (Lang::En, 0) => "no sessions".into(),
            (Lang::Es, 0) => "sin sesiones".into(),
            (Lang::En, 1) => "1 session".into(),
            (Lang::Es, 1) => "1 sesión".into(),
            (Lang::En, n) => format!("{n} sessions"),
            (Lang::Es, n) => format!("{n} sesiones"),
        }
    }

    pub fn need_you(self, count: usize) -> String {
        match (self, count) {
            (Lang::En, 1) => "1 needs you".into(),
            (Lang::En, n) => format!("{n} need you"),
            (Lang::Es, 1) => "1 te necesita".into(),
            (Lang::Es, n) => format!("{n} te necesitan"),
        }
    }

    pub fn connected(self, count: usize) -> String {
        format!("{} · {}", self.say(Text::Connected), self.sessions(count))
    }

    pub fn waiting_tip(self, count: usize) -> String {
        match (self, count) {
            (Lang::En, n) => format!("WinSill · {n} waiting for you"),
            (Lang::Es, 1) => "WinSill · 1 te espera".into(),
            (Lang::Es, n) => format!("WinSill · {n} te esperan"),
        }
    }

    pub fn newer_protocol(self, agent: &str, protocol: u32) -> String {
        match self {
            Lang::En => format!("Agent {agent} speaks protocol {protocol}: update WinSill"),
            Lang::Es => format!("El agente {agent} usa el protocolo {protocol}: actualiza WinSill"),
        }
    }

    /// How long ago, in the unit that fits; `now` for the first few seconds.
    pub fn ago(self, seconds: i64) -> String {
        match seconds.max(0) {
            0..=4 => self.say(Text::Now).into(),
            s @ 5..=59 => format!("{s}s"),
            s @ 60..=3599 => format!("{}m", s / 60),
            s @ 3600..=86399 => format!("{}h", s / 3600),
            s => format!("{}d", s / 86400),
        }
    }
}

macro_rules! texts {
    ($($name:ident: $en:literal, $es:literal;)*) => {
        #[derive(Clone, Copy, Debug)]
        pub enum Text { $($name),* }

        impl Text {
            pub const ALL: &'static [Text] = &[$(Text::$name),*];
        }

        impl Lang {
            pub fn say(self, text: Text) -> &'static str {
                match text {
                    $(Text::$name => match self { Lang::En => $en, Lang::Es => $es }),*
                }
            }
        }
    };
}

// No `&` in a menu text: Windows would read it as the key to press.
texts! {
    // Menus.
    Open: "Open", "Abrir";
    Pin: "Pin", "Fijar";
    Unpin: "Unpin", "Desfijar";
    Character: "Character", "Personaje";
    Color: "Color", "Color";
    PinASession: "Pin a session", "Fijar una sesión";
    NoSessionsInApp: "No sessions in the Claude app", "No hay sesiones en la app de Claude";
    ShowStrip: "Show strip", "Mostrar la tira";
    HideStrip: "Hide strip", "Ocultar la tira";
    Edge: "Edge", "Borde";
    Left: "Left", "Izquierdo";
    Right: "Right", "Derecho";
    Monitor: "Monitor", "Monitor";
    MainMonitor: "Main monitor", "Monitor principal";
    Language: "Language", "Idioma";
    Automatic: "Automatic", "Automático";
    Notifications: "Notifications", "Notificaciones";
    NotifyOff: "Off", "Desactivadas";
    NotifyAttention: "When a session needs me", "Cuando una sesión me necesita";
    NotifyAll: "Also when one finishes", "También cuando una termina";
    Sound: "Sound", "Sonido";
    NotifyTest: "Send a test", "Enviar una prueba";
    NotifyTestBody: "Notifications work", "Las notificaciones funcionan";
    PrivateLabels: "Hide details in labels", "Ocultar detalles en las etiquetas";
    Shortcut: "Keyboard shortcut", "Atajo de teclado";
    InUse: "in use by another app", "en uso por otra app";
    Quit: "Quit WinSill", "Salir de WinSill";
    // The agent.
    Connected: "Connected", "Conectado";
    Connecting: "Connecting to the agent…", "Conectando con el agente…";
    AgentNotRunning: "Agent not running · retrying", "El agente no está en marcha · reintentando";
    AgentNotSetUp: "Agent not set up · retrying", "El agente no está configurado · reintentando";
    NotConnected: "not connected to the agent", "sin conexión con el agente";
    // Labels.
    AddASession: "Add a session", "Añadir una sesión";
    PinFromApp: "pin any session from the Claude app", "fija cualquier sesión de la app de Claude";
    Expand: "Expand", "Desplegar";
    Collapse: "Collapse", "Plegar";
    // What a session is doing.
    Working: "working", "trabajando";
    NeedsPermission: "needs permission", "necesita permiso";
    WaitingForYou: "waiting for you", "te espera";
    Compacting: "compacting", "compactando";
    Error: "error", "error";
    Idle: "idle", "en reposo";
    Done: "done", "terminada";
    Closed: "closed", "cerrada";
    Now: "now", "ahora";
    // Notifications, after the session's name.
    AlertPermission: "needs your permission", "necesita tu permiso";
    AlertAsking: "has a question for you", "tiene una pregunta para ti";
    AlertDone: "finished", "ha terminado";
    AlertError: "failed", "ha fallado";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_text_is_said_in_both_languages() {
        for &text in Text::ALL {
            for lang in [Lang::En, Lang::Es] {
                let said = lang.say(text);
                assert!(!said.trim().is_empty(), "{text:?} in {}", lang.code());
                assert!(!said.contains('&'), "{text:?} would make a menu accelerator");
            }
        }
    }

    #[test]
    fn spanish_is_not_a_copy_of_english() {
        let same = Text::ALL.iter().filter(|&&t| Lang::En.say(t) == Lang::Es.say(t)).count();
        assert!(same <= 4, "{same} texts were left in English");
    }

    #[test]
    fn language_follows_the_windows_ui_language() {
        assert_eq!(Lang::from_langid(0x0c0a), Lang::Es); // Spain
        assert_eq!(Lang::from_langid(0x240a), Lang::Es); // Colombia
        assert_eq!(Lang::from_langid(0x0409), Lang::En);
        assert_eq!(Lang::from_langid(0x0416), Lang::En); // Portuguese is not Spanish
        assert_eq!(Lang::parse("es"), Some(Lang::Es));
        assert_eq!(Lang::parse("auto"), None);
    }

    #[test]
    fn counts_agree_in_number() {
        assert_eq!(Lang::Es.sessions(1), "1 sesión");
        assert_eq!(Lang::Es.sessions(3), "3 sesiones");
        assert_eq!(Lang::Es.need_you(1), "1 te necesita");
        assert_eq!(Lang::Es.need_you(2), "2 te necesitan");
        assert_eq!(Lang::En.need_you(1), "1 needs you");
        assert_eq!(Lang::Es.waiting_tip(1), "WinSill · 1 te espera");
        assert_eq!(Lang::En.connected(0), "Connected · no sessions");
    }

    #[test]
    fn ago_steps() {
        assert_eq!(Lang::En.ago(4), "now");
        assert_eq!(Lang::Es.ago(4), "ahora");
        assert_eq!(Lang::En.ago(5), "5s");
        assert_eq!(Lang::En.ago(59), "59s");
        assert_eq!(Lang::En.ago(86_400 * 3), "3d");
        assert_eq!(Lang::En.ago(-10), "now");
    }

    #[test]
    fn the_palette_has_a_spanish_name_for_every_color() {
        for (name, _) in crate::protocol::PALETTE {
            assert_ne!(Lang::Es.color_name(name), name, "{name}");
        }
        assert_eq!(Lang::En.color_name("Clay"), "Clay");
    }
}
