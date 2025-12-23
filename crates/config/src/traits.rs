//! The Configure trait and related utilities.

use crate::error::ConfigError;
use crate::schema::{ConfigSchema, FieldSchema, FieldType, VariantData};
use crate::value::{ConfigValue, ConfigValues};

/// Trait for types that can be configured via UI.
///
/// Types implementing this trait can be:
/// - Introspected via [`schema()`](Configure::schema) to discover their fields
/// - Converted to [`ConfigValues`] for editing via [`to_values()`](Configure::to_values)
/// - Reconstructed from [`ConfigValues`] via [`from_values()`](Configure::from_values)
///
/// # Requirements
///
/// - Must implement `Default` (for default values and skipped fields)
/// - Must implement `Sized`
///
/// # Derive Macro
///
/// This trait can be automatically implemented using `#[derive(Configure)]`.
/// See the `config-derive` crate for details.
pub trait Configure: Sized + Default {
    /// Get the schema (static, known at compile time).
    ///
    /// The schema describes the structure of this type's configurable fields.
    fn schema() -> &'static ConfigSchema;

    /// Get the field type representation for this type.
    ///
    /// For enums, this returns `FieldType::Enum { variants: ... }`.
    /// For structs, this returns `FieldType::Struct { schema: ... }`.
    ///
    /// This is used by the derive macro when a struct has a field of a
    /// Configure type - it needs the `FieldType` to include in the schema.
    fn field_type() -> &'static FieldType;

    /// Extract current values.
    ///
    /// Returns a [`ConfigValues`] containing the current state of all configurable fields.
    fn to_values(&self) -> ConfigValues;

    /// Create from values (with validation).
    ///
    /// Validates the values against the schema and constructs a new instance.
    /// Returns an error if validation fails.
    fn from_values(values: &ConfigValues) -> Result<Self, ConfigError>;

    /// Apply values in place.
    ///
    /// Validates and applies the values to this instance.
    /// This is a convenience method that calls [`from_values()`](Configure::from_values)
    /// and replaces `self`.
    fn apply(&mut self, values: &ConfigValues) -> Result<(), ConfigError> {
        *self = Self::from_values(values)?;
        Ok(())
    }

    /// Get default values (convenience method).
    ///
    /// Returns the values from a default-constructed instance.
    fn default_values() -> ConfigValues {
        Self::default().to_values()
    }
}

/// Validate values against a schema without applying.
///
/// Returns a list of all validation errors (empty if valid).
/// This allows checking values before attempting to apply them.
pub fn validate(schema: &ConfigSchema, values: &ConfigValues) -> Vec<ConfigError> {
    let mut errors = Vec::new();

    if values.len() != schema.fields.len() {
        errors.push(ConfigError::wrong_value_count(schema.fields.len(), values.len()));
        return errors;
    }

    for (field, value) in schema.fields.iter().zip(values.values.iter()) {
        validate_field(field, value, &mut errors);
    }

    errors
}

/// Validate a single field value against its schema.
fn validate_field(field: &FieldSchema, value: &ConfigValue, errors: &mut Vec<ConfigError>) {
    validate_value(&field.name, field.field_type, value, errors);
}

/// Validate a value against a field type.
fn validate_value(
    field_name: &str,
    field_type: &FieldType,
    value: &ConfigValue,
    errors: &mut Vec<ConfigError>,
) {
    match (field_type, value) {
        (FieldType::String, ConfigValue::String(_)) => {}
        (FieldType::Char, ConfigValue::Char(_)) => {}
        (FieldType::Bool, ConfigValue::Bool(_)) => {}

        (FieldType::Int { min, max }, ConfigValue::Int(v)) => {
            if let Some(min_val) = min {
                if v < min_val {
                    errors.push(ConfigError::int_out_of_range(field_name, *v, *min, *max));
                }
            }
            if let Some(max_val) = max {
                if v > max_val {
                    errors.push(ConfigError::int_out_of_range(field_name, *v, *min, *max));
                }
            }
        }

        (FieldType::UInt { min, max }, ConfigValue::UInt(v)) => {
            if let Some(min_val) = min {
                if v < min_val {
                    errors.push(ConfigError::uint_out_of_range(field_name, *v, *min, *max));
                }
            }
            if let Some(max_val) = max {
                if v > max_val {
                    errors.push(ConfigError::uint_out_of_range(field_name, *v, *min, *max));
                }
            }
        }

        (FieldType::Float { min, max }, ConfigValue::Float(v)) => {
            if let Some(min_val) = min {
                if v < min_val {
                    errors.push(ConfigError::float_out_of_range(field_name, *v, *min, *max));
                }
            }
            if let Some(max_val) = max {
                if v > max_val {
                    errors.push(ConfigError::float_out_of_range(field_name, *v, *min, *max));
                }
            }
        }

        (FieldType::List { element, min_len, max_len }, ConfigValue::List(items)) => {
            // Check length constraints
            if let Some(min) = min_len {
                if items.len() < *min {
                    errors.push(ConfigError::list_length_out_of_range(
                        field_name,
                        items.len(),
                        *min_len,
                        *max_len,
                    ));
                }
            }
            if let Some(max) = max_len {
                if items.len() > *max {
                    errors.push(ConfigError::list_length_out_of_range(
                        field_name,
                        items.len(),
                        *min_len,
                        *max_len,
                    ));
                }
            }

            // Validate each element
            for (i, item) in items.iter().enumerate() {
                let item_field_name = format!("{}[{}]", field_name, i);
                validate_value(&item_field_name, element, item, errors);
            }
        }

        (FieldType::Optional { inner }, ConfigValue::Optional(opt)) => {
            if let Some(inner_value) = opt {
                validate_value(field_name, inner, inner_value, errors);
            }
        }

        (FieldType::Enum { variants }, ConfigValue::Enum { variant_index, data }) => {
            if *variant_index >= variants.len() {
                errors.push(ConfigError::invalid_variant(
                    field_name,
                    *variant_index,
                    variants.len().saturating_sub(1),
                ));
                return;
            }

            let variant = &variants[*variant_index];
            match (&variant.data, data) {
                (VariantData::None, None) => {}
                (VariantData::None, Some(_)) => {
                    errors.push(ConfigError::validation_failed(
                        field_name,
                        format!("variant '{}' should not have data", variant.name),
                    ));
                }
                (VariantData::Single(_) | VariantData::Struct(_), None) => {
                    errors.push(ConfigError::validation_failed(
                        field_name,
                        format!("variant '{}' requires data", variant.name),
                    ));
                }
                (VariantData::Single(inner_type), Some(inner_values)) => {
                    if inner_values.len() != 1 {
                        errors.push(ConfigError::validation_failed(
                            field_name,
                            format!(
                                "variant '{}' expects 1 value, got {}",
                                variant.name,
                                inner_values.len()
                            ),
                        ));
                    } else {
                        let nested_name = format!("{}.{}", field_name, variant.name);
                        validate_value(&nested_name, inner_type, &inner_values.values[0], errors);
                    }
                }
                (VariantData::Struct(schema), Some(inner_values)) => {
                    if inner_values.len() != schema.fields.len() {
                        errors.push(ConfigError::wrong_value_count(
                            schema.fields.len(),
                            inner_values.len(),
                        ));
                    } else {
                        for (inner_field, inner_value) in
                            schema.fields.iter().zip(inner_values.values.iter())
                        {
                            let nested_name =
                                format!("{}.{}.{}", field_name, variant.name, inner_field.name);
                            validate_value(&nested_name, inner_field.field_type, inner_value, errors);
                        }
                    }
                }
            }
        }

        (FieldType::Struct { schema }, ConfigValue::Struct(inner_values)) => {
            if inner_values.len() != schema.fields.len() {
                errors.push(ConfigError::wrong_value_count(
                    schema.fields.len(),
                    inner_values.len(),
                ));
            } else {
                for (inner_field, inner_value) in
                    schema.fields.iter().zip(inner_values.values.iter())
                {
                    let nested_name = format!("{}.{}", field_name, inner_field.name);
                    validate_value(&nested_name, inner_field.field_type, inner_value, errors);
                }
            }
        }

        // Type mismatch
        (expected_type, got_value) => {
            errors.push(ConfigError::type_mismatch(
                field_name,
                field_type_name(expected_type),
                got_value.type_name(),
            ));
        }
    }
}

/// Get a human-readable name for a field type.
fn field_type_name(field_type: &FieldType) -> &'static str {
    match field_type {
        FieldType::String => "String",
        FieldType::Char => "Char",
        FieldType::Bool => "Bool",
        FieldType::Int { .. } => "Int",
        FieldType::UInt { .. } => "UInt",
        FieldType::Float { .. } => "Float",
        FieldType::List { .. } => "List",
        FieldType::Optional { .. } => "Optional",
        FieldType::Enum { .. } => "Enum",
        FieldType::Struct { .. } => "Struct",
    }
}
