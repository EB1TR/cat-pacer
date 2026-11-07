//! Monitor nativo de Windows. La cola acotada nunca bloquea el bucle CAT.

use crate::{Config, STOP_REQUESTED};
use std::cell::{Cell, RefCell};
use std::io;
use std::mem::{size_of, zeroed};
use std::path::Path;
use std::ptr::null;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, EndPaint, FillRect, HBRUSH,
    HFONT, HPEN, PAINTSTRUCT, PS_SOLID, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE, RedrawWindow,
    RoundRect, SelectObject, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::SetScrollInfo;
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_REALTIME, NIF_SHOWTIP, NIF_TIP, NIIF_INFO, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NINF_KEY, NOTIFYICON_VERSION_4,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const TRAY_MESSAGE: u32 = WM_APP + 1;
const EXIT_MESSAGE: u32 = WM_APP + 2;
const KEY_SELECT: u32 = NIN_SELECT | NINF_KEY;
const SHOW: usize = 1001;
const EXIT: usize = 1002;
const REFRESH: Duration = Duration::from_millis(200);
const FREQUENCY: usize = 0;
const FRESHNESS: usize = 1;
const RX_STATE: usize = 2;
const TX_STATE: usize = 3;
const LAST_TX: usize = 4;
const COUNTERS: usize = 5;
const EVENT: usize = 6;
const APP_TITLE: usize = 7;
const APP_SUBTITLE: usize = 8;
const FREQUENCY_TITLE: usize = 9;
const RX_TITLE: usize = 10;
const RX_DETAILS: usize = 11;
const TX_TITLE: usize = 12;
const TX_DETAILS: usize = 13;
const DOSAGE_TITLE: usize = 14;
const LAST_TX_TITLE: usize = 15;
const INTERVAL_TITLE: usize = 16;
const INTERVAL_VALUE: usize = 17;
const EXPIRY_TITLE: usize = 18;
const EXPIRY_VALUE: usize = 19;
const POLLING_TITLE: usize = 20;
const POLLING_VALUE: usize = 21;
const FOOTER: usize = 22;
const CONTROL_COUNT: usize = 23;

const BACKGROUND: COLORREF = rgb(0x28, 0x2a, 0x36);
const PANEL: COLORREF = rgb(0x21, 0x22, 0x2c);
const SELECTION: COLORREF = rgb(0x44, 0x47, 0x5a);
const FOREGROUND: COLORREF = rgb(0xf8, 0xf8, 0xf2);
const MUTED: COLORREF = rgb(0xbd, 0xc0, 0xd4);
const PURPLE: COLORREF = rgb(0xbd, 0x93, 0xf9);
const CYAN: COLORREF = rgb(0x8b, 0xe9, 0xfd);
const GREEN: COLORREF = rgb(0x50, 0xfa, 0x7b);
const ORANGE: COLORREF = rgb(0xff, 0xb8, 0x6c);
const TONE_MUTED: isize = 1;
const TONE_PURPLE: isize = 2;
const TONE_CYAN: isize = 3;
const TONE_FRESH: isize = 4;
const TONE_RX: isize = 5;
const TONE_TX: isize = 6;
const ON_PANEL: isize = 0x100;
const ALIGN_RIGHT: isize = 0x200;
static TASKBAR_MESSAGE: AtomicU32 = AtomicU32::new(0);
static TRAY_AVAILABLE: AtomicBool = AtomicBool::new(false);
static RESTORE_TRAY: AtomicBool = AtomicBool::new(false);

struct Details {
    rx: String,
    tx: String,
    interval: String,
    expiry: String,
    polling: String,
    config_path: String,
    repeater: bool,
}

struct Theme {
    background: HBRUSH,
    panel: HBRUSH,
    border: HPEN,
    dpi: i32,
    fresh: Cell<bool>,
    rx_connected: Cell<bool>,
    tx_connected: Cell<bool>,
    controls: RefCell<Vec<Control>>,
    fonts: Cell<[HFONT; 5]>,
    layout: Cell<Option<Layout>>,
    scroll: Cell<i32>,
    viewport: Cell<i32>,
    arranging: Cell<bool>,
    wheel_delta: Cell<i32>,
}

struct Control {
    window: HWND,
    font: usize,
    current_font: usize,
}

#[derive(Clone, Copy)]
struct Layout {
    controls: [[i32; 4]; CONTROL_COUNT],
    panels: [[i32; 4]; 5],
    content_height: i32,
    frequency_font: usize,
    compact: bool,
}

impl Layout {
    fn calculate(width: i32, height: i32) -> Self {
        let narrow = width < 600;
        let compact = height < 620;
        let padding = if narrow || compact { 16 } else { 24 };
        let gap = if compact { 10 } else { 16 };
        let inner = (width - 2 * padding).max(1);
        let inset = 16;
        let text_width = (inner - 2 * inset).max(1);
        let mut layout = Self {
            controls: [[0; 4]; CONTROL_COUNT],
            panels: [[0; 4]; 5],
            content_height: 0,
            frequency_font: if narrow || compact { 4 } else { 1 },
            compact,
        };
        let controls = &mut layout.controls;
        let mut y = padding;
        let heading_height = if compact { 24 } else { 26 };
        controls[APP_TITLE] = [padding, y, inner, heading_height];
        y += heading_height;
        controls[APP_SUBTITLE] = [padding, y, inner, 20];
        y += 20 + gap;

        let number_height = if layout.frequency_font == 1 { 58 } else { 40 };
        let status_height = if narrow { 44 } else { 24 };
        let frequency_height = 24 + 20 + number_height + status_height;
        layout.panels[0] = [padding, y, inner, frequency_height];
        controls[FREQUENCY_TITLE] = [padding + inset, y + 12, text_width, 20];
        controls[FREQUENCY] = [padding + inset, y + 32, text_width, number_height];
        controls[FRESHNESS] = [
            padding + inset,
            y + 32 + number_height,
            text_width,
            status_height,
        ];
        y += frequency_height + gap;

        let card_height = if compact { 96 } else { 112 };
        let card_width = if narrow { inner } else { (inner - gap) / 2 };
        for (column, panel, title, state, details) in [
            (0, 1, RX_TITLE, RX_STATE, RX_DETAILS),
            (1, 2, TX_TITLE, TX_STATE, TX_DETAILS),
        ] {
            let x = padding
                + if narrow {
                    0
                } else {
                    column * (card_width + gap)
                };
            let top = y + if narrow {
                column * (card_height + gap)
            } else {
                0
            };
            let line_width = (card_width - 2 * inset).max(1);
            layout.panels[panel] = [x, top, card_width, card_height];
            controls[title] = [x + inset, top + 12, line_width, 26];
            controls[state] = [x + inset, top + 40, line_width, 24];
            controls[details] = [x + inset, top + card_height - 30, line_width, 20];
        }
        y += if narrow {
            card_height * 2 + gap
        } else {
            card_height
        };
        y += gap;
        controls[DOSAGE_TITLE] = [padding, y, inner, 20];
        y += 28;

        let row_height = if narrow { 50 } else { 32 };
        let table_height = row_height * 4 + 24;
        layout.panels[3] = [padding, y, inner, table_height];
        for (row, (caption, value)) in [
            (LAST_TX_TITLE, LAST_TX),
            (INTERVAL_TITLE, INTERVAL_VALUE),
            (EXPIRY_TITLE, EXPIRY_VALUE),
            (POLLING_TITLE, POLLING_VALUE),
        ]
        .into_iter()
        .enumerate()
        {
            let top = y + 12 + row as i32 * row_height;
            if narrow {
                controls[caption] = [padding + inset, top, text_width, 22];
                controls[value] = [padding + inset, top + 24, text_width, 22];
            } else {
                let half = text_width / 2;
                controls[caption] = [padding + inset, top, half, 24];
                controls[value] = [padding + inset + half, top, text_width - half, 24];
            }
        }
        y += table_height + gap;
        let counters_height = if narrow { 72 } else { 28 };
        controls[COUNTERS] = [padding, y, inner, counters_height];
        y += counters_height + gap;
        let event_height = if narrow { 88 } else { 64 };
        layout.panels[4] = [padding, y, inner, event_height];
        controls[EVENT] = [padding + inset, y + 12, text_width, event_height - 24];
        y += event_height + gap;
        let footer_height = if narrow { 44 } else { 20 };
        y = y.max(height - padding - footer_height);
        controls[FOOTER] = [padding, y, inner, footer_height];
        layout.content_height = y + footer_height + padding;
        layout
    }
}

impl Theme {
    fn new() -> Result<Self, String> {
        // SAFETY: colores y estilo son valores Win32 validos; el objeto posee los recursos.
        let theme = unsafe {
            Self {
                background: CreateSolidBrush(BACKGROUND),
                panel: CreateSolidBrush(PANEL),
                border: CreatePen(PS_SOLID, 1, SELECTION),
                dpi: GetDpiForSystem().max(96) as i32,
                fresh: Cell::new(false),
                rx_connected: Cell::new(false),
                tx_connected: Cell::new(false),
                controls: RefCell::new(Vec::with_capacity(CONTROL_COUNT)),
                fonts: Cell::new([0; 5]),
                layout: Cell::new(None),
                scroll: Cell::new(0),
                viewport: Cell::new(0),
                arranging: Cell::new(false),
                wheel_delta: Cell::new(0),
            }
        };
        if theme.background == 0 || theme.panel == 0 || theme.border == 0 {
            return Err(native_error("crear el tema Dracula"));
        }
        Ok(theme)
    }

    fn scale(&self, value: i32) -> i32 {
        value * self.dpi / 96
    }

    fn unscale(&self, value: i32) -> i32 {
        value * 96 / self.dpi
    }
}

impl Drop for Theme {
    fn drop(&mut self) {
        // SAFETY: recursos propios; la ventana y sus controles ya se han destruido.
        unsafe {
            for resource in [self.background, self.panel, self.border] {
                if resource != 0 {
                    DeleteObject(resource);
                }
            }
        }
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct Status {
    pub frequency: Option<[u8; 11]>,
    pub last_tx: Option<[u8; 11]>,
    pub fresh: bool,
    pub poll_pending: bool,
    pub received_after_poll: bool,
    pub rx_connected: bool,
    pub tx_connected: bool,
    pub rx_count: u64,
    pub tx_count: u64,
    pub poll_count: u64,
    pub last_event: Arc<str>,
}

pub struct Monitor {
    pub state: Status,
    sender: SyncSender<Status>,
    window: HWND,
    worker: Option<JoinHandle<()>>,
    next_update: Instant,
}

impl Monitor {
    pub fn start(config: &Config, path: &Path) -> Result<Self, String> {
        let (sender, receiver) = sync_channel(1);
        let (ready, started) = sync_channel(1);
        let details = Details {
            rx: format!("{}  ·  {} baudios", config.rx_port, config.rx_baud),
            tx: format!("{}  ·  {} baudios", config.tx_port, config.tx_baud),
            interval: format!("{:.0} ms", config.tx_time * 1000.0),
            expiry: format!("{:.2} s", config.ex_data),
            polling: if config.polling {
                "Activado"
            } else {
                "Desactivado"
            }
            .into(),
            config_path: format!("Configuración: {}", path.display()),
            repeater: config.repeater,
        };
        let worker = thread::Builder::new()
            .name("cat-pacer-ui".into())
            .spawn(move || match NativeWindow::new(&details) {
                Ok(mut ui) => {
                    if ready.send(Ok(ui.window)).is_ok() {
                        ui.run(receiver, &details);
                    }
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            })
            .map_err(|error| format!("no se puede iniciar la ventana: {error}"))?;
        let window = started
            .recv()
            .map_err(|_| "el monitor no pudo iniciarse".to_string())??;
        Ok(Self {
            state: Status::default(),
            sender,
            window,
            worker: Some(worker),
            next_update: Instant::now(),
        })
    }

    pub fn update(&mut self, now: Instant) {
        if now >= self.next_update {
            // Si la ventana esta ocupada, se omite esta actualizacion; CAT sigue.
            if let Err(TrySendError::Disconnected(_)) = self.sender.try_send(self.state.clone()) {
                STOP_REQUESTED.store(true, Ordering::Relaxed);
            }
            self.next_update = Instant::now() + REFRESH;
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        // SAFETY: el HWND procede del hilo de interfaz; PostMessage no lo desreferencia.
        unsafe { PostMessageW(self.window, EXIT_MESSAGE, 0, 0) };
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Label {
    window: HWND,
    text: String,
}

impl Label {
    fn set(&mut self, text: String) {
        if self.text != text {
            // SAFETY: el control pertenece al hilo actual y Windows copia el texto.
            unsafe { SetWindowTextW(self.window, wide(&text).as_ptr()) };
            self.text = text;
        }
    }
}

struct NativeWindow {
    window: HWND,
    icon: HICON,
    fonts: [HFONT; 5],
    labels: Vec<Label>,
    theme: Box<Theme>,
    notification: NOTIFYICONDATAW,
    registered: bool,
    next_tray_attempt: Instant,
}

impl NativeWindow {
    fn new(details: &Details) -> Result<Self, String> {
        // SAFETY: se configura el escalado antes de crear ventanas en este proceso.
        unsafe { SetProcessDPIAware() };
        // SAFETY: cero es un valor inicial valido para los handles y NOTIFYICONDATAW.
        let mut ui = Self {
            window: 0,
            icon: 0,
            fonts: [0; 5],
            labels: Vec::with_capacity(7),
            theme: Box::new(Theme::new()?),
            notification: unsafe { zeroed() },
            registered: false,
            next_tray_attempt: Instant::now(),
        };
        // SAFETY: todas las estructuras se inicializan y los buffers viven durante cada llamada.
        unsafe {
            let dpi = ui.theme.dpi;
            let scale = |value: i32| value * dpi / 96;
            let instance = GetModuleHandleW(null());
            ui.icon = LoadImageW(
                instance,
                1_usize as *const u16,
                IMAGE_ICON,
                32,
                32,
                LR_DEFAULTCOLOR,
            );
            if instance == 0 || ui.icon == 0 {
                return Err(native_error("crear el icono"));
            }
            let class = wide("CATPacerWindow");
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                hIcon: ui.icon,
                hCursor: LoadCursorW(0, IDC_ARROW),
                hbrBackground: ui.theme.background,
                lpszClassName: class.as_ptr(),
                ..zeroed()
            };
            if RegisterClassW(&window_class) == 0 {
                return Err(native_error("registrar la ventana"));
            }
            ui.registered = true;
            TASKBAR_MESSAGE.store(
                RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
                Ordering::Relaxed,
            );
            let style = WS_OVERLAPPED
                | WS_CAPTION
                | WS_SYSMENU
                | WS_CLIPCHILDREN
                | WS_THICKFRAME
                | WS_VSCROLL;
            let mut bounds = RECT {
                left: 0,
                top: 0,
                right: scale(700),
                bottom: scale(724),
            };
            if AdjustWindowRectEx(&mut bounds, style, 0, 0) == 0 {
                return Err(native_error("calcular el tamaño de la ventana"));
            }
            ui.window = CreateWindowExW(
                0,
                class.as_ptr(),
                wide("CAT Pacer").as_ptr(),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                0,
                0,
                instance,
                null(),
            );
            if ui.window == 0 {
                return Err(native_error("crear la ventana"));
            }
            // Box mantiene estable la direccion del tema durante toda la vida del HWND.
            SetWindowLongPtrW(
                ui.window,
                GWLP_USERDATA,
                (&*ui.theme as *const Theme) as isize,
            );
            for (attribute, value) in [
                (DWMWA_USE_IMMERSIVE_DARK_MODE, 1_u32),
                (DWMWA_CAPTION_COLOR, BACKGROUND),
                (DWMWA_TEXT_COLOR, FOREGROUND),
                (DWMWA_BORDER_COLOR, SELECTION),
            ] {
                // Los Windows anteriores ignoran los atributos de titulo no soportados.
                DwmSetWindowAttribute(
                    ui.window,
                    attribute as u32,
                    (&value as *const u32).cast(),
                    size_of::<u32>() as u32,
                );
            }
            for (index, (size, weight, face)) in [
                (16, 400, "Segoe UI"),
                (40, 600, "Consolas"),
                (13, 400, "Segoe UI"),
                (20, 600, "Segoe UI"),
                (24, 600, "Consolas"),
            ]
            .into_iter()
            .enumerate()
            {
                ui.fonts[index] = CreateFontW(
                    -scale(size),
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    1,
                    0,
                    0,
                    5,
                    0,
                    wide(face).as_ptr(),
                );
                if ui.fonts[index] == 0 {
                    return Err(native_error("crear la fuente"));
                }
            }
            ui.theme.fonts.set(ui.fonts);
            // Campos dinamicos: el orden coincide con las constantes FREQUENCY..EVENT.
            for (text, font, paint) in [
                ("—", 1, ON_PANEL | TONE_CYAN),
                (
                    "Esperando una frecuencia CAT válida",
                    0,
                    ON_PANEL | TONE_FRESH,
                ),
                ("Esperando conexión", 0, ON_PANEL | TONE_RX),
                ("Esperando conexión", 0, ON_PANEL | TONE_TX),
                ("—", 0, ON_PANEL | ALIGN_RIGHT | TONE_CYAN),
                (
                    "CAT recibidos: 0      Envíos: 0      Polling: 0",
                    0,
                    TONE_CYAN,
                ),
                ("Iniciando…", 2, ON_PANEL),
            ] {
                let label = ui.label(text, font, paint)?;
                ui.labels.push(label);
            }
            for (text, font, paint) in [
                ("CAT Pacer", 3, 0),
                ("DOSIFICADOR DE FRECUENCIA CAT", 2, TONE_MUTED),
                ("FRECUENCIA DEL K3", 2, ON_PANEL | TONE_PURPLE),
                ("Elecraft K3", 3, ON_PANEL),
                (details.rx.as_str(), 2, ON_PANEL | TONE_MUTED),
                ("Expert 1K3FA", 3, ON_PANEL),
                (details.tx.as_str(), 2, ON_PANEL | TONE_MUTED),
                ("DOSIFICACIÓN", 2, TONE_PURPLE),
                ("Última frecuencia enviada", 0, ON_PANEL | TONE_MUTED),
                ("Cadencia de envío", 0, ON_PANEL | TONE_MUTED),
                (details.interval.as_str(), 0, ON_PANEL | ALIGN_RIGHT),
                ("Vigencia de datos", 0, ON_PANEL | TONE_MUTED),
                (details.expiry.as_str(), 0, ON_PANEL | ALIGN_RIGHT),
                ("Polling de respaldo", 0, ON_PANEL | TONE_MUTED),
                (details.polling.as_str(), 0, ON_PANEL | ALIGN_RIGHT),
                (
                    "Cerrar envía a la bandeja. Para terminar, elige Salir desde el icono.",
                    2,
                    TONE_MUTED,
                ),
            ] {
                ui.label(text, font, paint)?;
            }
            SetWindowTextW(
                ui.window,
                wide(&format!("CAT Pacer {}", env!("CARGO_PKG_VERSION"))).as_ptr(),
            );
            ui.notification.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            ui.notification.hWnd = ui.window;
            ui.notification.uID = 1;
            ui.notification.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
            ui.notification.uCallbackMessage = TRAY_MESSAGE;
            ui.notification.hIcon = ui.icon;
            ui.set_tooltip("CAT Pacer · Esperando frecuencia");
            ui.add_tray();
            if SetTimer(ui.window, 1, REFRESH.as_millis() as u32, None) == 0 {
                return Err(native_error("iniciar el refresco del monitor"));
            }
            arrange(ui.window, &ui.theme);
            ShowWindow(ui.window, SW_SHOW);
        }
        Ok(ui)
    }

    fn label(&self, text: &str, font: usize, paint: isize) -> Result<Label, String> {
        // SAFETY: el control es hijo del HWND vivo; los textos se copian durante la llamada.
        unsafe {
            let child = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | 0x80 | if paint & ALIGN_RIGHT != 0 { 2 } else { 0 },
                0,
                0,
                0,
                0,
                self.window,
                0,
                GetModuleHandleW(null()),
                null(),
            );
            if child == 0 {
                return Err(native_error("crear un campo del monitor"));
            }
            SetWindowLongPtrW(child, GWLP_USERDATA, paint);
            SendMessageW(child, WM_SETFONT, self.fonts[font] as usize, 1);
            self.theme.controls.borrow_mut().push(Control {
                window: child,
                font,
                current_font: font,
            });
            Ok(Label {
                window: child,
                text: text.into(),
            })
        }
    }

    fn run(&mut self, receiver: Receiver<Status>, details: &Details) {
        // SAFETY: GetMessage inicializa MSG; el HWND y sus controles viven hasta Drop.
        unsafe {
            let mut message: MSG = zeroed();
            loop {
                let result = GetMessageW(&mut message, 0, 0, 0);
                if result <= 0 {
                    STOP_REQUESTED.store(true, Ordering::Relaxed);
                    break;
                }
                if let Some(status) = receiver.try_iter().last() {
                    self.render(&status, details);
                }
                if RESTORE_TRAY.swap(false, Ordering::Relaxed) {
                    TRAY_AVAILABLE.store(false, Ordering::Relaxed);
                    self.next_tray_attempt = Instant::now();
                }
                if !TRAY_AVAILABLE.load(Ordering::Relaxed)
                    && Instant::now() >= self.next_tray_attempt
                {
                    self.add_tray();
                    self.next_tray_attempt = Instant::now() + Duration::from_secs(2);
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    fn render(&mut self, status: &Status, details: &Details) {
        self.theme.fresh.set(status.fresh);
        self.theme.rx_connected.set(status.rx_connected);
        self.theme.tx_connected.set(status.tx_connected);
        let frequency = format_frequency(status.frequency);
        self.labels[FREQUENCY].set(frequency.clone());
        self.labels[FRESHNESS].set(
            if !status.rx_connected {
                "Sin conexión con el K3 · Esperando reconexión"
            } else if status.poll_pending {
                "Polling propio · Esperando respuesta del K3"
            } else if status.fresh && status.received_after_poll {
                "Información vigente · Recibida tras polling propio"
            } else if status.fresh {
                "Información vigente · Sin consulta propia pendiente"
            } else if status.frequency.is_some() {
                "Información caducada · Esperando actualización"
            } else {
                "Esperando una frecuencia CAT válida"
            }
            .into(),
        );
        self.labels[RX_STATE].set(connection(status.rx_connected).into());
        self.labels[TX_STATE].set(if details.repeater {
            connection(status.tx_connected).into()
        } else {
            "Repetidor desactivado".into()
        });
        self.labels[LAST_TX].set(format_frequency(status.last_tx));
        self.labels[COUNTERS].set(format!(
            "CAT recibidos: {}      Envíos: {}      Polling: {}",
            status.rx_count, status.tx_count, status.poll_count,
        ));
        self.labels[EVENT].set(if status.last_event.is_empty() {
            details.config_path.clone()
        } else {
            status.last_event.to_string()
        });
        let tip = format!(
            "CAT Pacer · {frequency} · K3 {}",
            connection(status.rx_connected)
        );
        if self.set_tooltip(&tip) && TRAY_AVAILABLE.load(Ordering::Relaxed) {
            // SAFETY: notification pertenece a esta ventana y todos sus campos estan inicializados.
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &self.notification) };
        }
    }

    fn set_tooltip(&mut self, text: &str) -> bool {
        let mut tip = [0_u16; 128];
        for (slot, unit) in tip.iter_mut().take(127).zip(text.encode_utf16()) {
            *slot = unit;
        }
        let changed = self.notification.szTip != tip;
        self.notification.szTip = tip;
        changed
    }

    fn add_tray(&mut self) {
        // SAFETY: notification referencia la ventana y el icono vivos de este objeto.
        unsafe {
            if Shell_NotifyIconW(NIM_ADD, &self.notification) != 0 {
                self.notification.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                Shell_NotifyIconW(NIM_SETVERSION, &self.notification);
                TRAY_AVAILABLE.store(true, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        // SAFETY: todos estos recursos fueron creados y son liberados por el mismo hilo.
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &self.notification);
            TRAY_AVAILABLE.store(false, Ordering::Relaxed);
            if self.window != 0 {
                DestroyWindow(self.window);
            }
            for font in self.fonts {
                if font != 0 {
                    DeleteObject(font);
                }
            }
            if self.registered {
                UnregisterClassW(wide("CATPacerWindow").as_ptr(), GetModuleHandleW(null()));
            }
            if self.icon != 0 {
                DestroyIcon(self.icon);
            }
        }
    }
}

fn arrange(window: HWND, theme: &Theme) {
    // SetScrollInfo puede provocar WM_SIZE; el guard evita recalculos reentrantes.
    if theme.arranging.replace(true) {
        return;
    }
    if theme.controls.borrow().len() != CONTROL_COUNT {
        theme.arranging.set(false);
        return;
    }
    // SAFETY: el HWND, los controles y las fuentes pertenecen al hilo de interfaz.
    unsafe {
        for _ in 0..3 {
            let mut client: RECT = zeroed();
            GetClientRect(window, &mut client);
            if client.right <= 0 || client.bottom <= 0 {
                theme.arranging.set(false);
                return;
            }
            let width = theme.unscale(client.right);
            let height = theme.unscale(client.bottom).max(1);
            let layout = Layout::calculate(width, height);
            let scroll = theme
                .scroll
                .get()
                .clamp(0, (layout.content_height - height).max(0));
            let info = SCROLLINFO {
                cbSize: size_of::<SCROLLINFO>() as u32,
                fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
                nMin: 0,
                nMax: layout.content_height - 1,
                nPage: height as u32,
                nPos: scroll,
                ..zeroed()
            };
            theme.layout.set(Some(layout));
            theme.scroll.set(scroll);
            theme.viewport.set(height);
            SetScrollInfo(window, SB_VERT, &info, 1);
            let mut adjusted: RECT = zeroed();
            GetClientRect(window, &mut adjusted);
            if adjusted.right == client.right {
                break;
            }
        }
        if let Some(layout) = theme.layout.get() {
            let fonts = theme.fonts.get();
            for (index, control) in theme.controls.borrow_mut().iter_mut().enumerate() {
                let [x, y, width, height] = layout.controls[index];
                let font = if index == FREQUENCY {
                    layout.frequency_font
                } else if layout.compact && control.font == 3 {
                    0
                } else {
                    control.font
                };
                if control.current_font != font {
                    SendMessageW(control.window, WM_SETFONT, fonts[font] as usize, 0);
                    control.current_font = font;
                }
                MoveWindow(
                    control.window,
                    theme.scale(x),
                    theme.scale(y - theme.scroll.get()),
                    theme.scale(width),
                    theme.scale(height),
                    0,
                );
            }
        }
        RedrawWindow(
            window,
            null(),
            0,
            RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_ERASE,
        );
    }
    theme.arranging.set(false);
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SAFETY: Windows proporciona el HWND y los parametros del mensaje; no se usan punteros propios.
    unsafe {
        if message != 0 && message == TASKBAR_MESSAGE.load(Ordering::Relaxed) {
            RESTORE_TRAY.store(true, Ordering::Relaxed);
            return 0;
        }
        match message {
            WM_SIZE => {
                let theme = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Theme;
                if w != SIZE_MINIMIZED as usize
                    && let Some(theme) = theme.as_ref()
                {
                    arrange(window, theme);
                }
                0
            }
            WM_GETMINMAXINFO => {
                let theme = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Theme;
                if let (Some(theme), Some(info)) = (theme.as_ref(), (l as *mut MINMAXINFO).as_mut())
                {
                    let mut minimum = RECT {
                        left: 0,
                        top: 0,
                        right: theme.scale(320) + GetSystemMetrics(SM_CXVSCROLL),
                        bottom: theme.scale(220),
                    };
                    AdjustWindowRectEx(
                        &mut minimum,
                        GetWindowLongPtrW(window, GWL_STYLE) as u32,
                        0,
                        0,
                    );
                    info.ptMinTrackSize.x = minimum.right - minimum.left;
                    info.ptMinTrackSize.y = minimum.bottom - minimum.top;
                    0
                } else {
                    DefWindowProcW(window, message, w, l)
                }
            }
            WM_VSCROLL => {
                let theme = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Theme;
                if let Some(theme) = theme.as_ref()
                    && let Some(layout) = theme.layout.get()
                {
                    let current = theme.scroll.get();
                    let maximum = (layout.content_height - theme.viewport.get()).max(0);
                    let page = (theme.viewport.get() - 32).max(24);
                    let position = match (w & 0xffff) as i32 {
                        SB_LINEUP => current - 24,
                        SB_LINEDOWN => current + 24,
                        SB_PAGEUP => current - page,
                        SB_PAGEDOWN => current + page,
                        SB_TOP => 0,
                        SB_BOTTOM => maximum,
                        SB_THUMBTRACK | SB_THUMBPOSITION => {
                            let mut info = SCROLLINFO {
                                cbSize: size_of::<SCROLLINFO>() as u32,
                                fMask: SIF_TRACKPOS,
                                ..zeroed()
                            };
                            if GetScrollInfo(window, SB_VERT, &mut info) != 0 {
                                info.nTrackPos
                            } else {
                                current
                            }
                        }
                        _ => return 0,
                    };
                    theme.scroll.set(position.clamp(0, maximum));
                    arrange(window, theme);
                }
                0
            }
            WM_MOUSEWHEEL => {
                let theme = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Theme;
                if let Some(theme) = theme.as_ref()
                    && let Some(layout) = theme.layout.get()
                {
                    let delta = ((w >> 16) as u16 as i16) as i32 + theme.wheel_delta.get();
                    theme.wheel_delta.set(delta % 120);
                    let steps = delta / 120;
                    let maximum = (layout.content_height - theme.viewport.get()).max(0);
                    let scroll = (theme.scroll.get() - steps * 48).clamp(0, maximum);
                    if scroll != theme.scroll.get() {
                        theme.scroll.set(scroll);
                        arrange(window, theme);
                    }
                }
                0
            }
            WM_CLOSE => {
                if TRAY_AVAILABLE.load(Ordering::Relaxed) {
                    ShowWindow(window, SW_HIDE);
                    let mut notice: NOTIFYICONDATAW = zeroed();
                    notice.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
                    notice.hWnd = window;
                    notice.uID = 1;
                    notice.uFlags = NIF_INFO | NIF_REALTIME;
                    notice.dwInfoFlags = NIIF_INFO;
                    for (slot, unit) in notice
                        .szInfoTitle
                        .iter_mut()
                        .zip("CAT Pacer".encode_utf16())
                    {
                        *slot = unit;
                    }
                    for (slot, unit) in notice.szInfo.iter_mut().zip(
                        "Sigue ejecutándose en segundo plano. Para terminar, elige Salir en el icono de bandeja."
                            .encode_utf16(),
                    ) {
                        *slot = unit;
                    }
                    Shell_NotifyIconW(NIM_MODIFY, &notice);
                } else {
                    DestroyWindow(window);
                }
                0
            }
            WM_COMMAND => {
                match w & 0xffff {
                    SHOW => {
                        ShowWindow(window, SW_RESTORE);
                        SetForegroundWindow(window);
                    }
                    EXIT => {
                        DestroyWindow(window);
                    }
                    _ => {}
                }
                0
            }
            TRAY_MESSAGE => {
                match l as u32 & 0xffff {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK | NIN_SELECT | KEY_SELECT => {
                        ShowWindow(window, SW_RESTORE);
                        SetForegroundWindow(window);
                    }
                    WM_CONTEXTMENU | WM_RBUTTONUP => {
                        let menu = CreatePopupMenu();
                        if menu != 0 {
                            AppendMenuW(menu, MF_STRING, SHOW, wide("Mostrar CAT Pacer").as_ptr());
                            AppendMenuW(menu, MF_SEPARATOR, 0, null());
                            AppendMenuW(menu, MF_STRING, EXIT, wide("Salir").as_ptr());
                            let mut point: POINT = zeroed();
                            GetCursorPos(&mut point);
                            SetForegroundWindow(window);
                            let selected = TrackPopupMenu(
                                menu,
                                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                                point.x,
                                point.y,
                                0,
                                window,
                                null(),
                            );
                            DestroyMenu(menu);
                            if selected != 0 {
                                PostMessageW(window, WM_COMMAND, selected as usize, 0);
                            }
                            PostMessageW(window, WM_NULL, 0, 0);
                        }
                    }
                    _ => {}
                }
                0
            }
            EXIT_MESSAGE => {
                EndMenu();
                DestroyWindow(window);
                0
            }
            WM_DESTROY => {
                STOP_REQUESTED.store(true, Ordering::Relaxed);
                PostQuitMessage(0);
                0
            }
            WM_PAINT => {
                let theme = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Theme;
                if let Some(theme) = theme.as_ref() {
                    let mut paint: PAINTSTRUCT = zeroed();
                    let dc = BeginPaint(window, &mut paint);
                    let mut client: RECT = zeroed();
                    GetClientRect(window, &mut client);
                    FillRect(dc, &client, theme.background);
                    let old_brush = SelectObject(dc, theme.panel);
                    let old_pen = SelectObject(dc, theme.border);
                    if let Some(layout) = theme.layout.get() {
                        for [x, y, width, height] in layout.panels {
                            let y = y - theme.scroll.get();
                            RoundRect(
                                dc,
                                theme.scale(x),
                                theme.scale(y),
                                theme.scale(x + width),
                                theme.scale(y + height),
                                theme.scale(12),
                                theme.scale(12),
                            );
                        }
                    }
                    SelectObject(dc, old_pen);
                    SelectObject(dc, old_brush);
                    EndPaint(window, &paint);
                    0
                } else {
                    DefWindowProcW(window, message, w, l)
                }
            }
            WM_CTLCOLORSTATIC => {
                let theme = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Theme;
                if let Some(theme) = theme.as_ref() {
                    let style = GetWindowLongPtrW(l, GWLP_USERDATA);
                    let color = match style & 0xff {
                        TONE_MUTED => MUTED,
                        TONE_PURPLE => PURPLE,
                        TONE_CYAN => CYAN,
                        TONE_FRESH => {
                            if theme.fresh.get() {
                                GREEN
                            } else {
                                ORANGE
                            }
                        }
                        TONE_RX => {
                            if theme.rx_connected.get() {
                                GREEN
                            } else {
                                ORANGE
                            }
                        }
                        TONE_TX => {
                            if theme.tx_connected.get() {
                                GREEN
                            } else {
                                ORANGE
                            }
                        }
                        _ => FOREGROUND,
                    };
                    let panel = style & ON_PANEL != 0;
                    SetTextColor(w as isize, color);
                    SetBkColor(w as isize, if panel { PANEL } else { BACKGROUND });
                    SetBkMode(w as isize, TRANSPARENT as i32);
                    if panel { theme.panel } else { theme.background }
                } else {
                    DefWindowProcW(window, message, w, l)
                }
            }
            _ => DefWindowProcW(window, message, w, l),
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as u32 | ((green as u32) << 8) | ((blue as u32) << 16)
}

pub fn show_error(error: &str) {
    // SAFETY: Windows copia los textos antes de retornar; no se crea una consola.
    unsafe {
        MessageBoxW(
            0,
            wide(error).as_ptr(),
            wide("CAT Pacer · Error").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn activate_existing() -> bool {
    // SAFETY: solo se localiza la ventana de CAT Pacer y se publica su comando Mostrar.
    unsafe {
        let window = FindWindowW(wide("CATPacerWindow").as_ptr(), null());
        if window == 0 {
            return false;
        }
        let mut owner = 0;
        if GetWindowThreadProcessId(window, &mut owner) != 0 {
            AllowSetForegroundWindow(owner);
        }
        PostMessageW(window, WM_COMMAND, SHOW, 0);
        true
    }
}

fn native_error(action: &str) -> String {
    format!("no se puede {action}: {}", io::Error::last_os_error())
}

fn connection(connected: bool) -> &'static str {
    if connected {
        "conectado"
    } else {
        "reintentando"
    }
}

fn format_frequency(frequency: Option<[u8; 11]>) -> String {
    match frequency {
        Some(value) => {
            let hz = value
                .iter()
                .fold(0_u64, |total, digit| total * 10 + u64::from(digit - b'0'));
            format!("{}.{:06} MHz", hz / 1_000_000, hz % 1_000_000)
        }
        None => "—".into(),
    }
}
