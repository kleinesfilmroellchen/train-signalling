//! Module for parsing serial commands.

use core::convert::Infallible;

use crate::SIGNAL_ID;

use arrayvec::ArrayString;

/// If the error is None, the command is empty, or not intended for this signal, and can be ignored.
/// If the error is a string, it’s an error response to be sent back to the command sender.
pub struct CommandError(pub(crate) Option<ArrayString<128>>);

impl Default for CommandError {
    fn default() -> Self {
        Self(None)
    }
}

impl ufmt::uWrite for CommandError {
    type Error = Infallible;

    fn write_str(&mut self, s: &str) -> Result<(), Self::Error> {
        match self.0 {
            Some(ref mut string) => string.push_str(s),
            None => self.0 = Some(ArrayString::from(s).unwrap()),
        }
        Ok(())
    }
}

impl ufmt::uDisplay for CommandError {
    fn fmt<W>(&self, formatter: &mut ufmt::Formatter<'_, W>) -> Result<(), W::Error>
    where
        W: ufmt::uWrite + ?Sized,
    {
        match self.0 {
            Some(string) => formatter.write_str(string.as_str()),
            None => Ok(()),
        }
    }
}

macro_rules! format_error {
    ($($t:tt)*) => {{
        let mut e = CommandError::default();
        ufmt::uwriteln!(
            e,
            $($t)*
        ).unwrap();
        Err(e)
    }};
}

#[repr(u8)]
pub enum AspectCommand {
    Zero = 0,
    One = 1,
    Two = 2,
    Deactivated = b'A',
    Dark = b'D',
}

/// Parses the next command from the single line input given.
///
/// The result is either
/// - a pair of aspects; the aspect that the command wants this signal to switch to, or
/// - an optional error.
pub fn get_next_command(line: &[u8]) -> Result<AspectCommand, CommandError> {
    let before_comment = line
        .split(|c| *c == b'#')
        .next()
        .unwrap_or_else(|| line)
        .trim_ascii();
    let mut sections = before_comment.split(|c| *c == b':');
    match sections.next() {
        Some(signal_id) => {
            if signal_id != SIGNAL_ID.as_bytes() {
                return Err(CommandError::default());
            }
        }
        None => {
            return format_error!(
                "{}:E:0#Missing signal ID in {:?}",
                SIGNAL_ID,
                before_comment
            );
        }
    }
    match sections.next() {
        None => return format_error!("{}:E:0#Missing command in {:?}", SIGNAL_ID, before_comment),
        Some(command) => {
            return match command {
                b"A" => Ok(AspectCommand::Deactivated),
                b"D" => Ok(AspectCommand::Dark),
                b"0" => Ok(AspectCommand::Zero),
                b"1" => Ok(AspectCommand::One),
                b"2" => Ok(AspectCommand::Two),
                _ => return format_error!("{}:E:0#Unknown command {:?}", SIGNAL_ID, command),
            };
        }
    }
}
