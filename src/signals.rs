use embedded_hal::digital::OutputPin;
use embedded_hal::digital::PinState;

use crate::commands::AspectCommand;

/// An optical main signal aspect in the H/V signalling system.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HVMainSignalAspect {
    // Hp0: Halt
    Stop,
    // Hp1: Fahrt
    Proceed,
    // Hp2: Langsamfahrt mit 40km/h oder mit der im Buchfahrplan oder durch Zs3 angegebenen Geschwindigkeit.
    ProceedSlow,
    // Signal betrieblich abgeschaltet, Kennlicht aktiv.
    Deactivated,
    // Signal dunkel, da übergeordnete Zugbeeinflussung (LZB oder ETCS) statt dem Lichtsignal gültig ist.
    Dark,
}

impl HVMainSignalAspect {
    pub fn command_id(self) -> &'static str {
        match self {
            Self::Stop => "0",
            Self::Proceed => "1",
            Self::ProceedSlow => "2",
            Self::Deactivated => "A",
            Self::Dark => "D",
        }
    }

    pub fn from_command_id(command_id: &[u8]) -> Option<Self> {
        match command_id {
            b"0" => Some(Self::Stop),
            b"1" => Some(Self::Proceed),
            b"2" => Some(Self::ProceedSlow),
            b"A" => Some(Self::Deactivated),
            b"D" => Some(Self::Dark),
            _ => None,
        }
    }
}

impl From<AspectCommand> for HVMainSignalAspect {
    fn from(value: AspectCommand) -> Self {
        match value {
            AspectCommand::Zero => Self::Stop,
            AspectCommand::One => Self::Proceed,
            AspectCommand::Two => Self::ProceedSlow,
            AspectCommand::Deactivated => Self::Deactivated,
            AspectCommand::Dark => Self::Dark,
        }
    }
}

/// An optical announcement signal aspect in the H/V signalling system.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HVAnnouncementSignalAspect {
    // Vr0: Halt erwarten
    ExpectStop,
    // Vr1: Fahrt erwarten
    ExpectProceed,
    // Vr2: Langsamfahrt erwarten
    ExpectProceedSlow,
    // Signal betrieblich abgeschaltet, Kennlicht aktiv.
    Deactivated,
    // Signal dunkel, da übergeordnete Zugbeeinflussung (LZB oder ETCS) statt dem Lichtsignal gültig ist.
    Dark,
}

impl From<HVMainSignalAspect> for HVAnnouncementSignalAspect {
    fn from(value: HVMainSignalAspect) -> Self {
        match value {
            HVMainSignalAspect::Stop => Self::ExpectStop,
            HVMainSignalAspect::Proceed => Self::ExpectProceed,
            HVMainSignalAspect::ProceedSlow => Self::ExpectProceedSlow,
            HVMainSignalAspect::Deactivated => Self::Deactivated,
            HVMainSignalAspect::Dark => Self::Dark,
        }
    }
}

/// An optical main signal in the H/V signalling system.
///
/// # Type parameters
///
/// This type is generic over the kind of output pin used. Its parameters additionally include the output pin’s error type (which some functions also return).
pub struct HVMainSignal<Error, PinType: OutputPin<Error = Error>> {
    // First (main) red lamp.
    red_lamp_1: PinType,
    // Yellow lamp. May not exist if the signal cannot show Hp2 (Langsamfahrt).
    yellow_lamp: Option<PinType>,
    // Green lamp.
    green_lamp: PinType,
    // Notice lamp, used for Deactivated state.
    notice_lamp: Option<PinType>,
}

impl<Error, PinType: OutputPin<Error = Error>> HVMainSignal<Error, PinType> {
    pub fn new(red_lamp: PinType, green_lamp: PinType) -> Self {
        Self {
            red_lamp_1: red_lamp,
            yellow_lamp: None,
            green_lamp,
            notice_lamp: None,
        }
    }

    /// Adds a yellow lamp to this main signal.
    pub fn with_yellow_lamp(mut self, yellow_lamp: PinType) -> Self {
        self.yellow_lamp = Some(yellow_lamp);
        self
    }

    /// Adds a notice lamp to this main signal.
    pub fn with_notice_lamp(mut self, notice_lamp: PinType) -> Self {
        self.notice_lamp = Some(notice_lamp);
        self
    }

    /// Returns whether this signal supports the given aspect, since some aspects require optional lights.
    pub fn supports_aspect(&self, aspect: HVMainSignalAspect) -> bool {
        match aspect {
            // always supported
            HVMainSignalAspect::Stop | HVMainSignalAspect::Dark | HVMainSignalAspect::Proceed => {
                true
            }
            HVMainSignalAspect::ProceedSlow => self.yellow_lamp.is_some(),
            HVMainSignalAspect::Deactivated => self.notice_lamp.is_some(),
        }
    }

    fn switch_optionally(pin: &mut Option<PinType>, state: PinState) -> Result<(), Error> {
        pin.as_mut().map(|pin| pin.set_state(state)).transpose()?;
        Ok(())
    }

    /// Switches this signal to the given aspect.
    ///
    /// # Errors
    /// Errors are returned from the HAL’s digital I/O functions.
    ///
    /// # Panics
    /// This function will panic if an unsupported aspect is set on this signal due to missing lamps. This condition is considered a logic bug; user code must ensure that signals are only ever used with aspects that they are designed for. The function [`Self::supports_aspect`] can be used to test whether a signal supports a certain aspect beforehand.
    pub fn switch_to_aspect(&mut self, aspect: HVMainSignalAspect) -> Result<(), Error> {
        // to ensure safety, first switch on the new aspect’s light,
        // then switch off any previously enabled aspect lights.
        // this may lead to an intermittent unclear aspect, but in that case the driver has to assume stop aspect anyways.
        match aspect {
            HVMainSignalAspect::Stop => {
                self.red_lamp_1.set_high()?;

                self.green_lamp.set_low()?;
                Self::switch_optionally(&mut self.yellow_lamp, PinState::Low)?;
                Self::switch_optionally(&mut self.notice_lamp, PinState::Low)?;
            }
            HVMainSignalAspect::Proceed => {
                self.green_lamp.set_high()?;

                self.red_lamp_1.set_low()?;
                Self::switch_optionally(&mut self.yellow_lamp, PinState::Low)?;
                Self::switch_optionally(&mut self.notice_lamp, PinState::Low)?;
            }
            HVMainSignalAspect::ProceedSlow => {
                // logic bug, since user code should ensure to never try to enable illegal aspects on signals that don’t support them
                if self.yellow_lamp.is_none() {
                    panic!("illegal aspect for this light, no yellow available");
                }

                // switch yellow on before green to avoid transient proceed aspect (whose speed would be too high)
                Self::switch_optionally(&mut self.yellow_lamp, PinState::High)?;
                self.green_lamp.set_high()?;

                self.red_lamp_1.set_low()?;
                Self::switch_optionally(&mut self.notice_lamp, PinState::Low)?;
            }
            HVMainSignalAspect::Deactivated => {
                if self.notice_lamp.is_none() {
                    panic!("illegal aspect for this light, no notice lamp available");
                }

                Self::switch_optionally(&mut self.notice_lamp, PinState::High)?;

                Self::switch_optionally(&mut self.yellow_lamp, PinState::Low)?;
                self.green_lamp.set_low()?;
                self.red_lamp_1.set_low()?;
            }
            HVMainSignalAspect::Dark => {
                Self::switch_optionally(&mut self.notice_lamp, PinState::Low)?;
                Self::switch_optionally(&mut self.yellow_lamp, PinState::Low)?;
                self.green_lamp.set_low()?;
                self.red_lamp_1.set_low()?;
            }
        }
        Ok(())
    }
}

/// An optical announcement signal in the H/V signalling system.
///
/// # Type parameters
///
/// This type is generic over the kind of output pin used. Its parameters additionally include the output pin’s error type (which some functions also return).
pub struct HVAnnouncementSignal<Error, PinType: OutputPin<Error = Error>> {
    // Upper right green lamp.
    green_lamp_upper: PinType,
    // Lower left green lamp.
    green_lamp_lower: PinType,
    // Upper right green lamp.
    yellow_lamp_upper: PinType,
    // Lower left green lamp.
    yellow_lamp_lower: PinType,
    // Notice lamp, used for Deactivated state.
    notice_lamp: Option<PinType>,
    // Whether this signal is a repeater signal or is at reduced breaking distance from the corresponding main signal.
    pub is_repeater_or_reduced_distance: bool,
}

impl<Error, PinType: OutputPin<Error = Error>> HVAnnouncementSignal<Error, PinType> {
    pub fn new(
        green_lamp_upper: PinType,
        green_lamp_lower: PinType,
        yellow_lamp_upper: PinType,
        yellow_lamp_lower: PinType,
    ) -> Self {
        Self {
            green_lamp_upper,
            green_lamp_lower,
            yellow_lamp_upper,
            yellow_lamp_lower,
            notice_lamp: None,
            is_repeater_or_reduced_distance: false,
        }
    }

    /// Adds a notice lamp to this announcement signal.
    pub fn with_notice_lamp(mut self, notice_lamp: PinType) -> Self {
        self.notice_lamp = Some(notice_lamp);
        self
    }

    /// Returns whether this signal supports the given aspect, since some aspects require optional lights.
    pub fn supports_aspect(&self, aspect: HVAnnouncementSignalAspect) -> bool {
        match aspect {
            HVAnnouncementSignalAspect::Deactivated => self.notice_lamp.is_some(),
            // always supported
            _ => true,
        }
    }

    fn switch_optionally(pin: &mut Option<PinType>, state: PinState) -> Result<(), Error> {
        pin.as_mut().map(|pin| pin.set_state(state)).transpose()?;
        Ok(())
    }

    /// Switches this signal to the given aspect.
    ///
    /// # Errors
    /// Errors are returned from the HAL’s digital I/O functions.
    ///
    /// # Panics
    /// This function will panic if an unsupported aspect is set on this signal due to missing lamps. This condition is considered a logic bug; user code must ensure that signals are only ever used with aspects that they are designed for. The function [`Self::supports_aspect`] can be used to test whether a signal supports a certain aspect beforehand.
    pub fn switch_to_aspect(&mut self, aspect: HVAnnouncementSignalAspect) -> Result<(), Error> {
        let normal_notice_lamp_state = self.notice_lamp_for_distance();
        Self::switch_optionally(&mut self.notice_lamp, normal_notice_lamp_state)?;
        match aspect {
            HVAnnouncementSignalAspect::ExpectStop => {
                self.yellow_lamp_upper.set_high()?;
                self.yellow_lamp_lower.set_high()?;
                self.green_lamp_lower.set_low()?;
                self.green_lamp_upper.set_low()?;
            }
            HVAnnouncementSignalAspect::ExpectProceed => {
                self.green_lamp_lower.set_high()?;
                self.green_lamp_upper.set_high()?;
                self.yellow_lamp_upper.set_low()?;
                self.yellow_lamp_lower.set_low()?;
            }
            HVAnnouncementSignalAspect::ExpectProceedSlow => {
                self.yellow_lamp_lower.set_high()?;
                self.green_lamp_upper.set_high()?;
                self.yellow_lamp_upper.set_low()?;
                self.green_lamp_lower.set_low()?;
            }
            HVAnnouncementSignalAspect::Deactivated => {
                if self.notice_lamp.is_none() {
                    panic!("illegal aspect for this light, no notice lamp available");
                }

                Self::switch_optionally(&mut self.notice_lamp, PinState::High)?;
                self.yellow_lamp_upper.set_low()?;
                self.yellow_lamp_lower.set_low()?;
                self.green_lamp_lower.set_low()?;
                self.green_lamp_upper.set_low()?;
            }
            HVAnnouncementSignalAspect::Dark => {
                self.green_lamp_lower.set_low()?;
                self.green_lamp_upper.set_low()?;
                self.yellow_lamp_upper.set_low()?;
                self.yellow_lamp_lower.set_low()?;
                Self::switch_optionally(&mut self.notice_lamp, PinState::Low)?;
            }
        }
        Ok(())
    }

    fn notice_lamp_for_distance(&self) -> PinState {
        match self.is_repeater_or_reduced_distance {
            true => PinState::High,
            false => PinState::Low,
        }
    }
}

/// A grouping of an announcement and main signal in the H/V signaling system.
pub struct HVSignalGroup<Error, PinType: OutputPin<Error = Error>> {
    main_signal: HVMainSignal<Error, PinType>,
    announcement_signal: HVAnnouncementSignal<Error, PinType>,
    // A repeater signal’s notice lamp. Other signal wiring is connected to normal announcement lamps, since it’s always identical.
    repeater_signal_notice_lamp: Option<PinType>,
}

impl<Error, PinType: OutputPin<Error = Error>> HVSignalGroup<Error, PinType> {
    /// Creates a new signal group without a slow aspect.
    pub fn new(
        main_red_lamp: PinType,
        main_green_lamp: PinType,
        announcement_green_lamp_upper: PinType,
        announcement_green_lamp_lower: PinType,
        announcement_yellow_lamp_upper: PinType,
        announcement_yellow_lamp_lower: PinType,
    ) -> Self {
        Self {
            main_signal: HVMainSignal::new(main_red_lamp, main_green_lamp),
            announcement_signal: HVAnnouncementSignal::new(
                announcement_green_lamp_upper,
                announcement_green_lamp_lower,
                announcement_yellow_lamp_upper,
                announcement_yellow_lamp_lower,
            ),
            repeater_signal_notice_lamp: None,
        }
    }

    /// Adds the ability to signal a slow aspect on the main signal.
    pub fn with_slow_aspect(mut self, main_yellow_lamp: PinType) -> Self {
        self.main_signal = self.main_signal.with_yellow_lamp(main_yellow_lamp);
        self
    }

    /// Makes this signal group as having a reduced breaking distance between announcement and main signal. If a notice lamp was already provided, it does not need to be provided a second time.
    pub fn with_reduced_distance(mut self, announcement_notice_lamp: Option<PinType>) -> Self {
        if let Some(announcement_notice_lamp) = announcement_notice_lamp {
            self.announcement_signal = self
                .announcement_signal
                .with_notice_lamp(announcement_notice_lamp);
        }
        self.announcement_signal.is_repeater_or_reduced_distance = true;
        self
    }

    /// Adds deactivation capability to the signals in the signal group.
    pub fn with_deactivation_capability(
        mut self,
        main_notice_lamp: PinType,
        announcement_notice_lamp: PinType,
    ) -> Self {
        self.main_signal = self.main_signal.with_notice_lamp(main_notice_lamp);
        self.announcement_signal = self
            .announcement_signal
            .with_notice_lamp(announcement_notice_lamp);
        self
    }

    /// Adds a notice lamp for a repeater signal, which otherwise shares pins with the announcement signal.
    pub fn with_repeater_signal(mut self, repeater_notice_lamp: PinType) -> Self {
        self.repeater_signal_notice_lamp = Some(repeater_notice_lamp);
        self
    }

    fn switch_optionally(pin: &mut Option<PinType>, state: PinState) -> Result<(), Error> {
        pin.as_mut().map(|pin| pin.set_state(state)).transpose()?;
        Ok(())
    }

    pub fn switch_to_aspect(&mut self, aspect: HVMainSignalAspect) -> Result<(), Error> {
        // switch main signal first to make sure that the announcement signal never announces a main signal aspect that isn’t currently valid.
        self.main_signal.switch_to_aspect(aspect)?;
        self.announcement_signal.switch_to_aspect(aspect.into())?;
        Self::switch_optionally(
            &mut self.repeater_signal_notice_lamp,
            if aspect == HVMainSignalAspect::Dark {
                PinState::Low
            } else {
                PinState::High
            },
        )?;
        Ok(())
    }

    pub fn supports_aspect(&self, aspect: HVMainSignalAspect) -> bool {
        self.main_signal.supports_aspect(aspect)
            && self.announcement_signal.supports_aspect(aspect.into())
    }
}

/// A signal in the Ks signalling system.
///
/// # Type parameters
///
/// This type is generic over the kind of output pin used. Its parameters additionally include the output pin’s error type (which some functions also return).
pub struct KsSignal<Error, PinType: OutputPin<Error = Error>> {
    other_pins: ExtraKsPins<Error, PinType>,
    // Green lamp.
    green_lamp: PinType,
    // Notice lamp, used for Deactivated state.
    notice_lamp: Option<PinType>,
}

/// A signal aspect in the Ks signalling system.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KsSignalAspect {
    // Hp0: Halt
    Stop,
    // Ks1: Fahrt, oder Geschwindigkeitsbeschränkung erwarten
    Proceed,
    // Ks2: Halt erwarten
    ExpectStop,
    // Signal betrieblich abgeschaltet, Kennlicht aktiv.
    Deactivated,
    // Signal dunkel, da übergeordnete Zugbeeinflussung (LZB oder ETCS) statt dem Lichtsignal gültig ist.
    Dark,
}

enum ExtraKsPins<Error, PinType: OutputPin<Error = Error>> {
    MultiBlockSignal {
        red_lamp: PinType,
        yellow_lamp: PinType,
    },
    MainSignal {
        red_lamp: PinType,
    },
    AnnouncementSignal {
        yellow_lamp: PinType,
    },
}

impl<Error, PinType: OutputPin<Error = Error>> ExtraKsPins<Error, PinType> {
    pub fn red_lamp(&mut self) -> Option<&mut PinType> {
        match self {
            ExtraKsPins::MultiBlockSignal { red_lamp, .. } => Some(red_lamp),
            ExtraKsPins::MainSignal { red_lamp } => Some(red_lamp),
            ExtraKsPins::AnnouncementSignal { .. } => None,
        }
    }
    pub fn yellow_lamp(&mut self) -> Option<&mut PinType> {
        match self {
            ExtraKsPins::MultiBlockSignal { yellow_lamp, .. } => Some(yellow_lamp),
            ExtraKsPins::MainSignal { .. } => None,
            ExtraKsPins::AnnouncementSignal { yellow_lamp } => Some(yellow_lamp),
        }
    }
    pub fn has_red_lamp(&self) -> bool {
        match self {
            ExtraKsPins::MultiBlockSignal { .. } => true,
            ExtraKsPins::MainSignal { .. } => true,
            ExtraKsPins::AnnouncementSignal { .. } => false,
        }
    }
    pub fn has_yellow_lamp(&self) -> bool {
        match self {
            ExtraKsPins::MultiBlockSignal { .. } => true,
            ExtraKsPins::MainSignal { .. } => false,
            ExtraKsPins::AnnouncementSignal { .. } => true,
        }
    }
}

impl<Error, PinType: OutputPin<Error = Error>> KsSignal<Error, PinType> {
    pub fn new_main(red_lamp: PinType, green_lamp: PinType) -> Self {
        Self {
            other_pins: ExtraKsPins::MainSignal { red_lamp },
            green_lamp,
            notice_lamp: None,
        }
    }
    pub fn new_announcement(green_lamp: PinType, yellow_lamp: PinType) -> Self {
        Self {
            other_pins: ExtraKsPins::AnnouncementSignal { yellow_lamp },
            green_lamp,
            notice_lamp: None,
        }
    }
    pub fn new_multi_block(red_lamp: PinType, green_lamp: PinType, yellow_lamp: PinType) -> Self {
        Self {
            other_pins: ExtraKsPins::MultiBlockSignal {
                red_lamp,
                yellow_lamp,
            },
            green_lamp,
            notice_lamp: None,
        }
    }

    /// Adds a notice lamp to this main signal.
    pub fn with_notice_lamp(mut self, notice_lamp: PinType) -> Self {
        self.notice_lamp = Some(notice_lamp);
        self
    }

    /// Returns whether this signal supports the given aspect, since some aspects require optional lights.
    pub fn supports_aspect(&self, aspect: KsSignalAspect) -> bool {
        match aspect {
            // always supported
            KsSignalAspect::Dark | KsSignalAspect::Proceed => true,
            KsSignalAspect::Stop => self.other_pins.has_red_lamp(),
            KsSignalAspect::ExpectStop => self.other_pins.has_yellow_lamp(),
            KsSignalAspect::Deactivated => self.notice_lamp.is_some(),
        }
    }

    fn switch_optionally(pin: Option<&mut PinType>, state: PinState) -> Result<(), Error> {
        pin.map(|pin| pin.set_state(state)).transpose()?;
        Ok(())
    }

    /// Switches this signal to the given aspect.
    ///
    /// # Errors
    /// Errors are returned from the HAL’s digital I/O functions.
    ///
    /// # Panics
    /// This function will panic if an unsupported aspect is set on this signal due to missing lamps. This condition is considered a logic bug; user code must ensure that signals are only ever used with aspects that they are designed for. The function [`Self::supports_aspect`] can be used to test whether a signal supports a certain aspect beforehand.
    pub fn switch_to_aspect(&mut self, aspect: KsSignalAspect) -> Result<(), Error> {
        // to ensure safety, first switch on the new aspect’s light,
        // then switch off any previously enabled aspect lights.
        // this may lead to an intermittent unclear aspect, but in that case the driver has to assume stop aspect anyways.
        match aspect {
            KsSignalAspect::Stop => {
                if !self.other_pins.has_red_lamp() {
                    panic!("illegal aspect for this light, no red available");
                }
                Self::switch_optionally(self.other_pins.red_lamp(), PinState::High)?;

                self.green_lamp.set_low()?;
                Self::switch_optionally(self.other_pins.yellow_lamp(), PinState::Low)?;
                Self::switch_optionally(self.notice_lamp.as_mut(), PinState::Low)?;
            }
            KsSignalAspect::Proceed => {
                self.green_lamp.set_high()?;

                Self::switch_optionally(self.other_pins.red_lamp(), PinState::Low)?;
                Self::switch_optionally(self.other_pins.yellow_lamp(), PinState::Low)?;
                Self::switch_optionally(self.notice_lamp.as_mut(), PinState::Low)?;
            }
            KsSignalAspect::ExpectStop => {
                // logic bug, since user code should ensure to never try to enable illegal aspects on signals that don’t support them
                if !self.other_pins.has_yellow_lamp() {
                    panic!("illegal aspect for this light, no yellow available");
                }

                // switch yellow on before green to avoid transient proceed aspect (whose speed would be too high)
                Self::switch_optionally(self.other_pins.yellow_lamp(), PinState::High)?;

                self.green_lamp.set_low()?;
                Self::switch_optionally(self.other_pins.red_lamp(), PinState::Low)?;
                Self::switch_optionally(self.notice_lamp.as_mut(), PinState::Low)?;
            }
            KsSignalAspect::Deactivated => {
                if self.notice_lamp.is_none() {
                    panic!("illegal aspect for this light, no notice lamp available");
                }

                Self::switch_optionally(self.notice_lamp.as_mut(), PinState::High)?;

                Self::switch_optionally(self.other_pins.yellow_lamp(), PinState::Low)?;
                self.green_lamp.set_low()?;
                Self::switch_optionally(self.other_pins.red_lamp(), PinState::Low)?;
            }
            KsSignalAspect::Dark => {
                Self::switch_optionally(self.notice_lamp.as_mut(), PinState::Low)?;
                self.green_lamp.set_low()?;
                Self::switch_optionally(self.other_pins.yellow_lamp(), PinState::Low)?;
                Self::switch_optionally(self.other_pins.red_lamp(), PinState::Low)?;
            }
        }
        Ok(())
    }
}
