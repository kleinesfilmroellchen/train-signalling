#![no_std]
#![no_main]
#![feature(let_chains, abi_avr_interrupt, byte_slice_trim_ascii)]

use core::cell::RefCell;
use core::sync::atomic::compiler_fence;
use core::sync::atomic::Ordering;

use arduino_hal::hal::usart::Event;
use arduino_hal::hal::Wdt;
use arduino_hal::prelude::*;
use arduino_hal::Delay;
use arduino_hal::Eeprom;
use arrayvec::ArrayVec;
use avr_device::interrupt;
use avr_device::interrupt::Mutex;
use commands::get_next_command;
use nb::Error;
use signals::HVMainSignalAspect;
use signals::HVSignalGroup;

use crate::commands::CommandError;

pub mod commands;
pub mod signals;

// ----------------------------
// Signal constants: adopt these per signal.
// Signal ID, used in commands. Should be the same as the ID used by the control box.
pub const SIGNAL_ID: &str = "F";
// Whether the signal can show a slow aspect.
pub const HAS_SLOW_ASPECT: bool = true;
// Whether the signal has the capability to be deactivated with an indicator light.
pub const HAS_DEACTIVATION_CAPABILITY: bool = false;
// Whether the announcement signal has reduced distance to the main signal.
pub const HAS_REDUCED_SIGNAL_DISTANCE: bool = false;

panic_serial::impl_panic_handler!(
  // This is the type of the UART port to use for printing the message:
  arduino_hal::usart::Usart<
    arduino_hal::pac::USART0,
    arduino_hal::port::Pin<arduino_hal::port::mode::Input, arduino_hal::hal::port::PD0>,
    arduino_hal::port::Pin<arduino_hal::port::mode::Output, arduino_hal::hal::port::PD1>
  >
);

type Serial = arduino_hal::hal::usart::Usart0<arduino_hal::DefaultClock>;
static SERIAL: Mutex<RefCell<Option<&mut Serial>>> = Mutex::new(RefCell::new(None));
// a small static buffer for receiving data in the interrupt.
// 32 bytes takes fairly long and before this is exhausted
static SERIAL_BUFFER: Mutex<RefCell<ArrayVec<u8, 32>>> =
    Mutex::new(RefCell::new(ArrayVec::new_const()));

#[avr_device::interrupt(atmega328p)]
#[allow(non_snake_case)]
fn USART_RX() {
    // Disable interrupts to safely access the serial port.
    interrupt::free(|cs| {
        // If serial port is occupied, try again later.
        if let Some(serial) = SERIAL.borrow(cs).borrow_mut().as_mut() {
            match serial.read() {
                Ok(byte) => SERIAL_BUFFER.borrow(cs).borrow_mut().push(byte),
                // The buffer is now empty, we can stop reading.
                Err(Error::WouldBlock) => return,
                Err(Error::Other(_)) => unreachable!(),
            }
        }
    });
}

/// Run some code (typically a closure) with access to the serial port.
fn with_serial(function: impl FnOnce(&mut Serial)) {
    interrupt::free(|cs| loop {
        if let Some(serial) = SERIAL.borrow(cs).borrow_mut().as_mut() {
            function(serial);
            serial.flush();
            compiler_fence(Ordering::SeqCst);
            break;
        }
    });
}

macro_rules! serial_writeln {
    ($($t:tt)*) => {
        with_serial(|serial|{
            ufmt::uwriteln!(serial, $($t)*).unwrap_infallible();
        });
    };
}

#[arduino_hal::entry]
fn main() -> ! {
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let serial = arduino_hal::default_serial!(dp, pins, 57600);
    let serial = share_serial_port_with_panic(serial);
    let mut eeprom = Eeprom::new(dp.EEPROM);
    let mut wdt = Wdt::new(dp.WDT, &dp.CPU.mcusr);

    wdt.start(arduino_hal::hal::wdt::Timeout::Ms4000).unwrap();
    serial.listen(Event::RxComplete);
    interrupt::free(|cs| {
        *SERIAL.borrow(cs).borrow_mut() = Some(serial);
    });
    compiler_fence(Ordering::SeqCst);
    unsafe { interrupt::enable() };

    let mut signal_group = HVSignalGroup::new(
        pins.d7.into_output().downgrade(),
        pins.d8.into_output().downgrade(),
        pins.d4.into_output().downgrade(),
        pins.d2.into_output().downgrade(),
        pins.d5.into_output().downgrade(),
        pins.d3.into_output().downgrade(),
    );
    if HAS_DEACTIVATION_CAPABILITY {
        signal_group = signal_group.with_deactivation_capability(
            pins.d9.into_output().downgrade(),
            pins.d10.into_output().downgrade(),
        );
    }
    if HAS_SLOW_ASPECT {
        signal_group = signal_group.with_slow_aspect(pins.d6.into_output().downgrade());
    }

    if HAS_REDUCED_SIGNAL_DISTANCE {
        signal_group = signal_group.with_reduced_distance(None);
    }

    signal_group
        .switch_to_aspect(signals::HVMainSignalAspect::Stop, &mut Delay::new())
        .unwrap_infallible();

    let mut saved_aspect = [0];
    eeprom.read(0, &mut saved_aspect).unwrap();
    if let Some(saved_aspect) = HVMainSignalAspect::from_command_id(&saved_aspect)
        && signal_group.supports_aspect(saved_aspect)
    {
        signal_group
            .switch_to_aspect(saved_aspect, &mut Delay::new())
            .unwrap_infallible();
    }

    let mut serial_buffer: ArrayVec<u8, 512> = ArrayVec::new();

    loop {
        wdt.feed();

        avr_device::asm::sleep();
        interrupt::free(|cs| {
            let mut interrupt_buffer = SERIAL_BUFFER.borrow(cs).borrow_mut();
            for value in interrupt_buffer.iter() {
                serial_buffer.push(*value);
            }
            interrupt_buffer.clear();
        });

        let maybe_position_of_newline =
            serial_buffer.iter().enumerate().find(|(_, x)| **x == b'\n');
        if let Some((position_of_newline, _)) = maybe_position_of_newline {
            let (line, _) = serial_buffer.split_at(position_of_newline + 1);

            let result = get_next_command(&line);
            match result {
                Ok(command) => {
                    let next_hv_aspect = command.into();
                    if !signal_group.supports_aspect(next_hv_aspect) {
                        serial_writeln!("{}:E:1", SIGNAL_ID);
                    } else {
                        eeprom
                            .write(0, next_hv_aspect.command_id().as_bytes())
                            .unwrap();
                        signal_group
                            .switch_to_aspect(next_hv_aspect, &mut Delay::new())
                            .unwrap_infallible();
                        serial_writeln!("{}:A:{}", SIGNAL_ID, next_hv_aspect.command_id());
                    }
                }
                Err(CommandError(None)) => {}
                Err(CommandError(Some(why))) => with_serial(|serial| {
                    serial.write_str(why.as_str()).unwrap_infallible();
                }),
            }

            serial_buffer.drain(0..=position_of_newline);
        }
    }
}
