#![cfg_attr(windows, windows_subsystem = "windows")]

use serde::Deserialize;
use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    CTRL_BREAK_EVENT, CTRL_C_EVENT, GetConsoleWindow, SetConsoleCtrlHandler,
};

#[cfg(windows)]
mod instance;
#[cfg(windows)]
mod ui;

const SERIAL_TIMEOUT: Duration = Duration::from_millis(20);
const POLL_RETRY: Duration = Duration::from_secs(1);
const RECONNECT_RETRY: Duration = Duration::from_secs(2);
const MAX_RX_BUFFER: usize = 4096;
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
unsafe extern "system" fn console_control_handler(event: u32) -> i32 {
    match event {
        CTRL_C_EVENT | CTRL_BREAK_EVENT => {
            // El manejador solo avisa; el bucle principal libera los puertos.
            STOP_REQUESTED.store(true, Ordering::Relaxed);
            1
        }
        _ => 0,
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    rx_port: String,
    rx_baud: u32,
    tx_port: String,
    tx_baud: u32,
    ex_data: f64,
    tx_time: f64,
    polling: bool,
    repeater: bool,
}

impl Config {
    fn load(path: &Path) -> Result<Self, String> {
        let contents = fs::read_to_string(path)
            .map_err(|error| format!("no se puede leer {}: {error}", path.display()))?;
        let config: Self = toml::from_str(&contents)
            .map_err(|error| format!("configuracion no valida en {}: {error}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.rx_port.trim().is_empty() || self.tx_port.trim().is_empty() {
            return Err("los nombres de los puertos no pueden estar vacios".into());
        }
        if self.rx_baud == 0 || self.tx_baud == 0 {
            return Err("las velocidades serie deben ser mayores que cero".into());
        }
        if !self.ex_data.is_finite() || self.ex_data <= 0.0 {
            return Err("ex_data debe ser un numero positivo".into());
        }
        if !self.tx_time.is_finite() || self.tx_time <= 0.0 {
            return Err("tx_time debe ser un numero positivo".into());
        }
        if self.repeater && self.rx_port.eq_ignore_ascii_case(&self.tx_port) {
            return Err("rx_port y tx_port deben ser diferentes".into());
        }
        Ok(())
    }
}

fn open_port(name: &str, baud: u32) -> serialport::Result<Box<dyn SerialPort>> {
    serialport::new(name, baud)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::Two)
        .flow_control(FlowControl::None)
        .timeout(SERIAL_TIMEOUT)
        .open()
}

fn extract_frequency(frame: &[u8]) -> Option<[u8; 11]> {
    if frame.len() < 14 || (!frame.starts_with(b"IF") && !frame.starts_with(b"FA")) {
        return None;
    }

    let digits: [u8; 11] = frame[2..13].try_into().ok()?;
    digits.iter().all(u8::is_ascii_digit).then_some(digits)
}

fn frequency_command(frequency: [u8; 11]) -> [u8; 14] {
    let mut command = [0_u8; 14];
    command[0..2].copy_from_slice(b"FA");
    command[2..13].copy_from_slice(&frequency);
    command[13] = b';';
    command
}

fn config_path() -> Result<PathBuf, String> {
    if let Some(path) = env::args_os().nth(1) {
        return Ok(PathBuf::from(path));
    }

    let executable = env::current_exe()
        .map_err(|error| format!("no se puede determinar la ruta del ejecutable: {error}"))?;
    let directory = executable
        .parent()
        .ok_or_else(|| "el ejecutable no tiene un directorio valido".to_string())?;
    Ok(directory.join("config.toml"))
}

fn run() -> Result<(), String> {
    let config_file = config_path()?;
    let config = Config::load(&config_file)?;
    let expiry = Duration::try_from_secs_f64(config.ex_data)
        .map_err(|_| "ex_data esta fuera del rango admitido".to_string())?;
    let send_interval = Duration::try_from_secs_f64(config.tx_time)
        .map_err(|_| "tx_time esta fuera del rango admitido".to_string())?;

    #[cfg(windows)]
    if unsafe { GetConsoleWindow() } != 0 {
        // SAFETY: el callback tiene la firma Win32 y solo usa una bandera atomica estatica.
        // Tambien se habilita Ctrl+C si el proceso padre lo tenia desactivado.
        if unsafe { SetConsoleCtrlHandler(Some(console_control_handler), 1) } == 0
            || unsafe { SetConsoleCtrlHandler(None, 0) } == 0
        {
            return Err(format!(
                "no se puede registrar el manejador de Ctrl+C: {}",
                io::Error::last_os_error()
            ));
        }
    }

    #[cfg(any(debug_assertions, not(windows)))]
    {
        println!("CAT Pacer {}", env!("CARGO_PKG_VERSION"));
        println!("Configuracion: {}", config_file.display());
    }

    #[cfg(windows)]
    let mut monitor = ui::Monitor::start(&config, &config_file)?;

    let mut rx_port: Option<Box<dyn SerialPort>> = None;
    let mut tx_port: Option<Box<dyn SerialPort>> = None;
    let mut next_rx_open = Instant::now();
    let mut next_tx_open = Instant::now();
    let mut rx_buffer = [0_u8; MAX_RX_BUFFER];
    let mut rx_len = 0;
    let mut scan_from = 0;
    let mut read_buffer = [0_u8; 256];
    let mut frequency: Option<[u8; 11]> = None;
    let mut frequency_at: Option<Instant> = None;
    let mut last_sent: Option<Instant> = None;
    let mut last_poll: Option<Instant> = None;

    while !STOP_REQUESTED.load(Ordering::Relaxed) {
        let now = Instant::now();

        if rx_port.is_none() && now >= next_rx_open {
            match open_port(&config.rx_port, config.rx_baud) {
                Ok(port) => {
                    #[cfg(any(debug_assertions, not(windows)))]
                    println!("RX conectado: {} @ {}", config.rx_port, config.rx_baud);
                    rx_port = Some(port);
                    last_poll = None;
                    #[cfg(windows)]
                    {
                        monitor.state.last_event = "K3 conectado.".into();
                    }
                }
                Err(error) => {
                    #[cfg(any(debug_assertions, not(windows)))]
                    eprintln!("RX no disponible: {error}; nuevo intento en 2 s");
                    next_rx_open = now + RECONNECT_RETRY;
                    #[cfg(windows)]
                    {
                        monitor.state.last_event = format!("K3: {error} Reintento en 2 s.").into();
                    }
                }
            }
        }

        if config.repeater && tx_port.is_none() && now >= next_tx_open {
            match open_port(&config.tx_port, config.tx_baud) {
                Ok(port) => {
                    #[cfg(any(debug_assertions, not(windows)))]
                    println!("TX conectado: {} @ {}", config.tx_port, config.tx_baud);
                    tx_port = Some(port);
                    #[cfg(windows)]
                    {
                        monitor.state.last_event = "Expert conectado.".into();
                    }
                }
                Err(error) => {
                    #[cfg(any(debug_assertions, not(windows)))]
                    eprintln!("TX no disponible: {error}; nuevo intento en 2 s");
                    next_tx_open = now + RECONNECT_RETRY;
                    #[cfg(windows)]
                    {
                        monitor.state.last_event =
                            format!("Expert: {error} Reintento en 2 s.").into();
                    }
                }
            }
        }

        if let Some(port) = rx_port.as_mut() {
            match port.read(&mut read_buffer) {
                Ok(count) if count > 0 => {
                    if rx_len + count > MAX_RX_BUFFER {
                        #[cfg(any(debug_assertions, not(windows)))]
                        eprintln!("RX descartado: trama CAT demasiado larga");
                        rx_len = 0;
                        scan_from = 0;
                        #[cfg(windows)]
                        {
                            monitor.state.last_event =
                                "Trama CAT demasiado larga: descartada.".into();
                        }
                    } else {
                        rx_buffer[rx_len..rx_len + count].copy_from_slice(&read_buffer[..count]);
                        rx_len += count;
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::TimedOut => {}
                Err(error) => {
                    #[cfg(any(debug_assertions, not(windows)))]
                    eprintln!("RX desconectado: {error}");
                    rx_port = None;
                    rx_len = 0;
                    scan_from = 0;
                    next_rx_open = now + RECONNECT_RETRY;
                    #[cfg(windows)]
                    {
                        monitor.state.last_event = format!("K3 desconectado: {error}").into();
                    }
                }
            }
        } else {
            thread::sleep(SERIAL_TIMEOUT);
        }

        if STOP_REQUESTED.load(Ordering::Relaxed) {
            break;
        }

        let now = Instant::now();
        let mut consumed = 0;
        // Cada byte se examina una vez; las tramas se prestan sin asignar memoria.
        for end in scan_from..rx_len {
            if rx_buffer[end] != b';' {
                continue;
            }
            if let Some(received) = extract_frequency(&rx_buffer[consumed..=end]) {
                #[cfg(any(debug_assertions, not(windows)))]
                if frequency != Some(received) {
                    println!("RX frecuencia: {}", String::from_utf8_lossy(&received));
                }
                frequency = Some(received);
                frequency_at = Some(now);
                #[cfg(windows)]
                {
                    monitor.state.received_after_poll = last_poll.is_some();
                    monitor.state.rx_count = monitor.state.rx_count.saturating_add(1);
                }
                last_poll = None;
            }
            consumed = end + 1;
        }
        if consumed > 0 {
            // Solo se conserva la trama incompleta al final de la lectura.
            rx_buffer.copy_within(consumed..rx_len, 0);
            rx_len -= consumed;
        }
        scan_from = rx_len;

        let fresh = frequency_at.is_some_and(|received| now.duration_since(received) <= expiry);
        let send_due = last_sent.is_none_or(|sent| now.duration_since(sent) >= send_interval);

        if config.repeater
            && fresh
            && send_due
            && let (Some(port), Some(current)) = (tx_port.as_mut(), frequency)
        {
            let command = frequency_command(current);
            match port.write_all(&command) {
                Ok(()) => {
                    last_sent = Some(now);
                    #[cfg(windows)]
                    {
                        monitor.state.last_tx = Some(current);
                        monitor.state.tx_count = monitor.state.tx_count.saturating_add(1);
                    }
                }
                Err(error) => {
                    #[cfg(any(debug_assertions, not(windows)))]
                    eprintln!("TX desconectado: {error}");
                    tx_port = None;
                    next_tx_open = now + RECONNECT_RETRY;
                    #[cfg(windows)]
                    {
                        monitor.state.last_event = format!("Expert desconectado: {error}").into();
                    }
                }
            }
        }

        let poll_due = last_poll.is_none_or(|poll| now.duration_since(poll) >= POLL_RETRY);
        if config.polling
            && !fresh
            && poll_due
            && let Some(port) = rx_port.as_mut()
        {
            match port.write_all(b"FA;") {
                Ok(()) => {
                    last_poll = Some(now);
                    #[cfg(windows)]
                    {
                        monitor.state.poll_count = monitor.state.poll_count.saturating_add(1);
                    }
                }
                Err(error) => {
                    #[cfg(any(debug_assertions, not(windows)))]
                    eprintln!("RX desconectado al enviar polling: {error}");
                    rx_port = None;
                    rx_len = 0;
                    scan_from = 0;
                    next_rx_open = now + RECONNECT_RETRY;
                    #[cfg(windows)]
                    {
                        monitor.state.last_event =
                            format!("Error al consultar al K3: {error}").into();
                    }
                }
            }
        }

        #[cfg(windows)]
        {
            monitor.state.frequency = frequency;
            monitor.state.fresh = fresh;
            monitor.state.poll_pending = rx_port.is_some() && last_poll.is_some();
            monitor.state.rx_connected = rx_port.is_some();
            monitor.state.tx_connected = tx_port.is_some();
            monitor.update(now);
        }
    }

    #[cfg(any(debug_assertions, not(windows)))]
    println!("CAT Pacer detenido.");
    Ok(())
}

fn main() -> ExitCode {
    #[cfg(all(windows, debug_assertions))]
    // SAFETY: se adjunta a la consola del padre si existe; nunca se crea una nueva.
    unsafe {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }

    // El guard vive hasta que run haya cerrado puertos y terminado el hilo de interfaz.
    #[cfg(windows)]
    let _instance = {
        // Permite detectar tambien una version anterior que aun no tenga mutex.
        if ui::activate_existing() {
            return ExitCode::SUCCESS;
        }
        match instance::Guard::acquire() {
            Ok(Some(guard)) => guard,
            Ok(None) => {
                ui::activate_existing();
                return ExitCode::SUCCESS;
            }
            Err(error) => {
                ui::show_error(&format!(
                    "No se puede comprobar la instancia única: {error}"
                ));
                return ExitCode::FAILURE;
            }
        }
    };

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            #[cfg(any(debug_assertions, not(windows)))]
            eprintln!("Error: {error}");
            #[cfg(windows)]
            ui::show_error(&error);
            ExitCode::FAILURE
        }
    }
}
