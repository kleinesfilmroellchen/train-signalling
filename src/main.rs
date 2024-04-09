#![no_std]
#![no_main]
#![feature(let_chains)]

use arduino_hal::prelude::_unwrap_infallible_UnwrapInfallible;
use arduino_hal::Eeprom;
use commands::get_next_command;
use panic_halt as _;
use signals::HVMainSignalAspect;
use signals::HVSignalGroup;

pub mod commands;
pub mod signals;

// ----------------------------
// Signal constants: adopt these per signal.
// Signal ID, used in commands. Should be the same as the ID used by the control box.
pub const SIGNAL_ID: &str = "F";
// Whether the signal can show a slow aspect.
pub const HAS_SLOW_ASPECT: bool = false;
// Whether the signal has the capability to be deactivated with an indicator light.
pub const HAS_DEACTIVATION_CAPABILITY: bool = true;
// Whether the announcement signal has reduced distance to the main signal.
pub const HAS_REDUCED_SIGNAL_DISTANCE: bool = false;

#[arduino_hal::entry]
fn main() -> ! {
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let serial = arduino_hal::default_serial!(dp, pins, 57600);
    let (mut serial_reader, mut serial_writer) = serial.split();
    let mut eeprom = Eeprom::new(dp.EEPROM);

    pins.d13.into_output().set_low();

    let mut signal_group = HVSignalGroup::new(
        pins.d2.into_output().downgrade(),
        pins.d4.into_output().downgrade(),
        pins.d5.into_output().downgrade(),
        pins.d6.into_output().downgrade(),
        pins.d7.into_output().downgrade(),
        pins.d8.into_output().downgrade(),
    );
    if HAS_DEACTIVATION_CAPABILITY {
        signal_group = signal_group.with_deactivation_capability(
            pins.d9.into_output().downgrade(),
            pins.d10.into_output().downgrade(),
        );
    }
    if HAS_SLOW_ASPECT {
        signal_group = signal_group.with_slow_aspect(pins.d3.into_output().downgrade());
    }

    if HAS_REDUCED_SIGNAL_DISTANCE {
        signal_group = signal_group.with_reduced_distance(None);
    }

    signal_group
        .switch_to_aspect(signals::HVMainSignalAspect::Stop)
        .unwrap_infallible();

    let mut saved_aspect = [0];
    eeprom.read(0, &mut saved_aspect).unwrap();
    if let Some(saved_aspect) = HVMainSignalAspect::from_command_id(&saved_aspect)
        && signal_group.supports_aspect(saved_aspect)
    {
        signal_group.switch_to_aspect(saved_aspect).unwrap_infallible();
    }

    loop {
        let next_aspect = get_next_command(&mut serial_reader, &mut serial_writer);
        if !signal_group.supports_aspect(next_aspect) {
            ufmt::uwriteln!(&mut serial_writer, "{}:E:1", SIGNAL_ID).unwrap_infallible();
            continue;
        }
        eeprom
            .write(0, next_aspect.command_id().as_bytes())
            .unwrap();
        signal_group.switch_to_aspect(next_aspect).unwrap_infallible();

        ufmt::uwriteln!(
            &mut serial_writer,
            "{}:A:{}",
            SIGNAL_ID,
            next_aspect.command_id()
        )
        .unwrap_infallible();
    }
}
