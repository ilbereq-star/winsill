//! The WebSocket to the agent, on a thread of its own. It reconnects forever; while it is down the
//! last known sessions stay on screen. The thread never touches the UI: it queues events and calls
//! `wake`, which posts a message to the strip's window.

use std::collections::VecDeque;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Error, HandshakeError, Message, WebSocket};
use winsill_strip::config;
use winsill_strip::protocol::{ClientMessage, PANEL_PORT, ServerMessage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Connecting,
    Online,
    /// There is no token yet: the agent hasn't run, or refused ours.
    NotPaired,
    Offline,
}

pub enum Event {
    Status(Status),
    Message(ServerMessage),
}

const MIN_RETRY: Duration = Duration::from_secs(1);
const MAX_RETRY: Duration = Duration::from_secs(10);
/// How long a read waits, which is also how long a message to the agent can wait its turn.
const POLL: Duration = Duration::from_millis(100);
/// A socket can die silently while the computer sleeps; a failed ping notices it.
const PING_EVERY: Duration = Duration::from_secs(20);

struct Shared {
    events: Mutex<VecDeque<Event>>,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn push(&self, event: Event) {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).push_back(event);
        (self.wake)();
    }
}

pub struct Link {
    outgoing: Sender<ClientMessage>,
    shared: Arc<Shared>,
}

impl Link {
    pub fn start(data_dir: PathBuf, wake: impl Fn() + Send + Sync + 'static) -> Link {
        let (outgoing, incoming) = channel();
        let shared = Arc::new(Shared { events: Mutex::new(VecDeque::new()), wake: Box::new(wake) });
        let worker = Arc::clone(&shared);
        thread::Builder::new()
            .name("agent-link".into())
            .stack_size(256 * 1024)
            .spawn(move || run(data_dir, incoming, worker))
            .expect("cannot start the agent link");
        Link { outgoing, shared }
    }

    pub fn send(&self, message: ClientMessage) {
        let _ = self.outgoing.send(message);
    }

    /// What has arrived since the last call, oldest first.
    pub fn drain(&self) -> Vec<Event> {
        self.shared.events.lock().unwrap_or_else(|e| e.into_inner()).drain(..).collect()
    }
}

enum Failure {
    /// The agent answered, but refused the upgrade: a token it doesn't know.
    Rejected,
    Down,
}

fn run(data_dir: PathBuf, incoming: Receiver<ClientMessage>, shared: Arc<Shared>) {
    let mut delay = MIN_RETRY;
    loop {
        match config::read_token(&data_dir) {
            None => shared.push(Event::Status(Status::NotPaired)),
            Some(token) => match connect(&token) {
                Ok(mut socket) => {
                    delay = MIN_RETRY;
                    shared.push(Event::Status(Status::Online));
                    converse(&mut socket, &incoming, &shared);
                    shared.push(Event::Status(Status::Offline));
                }
                Err(Failure::Rejected) => shared.push(Event::Status(Status::NotPaired)),
                Err(Failure::Down) => shared.push(Event::Status(Status::Offline)),
            },
        }
        thread::sleep(delay);
        delay = (delay * 2).min(MAX_RETRY);
        while incoming.try_recv().is_ok() {} // whatever was asked while offline is stale
    }
}

fn connect(token: &str) -> Result<WebSocket<TcpStream>, Failure> {
    let address = SocketAddr::from(([127, 0, 0, 1], PANEL_PORT));
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).map_err(|_| Failure::Down)?;
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5))); // for the handshake only
    let settings = WebSocketConfig::default().max_message_size(Some(4 << 20));
    match tungstenite::client::client_with_config(config::websocket_url(token), stream, Some(settings)) {
        Ok((socket, _)) => {
            let _ = socket.get_ref().set_read_timeout(Some(POLL));
            Ok(socket)
        }
        Err(HandshakeError::Failure(Error::Http(_))) => Err(Failure::Rejected),
        Err(_) => Err(Failure::Down),
    }
}

/// Reads until the connection ends, sending what the UI asks for between reads.
fn converse(socket: &mut WebSocket<TcpStream>, incoming: &Receiver<ClientMessage>, shared: &Shared) {
    let mut last_ping = Instant::now();
    loop {
        while let Ok(message) = incoming.try_recv() {
            if socket.send(Message::text(message.json())).is_err() {
                return;
            }
        }
        if last_ping.elapsed() >= PING_EVERY {
            if socket.send(Message::Ping(Vec::new().into())).is_err() {
                return;
            }
            last_ping = Instant::now();
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                if let Some(message) = ServerMessage::parse(text.as_str()) {
                    shared.push(Event::Message(message));
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
    }
}
