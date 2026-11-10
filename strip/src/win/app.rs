//! The strip's controller: it owns the windows, the session list and the link to the agent, and
//! ties them together. A port of `StripController.swift`, the menus and the menu bar item.
//!
//! Everything runs on the UI thread. The state lives in a thread-local, and the window procedure
//! works on it only while it can borrow it: a message that arrives while the state is already in
//! use (a call that sends messages to our own window) falls through to the default handling.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::size_of;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Globalization::GetUserDefaultUILanguage;
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForMonitor, MDT_EFFECTIVE_DPI, SetProcessDpiAwarenessContext};
use windows::Win32::UI::Input::KeyboardAndMouse::{HOT_KEY_MODIFIERS, RegisterHotKey, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, UnregisterHotKey};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetCursorPos, GetMessageW, IDC_ARROW, KillTimer, LoadCursorW, MSG, PostMessageW,
    PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SetTimer, TranslateMessage, WM_APP, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_DPICHANGED, WM_HOTKEY, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_RBUTTONUP, WM_SETTINGCHANGE,
    WM_TIMER, WNDCLASSEXW, WNDPROC, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    WINDOW_EX_STYLE,
};
use windows::core::{BOOL, PCWSTR, w};
use winsill_strip::appearance::{Appearance, Paint};
use winsill_strip::config::{self, Prefs};
use winsill_strip::hotkey::Hotkey;
use winsill_strip::lang::{Lang, Text};
use winsill_strip::layout::{Edge, Hit, Layout, Rect};
use winsill_strip::protocol::{ClientMessage, Icon, LibraryEntry, PALETTE, PROTOCOL_VERSION, ServerMessage, Session};
use winsill_strip::raster::{Canvas, Corners};
use winsill_strip::screens::{self, Screen};
use winsill_strip::sessions::{Alert, AlertKind, Notify, SessionList, ago, needs_attention, status_line, summary};
use winsill_strip::sprite::{Rgb, character, characters};
use winsill_strip::view::{self, BACKDROP, LINE, MUTED, Scene, TEXT};

use super::link::{Event, Link, Status};
use super::menu::{self, Item, Popup};
use super::text::{self, Fonts, Mask};
use super::tray::Tray;
use super::window::Surface;

/// Not in the bindings' WindowsAndMessaging module.
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_APP_LINK: u32 = WM_APP + 1;
const WM_APP_TRAY: u32 = WM_APP + 2;
/// The tray callback's word for a click on one of our balloons.
const NIN_BALLOONUSERCLICK: u32 = 0x0405;
const HOTKEY_ID: i32 = 1;
const TIMER_CLOCK: usize = 1;
const TIMER_SLIDE: usize = 2;
/// The creatures move four times a second.
const FRAME_MS: i64 = 250;
const SLIDE_MS: u64 = 250;
const NOTICE_FOR: Duration = Duration::from_secs(4);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// What the pointer can be over: a session's creature, the add button after the last one, or the tab.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Cell {
    Session(String),
    Add,
    Tab,
}

struct Notice {
    /// None when the notice is about the add button's request for the session library.
    session_id: Option<String>,
    message: String,
    until: Instant,
}

#[derive(Clone, PartialEq)]
struct LabelContent {
    name: String,
    status: String,
    color: Rgb,
    beside: Rect,
    outside: Rect,
}

/// Everything the strip's last drawing depended on: if none of it changed, there is nothing to redraw.
#[derive(PartialEq)]
struct Drawn {
    frame: Rect,
    layout: Layout,
    offset: i32,
    ui: f32,
    creatures: Vec<Appearance>,
    tab: Appearance,
}

struct Slide {
    started: Instant,
    from: i32,
    to: i32,
}

#[derive(Clone)]
enum Command {
    Open(String),
    Pin(String, bool),
    SetIcon(String, Icon),
    ToggleStrip,
    SetEdge(Edge),
    SetMonitor(Option<String>),
    SetLanguage(Option<Lang>),
    SetNotify(Notify),
    ToggleSound,
    TestNotification,
    TogglePrivate,
    ToggleHotkey,
    Quit,
}

type Menu = (Popup, Vec<Command>);

struct Handled {
    result: LRESULT,
    /// A menu to show once the state is free again: showing one runs a modal loop.
    popup: Option<Menu>,
}

impl Handled {
    fn done() -> Option<Handled> {
        Some(Handled { result: LRESULT(0), popup: None })
    }

    fn popup(menu: Option<Menu>) -> Option<Handled> {
        Some(Handled { result: LRESULT(0), popup: menu })
    }
}

struct App {
    hwnd: HWND,
    strip: Surface,
    label: Surface,
    tray: Tray,
    taskbar_created: u32,
    fonts: Fonts,
    fonts_ui: f32,
    link: Link,
    status: Status,
    /// Set when the agent speaks a newer protocol than this strip: its version and the protocol it speaks.
    protocol_note: Option<(String, u32)>,
    /// Windows' own language, for when the user has not chosen one.
    system_lang: Lang,
    /// False when another program already holds the shortcut.
    hotkey_held: bool,
    /// The session the last notification was about, for when the user clicks it.
    alert_target: Option<String>,
    list: SessionList,
    last_sent: Option<ClientMessage>,
    notice: Option<Notice>,
    awaiting_library: bool,
    prefs: Prefs,
    data_dir: PathBuf,
    canvas: Canvas,
    label_canvas: Canvas,
    /// The sessions the current layout has room for.
    shown: Vec<Session>,
    layout: Layout,
    frame: Rect,
    work: Rect,
    ui: f32,
    /// How far the strip is slid toward the edge right now.
    offset: i32,
    slide: Option<Slide>,
    hovered: Option<Cell>,
    pressed: Option<Hit>,
    tracking_leave: bool,
    drawn: Option<Drawn>,
    label_shown: Option<LabelContent>,
    ticks: u32,
}

pub fn run() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // One strip at a time; the mutex lasts as long as the process, and Windows drops it on exit, even after a crash.
        let _single_instance = CreateMutexW(None, true, w!("Local\\WinSillStrip"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        let instance: HINSTANCE = GetModuleHandleW(None).expect("no module handle").into();
        let hwnd = create_window(w!("WinSillStrip"), instance, Some(strip_proc), WINDOW_EX_STYLE(0));
        let label_hwnd = create_window(w!("WinSillLabel"), instance, Some(label_proc), WS_EX_TRANSPARENT);

        let data_dir = config::data_dir().unwrap_or_else(|| PathBuf::from("."));
        let wake_target = hwnd.0 as isize;
        let link = Link::start(data_dir.clone(), move || {
            let _ = PostMessageW(Some(HWND(wake_target as *mut c_void)), WM_APP_LINK, WPARAM(0), LPARAM(0));
        });

        let ui = 1.0;
        let app = App {
            hwnd,
            strip: Surface::new(hwnd),
            label: Surface::new(label_hwnd),
            tray: Tray::add(hwnd, WM_APP_TRAY),
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            fonts: Fonts::new(ui),
            fonts_ui: ui,
            link,
            status: Status::Connecting,
            protocol_note: None,
            system_lang: Lang::from_langid(GetUserDefaultUILanguage()),
            hotkey_held: false,
            alert_target: None,
            list: SessionList::default(),
            last_sent: None,
            notice: None,
            awaiting_library: false,
            prefs: Prefs::load(&data_dir),
            data_dir,
            canvas: Canvas::new(0, 0),
            label_canvas: Canvas::new(0, 0),
            shown: Vec::new(),
            layout: Layout::fitting(1, 0, ui),
            frame: Rect::new(0, 0, 0, 0),
            work: Rect::new(0, 0, 0, 0),
            ui,
            offset: 0,
            slide: None,
            hovered: None,
            pressed: None,
            tracking_leave: false,
            drawn: None,
            label_shown: None,
            ticks: 0,
        };
        APP.with(|cell| *cell.borrow_mut() = Some(app));
        SetTimer(Some(hwnd), TIMER_CLOCK, FRAME_MS as u32, None);
        with_app(|app| {
            app.register_hotkey();
            app.relayout();
        });

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        APP.with(|cell| cell.borrow_mut().take()); // drops the tray icon
    }
}

unsafe fn create_window(class: PCWSTR, instance: HINSTANCE, proc: WNDPROC, extra: WINDOW_EX_STYLE) -> HWND {
    unsafe {
        let class_info = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: proc,
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&class_info);
        // Never in front of the user's work (no activation), out of Alt-Tab and the taskbar (tool window),
        // and drawn by us with per-pixel alpha (layered).
        let ex = WINDOW_EX_STYLE(WS_EX_LAYERED.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0 | WS_EX_NOACTIVATE.0 | extra.0);
        CreateWindowExW(ex, class, w!("WinSill"), WS_POPUP, 0, 0, 1, 1, None, None, Some(instance), None).expect("cannot create the strip's window")
    }
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Ok(mut slot) = cell.try_borrow_mut() {
            if let Some(app) = slot.as_mut() {
                f(app);
            }
        }
    });
}

unsafe extern "system" fn label_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn strip_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let handled = APP.with(|cell| {
        let mut slot = cell.try_borrow_mut().ok()?;
        slot.as_mut()?.handle(msg, wparam, lparam)
    });
    let Some(Handled { result, popup }) = handled else {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    };
    if let Some((popup, commands)) = popup {
        // The state is free here: the menu's own message loop may call back into the window.
        if let Some(command) = menu::show(hwnd, &popup).and_then(|i| commands.get(i).cloned()) {
            with_app(|app| app.run(command));
        }
    }
    result
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Every monitor, with its work area (the monitor minus the taskbar) and DPI factor.
fn monitors() -> Vec<Screen> {
    unsafe extern "system" fn collect(monitor: HMONITOR, _dc: HDC, _area: *mut RECT, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the Vec that `monitors` lent for this call.
        let list = unsafe { &mut *(data.0 as *mut Vec<Screen>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool() {
            let (work, full) = (info.monitorInfo.rcWork, info.monitorInfo.rcMonitor);
            let length = info.szDevice.iter().position(|&unit| unit == 0).unwrap_or(info.szDevice.len());
            let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
            let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
            list.push(Screen {
                name: String::from_utf16_lossy(&info.szDevice[..length]),
                work: Rect::new(work.left, work.top, work.right - work.left, work.bottom - work.top),
                size: (full.right - full.left, full.bottom - full.top),
                ui: dpi_x as f32 / 96.0,
                primary: info.monitorInfo.dwFlags & 1 != 0, // MONITORINFOF_PRIMARY
            });
        }
        true.into()
    }
    let mut list: Vec<Screen> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect), LPARAM(&mut list as *mut Vec<Screen> as isize));
    }
    list
}

fn signed_low(value: isize) -> i32 {
    (value & 0xffff) as i16 as i32
}

fn signed_high(value: isize) -> i32 {
    ((value >> 16) & 0xffff) as i16 as i32
}

impl App {
    fn handle(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<Handled> {
        let (x, y) = (signed_low(lparam.0), signed_high(lparam.0));
        match msg {
            WM_MOUSEMOVE => {
                self.watch_for_leave();
                let cell = self.hit(x, y).and_then(|hit| self.cell_of(hit));
                self.set_hover(cell);
                Handled::done()
            }
            WM_MOUSELEAVE => {
                self.tracking_leave = false;
                self.set_hover(None);
                Handled::done()
            }
            WM_LBUTTONDOWN => {
                self.pressed = self.hit(x, y);
                Handled::done()
            }
            WM_LBUTTONUP => {
                let pressed = self.pressed.take();
                match self.hit(x, y) {
                    Some(hit) if Some(hit) == pressed => self.click(hit),
                    _ => Handled::done(),
                }
            }
            WM_RBUTTONUP => match self.hit(x, y) {
                Some(Hit::Cell(index)) => match self.shown.get(index).cloned() {
                    Some(session) => {
                        self.set_hover(None); // the label would sit under the menu
                        Handled::popup(Some(self.creature_menu(&session)))
                    }
                    None => Handled::done(),
                },
                _ => Handled::done(),
            },
            WM_MOUSEACTIVATE => Some(Handled { result: LRESULT(3), popup: None }), // MA_NOACTIVATE
            WM_TIMER => {
                match wparam.0 {
                    TIMER_CLOCK => self.tick(),
                    TIMER_SLIDE => self.step_slide(),
                    _ => {}
                }
                Handled::done()
            }
            WM_APP_LINK => Handled::popup(self.on_link()),
            WM_APP_TRAY => match (lparam.0 & 0xffff) as u32 {
                WM_LBUTTONUP | WM_RBUTTONUP => Handled::popup(Some(self.tray_menu())),
                NIN_BALLOONUSERCLICK => {
                    if let Some(id) = self.alert_target.clone() {
                        self.send(ClientMessage::Open(id));
                    }
                    Handled::done()
                }
                _ => Handled::done(),
            },
            WM_HOTKEY => {
                self.run(Command::ToggleStrip);
                Handled::done()
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                self.relayout();
                Handled::done()
            }
            WM_DESTROY => {
                unsafe { PostQuitMessage(0) };
                Handled::done()
            }
            m if m == self.taskbar_created => {
                self.tray.re_add();
                Handled::done()
            }
            _ => None,
        }
    }

    // ---- layout and drawing ------------------------------------------------------------------

    fn target_offset(&self) -> i32 {
        if self.prefs.collapsed { self.layout.collapse_offset(self.prefs.edge) } else { 0 }
    }

    fn visible_sessions(&self, now: i64) -> Vec<Session> {
        self.list.shown(now).into_iter().cloned().collect()
    }

    /// Takes the sessions the strip should show and fits the window to them.
    fn relayout(&mut self) {
        self.shown = self.visible_sessions(now_ms());
        if self.prefs.hidden {
            self.strip.hide();
            self.label.hide();
            self.label_shown = None;
            self.hovered = None;
            self.drawn = None;
            return;
        }
        let screens = monitors();
        let (work, ui) = screens::pick(&screens, self.prefs.monitor.as_deref())
            .map_or((Rect::new(0, 0, 1366, 728), 1.0), |screen| (screen.work, screen.ui));
        if ui != self.fonts_ui {
            self.fonts = Fonts::new(ui);
            self.fonts_ui = ui;
        }
        self.work = work;
        self.ui = ui;
        self.layout = Layout::fitting(self.shown.len() + 1, work.h, ui);
        self.frame = self.layout.frame(work, self.prefs.edge);
        if self.slide.is_none() {
            self.offset = self.target_offset();
        }
        self.drawn = None;
        self.render();
    }

    /// Draws what changed since last time, which is usually nothing.
    fn render(&mut self) {
        if self.prefs.hidden {
            return;
        }
        let now = now_ms();
        let current = self.visible_sessions(now);
        // A finished session that dozes off leaves the strip, which then needs a new layout.
        if current.iter().map(|s| &s.id).ne(self.shown.iter().map(|s| &s.id)) {
            return self.relayout();
        }
        self.shown = current;

        let frame_number = now / FRAME_MS;
        let edge = self.prefs.edge;
        let collapsed = self.prefs.collapsed;
        let focused = self.list.focused_id.as_deref();
        let creatures = if !collapsed || self.slide.is_some() {
            let hovered = |cell: Cell| self.hovered.as_ref() == Some(&cell);
            let sessions = self.shown.iter().map(|s| {
                Appearance::session(s, focused == Some(s.id.as_str()), hovered(Cell::Session(s.id.clone())), frame_number, now)
            });
            sessions.chain([Appearance::add_button(hovered(Cell::Add))]).collect()
        } else {
            Vec::new()
        };
        let refs: Vec<&Session> = self.shown.iter().collect();
        let tab = Appearance::tab(&refs, collapsed, edge, self.hovered == Some(Cell::Tab), frame_number, now);
        let drawn = Drawn { frame: self.frame, layout: self.layout, offset: self.offset, ui: self.ui, creatures, tab };

        if self.drawn.as_ref() != Some(&drawn) {
            self.canvas.reset(drawn.frame.w, drawn.frame.h);
            let scene = Scene { layout: &drawn.layout, edge, offset: drawn.offset, ui: drawn.ui, creatures: &drawn.creatures, tab: &drawn.tab };
            view::render(&mut self.canvas, &scene);
            self.strip.present(&self.canvas, drawn.frame.x, drawn.frame.y);
            self.drawn = Some(drawn);
        }
        self.update_label(now);
        let waiting = self.shown.iter().filter(|s| needs_attention(s)).count();
        let lang = self.lang();
        self.tray.set_tip(&if waiting > 0 { lang.waiting_tip(waiting) } else { "WinSill".to_owned() });
    }

    fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        if self.notice.as_ref().is_some_and(|n| n.until <= Instant::now()) {
            self.notice = None;
        }
        if self.ticks % 40 == 0 {
            self.strip.raise();
            self.label.raise();
        }
        self.render();
    }

    // ---- the label beside what the pointer is over -----------------------------------------------

    fn label_content(&self, now: i64) -> Option<LabelContent> {
        let notice_cell = self.notice.as_ref().map(|n| match &n.session_id {
            Some(id) => Cell::Session(id.clone()),
            None => Cell::Add,
        });
        let labelled = self.hovered.clone().or_else(|| notice_cell.clone())?;
        if self.prefs.collapsed && labelled != Cell::Tab {
            return None;
        }
        let failure = if notice_cell.as_ref() == Some(&labelled) { self.notice.as_ref().map(|n| n.message.clone()) } else { None };
        let (frame, edge, lang) = (self.frame, self.prefs.edge, self.lang());
        let strip = self.layout.strip_rect(frame.h, edge).offset(frame.x + self.offset, frame.y);
        let paint_color = |paint: Paint| paint.rgb;
        let failed = paint_color(Paint::DANGER);
        match labelled {
            Cell::Session(id) => {
                let index = self.shown.iter().position(|s| s.id == id)?;
                let session = &self.shown[index];
                let color = if failure.is_some() {
                    failed
                } else if needs_attention(session) {
                    paint_color(Paint::ACCENT)
                } else {
                    MUTED
                };
                Some(LabelContent {
                    name: session.name.clone(),
                    status: failure.unwrap_or_else(|| status_line(session, now, lang, self.prefs.private)),
                    color,
                    beside: self.layout.cell_frame(index, strip),
                    outside: frame,
                })
            }
            Cell::Add => Some(LabelContent {
                name: lang.say(Text::AddASession).into(),
                color: if failure.is_some() { failed } else { MUTED },
                status: failure.unwrap_or_else(|| lang.say(Text::PinFromApp).into()),
                beside: self.layout.cell_frame(self.shown.len(), strip),
                outside: frame,
            }),
            Cell::Tab => {
                let tab = self.layout.tab_rect(frame.h, edge, self.prefs.collapsed).offset(frame.x, frame.y);
                let refs: Vec<&Session> = self.shown.iter().collect();
                let waiting = self.shown.iter().any(needs_attention);
                Some(LabelContent {
                    name: lang.say(if self.prefs.collapsed { Text::Expand } else { Text::Collapse }).into(),
                    status: summary(&refs, lang),
                    color: if waiting { paint_color(Paint::ACCENT) } else { MUTED },
                    beside: tab,
                    outside: if self.prefs.collapsed { tab } else { frame },
                })
            }
        }
    }

    fn update_label(&mut self, now: i64) {
        let content = self.label_content(now);
        if content == self.label_shown {
            return;
        }
        match &content {
            None => self.label.hide(),
            Some(content) => self.draw_label(content),
        }
        self.label_shown = content;
    }

    fn draw_label(&mut self, content: &LabelContent) {
        let ui = self.ui;
        let scaled = |points: f32| (points * ui).round() as i32;
        let (pad_x, pad_y, gap) = (scaled(10.0), scaled(6.0), scaled(2.0));
        let max_width = scaled(300.0);
        let name = text::mask(self.fonts.name, &content.name, max_width);
        let status = text::mask(self.fonts.status, &content.status, max_width);
        let inner = name.width.max(status.width);
        let (w, h) = (inner + 2 * pad_x, 2 * pad_y + name.height + gap + status.height);

        let canvas = &mut self.label_canvas;
        canvas.reset(w, h);
        let bounds = Rect::new(0, 0, w, h);
        canvas.fill_rrect(bounds, 8.0 * ui, Corners::ALL, BACKDROP);
        canvas.stroke_rrect(bounds, 8.0 * ui, Corners::ALL, ui.max(1.0), LINE);
        // Text leans toward the edge, like the label's neighbors.
        let place = |m: &Mask| if self.prefs.edge == Edge::Right { pad_x + inner - m.width } else { pad_x };
        canvas.draw_mask(place(&name), pad_y, name.width, name.height, &name.data, TEXT, 1.0);
        canvas.draw_mask(place(&status), pad_y + name.height + gap, status.width, status.height, &status.data, content.color, 1.0);

        let mut at = self.layout.label_frame((w, h), content.beside, content.outside, self.prefs.edge);
        at.y = at.y.min(self.work.bottom() - h).max(self.work.y);
        self.label.present(&self.label_canvas, at.x, at.y);
    }

    // ---- the pointer -------------------------------------------------------------------------

    fn hit(&self, x: i32, y: i32) -> Option<Hit> {
        if self.slide.is_some() || self.prefs.hidden {
            return None;
        }
        self.layout.hit(x, y, self.frame.h, self.prefs.edge, self.prefs.collapsed)
    }

    fn cell_of(&self, hit: Hit) -> Option<Cell> {
        match hit {
            Hit::Tab => Some(Cell::Tab),
            Hit::Cell(index) => self.cell_at(index),
        }
    }

    fn cell_at(&self, index: usize) -> Option<Cell> {
        if let Some(session) = self.shown.get(index) {
            return Some(Cell::Session(session.id.clone()));
        }
        (index == self.shown.len()).then_some(Cell::Add)
    }

    /// Windows says nothing when the pointer leaves, unless asked.
    fn watch_for_leave(&mut self) {
        if !self.tracking_leave {
            let mut request = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: self.hwnd, dwHoverTime: 0 };
            self.tracking_leave = unsafe { TrackMouseEvent(&mut request).is_ok() };
        }
    }

    fn set_hover(&mut self, cell: Option<Cell>) {
        if cell != self.hovered {
            self.hovered = cell;
            self.render();
        }
    }

    fn click(&mut self, hit: Hit) -> Option<Handled> {
        match hit {
            Hit::Tab => self.toggle_collapsed(),
            Hit::Cell(index) => match self.cell_at(index) {
                Some(Cell::Session(id)) => self.send(ClientMessage::Open(id)),
                Some(Cell::Add) => {
                    self.set_hover(None);
                    self.awaiting_library = true;
                    self.send(ClientMessage::Library);
                }
                _ => {}
            },
        }
        Handled::done()
    }

    // ---- collapsing -----------------------------------------------------------------------------

    fn toggle_collapsed(&mut self) {
        self.prefs.collapsed = !self.prefs.collapsed;
        self.prefs.save(&self.data_dir);
        self.hovered = None; // the tab slides out from under the pointer
        self.start_slide(self.target_offset());
        self.render();
    }

    fn start_slide(&mut self, to: i32) {
        self.slide = Some(Slide { started: Instant::now(), from: self.offset, to });
        unsafe {
            SetTimer(Some(self.hwnd), TIMER_SLIDE, 15, None);
        }
    }

    fn step_slide(&mut self) {
        let Some(slide) = &self.slide else { return };
        let t = (slide.started.elapsed().as_millis() as f32 / SLIDE_MS as f32).min(1.0);
        // Ease in when leaving, out when coming back.
        let eased = if self.prefs.collapsed { t * t } else { 1.0 - (1.0 - t) * (1.0 - t) };
        self.offset = slide.from + ((slide.to - slide.from) as f32 * eased).round() as i32;
        if t >= 1.0 {
            self.offset = slide.to;
            self.slide = None;
            unsafe {
                let _ = KillTimer(Some(self.hwnd), TIMER_SLIDE);
            }
        }
        self.render();
    }

    // ---- the agent ------------------------------------------------------------------------------

    fn send(&mut self, message: ClientMessage) {
        if self.status != Status::Online {
            let note = self.lang().say(Text::NotConnected).to_owned();
            return self.show_notice(message.session_id().map(str::to_owned), note);
        }
        self.last_sent = Some(message.clone());
        self.link.send(message);
    }

    fn show_notice(&mut self, session_id: Option<String>, message: String) {
        self.notice = Some(Notice { session_id, message, until: Instant::now() + NOTICE_FOR });
        self.render();
    }

    /// Takes in what the link thread queued. Returns a menu when the library the user asked for has arrived.
    fn on_link(&mut self) -> Option<Menu> {
        let mut menu = None;
        let mut alerts = Vec::new();
        for event in self.link.drain() {
            match event {
                Event::Status(status) => {
                    self.status = status;
                    if status != Status::Online {
                        self.awaiting_library = false;
                    }
                }
                Event::Message(ServerMessage::Hello { protocol, agent }) => {
                    self.protocol_note = (protocol > PROTOCOL_VERSION).then_some((agent, protocol));
                }
                Event::Message(ServerMessage::Error(text)) => {
                    // The agent's errors don't name a session; they answer the last message sent.
                    let session = self.last_sent.as_ref().and_then(|m| m.session_id().map(str::to_owned));
                    self.show_notice(session, text);
                }
                Event::Message(ServerMessage::Library(entries)) => {
                    if std::mem::take(&mut self.awaiting_library) {
                        menu = Some(self.library_menu(&entries));
                    }
                }
                Event::Message(message) => alerts.extend(self.list.apply(message)),
            }
        }
        self.notify(alerts);
        self.render();
        menu
    }

    // ---- menus ----------------------------------------------------------------------------------

    /// Where menus open: beside the strip, toward the middle of the screen.
    fn menu_anchor(&self, index: usize) -> ((i32, i32), bool) {
        let frame = self.frame;
        let strip = self.layout.strip_rect(frame.h, self.prefs.edge).offset(frame.x, frame.y);
        let cell = self.layout.cell_frame(index, strip);
        match self.prefs.edge {
            Edge::Right => ((frame.x, cell.mid_y()), true),
            Edge::Left => ((frame.right(), cell.mid_y()), false),
        }
    }

    fn creature_menu(&self, session: &Session) -> Menu {
        let lang = self.lang();
        let index = self.shown.iter().position(|s| s.id == session.id).unwrap_or(0);
        let (at, leftward) = self.menu_anchor(index);
        let (id, icon) = (&session.id, &session.icon);
        let mut commands = Vec::new();
        let mut entry = |text: &str, command: Command, checked: bool| {
            commands.push(command);
            Item::Entry { text: text.to_owned(), command: commands.len() - 1, checked }
        };
        let current = character(&icon.character).id.clone();
        let characters = characters()
            .iter()
            .map(|c| entry(&c.label, Command::SetIcon(id.clone(), Icon { character: c.id.clone(), color: icon.color.clone() }), c.id == current))
            .collect();
        let colors = PALETTE
            .iter()
            .map(|&(name, color)| {
                let icon = Icon { character: icon.character.clone(), color: color.to_owned() };
                entry(&lang.color_name(name), Command::SetIcon(id.clone(), icon), color == icon_color_lower(session))
            })
            .collect();
        let items = vec![
            Item::Header(session.name.clone()),
            entry(lang.say(Text::Open), Command::Open(id.clone()), false),
            entry(lang.say(if session.pinned { Text::Unpin } else { Text::Pin }), Command::Pin(id.clone(), !session.pinned), false),
            Item::Separator,
            Item::Submenu(lang.say(Text::Character).into(), characters),
            Item::Submenu(lang.say(Text::Color).into(), colors),
        ];
        (Popup { items, at, leftward }, commands)
    }

    fn library_menu(&self, entries: &[LibraryEntry]) -> Menu {
        let lang = self.lang();
        let (at, leftward) = self.menu_anchor(self.shown.len());
        let now = now_ms();
        let mut commands = Vec::new();
        let mut items = vec![Item::Header(lang.say(Text::PinASession).into())];
        if entries.is_empty() {
            items.push(Item::Header(lang.say(Text::NoSessionsInApp).into()));
        }
        for entry in entries {
            commands.push(Command::Pin(entry.id.clone(), !entry.pinned));
            // After the tab, Windows lines the text up on the right.
            let text = format!("{}\t{}", entry.name.replace('&', "&&"), ago(entry.last_activity_at, now, lang));
            items.push(Item::Entry { text, command: commands.len() - 1, checked: entry.pinned });
        }
        (Popup { items, at, leftward }, commands)
    }

    fn tray_menu(&self) -> Menu {
        let lang = self.lang();
        let say = |text: Text| lang.say(text).to_owned();
        let mut cursor = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut cursor);
        }
        let status = match (&self.protocol_note, self.status) {
            (Some((agent, protocol)), _) => lang.newer_protocol(agent, *protocol),
            (None, Status::Online) => lang.connected(self.shown.len()),
            (None, Status::Connecting) => say(Text::Connecting),
            (None, Status::Offline) => say(Text::AgentNotRunning),
            (None, Status::NotPaired) => say(Text::AgentNotSetUp),
        };

        let mut commands = Vec::new();
        let mut entry = |text: String, command: Command, checked: bool| {
            commands.push(command);
            Item::Entry { text, command: commands.len() - 1, checked }
        };
        let prefs = &self.prefs;
        let mut items = vec![
            Item::Header(status),
            Item::Separator,
            entry(say(if prefs.hidden { Text::ShowStrip } else { Text::HideStrip }), Command::ToggleStrip, false),
            Item::Submenu(
                say(Text::Edge),
                vec![
                    entry(say(Text::Left), Command::SetEdge(Edge::Left), prefs.edge == Edge::Left),
                    entry(say(Text::Right), Command::SetEdge(Edge::Right), prefs.edge == Edge::Right),
                ],
            ),
        ];

        let screens = monitors();
        if screens.len() > 1 {
            let chosen = screens::pick(&screens, prefs.monitor.as_deref()).map(|s| s.name.clone());
            let mut choices = vec![entry(say(Text::MainMonitor), Command::SetMonitor(None), prefs.monitor.is_none())];
            for screen in &screens {
                let checked = prefs.monitor.is_some() && chosen.as_deref() == Some(screen.name.as_str());
                choices.push(entry(screen.label(), Command::SetMonitor(Some(screen.name.clone())), checked));
            }
            items.push(Item::Submenu(say(Text::Monitor), choices));
        }

        items.push(Item::Separator);
        items.push(Item::Submenu(
            say(Text::Notifications),
            vec![
                entry(say(Text::NotifyOff), Command::SetNotify(Notify::Off), prefs.notify == Notify::Off),
                entry(say(Text::NotifyAttention), Command::SetNotify(Notify::Attention), prefs.notify == Notify::Attention),
                entry(say(Text::NotifyAll), Command::SetNotify(Notify::All), prefs.notify == Notify::All),
                Item::Separator,
                entry(say(Text::Sound), Command::ToggleSound, prefs.sound),
                entry(say(Text::NotifyTest), Command::TestNotification, false),
            ],
        ));
        items.push(entry(say(Text::PrivateLabels), Command::TogglePrivate, prefs.private));
        let shortcut = prefs.hotkey.unwrap_or(Hotkey::DEFAULT).label();
        let held = if prefs.hotkey.is_some() && !self.hotkey_held { format!(" ({})", say(Text::InUse)) } else { String::new() };
        items.push(entry(format!("{} {shortcut}{held}", say(Text::Shortcut)), Command::ToggleHotkey, prefs.hotkey.is_some()));
        items.push(Item::Submenu(
            say(Text::Language),
            vec![
                entry(say(Text::Automatic), Command::SetLanguage(None), prefs.language.is_none()),
                entry("English".into(), Command::SetLanguage(Some(Lang::En)), prefs.language == Some(Lang::En)),
                entry("Español".into(), Command::SetLanguage(Some(Lang::Es)), prefs.language == Some(Lang::Es)),
            ],
        ));
        items.push(Item::Separator);
        items.push(entry(say(Text::Quit), Command::Quit, false));
        (Popup { items, at: (cursor.x, cursor.y), leftward: true }, commands)
    }

    fn run(&mut self, command: Command) {
        match command {
            Command::Open(id) => self.send(ClientMessage::Open(id)),
            Command::Pin(id, pinned) => self.send(ClientMessage::Pin(id, pinned)),
            Command::SetIcon(id, icon) => self.send(ClientMessage::Icon(id, icon)),
            Command::ToggleStrip => {
                self.prefs.hidden = !self.prefs.hidden;
                self.prefs.save(&self.data_dir);
                self.relayout();
            }
            Command::SetEdge(edge) => {
                self.prefs.edge = edge;
                self.prefs.save(&self.data_dir);
                self.label_shown = None;
                self.relayout();
            }
            Command::SetMonitor(name) => {
                self.prefs.monitor = name;
                self.prefs.save(&self.data_dir);
                self.label_shown = None;
                self.relayout();
            }
            Command::SetLanguage(language) => {
                self.prefs.language = language;
                self.prefs.save(&self.data_dir);
                self.label_shown = None;
                self.render();
            }
            Command::SetNotify(level) => {
                self.prefs.notify = level;
                self.prefs.save(&self.data_dir);
            }
            Command::ToggleSound => {
                self.prefs.sound = !self.prefs.sound;
                self.prefs.save(&self.data_dir);
            }
            Command::TestNotification => {
                // Windows may hold it back (Focus Assist, a full-screen app): this is how to find out.
                let body = self.lang().say(Text::NotifyTestBody);
                self.alert_target = None;
                self.tray.balloon("WinSill", body, self.prefs.sound);
            }
            Command::TogglePrivate => {
                self.prefs.private = !self.prefs.private;
                self.prefs.save(&self.data_dir);
                self.label_shown = None;
                self.render();
            }
            Command::ToggleHotkey => {
                self.prefs.hotkey = if self.prefs.hotkey.is_some() { None } else { Some(Hotkey::DEFAULT) };
                self.prefs.save(&self.data_dir);
                self.register_hotkey();
            }
            Command::Quit => unsafe { PostQuitMessage(0) },
        }
    }

    // ---- language, notifications, shortcut ------------------------------------------------------

    fn lang(&self) -> Lang {
        self.prefs.language.unwrap_or(self.system_lang)
    }

    /// Tells the user, through the tray icon, about the sessions that just changed state: one
    /// notification for the lot, however many there are.
    fn notify(&mut self, alerts: Vec<Alert>) {
        let alerts: Vec<Alert> = alerts.into_iter().filter(|a| self.prefs.notify.allows(a.kind)).collect();
        let Some(first) = alerts.first() else { return };
        let lang = self.lang();
        let (title, body) = if alerts.len() == 1 {
            let detail = first.activity.as_deref().filter(|_| !self.prefs.private && first.kind != AlertKind::Done);
            let body = match detail {
                Some(detail) => format!("{}\n{detail}", lang.say(first.text())),
                None => lang.say(first.text()).to_owned(),
            };
            (first.name.clone(), body)
        } else {
            let mut lines: Vec<String> = alerts.iter().take(3).map(|a| format!("{} {}", a.name, lang.say(a.text()))).collect();
            if alerts.len() > 3 {
                lines.push(format!("+{}", alerts.len() - 3));
            }
            ("WinSill".to_owned(), lines.join("\n"))
        };
        self.alert_target = Some(first.session_id.clone());
        self.tray.balloon(&title, &body, self.prefs.sound);
    }

    /// Takes the shortcut the preferences name (and lets go of the one it had), and notes whether Windows gave it.
    fn register_hotkey(&mut self) {
        unsafe {
            let _ = UnregisterHotKey(Some(self.hwnd), HOTKEY_ID);
            self.hotkey_held = match self.prefs.hotkey {
                Some(hotkey) => RegisterHotKey(Some(self.hwnd), HOTKEY_ID, HOT_KEY_MODIFIERS(hotkey.modifiers()), hotkey.key).is_ok(),
                None => false,
            };
        }
    }
}

fn icon_color_lower(session: &Session) -> String {
    session.icon.color.to_lowercase()
}
