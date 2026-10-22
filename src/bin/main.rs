#![no_std]
#![no_main]

use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::usb_serial_jtag::UsbSerialJtag;
use esp_println::{print, println};

const COMMANDS: [(&str, &str, &str); 10] = [
    ("1", "Basic check", "AT"),
    ("2", "Model / firmware", "ATI"),
    ("3", "SIM status", "AT+CPIN?"),
    ("4", "Signal quality", "AT+CSQ"),
    ("5", "Network info", "AT+CPSI?"),
    ("6", "Registration", "AT+CEREG?"),
    ("7", "Packet attach", "AT+CGATT?"),
    ("8", "Operator", "AT+COPS?"),
    ("9", "Data connection", "AT+CNACT?"),
    ("10", "GNSS position", "AT+CGNSINF"),
];

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
    println!("=====================DLR AT Tester=====================");
    for (id, label, cmd) in COMMANDS.iter() {
        println!("{}. {} ({})", id, label, cmd);
    }
    display_input("");
}

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let mut usb = UsbSerialJtag::new(peripherals.USB_DEVICE);

    menu();

    let mut buffer: heapless::String<64> = heapless::String::new();

    loop {
        match usb.read_byte() {
            // Enter
            Ok(b'\r') | Ok(b'\n') => {
                let line = buffer.as_str().trim();
                if line.is_empty() {
                    menu();
                    continue;
                }

                println!();
                match COMMANDS.iter().find(|(id, _, _)| *id == line) {
                    Some((_, label, cmd)) => {
                        println!("-> {} ({})", label, cmd);
                        // TODO: send `cmd` to the modem UART here


                    }
                    None => println!("Unknown command"),
                }

                buffer.clear();
                println!("ENTER to continue");
            }

            Ok(0x08) | Ok(0x7F) => {
                if buffer.pop().is_some() {
                    display_input(buffer.as_str());
                }
            }

            Ok(c) if c.is_ascii_graphic() => {
                if buffer.push(c as char).is_ok() {
                    display_input(buffer.as_str());
                }
            }

            _ => {}
        }
    }
}