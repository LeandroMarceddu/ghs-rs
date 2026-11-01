//! Serial port configuration and its mapping to `serialport` settings.

use crate::TransportError;

/// Number of data bits in a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DataBits {
    /// Five data bits.
    Five,
    /// Six data bits.
    Six,
    /// Seven data bits.
    Seven,
    /// Eight data bits, the grblHAL default.
    #[default]
    Eight,
}

impl DataBits {
    pub(crate) fn native(self) -> serialport::DataBits {
        match self {
            Self::Five => serialport::DataBits::Five,
            Self::Six => serialport::DataBits::Six,
            Self::Seven => serialport::DataBits::Seven,
            Self::Eight => serialport::DataBits::Eight,
        }
    }
}

/// Parity checking mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Parity {
    /// No parity bit.
    #[default]
    None,
    /// Odd parity.
    Odd,
    /// Even parity.
    Even,
}

impl Parity {
    pub(crate) fn native(self) -> serialport::Parity {
        match self {
            Self::None => serialport::Parity::None,
            Self::Odd => serialport::Parity::Odd,
            Self::Even => serialport::Parity::Even,
        }
    }
}

/// Number of stop bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StopBits {
    /// One stop bit.
    #[default]
    One,
    /// Two stop bits.
    Two,
}

impl StopBits {
    pub(crate) fn native(self) -> serialport::StopBits {
        match self {
            Self::One => serialport::StopBits::One,
            Self::Two => serialport::StopBits::Two,
        }
    }
}

/// Hardware or software flow control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FlowControl {
    /// No flow control. This is the grblHAL default.
    #[default]
    None,
    /// XON/XOFF flow control.
    Software,
    /// RTS/CTS flow control.
    Hardware,
}

impl FlowControl {
    pub(crate) fn native(self) -> serialport::FlowControl {
        match self {
            Self::None => serialport::FlowControl::None,
            Self::Software => serialport::FlowControl::Software,
            Self::Hardware => serialport::FlowControl::Hardware,
        }
    }
}

/// Settings used to open a serial port.
///
/// The default is 115200 baud, 8 data bits, no parity, one stop bit, no flow
/// control, with DTR and RTS asserted. The port name is empty, so call
/// [`Self::new`] or set [`Self::port`] before opening.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SerialConfig {
    /// Operating system port name, for example `/dev/ttyUSB0` or `COM3`.
    pub port: String,
    /// Baud rate in bits per second.
    pub baud: u32,
    /// Number of data bits per character.
    pub data_bits: DataBits,
    /// Parity checking mode.
    pub parity: Parity,
    /// Number of stop bits.
    pub stop_bits: StopBits,
    /// Flow control mode.
    pub flow_control: FlowControl,
    /// Assert the DTR line after opening the port.
    ///
    /// Many boards wire DTR to reset. Setting this to `false` keeps the line
    /// deasserted; the transport never pulses DTR itself.
    pub dtr: bool,
    /// Assert the RTS line after opening the port.
    pub rts: bool,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port: String::new(),
            baud: 115_200,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
            dtr: true,
            rts: true,
        }
    }
}

impl SerialConfig {
    /// Creates a configuration for `port` with the default 115200 8N1 settings.
    #[must_use]
    pub fn new(port: impl Into<String>) -> Self {
        Self {
            port: port.into(),
            ..Self::default()
        }
    }

    /// Checks that the configuration can be handed to the operating system.
    ///
    /// Enum-valued settings are always valid; this rejects an empty port name,
    /// a port name containing a NUL byte, and a zero baud rate.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::InvalidConfig`] describing the first invalid
    /// field.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.port.is_empty() {
            return Err(TransportError::InvalidConfig(
                "port name must not be empty".into(),
            ));
        }
        if self.port.contains('\0') {
            return Err(TransportError::InvalidConfig(
                "port name must not contain NUL".into(),
            ));
        }
        if self.baud == 0 {
            return Err(TransportError::InvalidConfig(
                "baud rate must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_115200_8n1() {
        let config = SerialConfig::default();
        assert_eq!(config.baud, 115_200);
        assert_eq!(config.data_bits, DataBits::Eight);
        assert_eq!(config.parity, Parity::None);
        assert_eq!(config.stop_bits, StopBits::One);
        assert_eq!(config.flow_control, FlowControl::None);
        assert!(config.dtr);
        assert!(config.rts);
        assert!(config.port.is_empty());
    }

    #[test]
    fn new_sets_only_the_port() {
        let config = SerialConfig::new("/dev/ttyUSB0");
        assert_eq!(config.port, "/dev/ttyUSB0");
        assert_eq!(config.baud, 115_200);
    }

    #[test]
    fn validation_rejects_bad_input() {
        assert!(matches!(
            SerialConfig::default().validate(),
            Err(TransportError::InvalidConfig(_))
        ));

        let mut config = SerialConfig::new("/dev/ttyUSB0");
        config.baud = 0;
        assert!(matches!(
            config.validate(),
            Err(TransportError::InvalidConfig(_))
        ));

        let config = SerialConfig::new("bad\0name");
        assert!(matches!(
            config.validate(),
            Err(TransportError::InvalidConfig(_))
        ));
    }

    #[test]
    fn validation_accepts_useful_configs() {
        for baud in [9_600, 57_600, 115_200, 230_400, 250_000, 1_000_000] {
            let config = SerialConfig {
                baud,
                ..SerialConfig::new("/dev/ttyUSB0")
            };
            assert!(config.validate().is_ok(), "baud {baud} should be accepted");
        }
    }

    #[test]
    fn native_mapping_matches_serialport() {
        assert_eq!(DataBits::Five.native(), serialport::DataBits::Five);
        assert_eq!(DataBits::Six.native(), serialport::DataBits::Six);
        assert_eq!(DataBits::Seven.native(), serialport::DataBits::Seven);
        assert_eq!(DataBits::Eight.native(), serialport::DataBits::Eight);

        assert_eq!(Parity::None.native(), serialport::Parity::None);
        assert_eq!(Parity::Odd.native(), serialport::Parity::Odd);
        assert_eq!(Parity::Even.native(), serialport::Parity::Even);

        assert_eq!(StopBits::One.native(), serialport::StopBits::One);
        assert_eq!(StopBits::Two.native(), serialport::StopBits::Two);

        assert_eq!(FlowControl::None.native(), serialport::FlowControl::None);
        assert_eq!(
            FlowControl::Software.native(),
            serialport::FlowControl::Software
        );
        assert_eq!(
            FlowControl::Hardware.native(),
            serialport::FlowControl::Hardware
        );
    }
}
