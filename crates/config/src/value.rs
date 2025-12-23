//! Runtime value types for the configuration system.
//!
//! These types represent actual configuration values at runtime.
//! They are used to transfer values between the Configure trait and frontends.

use crate::schema::FieldType;

/// Runtime value (what the frontend manipulates).
///
/// Each variant corresponds to a [`FieldType`] in the schema.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    /// String value
    String(String),

    /// Single character
    Char(char),

    /// Boolean value
    Bool(bool),

    /// Signed integer
    Int(i64),

    /// Unsigned integer
    UInt(u64),

    /// Floating point
    Float(f64),

    /// List of values (all same type)
    List(Vec<ConfigValue>),

    /// Optional value
    ///
    /// - `Optional(None)` = no value set
    /// - `Optional(Some(...))` = value is present
    ///
    /// Note: `Optional(Some(String("")))` is semantically different from `Optional(None)`.
    /// The former is an explicitly empty string, the latter is no value at all.
    Optional(Option<Box<ConfigValue>>),

    /// Enum variant selection
    Enum {
        /// Index of the selected variant (matches order in VariantSchema)
        variant_index: usize,
        /// Data for this variant (if any)
        ///
        /// - `None` for unit variants
        /// - `Some(ConfigValues)` for tuple/struct variants
        data: Option<ConfigValues>,
    },

    /// Nested struct value
    Struct(ConfigValues),
}

impl ConfigValue {
    /// Get the label for an enum variant's current selection.
    ///
    /// Returns `None` if this is not an enum value or the field type doesn't match.
    pub fn enum_variant_label<'a>(&self, field_type: &'a FieldType) -> Option<&'a str> {
        match (self, field_type) {
            (
                ConfigValue::Enum { variant_index, .. },
                FieldType::Enum { variants },
            ) => variants.get(*variant_index).map(|v| v.label),
            _ => None,
        }
    }

    /// Returns the type name of this value (for error messages).
    pub fn type_name(&self) -> &'static str {
        match self {
            ConfigValue::String(_) => "String",
            ConfigValue::Char(_) => "Char",
            ConfigValue::Bool(_) => "Bool",
            ConfigValue::Int(_) => "Int",
            ConfigValue::UInt(_) => "UInt",
            ConfigValue::Float(_) => "Float",
            ConfigValue::List(_) => "List",
            ConfigValue::Optional(_) => "Optional",
            ConfigValue::Enum { .. } => "Enum",
            ConfigValue::Struct(_) => "Struct",
        }
    }
}

/// Collection of field values (matches schema field order).
///
/// The values in this collection correspond to the fields in a [`ConfigSchema`](crate::schema::ConfigSchema)
/// in the same order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfigValues {
    /// Field values in schema order
    pub values: Vec<ConfigValue>,
}

impl ConfigValues {
    /// Create a new collection with the given values.
    pub fn new(values: Vec<ConfigValue>) -> Self {
        Self { values }
    }

    /// Create an empty collection.
    pub fn empty() -> Self {
        Self { values: Vec::new() }
    }

    /// Get the number of values.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Get a value by index.
    pub fn get(&self, index: usize) -> Option<&ConfigValue> {
        self.values.get(index)
    }

    /// Get a mutable reference to a value by index.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut ConfigValue> {
        self.values.get_mut(index)
    }
}

impl From<Vec<ConfigValue>> for ConfigValues {
    fn from(values: Vec<ConfigValue>) -> Self {
        Self::new(values)
    }
}
