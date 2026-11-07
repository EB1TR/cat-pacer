use serde::Deserialize;
use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

const SERIAL_TIMEOUT: Duration = Duration::from_millis(20);
const POLL_RETRY: Duration = Duration::from_secs(1);
const RECONNECT_RETRY: Duration = Duration::from_secs(2);
const MAX_RX_BUFFER: usize = 4096;

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
        let config: Self = serde_json::from_str(&contents)
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
    Ok(directory.join("config.json"))
}

fn run() -> Result<(), String> {
    let config_file = config_path()?;
    let config = Config::load(&config_file)?;
    let expiry = Duration::try_from_secs_f64(config.ex_data)
        .map_err(|_| "ex_data esta fuera del rango admitido".to_string())?;
    let send_interval = Duration::try_from_secs_f64(config.tx_time)
        .map_err(|_| "tx_time esta fuera del rango admitido".to_string())?;

    println!("CAT Pacer {}", env!("CARGO_PKG_VERSION"));
    println!("Configuracion: {}", config_file.display());

    let mut rx_port: Option<Box<dyn SerialPort>> = None;
    let mut tx_port: Option<Box<dyn SerialPort>> = None;
    let mut next_rx_open = Instant::now();
    let mut next_tx_open = Instant::now();
    let mut rx_buffer = Vec::with_capacity(256);
    let mut read_buffer = [0_u8; 256];
    let mut frequency: Option<[u8; 11]> = None;
    let mut frequency_at: Option<Instant> = None;
    let mut last_sent: Option<Instant> = None;
    let mut last_poll: Option<Instant> = None;

    loop {
        let now = Instant::now();

        if rx_port.is_none() && now >= next_rx_open {
            match open_port(&config.rx_port, config.rx_baud) {
                Ok(port) => {
                    println!("RX conectado: {} @ {}", config.rx_port, config.rx_baud);
                    rx_port = Some(port);
                    last_poll = None;
                }
                Err(error) => {
                    eprintln!("RX no disponible: {error}; nuevo intento en 2 s");
                    next_rx_open = now + RECONNECT_RETRY;
                }
            }
        }

        if config.repeater && tx_port.is_none() && now >= next_tx_open {
            match open_port(&config.tx_port, config.tx_baud) {
                Ok(port) => {
                    println!("TX conectado: {} @ {}", config.tx_port, config.tx_baud);
                    tx_port = Some(port);
                }
                Err(error) => {
                    eprintln!("TX no disponible: {error}; nuevo intento en 2 s");
                    next_tx_open = now + RECONNECT_RETRY;
                }
            }
        }

        if let Some(port) = rx_port.as_mut() {
            match port.read(&mut read_buffer) {
                Ok(count) if count > 0 => {
                    if rx_buffer.len() + count > MAX_RX_BUFFER {
                        eprintln!("RX descartado: trama CAT demasiado larga");
                        rx_buffer.clear();
                    } else {
                        rx_buffer.extend_from_slice(&read_buffer[..count]);
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::TimedOut => {}
                Err(error) => {
                    eprintln!("RX desconectado: {error}");
                    rx_port = None;
                    rx_buffer.clear();
                    next_rx_open = now + RECONNECT_RETRY;
                }
            }
        } else {
            thread::sleep(SERIAL_TIMEOUT);
        }

        while let Some(end) = rx_buffer.iter().position(|byte| *byte == b';') {
            let frame: Vec<u8> = rx_buffer.drain(..=end).collect();
            if let Some(received) = extract_frequency(&frame) {
                if frequency != Some(received) {
                    println!("RX frecuencia: {}", String::from_utf8_lossy(&received));
                }
                frequency = Some(received);
                frequency_at = Some(Instant::now());
                last_poll = None;
            }
        }

        let now = Instant::now();
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
                }
                Err(error) => {
                    eprintln!("TX desconectado: {error}");
                    tx_port = None;
                    next_tx_open = now + RECONNECT_RETRY;
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
                }
                Err(error) => {
                    eprintln!("RX desconectado al enviar polling: {error}");
                    rx_port = None;
                    rx_buffer.clear();
                    next_rx_open = now + RECONNECT_RETRY;
                }
            }
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_frequency, frequency_command};

    #[test]
    fn extracts_fa_frequency() {
        assert_eq!(extract_frequency(b"FA00007123000;"), Some(*b"00007123000"));
    }

    #[test]
    fn extracts_if_frequency_with_trailing_fields() {
        assert_eq!(
            extract_frequency(b"IF00014250000+000000000020000000;"),
            Some(*b"00014250000")
        );
    }

    #[test]
    fn rejects_invalid_frames() {
        assert_eq!(extract_frequency(b"MD2;"), None);
        assert_eq!(extract_frequency(b"FA00007X23000;"), None);
        assert_eq!(extract_frequency(b"FA123;"), None);
    }

    #[test]
    fn builds_amplifier_command() {
        assert_eq!(frequency_command(*b"00007123000"), *b"FA00007123000;");
    }
}
