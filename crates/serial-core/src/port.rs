//! Serial port enumeration and configuration

use crate::error::Result;

pub use serialport::{DataBits, FlowControl, Parity, StopBits};

/// Information about an available serial port
#[derive(Debug, Clone)]
pub struct PortInfo {
    /// Port name (e.g., "/dev/ttyUSB0" or "COM3")
    pub name: String,
    /// USB vendor ID, if available
    pub vid: Option<u16>,
    /// USB product ID, if available
    pub pid: Option<u16>,
    /// Serial number, if available
    pub serial_number: Option<String>,
    /// Manufacturer, if available
    pub manufacturer: Option<String>,
    /// Product name, if available
    pub product: Option<String>,
}

impl From<serialport::SerialPortInfo> for PortInfo {
    fn from(info: serialport::SerialPortInfo) -> Self {
        let (vid, pid, serial_number, manufacturer, product) = match info.port_type {
            serialport::SerialPortType::UsbPort(usb) => (
                Some(usb.vid),
                Some(usb.pid),
                usb.serial_number,
                usb.manufacturer,
                usb.product,
            ),
            _ => (None, None, None, None, None),
        };

        Self {
            name: info.port_name,
            vid,
            pid,
            serial_number,
            manufacturer,
            product,
        }
    }
}

/// Configuration for a serial port connection
#[derive(Debug, Clone)]
pub struct SerialConfig {
    /// Baud rate
    pub baud_rate: u32,
    /// Data bits
    pub data_bits: DataBits,
    /// Parity
    pub parity: Parity,
    /// Stop bits
    pub stop_bits: StopBits,
    /// Flow control
    pub flow_control: FlowControl,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            baud_rate: 115200,
            data_bits: DataBits::Eight,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::None,
        }
    }
}

impl SerialConfig {
    /// Create a new config with the given baud rate, using defaults for other settings
    pub fn with_baud_rate(baud_rate: u32) -> Self {
        Self {
            baud_rate,
            ..Default::default()
        }
    }
}

/// List all available serial ports
pub fn list_ports() -> Result<Vec<PortInfo>> {
    let ports = serialport::available_ports()?;
    Ok(ports.into_iter().map(PortInfo::from).collect())
}
