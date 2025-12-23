//! Schema types for the configuration system.
//!
//! These types describe the structure of configurable types at compile time.
//! The schema is static and used by frontends to render appropriate UI controls.

/// Schema for a configurable type (static, compile-time).
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigSchema {
    /// Type name (e.g., "SerialConfig")
    pub name: &'static str,
    /// Optional description for tooltips
    pub description: Option<&'static str>,
    /// Fields in this schema
    pub fields: &'static [FieldSchema],
}

/// Schema for a single field.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldSchema {
    /// Rust field name (e.g., "baud_rate")
    pub name: &'static str,
    /// Pretty name for UI (e.g., "Baud Rate")
    pub label: &'static str,
    /// Optional description for tooltips
    pub description: Option<&'static str>,
    /// Type information for this field
    pub field_type: FieldType,
}

/// Type information for a field.
///
/// This enum describes what kind of value a field holds and any constraints.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    /// String field
    String,

    /// Single character field
    Char,

    /// Boolean field (toggle)
    Bool,

    /// Signed integer with optional min/max constraints
    Int {
        min: Option<i64>,
        max: Option<i64>,
    },

    /// Unsigned integer with optional min/max constraints
    UInt {
        min: Option<u64>,
        max: Option<u64>,
    },

    /// Floating point with optional min/max constraints
    Float {
        min: Option<f64>,
        max: Option<f64>,
    },

    /// List/vector of elements
    List {
        /// Type of each element
        element: &'static FieldType,
        /// Minimum number of elements
        min_len: Option<usize>,
        /// Maximum number of elements
        max_len: Option<usize>,
    },

    /// Optional value wrapper
    ///
    /// The frontend decides how to render optionality:
    /// - Checkbox + inner field
    /// - Empty text field means None
    /// - Dropdown with "None" option
    /// - etc.
    Optional {
        /// Type of the inner value when present
        inner: &'static FieldType,
    },

    /// Enum type with variants
    Enum {
        /// Available variants
        variants: &'static [VariantSchema],
    },

    /// Nested struct (must implement Configure)
    Struct {
        /// Schema of the nested struct
        schema: &'static ConfigSchema,
    },
}

/// Schema for an enum variant.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantSchema {
    /// Rust variant name (e.g., "Newline")
    pub name: &'static str,
    /// Pretty name for UI (e.g., "Newline (\\n)")
    pub label: &'static str,
    /// Optional description for tooltips
    pub description: Option<&'static str>,
    /// What data this variant holds
    pub data: VariantData,
}

/// What data an enum variant holds.
#[derive(Debug, Clone, PartialEq)]
pub enum VariantData {
    /// Unit variant: `Foo`
    None,

    /// Single value (tuple variant): `Foo(u8)` or `Foo(InnerConfig)`
    Single(&'static FieldType),

    /// Named fields (struct variant): `Foo { bar: u8, baz: String }`
    Struct(&'static ConfigSchema),
}
