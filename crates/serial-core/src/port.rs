//! Serial port enumeration and configuration

use crate::error::Result;

pub use tokio_serial::{DataBits, FlowControl, Parity, StopBits};

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

impl From<tokio_serial::SerialPortInfo> for PortInfo {
    fn from(info: tokio_serial::SerialPortInfo) -> Self {
        let (vid, pid, serial_number, manufacturer, product) = match info.port_type {
            tokio_serial::SerialPortType::UsbPort(usb) => (
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
#[derive(Debug, Clone, bon::Builder)]
pub struct SerialConfig {
    /// Baud rate
    #[builder(default = 115200)]
    pub baud_rate: u32,
    /// Data bits
    #[builder(default = DataBits::Eight)]
    pub data_bits: DataBits,
    /// Parity
    #[builder(default = Parity::None)]
    pub parity: Parity,
    /// Stop bits
    #[builder(default = StopBits::One)]
    pub stop_bits: StopBits,
    /// Flow control
    #[builder(default = FlowControl::None)]
    pub flow_control: FlowControl,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self::builder().build()
    }
}

/// List all available serial ports
pub fn list_ports() -> Result<Vec<PortInfo>> {
    let ports = tokio_serial::available_ports()?;
    Ok(ports.into_iter().map(PortInfo::from).collect())
}
