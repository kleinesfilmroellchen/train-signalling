//! Module for parsing serial commands.

use super::signals::HVMainSignalAspect;
use super::SIGNAL_ID;

use arrayvec::ArrayVec;
use ufmt::uWrite;

// TODO: Switch to embedded_io once arduino-hal uses that.
use embedded_hal_legacy::serial::{Read, Write};

const BUFFER_SIZES: usize = 128;

/// Blocks until it can receive the next valid change command from the serial interface.
pub fn get_next_command(
    reader: &mut impl Read<u8>,
    writer: &mut (impl Write<u8> + uWrite),
) -> HVMainSignalAspect {
    loop {
        let mut line = ArrayVec::new();
        let mut before_comment = line.as_slice();
        while before_comment.is_empty() {
            line = read_line_or_to_buffer_capacity(reader);
            before_comment = line
                .split(|c| *c == b'#')
                .next()
                .unwrap_or_else(|| line.as_slice());
        }
        let mut sections = before_comment.split(|c| *c == b':');
        match sections.next() {
            Some(signal_id) => {
                if signal_id != SIGNAL_ID.as_bytes() {
                    continue;
                }
            }
            None => {
                let _ = ufmt::uwriteln!(
                    writer,
                    "{}:E:0#Missing signal ID in {:?}",
                    SIGNAL_ID,
                    before_comment
                );
                continue;
            }
        }
        match sections.next() {
            None => {
                let _ = ufmt::uwriteln!(
                    writer,
                    "{}:E:0#Missing command in {:?}",
                    SIGNAL_ID,
                    before_comment
                );
                continue;
            }
            Some(command) => {
                return match command {
                    b"A" => HVMainSignalAspect::Deactivated,
                    b"D" => HVMainSignalAspect::Dark,
                    b"0" => HVMainSignalAspect::Stop,
                    b"1" => HVMainSignalAspect::Proceed,
                    b"2" => HVMainSignalAspect::ProceedSlow,
                    _ => {
                        let _ = ufmt::uwriteln!(
                            writer,
                            "{}:E:0#Unknown command {:?}",
                            SIGNAL_ID,
                            command
                        );
                        continue;
                    }
                };
            }
        }
    }
}

/// Reads from the input reader until either:
/// - the buffer capacity is exceeded
/// - any newline delimiter is reached
/// - a reading error occurs.
fn read_line_or_to_buffer_capacity(reader: &mut impl Read<u8>) -> ArrayVec<u8, BUFFER_SIZES> {
    let mut buffer = ArrayVec::new();
    loop {
        let byte = match nb::block!(reader.read()) {
            Err(_) => continue,
            Ok(byte) => byte,
        };
        if ![b'\n', b'\r'].contains(&byte) {
            match buffer.try_push(byte) {
                Err(_) => break,
                Ok(_) => {}
            }
        } else {
            break;
        }
    }
    buffer
}
