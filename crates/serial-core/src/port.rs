//! Serial port enumeration and configuration

use crate::error::Result;
use config::{
    ConfigError, ConfigSchema, ConfigValue, ConfigValues, Configure, FieldSchema, FieldType,
    VariantData, VariantSchema,
};

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
    let ports = tokio_serial::available_ports()?;
    Ok(ports.into_iter().map(PortInfo::from).collect())
}

// =============================================================================
// Configure Implementation for SerialConfig
// =============================================================================

// Note: We cannot implement Configure directly on DataBits, Parity, etc. because
// of Rust's orphan rules (they're from tokio_serial, and Configure is from config).
// Instead, we implement Configure on SerialConfig which contains these fields.

// Variant schemas for enum fields - declared as statics for 'static lifetime

static DATA_BITS_VARIANTS: &[VariantSchema] = &[
    VariantSchema { name: "Five", label: "5", description: None, data: VariantData::None },
    VariantSchema { name: "Six", label: "6", description: None, data: VariantData::None },
    VariantSchema { name: "Seven", label: "7", description: None, data: VariantData::None },
    VariantSchema { name: "Eight", label: "8", description: None, data: VariantData::None },
];

static PARITY_VARIANTS: &[VariantSchema] = &[
    VariantSchema { name: "None", label: "None", description: Some("No parity bit"), data: VariantData::None },
    VariantSchema { name: "Odd", label: "Odd", description: Some("Odd parity"), data: VariantData::None },
    VariantSchema { name: "Even", label: "Even", description: Some("Even parity"), data: VariantData::None },
];

static STOP_BITS_VARIANTS: &[VariantSchema] = &[
    VariantSchema { name: "One", label: "1", description: Some("One stop bit"), data: VariantData::None },
    VariantSchema { name: "Two", label: "2", description: Some("Two stop bits"), data: VariantData::None },
];

static FLOW_CONTROL_VARIANTS: &[VariantSchema] = &[
    VariantSchema { name: "None", label: "None", description: Some("No flow control"), data: VariantData::None },
    VariantSchema { name: "Software", label: "XON/XOFF", description: Some("Software flow control"), data: VariantData::None },
    VariantSchema { name: "Hardware", label: "RTS/CTS", description: Some("Hardware flow control"), data: VariantData::None },
];

// Static FieldType instances for each field
static BAUD_RATE_FIELD_TYPE: FieldType = FieldType::UInt { min: Some(1), max: None };
static DATA_BITS_FIELD_TYPE: FieldType = FieldType::Enum { variants: DATA_BITS_VARIANTS };
static PARITY_FIELD_TYPE: FieldType = FieldType::Enum { variants: PARITY_VARIANTS };
static STOP_BITS_FIELD_TYPE: FieldType = FieldType::Enum { variants: STOP_BITS_VARIANTS };
static FLOW_CONTROL_FIELD_TYPE: FieldType = FieldType::Enum { variants: FLOW_CONTROL_VARIANTS };

static SERIAL_CONFIG_FIELDS: &[FieldSchema] = &[
    FieldSchema {
        name: "baud_rate",
        label: "Baud Rate",
        description: Some("Communication speed in bits per second"),
        field_type: &BAUD_RATE_FIELD_TYPE,
    },
    FieldSchema {
        name: "data_bits",
        label: "Data Bits",
        description: Some("Number of data bits per character"),
        field_type: &DATA_BITS_FIELD_TYPE,
    },
    FieldSchema {
        name: "parity",
        label: "Parity",
        description: Some("Parity checking mode"),
        field_type: &PARITY_FIELD_TYPE,
    },
    FieldSchema {
        name: "stop_bits",
        label: "Stop Bits",
        description: Some("Number of stop bits"),
        field_type: &STOP_BITS_FIELD_TYPE,
    },
    FieldSchema {
        name: "flow_control",
        label: "Flow Control",
        description: Some("Flow control mode"),
        field_type: &FLOW_CONTROL_FIELD_TYPE,
    },
];

static SERIAL_CONFIG_SCHEMA: ConfigSchema = ConfigSchema {
    name: "SerialConfig",
    description: Some("Serial port connection settings"),
    fields: SERIAL_CONFIG_FIELDS,
};

static SERIAL_CONFIG_FIELD_TYPE: FieldType = FieldType::Struct {
    schema: &SERIAL_CONFIG_SCHEMA,
};

impl Configure for SerialConfig {
    fn schema() -> &'static ConfigSchema {
        &SERIAL_CONFIG_SCHEMA
    }

    fn field_type() -> &'static FieldType {
        &SERIAL_CONFIG_FIELD_TYPE
    }

    fn to_values(&self) -> ConfigValues {
        let data_bits_idx = match self.data_bits {
            DataBits::Five => 0,
            DataBits::Six => 1,
            DataBits::Seven => 2,
            DataBits::Eight => 3,
        };
        let parity_idx = match self.parity {
            Parity::None => 0,
            Parity::Odd => 1,
            Parity::Even => 2,
        };
        let stop_bits_idx = match self.stop_bits {
            StopBits::One => 0,
            StopBits::Two => 1,
        };
        let flow_control_idx = match self.flow_control {
            FlowControl::None => 0,
            FlowControl::Software => 1,
            FlowControl::Hardware => 2,
        };

        ConfigValues::new(vec![
            ConfigValue::UInt(self.baud_rate as u64),
            ConfigValue::Enum { variant_index: data_bits_idx, data: None },
            ConfigValue::Enum { variant_index: parity_idx, data: None },
            ConfigValue::Enum { variant_index: stop_bits_idx, data: None },
            ConfigValue::Enum { variant_index: flow_control_idx, data: None },
        ])
    }

    fn from_values(values: &ConfigValues) -> std::result::Result<Self, ConfigError> {
        if values.len() != 5 {
            return Err(ConfigError::wrong_value_count(5, values.len()));
        }

        let baud_rate = match &values.values[0] {
            ConfigValue::UInt(v) => *v as u32,
            other => return Err(ConfigError::type_mismatch("baud_rate", "UInt", other.type_name())),
        };

        let data_bits = match &values.values[1] {
            ConfigValue::Enum { variant_index, .. } => match variant_index {
                0 => DataBits::Five,
                1 => DataBits::Six,
                2 => DataBits::Seven,
                3 => DataBits::Eight,
                _ => return Err(ConfigError::invalid_variant("data_bits", *variant_index, 3)),
            },
            other => return Err(ConfigError::type_mismatch("data_bits", "Enum", other.type_name())),
        };

        let parity = match &values.values[2] {
            ConfigValue::Enum { variant_index, .. } => match variant_index {
                0 => Parity::None,
                1 => Parity::Odd,
                2 => Parity::Even,
                _ => return Err(ConfigError::invalid_variant("parity", *variant_index, 2)),
            },
            other => return Err(ConfigError::type_mismatch("parity", "Enum", other.type_name())),
        };

        let stop_bits = match &values.values[3] {
            ConfigValue::Enum { variant_index, .. } => match variant_index {
                0 => StopBits::One,
                1 => StopBits::Two,
                _ => return Err(ConfigError::invalid_variant("stop_bits", *variant_index, 1)),
            },
            other => return Err(ConfigError::type_mismatch("stop_bits", "Enum", other.type_name())),
        };

        let flow_control = match &values.values[4] {
            ConfigValue::Enum { variant_index, .. } => match variant_index {
                0 => FlowControl::None,
                1 => FlowControl::Software,
                2 => FlowControl::Hardware,
                _ => return Err(ConfigError::invalid_variant("flow_control", *variant_index, 2)),
            },
            other => return Err(ConfigError::type_mismatch("flow_control", "Enum", other.type_name())),
        };

        Ok(SerialConfig {
            baud_rate,
            data_bits,
            parity,
            stop_bits,
            flow_control,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_config_roundtrip() {
        let config = SerialConfig {
            baud_rate: 9600,
            data_bits: DataBits::Seven,
            parity: Parity::Even,
            stop_bits: StopBits::Two,
            flow_control: FlowControl::Hardware,
        };

        // Convert to values
        let values = config.to_values();
        assert_eq!(values.len(), 5);

        // Verify values
        assert_eq!(values.values[0], ConfigValue::UInt(9600));
        assert!(matches!(values.values[1], ConfigValue::Enum { variant_index: 2, .. })); // Seven
        assert!(matches!(values.values[2], ConfigValue::Enum { variant_index: 2, .. })); // Even
        assert!(matches!(values.values[3], ConfigValue::Enum { variant_index: 1, .. })); // Two
        assert!(matches!(values.values[4], ConfigValue::Enum { variant_index: 2, .. })); // Hardware

        // Convert back
        let restored = SerialConfig::from_values(&values).expect("from_values should succeed");
        assert_eq!(restored.baud_rate, 9600);
        assert_eq!(restored.data_bits, DataBits::Seven);
        assert_eq!(restored.parity, Parity::Even);
        assert_eq!(restored.stop_bits, StopBits::Two);
        assert_eq!(restored.flow_control, FlowControl::Hardware);
    }

    #[test]
    fn test_serial_config_schema() {
        let schema = SerialConfig::schema();
        assert_eq!(schema.name, "SerialConfig");
        assert_eq!(schema.fields.len(), 5);
        assert_eq!(schema.fields[0].label, "Baud Rate");
        assert_eq!(schema.fields[1].label, "Data Bits");
        assert_eq!(schema.fields[2].label, "Parity");
        assert_eq!(schema.fields[3].label, "Stop Bits");
        assert_eq!(schema.fields[4].label, "Flow Control");
    }

    #[test]
    fn test_serial_config_default() {
        let config = SerialConfig::default();
        assert_eq!(config.baud_rate, 115200);
        assert_eq!(config.data_bits, DataBits::Eight);
        assert_eq!(config.parity, Parity::None);
        assert_eq!(config.stop_bits, StopBits::One);
        assert_eq!(config.flow_control, FlowControl::None);
    }

    #[test]
    fn test_serial_config_apply() {
        let mut config = SerialConfig::default();
        let mut values = config.to_values();

        // Change baud rate
        values.values[0] = ConfigValue::UInt(57600);
        // Change parity to Odd
        values.values[2] = ConfigValue::Enum { variant_index: 1, data: None };

        config.apply(&values).expect("apply should succeed");
        assert_eq!(config.baud_rate, 57600);
        assert_eq!(config.parity, Parity::Odd);
    }
}
