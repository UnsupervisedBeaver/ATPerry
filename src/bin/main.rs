#![no_std]
#![no_main]

use embedded_io::Write;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::{Duration, Instant};
use esp_hal::uart::{Config as UartConfig, Uart};
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_hal::Blocking;
use esp_println::{print, println};

/// (id, label, command, timeout in ms)
const COMMANDS: [(&str, &str, &str, u64); 16] = [
    ("1", "Basic check", "AT", 1000),
    ("2", "Model / firmware", "ATI", 2000),
    ("3", "Model Name", "AT+CGMM",2000),
    ("4","EIMI", "AT+CGSN", 2000),
    ("5", "SIM status", "AT+CPIN?", 2000),
    ("6", "Signal quality", "AT+CSQ", 2000),
    ("7", "Network info", "AT+CPSI?", 3000),
    ("8", "Registration", "AT+CEREG?", 2000),
    ("9", "Packet attach", "AT+CGATT?", 3000),
    ("10", "Operator", "AT+COPS?", 5000),
    ("11", "Data connection", "AT+CNACT?", 3000),
    ("12", "GNSS power on", "AT+CGNSPWR=1", 3000),
    ("13", "GNSS position", "AT+CGNSINF", 5000),
    ("14", "Disable echo", "ATE0", 1000),
    ("15", "Use LTE-M", "AT+CMNB=1", 2000),
    ("16", "Use NB-IOT", "AT+CMNB=2", 2000),
];

const BAUDS: [u32; 3] = [115_200, 9_600, 57_600]; // TODO: Check the baud rate and clean at UART declaration

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

fn display_input(input: &str) {
    print!("\r\x1B[2KEnter a number : {}", input);
}

fn menu() {
    print!("\x1B[2J\x1B[1;1H");
    println!("===================== DLR AT Tester =====================");
    for (id, label, cmd, _) in COMMANDS.iter() {

            println!("{}. {} ({})", id, label, cmd);
    }
    display_input("");
}

/// Throw away any bytes already waiting in the RX FIFO.
fn drain(uart: &mut Uart<'_, Blocking>) {
    let mut tmp = [0u8; 32];
    while uart.read_ready() {
        let _ = uart.read(&mut tmp);
    }
}

/// Print bytes in a readable way
fn show(bytes: &[u8]) {
    for &b in bytes {
        match b {
            b'\r' => print!("\\r"),
            b'\n' => println!("\\n"),
            0x20..=0x7E => print!("{}", b as char),
            _ => print!("[{:02X}]", b),
        }
    }
}

/// Use a sliding window to find a given word
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// Check if it ends with OK, or with a line containing ERROR => Message complete
fn is_done(resp: &[u8]) -> bool {
    resp.ends_with(b"OK\r\n") || ( resp.ends_with(b"\r\n") && contains(resp, b"ERROR") )
}

/// Send one AT command and print what comes back.
/// Returns true if the modem answered OK.
fn send_at(uart: &mut Uart<'_, Blocking>, cmd: &str, timeout_ms: u64) -> bool {
    drain(uart);

    println!("[TX] {}", cmd);

    // Sends the cmd, then \r then flushes (wait until) => If one fails =>  return false
    if uart.write_all(cmd.as_bytes()).is_err() || uart.write_all(b"\r").is_err()  || uart.flush().is_err()
    {
        println!("[TX ERROR] failed to send");
        return false;
    }

    let mut resp: heapless::Vec<u8, 512> = heapless::Vec::new();
    let mut total: usize = 0;
    let start = Instant::now();
    let mut finished = false;

    while start.elapsed() < Duration::from_millis(timeout_ms) {
        if uart.read_ready() {
            let mut buf = [0u8; 32];
            if let Ok(n) = uart.read(&mut buf) {
                for &b in &buf[..n] {
                    total += 1;
                    let _ = resp.push(b);
                }
                if is_done(&resp) {
                    finished = true;
                    break;
                }
            }
        }
    }

    if total == 0 {
        println!("[RX] no bytes received (timeout after {} ms)", timeout_ms);
        return false;
    }

    print!("[RX] ");
    show(&resp);
    println!();

    if total > resp.len() {
        println!("[OVERFLOW] {} bytes did not fit in the buffer", total - resp.len());
    }
    if !finished {
        println!("[TIMEOUT] no OK/ERROR seen, timed out with {} bytes", total);
    }

    finished && contains(&resp, b"OK")
}



#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let mut usb = UsbSerialJtag::new(peripherals.USB_DEVICE);

    let baud_index: usize = 0;
    let mut uart = Uart::new(
        peripherals.UART1,
        UartConfig::default().with_baudrate(BAUDS[baud_index]),
    )
        .unwrap()
        .with_tx(peripherals.GPIO17) // ESP32 TX -> modem RX
        .with_rx(peripherals.GPIO16); // ESP32 RX <- modem TX

    let mut must_clear = false;
    let mut buffer: heapless::String<64> = heapless::String::new();

    menu();

    loop {
        match usb.read_byte() {
            // Enter
            Ok(b'\r') | Ok(b'\n') => {

                if must_clear {
                    must_clear = false;
                    buffer.clear();
                    menu();
                    continue;
                }

                let line = buffer.as_str().trim();
                if line.is_empty() {
                    menu();
                    continue;
                }

                println!();
                match COMMANDS.iter().find(|(id, _, _, _)| *id == line) {
                    Some((_, _label, cmd, timeout)) =>  {
                        send_at(&mut uart, cmd, *timeout);
                    },
                    None => println!("Unknown command"),
                }

                buffer.clear();
                println!("ENTER to continue");
                must_clear = true;
            }

            // Backspace
            Ok(0x08) | Ok(0x7F) => {
                if !must_clear && buffer.pop().is_some() {
                    display_input(buffer.as_str());
                }
            }

            // Printable characters
            Ok(c) if c.is_ascii_graphic() => {
                if !must_clear && buffer.push(c as char).is_ok() {
                    display_input(buffer.as_str());
                }
            }

            _ => {}
        }
    }
}