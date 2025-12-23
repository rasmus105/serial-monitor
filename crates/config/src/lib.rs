//! Unified configuration system for serial-monitor.
//!
//! This crate provides a type-safe configuration system that allows any struct or enum
//! to be configurable via frontends with zero boilerplate.
//!
//! # Overview
//!
//! The system consists of:
//! - **Schema types** ([`schema`]): Describe the structure of configurable types at compile time
//! - **Value types** ([`value`]): Runtime values that can be edited by frontends
//! - **Configure trait** ([`traits::Configure`]): The main trait for configurable types
//! - **Error types** ([`error`]): Validation and conversion errors
//!
//! # Usage
//!
//! Types can implement [`Configure`] manually or use `#[derive(Configure)]` from the
//! `config-derive` crate.
//!
//! ```ignore
//! use config::{Configure, ConfigValue, ConfigValues};
//!
//! #[derive(Debug, Clone, Default, Configure)]
//! pub struct MyConfig {
//!     #[config(label = "Name", desc = "User's name")]
//!     pub name: String,
//!     
//!     #[config(label = "Age", min = 0, max = 150)]
//!     pub age: u32,
//! }
//!
//! // Frontend can introspect the schema:
//! let schema = MyConfig::schema();
//! println!("Fields: {:?}", schema.fields);
//!
//! // Get current values:
//! let config = MyConfig::default();
//! let values = config.to_values();
//!
//! // Apply modified values:
//! let mut config = MyConfig::default();
//! config.apply(&modified_values)?;
//! ```

pub mod error;
pub mod schema;
pub mod traits;
pub mod value;

// Re-export main types at crate root
pub use error::ConfigError;
pub use schema::{ConfigSchema, FieldSchema, FieldType, VariantData, VariantSchema};
pub use traits::{validate, Configure};
pub use value::{ConfigValue, ConfigValues};

// Re-export the derive macro
pub use config_derive::Configure;

// Re-export this crate as `config` so the derive macro works within this crate's tests
#[doc(hidden)]
pub extern crate self as config;

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // Test types with manual Configure implementations
    // ==========================================================================

    /// A simple test struct to validate the API design.
    #[derive(Debug, Clone, PartialEq, Default)]
    struct SimpleConfig {
        name: String,
        count: u32,
        enabled: bool,
    }

    // Static schema for SimpleConfig
    static SIMPLE_CONFIG_SCHEMA: ConfigSchema = ConfigSchema {
        name: "SimpleConfig",
        description: Some("A simple configuration for testing"),
        fields: &[
            FieldSchema {
                name: "name",
                label: "Name",
                description: Some("The name of the thing"),
                field_type: FieldType::String,
            },
            FieldSchema {
                name: "count",
                label: "Count",
                description: Some("How many things"),
                field_type: FieldType::UInt {
                    min: Some(0),
                    max: Some(100),
                },
            },
            FieldSchema {
                name: "enabled",
                label: "Enabled",
                description: None,
                field_type: FieldType::Bool,
            },
        ],
    };

    impl Configure for SimpleConfig {
        fn schema() -> &'static ConfigSchema {
            &SIMPLE_CONFIG_SCHEMA
        }

        fn to_values(&self) -> ConfigValues {
            ConfigValues::new(vec![
                ConfigValue::String(self.name.clone()),
                ConfigValue::UInt(self.count as u64),
                ConfigValue::Bool(self.enabled),
            ])
        }

        fn from_values(values: &ConfigValues) -> Result<Self, ConfigError> {
            // Validate first
            let errors = validate(Self::schema(), values);
            if !errors.is_empty() {
                return Err(errors.into_iter().next().unwrap());
            }

            let name = match &values.values[0] {
                ConfigValue::String(s) => s.clone(),
                other => {
                    return Err(ConfigError::type_mismatch("name", "String", other.type_name()))
                }
            };

            let count = match &values.values[1] {
                ConfigValue::UInt(v) => *v as u32,
                other => {
                    return Err(ConfigError::type_mismatch("count", "UInt", other.type_name()))
                }
            };

            let enabled = match &values.values[2] {
                ConfigValue::Bool(b) => *b,
                other => {
                    return Err(ConfigError::type_mismatch("enabled", "Bool", other.type_name()))
                }
            };

            Ok(Self {
                name,
                count,
                enabled,
            })
        }
    }

    // ==========================================================================
    // Test enum with unit variants
    // ==========================================================================

    #[derive(Debug, Clone, PartialEq, Default)]
    enum SimpleEnum {
        #[default]
        OptionA,
        OptionB,
        OptionC,
    }

    static SIMPLE_ENUM_SCHEMA: ConfigSchema = ConfigSchema {
        name: "SimpleEnum",
        description: None,
        fields: &[], // Enums have no fields, they ARE the value
    };

    // For an enum, we need a separate schema that describes it as an enum type
    static SIMPLE_ENUM_FIELD_TYPE: FieldType = FieldType::Enum {
        variants: &[
            VariantSchema {
                name: "OptionA",
                label: "Option A",
                description: Some("First option"),
                data: VariantData::None,
            },
            VariantSchema {
                name: "OptionB",
                label: "Option B",
                description: None,
                data: VariantData::None,
            },
            VariantSchema {
                name: "OptionC",
                label: "Option C",
                description: None,
                data: VariantData::None,
            },
        ],
    };

    impl Configure for SimpleEnum {
        fn schema() -> &'static ConfigSchema {
            &SIMPLE_ENUM_SCHEMA
        }

        fn to_values(&self) -> ConfigValues {
            let variant_index = match self {
                SimpleEnum::OptionA => 0,
                SimpleEnum::OptionB => 1,
                SimpleEnum::OptionC => 2,
            };
            ConfigValues::new(vec![ConfigValue::Enum {
                variant_index,
                data: None,
            }])
        }

        fn from_values(values: &ConfigValues) -> Result<Self, ConfigError> {
            if values.len() != 1 {
                return Err(ConfigError::wrong_value_count(1, values.len()));
            }

            match &values.values[0] {
                ConfigValue::Enum {
                    variant_index,
                    data: None,
                } => match variant_index {
                    0 => Ok(SimpleEnum::OptionA),
                    1 => Ok(SimpleEnum::OptionB),
                    2 => Ok(SimpleEnum::OptionC),
                    _ => Err(ConfigError::invalid_variant("SimpleEnum", *variant_index, 2)),
                },
                ConfigValue::Enum { data: Some(_), .. } => Err(ConfigError::validation_failed(
                    "SimpleEnum",
                    "unit enum should not have data",
                )),
                other => Err(ConfigError::type_mismatch(
                    "SimpleEnum",
                    "Enum",
                    other.type_name(),
                )),
            }
        }
    }

    // ==========================================================================
    // Tests
    // ==========================================================================

    #[test]
    fn test_simple_config_schema() {
        let schema = SimpleConfig::schema();
        assert_eq!(schema.name, "SimpleConfig");
        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.fields[0].name, "name");
        assert_eq!(schema.fields[0].label, "Name");
        assert_eq!(schema.fields[1].name, "count");
        assert_eq!(schema.fields[2].name, "enabled");
    }

    #[test]
    fn test_simple_config_to_values() {
        let config = SimpleConfig {
            name: "test".to_string(),
            count: 42,
            enabled: true,
        };

        let values = config.to_values();
        assert_eq!(values.len(), 3);
        assert_eq!(values.values[0], ConfigValue::String("test".to_string()));
        assert_eq!(values.values[1], ConfigValue::UInt(42));
        assert_eq!(values.values[2], ConfigValue::Bool(true));
    }

    #[test]
    fn test_simple_config_from_values() {
        let values = ConfigValues::new(vec![
            ConfigValue::String("hello".to_string()),
            ConfigValue::UInt(10),
            ConfigValue::Bool(false),
        ]);

        let config = SimpleConfig::from_values(&values).unwrap();
        assert_eq!(config.name, "hello");
        assert_eq!(config.count, 10);
        assert_eq!(config.enabled, false);
    }

    #[test]
    fn test_simple_config_roundtrip() {
        let original = SimpleConfig {
            name: "roundtrip".to_string(),
            count: 99,
            enabled: true,
        };

        let values = original.to_values();
        let reconstructed = SimpleConfig::from_values(&values).unwrap();

        assert_eq!(original, reconstructed);
    }

    #[test]
    fn test_simple_config_apply() {
        let mut config = SimpleConfig::default();
        let values = ConfigValues::new(vec![
            ConfigValue::String("applied".to_string()),
            ConfigValue::UInt(50),
            ConfigValue::Bool(true),
        ]);

        config.apply(&values).unwrap();

        assert_eq!(config.name, "applied");
        assert_eq!(config.count, 50);
        assert_eq!(config.enabled, true);
    }

    #[test]
    fn test_simple_config_default_values() {
        let defaults = SimpleConfig::default_values();
        let default_config = SimpleConfig::default();

        assert_eq!(defaults, default_config.to_values());
    }

    #[test]
    fn test_validation_type_mismatch() {
        let values = ConfigValues::new(vec![
            ConfigValue::Int(42), // Wrong type! Should be String
            ConfigValue::UInt(10),
            ConfigValue::Bool(false),
        ]);

        let errors = validate(SimpleConfig::schema(), &values);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ConfigError::TypeMismatch { field, .. } => assert_eq!(field, "name"),
            _ => panic!("Expected TypeMismatch error"),
        }
    }

    #[test]
    fn test_validation_out_of_range() {
        let values = ConfigValues::new(vec![
            ConfigValue::String("test".to_string()),
            ConfigValue::UInt(150), // Out of range! Max is 100
            ConfigValue::Bool(false),
        ]);

        let errors = validate(SimpleConfig::schema(), &values);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ConfigError::OutOfRange { field, .. } => assert_eq!(field, "count"),
            _ => panic!("Expected OutOfRange error"),
        }
    }

    #[test]
    fn test_validation_wrong_value_count() {
        let values = ConfigValues::new(vec![
            ConfigValue::String("test".to_string()),
            // Missing 2 values!
        ]);

        let errors = validate(SimpleConfig::schema(), &values);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            ConfigError::WrongValueCount { expected, got } => {
                assert_eq!(*expected, 3);
                assert_eq!(*got, 1);
            }
            _ => panic!("Expected WrongValueCount error"),
        }
    }

    #[test]
    fn test_simple_enum_to_values() {
        assert_eq!(
            SimpleEnum::OptionA.to_values(),
            ConfigValues::new(vec![ConfigValue::Enum {
                variant_index: 0,
                data: None
            }])
        );
        assert_eq!(
            SimpleEnum::OptionB.to_values(),
            ConfigValues::new(vec![ConfigValue::Enum {
                variant_index: 1,
                data: None
            }])
        );
    }

    #[test]
    fn test_simple_enum_from_values() {
        let values = ConfigValues::new(vec![ConfigValue::Enum {
            variant_index: 2,
            data: None,
        }]);

        let result = SimpleEnum::from_values(&values).unwrap();
        assert_eq!(result, SimpleEnum::OptionC);
    }

    #[test]
    fn test_simple_enum_invalid_variant() {
        let values = ConfigValues::new(vec![ConfigValue::Enum {
            variant_index: 99,
            data: None,
        }]);

        let result = SimpleEnum::from_values(&values);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::InvalidVariant { index, .. } => assert_eq!(index, 99),
            _ => panic!("Expected InvalidVariant error"),
        }
    }

    #[test]
    fn test_enum_variant_label_helper() {
        let value = ConfigValue::Enum {
            variant_index: 1,
            data: None,
        };

        let label = value.enum_variant_label(&SIMPLE_ENUM_FIELD_TYPE);
        assert_eq!(label, Some("Option B"));
    }

    #[test]
    fn test_config_value_type_names() {
        assert_eq!(ConfigValue::String("".to_string()).type_name(), "String");
        assert_eq!(ConfigValue::Char('a').type_name(), "Char");
        assert_eq!(ConfigValue::Bool(true).type_name(), "Bool");
        assert_eq!(ConfigValue::Int(0).type_name(), "Int");
        assert_eq!(ConfigValue::UInt(0).type_name(), "UInt");
        assert_eq!(ConfigValue::Float(0.0).type_name(), "Float");
        assert_eq!(ConfigValue::List(vec![]).type_name(), "List");
        assert_eq!(ConfigValue::Optional(None).type_name(), "Optional");
        assert_eq!(
            ConfigValue::Enum {
                variant_index: 0,
                data: None
            }
            .type_name(),
            "Enum"
        );
        assert_eq!(
            ConfigValue::Struct(ConfigValues::empty()).type_name(),
            "Struct"
        );
    }

    // ==========================================================================
    // Tests for derive macro
    // ==========================================================================

    /// Test struct using derive macro
    #[derive(Debug, Clone, PartialEq, Default, Configure)]
    #[config(desc = "A derived configuration")]
    struct DerivedConfig {
        #[config(label = "Name", desc = "The name")]
        name: String,

        #[config(label = "Count", min = 0, max = 100)]
        count: u32,

        #[config(label = "Enabled")]
        enabled: bool,

        #[config(label = "Ratio")]
        ratio: f64,

        #[config(skip)]
        internal: i32,
    }

    #[test]
    fn test_derived_struct_schema() {
        let schema = DerivedConfig::schema();
        assert_eq!(schema.name, "DerivedConfig");
        assert_eq!(schema.description, Some("A derived configuration"));
        assert_eq!(schema.fields.len(), 4); // 4 fields, internal is skipped

        assert_eq!(schema.fields[0].name, "name");
        assert_eq!(schema.fields[0].label, "Name");
        assert_eq!(schema.fields[0].description, Some("The name"));
        assert_eq!(schema.fields[0].field_type, FieldType::String);

        assert_eq!(schema.fields[1].name, "count");
        assert_eq!(schema.fields[1].label, "Count");
        assert_eq!(
            schema.fields[1].field_type,
            FieldType::UInt {
                min: Some(0),
                max: Some(100)
            }
        );

        assert_eq!(schema.fields[2].name, "enabled");
        assert_eq!(schema.fields[2].field_type, FieldType::Bool);

        assert_eq!(schema.fields[3].name, "ratio");
        assert_eq!(
            schema.fields[3].field_type,
            FieldType::Float { min: None, max: None }
        );
    }

    #[test]
    fn test_derived_struct_to_values() {
        let config = DerivedConfig {
            name: "test".to_string(),
            count: 42,
            enabled: true,
            ratio: 3.14,
            internal: 999, // This should be ignored
        };

        let values = config.to_values();
        assert_eq!(values.len(), 4);
        assert_eq!(values.values[0], ConfigValue::String("test".to_string()));
        assert_eq!(values.values[1], ConfigValue::UInt(42));
        assert_eq!(values.values[2], ConfigValue::Bool(true));
        assert_eq!(values.values[3], ConfigValue::Float(3.14));
    }

    #[test]
    fn test_derived_struct_from_values() {
        let values = ConfigValues::new(vec![
            ConfigValue::String("hello".to_string()),
            ConfigValue::UInt(10),
            ConfigValue::Bool(false),
            ConfigValue::Float(2.71),
        ]);

        let config = DerivedConfig::from_values(&values).unwrap();
        assert_eq!(config.name, "hello");
        assert_eq!(config.count, 10);
        assert_eq!(config.enabled, false);
        assert_eq!(config.ratio, 2.71);
        assert_eq!(config.internal, 0); // Should be default
    }

    #[test]
    fn test_derived_struct_roundtrip() {
        let original = DerivedConfig {
            name: "roundtrip".to_string(),
            count: 50,
            enabled: true,
            ratio: 1.5,
            internal: 0,
        };

        let values = original.to_values();
        let reconstructed = DerivedConfig::from_values(&values).unwrap();

        assert_eq!(original, reconstructed);
    }

    #[test]
    fn test_derived_struct_validation() {
        let values = ConfigValues::new(vec![
            ConfigValue::String("test".to_string()),
            ConfigValue::UInt(150), // Out of range! Max is 100
            ConfigValue::Bool(false),
            ConfigValue::Float(1.0),
        ]);

        let result = DerivedConfig::from_values(&values);
        assert!(result.is_err());
    }

    /// Test enum using derive macro
    #[derive(Debug, Clone, PartialEq, Default, Configure)]
    enum DerivedEnum {
        #[default]
        #[config(label = "Option A", desc = "First option")]
        OptionA,

        #[config(label = "Option B")]
        OptionB,

        #[config(label = "With Value")]
        WithValue(#[config(label = "Value", min = 0, max = 255)] u8),
    }

    #[test]
    fn test_derived_enum_schema() {
        let schema = DerivedEnum::schema();
        assert_eq!(schema.name, "DerivedEnum");
        // Enums have no fields at the top level
        assert_eq!(schema.fields.len(), 0);
    }

    #[test]
    fn test_derived_enum_unit_to_values() {
        assert_eq!(
            DerivedEnum::OptionA.to_values(),
            ConfigValues::new(vec![ConfigValue::Enum {
                variant_index: 0,
                data: None
            }])
        );
        assert_eq!(
            DerivedEnum::OptionB.to_values(),
            ConfigValues::new(vec![ConfigValue::Enum {
                variant_index: 1,
                data: None
            }])
        );
    }

    #[test]
    fn test_derived_enum_tuple_to_values() {
        let value = DerivedEnum::WithValue(42);
        let values = value.to_values();

        assert_eq!(values.len(), 1);
        match &values.values[0] {
            ConfigValue::Enum {
                variant_index: 2,
                data: Some(inner),
            } => {
                assert_eq!(inner.len(), 1);
                assert_eq!(inner.values[0], ConfigValue::UInt(42));
            }
            _ => panic!("Expected Enum with data"),
        }
    }

    #[test]
    fn test_derived_enum_unit_from_values() {
        let values = ConfigValues::new(vec![ConfigValue::Enum {
            variant_index: 1,
            data: None,
        }]);

        let result = DerivedEnum::from_values(&values).unwrap();
        assert_eq!(result, DerivedEnum::OptionB);
    }

    #[test]
    fn test_derived_enum_tuple_from_values() {
        let values = ConfigValues::new(vec![ConfigValue::Enum {
            variant_index: 2,
            data: Some(ConfigValues::new(vec![ConfigValue::UInt(100)])),
        }]);

        let result = DerivedEnum::from_values(&values).unwrap();
        assert_eq!(result, DerivedEnum::WithValue(100));
    }

    #[test]
    fn test_derived_enum_roundtrip() {
        let original = DerivedEnum::WithValue(42);
        let values = original.to_values();
        let reconstructed = DerivedEnum::from_values(&values).unwrap();
        assert_eq!(original, reconstructed);
    }

    /// Test struct with Option fields
    #[derive(Debug, Clone, PartialEq, Default, Configure)]
    struct OptionalConfig {
        #[config(label = "Required")]
        required: String,

        #[config(label = "Optional Value")]
        optional: Option<u32>,
    }

    #[test]
    fn test_optional_field_schema() {
        let schema = OptionalConfig::schema();
        assert_eq!(schema.fields.len(), 2);

        // Check the optional field type
        match &schema.fields[1].field_type {
            FieldType::Optional { inner } => {
                assert_eq!(
                    **inner,
                    FieldType::UInt { min: None, max: None }
                );
            }
            _ => panic!("Expected Optional field type"),
        }
    }

    #[test]
    fn test_optional_field_to_values_some() {
        let config = OptionalConfig {
            required: "test".to_string(),
            optional: Some(42),
        };

        let values = config.to_values();
        assert_eq!(values.values[0], ConfigValue::String("test".to_string()));
        assert_eq!(
            values.values[1],
            ConfigValue::Optional(Some(Box::new(ConfigValue::UInt(42))))
        );
    }

    #[test]
    fn test_optional_field_to_values_none() {
        let config = OptionalConfig {
            required: "test".to_string(),
            optional: None,
        };

        let values = config.to_values();
        assert_eq!(values.values[1], ConfigValue::Optional(None));
    }

    #[test]
    fn test_optional_field_from_values() {
        let values = ConfigValues::new(vec![
            ConfigValue::String("hello".to_string()),
            ConfigValue::Optional(Some(Box::new(ConfigValue::UInt(100)))),
        ]);

        let config = OptionalConfig::from_values(&values).unwrap();
        assert_eq!(config.required, "hello");
        assert_eq!(config.optional, Some(100));
    }

    #[test]
    fn test_optional_field_from_values_none() {
        let values = ConfigValues::new(vec![
            ConfigValue::String("hello".to_string()),
            ConfigValue::Optional(None),
        ]);

        let config = OptionalConfig::from_values(&values).unwrap();
        assert_eq!(config.optional, None);
    }

    /// Test struct with Vec fields
    #[derive(Debug, Clone, PartialEq, Default, Configure)]
    struct ListConfig {
        #[config(label = "Items", min_len = 1, max_len = 5)]
        items: Vec<String>,
    }

    #[test]
    fn test_list_field_schema() {
        let schema = ListConfig::schema();
        assert_eq!(schema.fields.len(), 1);

        match &schema.fields[0].field_type {
            FieldType::List {
                element,
                min_len,
                max_len,
            } => {
                assert_eq!(**element, FieldType::String);
                assert_eq!(*min_len, Some(1));
                assert_eq!(*max_len, Some(5));
            }
            _ => panic!("Expected List field type"),
        }
    }

    #[test]
    fn test_list_field_to_values() {
        let config = ListConfig {
            items: vec!["a".to_string(), "b".to_string()],
        };

        let values = config.to_values();
        assert_eq!(
            values.values[0],
            ConfigValue::List(vec![
                ConfigValue::String("a".to_string()),
                ConfigValue::String("b".to_string()),
            ])
        );
    }

    #[test]
    fn test_list_field_from_values() {
        let values = ConfigValues::new(vec![ConfigValue::List(vec![
            ConfigValue::String("x".to_string()),
            ConfigValue::String("y".to_string()),
            ConfigValue::String("z".to_string()),
        ])]);

        let config = ListConfig::from_values(&values).unwrap();
        assert_eq!(config.items, vec!["x", "y", "z"]);
    }

    /// Test PathBuf support
    #[derive(Debug, Clone, PartialEq, Default, Configure)]
    struct PathConfig {
        #[config(label = "Directory")]
        directory: std::path::PathBuf,
    }

    #[test]
    fn test_pathbuf_to_values() {
        let config = PathConfig {
            directory: std::path::PathBuf::from("/home/user"),
        };

        let values = config.to_values();
        assert_eq!(
            values.values[0],
            ConfigValue::String("/home/user".to_string())
        );
    }

    #[test]
    fn test_pathbuf_from_values() {
        let values = ConfigValues::new(vec![ConfigValue::String("/tmp/test".to_string())]);

        let config = PathConfig::from_values(&values).unwrap();
        assert_eq!(config.directory, std::path::PathBuf::from("/tmp/test"));
    }
}
