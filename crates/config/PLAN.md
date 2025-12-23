# Configuration System Implementation Plan

A unified configuration system that allows any struct/enum to be configurable via frontends with zero boilerplate.

## Goals

1. **Zero frontend boilerplate** - Adding a new config field requires only a `#[config(...)]` attribute
2. **Type-safe** - Frontend gets typed values, validation happens automatically
3. **Hierarchical** - Enums with associated data render as nested config panels
4. **Frontend-agnostic** - Core provides schema + values, frontend decides how to render

## End-State Example

### Definition (in serial-core)

```rust
#[derive(Debug, Clone, Default, Configure)]
pub struct SerialConfig {
    #[config(label = "Baud Rate", desc = "Communication speed")]
    pub baud_rate: u32,
    
    #[config(label = "Data Bits")]
    pub data_bits: DataBits,
    
    #[config(label = "Parity")]
    pub parity: Parity,
}
```

### Usage (in serial-tui)

```rust
// This is ALL the frontend code needed:
ConfigPanel::new("Serial Port").render(f, area, &mut config);

// Reset to defaults:
let defaults = SerialConfig::default().to_values();
```

No match statements. No `get_config_option_strings()`. No `apply_dropdown_selection()`.

---

## Architecture

```
crates/
├── config/                  # Runtime types + trait (this crate)
│   └── src/
│       ├── lib.rs          # Re-exports
│       ├── schema.rs       # FieldSchema, FieldType, VariantSchema
│       ├── value.rs        # ConfigValue (runtime values)
│       ├── error.rs        # ConfigError
│       └── traits.rs       # Configure trait
│
├── config-derive/          # Proc-macro crate (NEW)
│   └── src/
│       └── lib.rs          # #[derive(Configure)] implementation
│
├── serial-core/            # Uses config, implements Configure for types
└── serial-tui/             # Uses config for generic rendering
```

---

## Phase 1: Config Crate + Derive Macro

Complete the config system including the derive macro. This must be done BEFORE TUI migration.

### 1.1 Schema Types (`schema.rs`)

```rust
/// Schema for a configurable type (static, compile-time)
pub struct ConfigSchema {
    pub name: &'static str,                  // Type name
    pub description: Option<&'static str>,   // Tooltip for frontend
    pub fields: &'static [FieldSchema],
}

/// Schema for a single field
pub struct FieldSchema {
    pub name: &'static str,           // Rust field name
    pub label: &'static str,          // Pretty name for UI
    pub description: Option<&'static str>,
    pub field_type: FieldType,
}

/// Type information for a field
pub enum FieldType {
    // Primitives
    String,
    Char,
    Bool,
    Int { min: Option<i64>, max: Option<i64> },
    UInt { min: Option<u64>, max: Option<u64> },
    Float { min: Option<f64>, max: Option<f64> },
    
    // Compound
    List {
        element: &'static FieldType,
        min_len: Option<usize>,
        max_len: Option<usize>,
    },
    
    // Optional wrapper
    Optional {
        inner: &'static FieldType,
    },
    
    // Enum (including enums with data)
    Enum {
        variants: &'static [VariantSchema],
    },
    
    // Nested struct (must implement Configure)
    Struct {
        schema: &'static ConfigSchema,
    },
}

/// Schema for an enum variant
pub struct VariantSchema {
    pub name: &'static str,                    // Rust variant name
    pub label: &'static str,                   // Pretty name
    pub description: Option<&'static str>,
    pub data: VariantData,
}

/// What data an enum variant holds
pub enum VariantData {
    /// Unit variant: `Foo`
    None,
    /// Single value (tuple variant): `Foo(u8)` or `Foo(InnerConfig)`
    Single(&'static FieldType),
    /// Named fields (struct variant): `Foo { bar: u8, baz: String }`
    Struct(&'static ConfigSchema),
}
```

### 1.2 Value Types (`value.rs`)

```rust
/// Runtime value (what the frontend manipulates)
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    String(String),
    Char(char),
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    List(Vec<ConfigValue>),
    Optional(Option<Box<ConfigValue>>),
    Enum {
        variant_index: usize,
        data: Option<ConfigValues>,   // Nested values for enum variant data
    },
    Struct(ConfigValues),
}

/// Collection of field values (matches schema field order)
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfigValues {
    pub values: Vec<ConfigValue>,
}

impl ConfigValues {
    pub fn new(values: Vec<ConfigValue>) -> Self {
        Self { values }
    }
}
```

### 1.3 Error Types (`error.rs`)

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigError {
    #[error("Field '{field}': expected {expected}, got {got}")]
    TypeMismatch {
        field: String,
        expected: &'static str,
        got: &'static str,
    },
    
    #[error("Field '{field}': value {value} out of range [{min}, {max}]")]
    OutOfRange {
        field: String,
        value: String,
        min: String,
        max: String,
    },
    
    #[error("Field '{field}': invalid variant index {index}")]
    InvalidVariant {
        field: String,
        index: usize,
    },
    
    #[error("Field '{field}': {message}")]
    ValidationFailed {
        field: String,
        message: String,
    },
    
    #[error("Field '{field}': list length {len} out of range [{min}, {max}]")]
    ListLengthOutOfRange {
        field: String,
        len: usize,
        min: Option<usize>,
        max: Option<usize>,
    },
}
```

### 1.4 Configure Trait (`traits.rs`)

```rust
/// Trait for types that can be configured via UI
pub trait Configure: Sized + Default {
    /// Get the schema (static, known at compile time)
    fn schema() -> &'static ConfigSchema;
    
    /// Extract current values
    fn to_values(&self) -> ConfigValues;
    
    /// Create from values (with validation)
    fn from_values(values: &ConfigValues) -> Result<Self, ConfigError>;
    
    /// Apply values in place
    fn apply(&mut self, values: &ConfigValues) -> Result<(), ConfigError> {
        *self = Self::from_values(values)?;
        Ok(())
    }
    
    /// Get default values (convenience method)
    fn default_values() -> ConfigValues {
        Self::default().to_values()
    }
}

/// Validate values against a schema without applying
pub fn validate(schema: &ConfigSchema, values: &ConfigValues) -> Vec<ConfigError> {
    // Returns all validation errors (empty if valid)
    todo!()
}
```

### 1.5 Validation

Validation happens in `from_values()`:
- Type checking (ConfigValue variant matches FieldType)
- Range checking for Int/UInt/Float with min/max
- Length checking for List with min_len/max_len
- Recursive validation for nested structs/enums

A standalone `validate()` function allows checking without applying.

### 1.6 Derive Macro (`config-derive` crate)

#### Supported Attributes

```rust
// Struct-level
#[derive(Configure)]
#[config(desc = "...")]  // Optional description
pub struct Foo { ... }

// Field-level
#[config(
    label = "Pretty Name",            // Required
    desc = "Description for tooltip", // Optional
    skip,                             // Skip this field (not configurable)
    min = 0,                          // For numeric types
    max = 100,                        // For numeric types
    min_len = 1,                      // For Vec
    max_len = 10,                     // For Vec
)]
pub field: Type,

// Enum variant-level
#[derive(Configure)]
pub enum Bar {
    #[config(label = "Option A", desc = "...")]
    VariantA,
    
    #[config(label = "Option B")]
    VariantB(InnerConfig),  // Tuple variant with Configure type
    
    #[config(label = "Option C")]
    VariantC(u8),           // Tuple variant with primitive
    
    #[config(label = "Option D")]
    VariantD {              // Struct variant with named fields
        #[config(label = "Field X")]
        x: u32,
        #[config(label = "Field Y")]
        y: String,
    },
}
```

#### Skipped Fields Behavior

Fields marked with `#[config(skip)]` are not configurable and won't appear in the schema.

In `from_values()`, skipped fields use `Default::default()`:

```rust
#[derive(Configure, Default)]
pub struct RegexParser {
    #[config(label = "Pattern")]
    pub pattern: String,
    
    #[config(skip)]
    compiled: Option<Regex>,  // Uses Option::default() = None
}

// Generated from_values():
fn from_values(values: &ConfigValues) -> Result<Self, ConfigError> {
    Ok(Self {
        pattern: /* extracted from values */,
        compiled: Default::default(),  // Skipped field
    })
}
```

**Requirement:** Skipped field types must implement `Default`.

#### Macro Implementation Structure

```rust
// crates/config-derive/src/lib.rs
use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};
use quote::quote;

#[proc_macro_derive(Configure, attributes(config))]
pub fn derive_configure(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    
    match &input.data {
        syn::Data::Struct(data) => derive_struct(&input, data),
        syn::Data::Enum(data) => derive_enum(&input, data),
        syn::Data::Union(_) => panic!("Configure cannot be derived for unions"),
    }
}
```

---

## Special Type Handling

### `Option<T>`

Optional fields are represented as:
- Schema: `FieldType::Optional { inner: &FieldType::... }`
- Value: `ConfigValue::Optional(Option<Box<ConfigValue>>)`

The schema just says "this is optional" - the **frontend decides** how to render it:
- Checkbox/toggle to enable/disable + inner field
- Empty text field means None (for strings)
- Dropdown with "None" as an option
- etc.

Semantically, `None` and empty string are different:
- `ConfigValue::Optional(None)` = no value set
- `ConfigValue::Optional(Some(ConfigValue::String("")))` = explicitly empty string

```rust
#[derive(Configure, Default)]
pub struct SessionConfig {
    #[config(label = "Buffer Size", desc = "Leave empty for default")]
    pub buffer_size: Option<usize>,  // Optional field
}
```

### `PathBuf`

**Decision:** Map `PathBuf` to `String`.

The macro recognizes `std::path::PathBuf` and:
- Schema: `FieldType::String` (with potential future `FieldType::Path` for file picker UIs)
- Value: `ConfigValue::String(path_string)`
- Conversion: `PathBuf::from(s)` / `p.to_string_lossy().to_string()`

```rust
#[derive(Configure, Default)]
pub struct FileSaveConfig {
    #[config(label = "Directory")]
    pub directory: PathBuf,  // Displayed as string, editable as text
}
```

### Enum Variants

The macro handles all enum variant types:

| Variant Type | Example | `VariantData` |
|--------------|---------|---------------|
| Unit | `Raw` | `VariantData::None` |
| Tuple (primitive) | `Byte(u8)` | `VariantData::Single(&FieldType::UInt{..})` |
| Tuple (Configure) | `Custom(Config)` | `VariantData::Single(&FieldType::Struct{..})` |
| Struct | `Foo { x: u8 }` | `VariantData::Struct(&ConfigSchema{..})` |

Example:
```rust
#[derive(Configure, Default)]
pub enum LineDelimiter {
    #[default]
    #[config(label = "Newline (\\n)")]
    Newline,
    
    #[config(label = "CRLF (\\r\\n)")]
    CrLf,
    
    #[config(label = "Custom Byte")]
    Byte(#[config(label = "Byte Value", min = 0, max = 255)] u8),
    
    #[config(label = "Custom Bytes")]
    Bytes(#[config(label = "Byte Sequence")] Vec<u8>),
}
```

---

## Phase 2: TUI Integration

Create a generic `ConfigPanel` widget in `serial-tui`.

### 2.1 Generic ConfigPanel Widget

```rust
pub struct ConfigPanel<'a, T: Configure> {
    title: &'a str,
    config: &'a mut T,
}

impl<T: Configure> Widget for ConfigPanel<'_, T> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let schema = T::schema();
        let values = self.config.to_values();
        
        for (i, field) in schema.fields.iter().enumerate() {
            render_field(field, &values.values[i], /* ... */);
        }
    }
}

fn render_field(schema: &FieldSchema, value: &ConfigValue, /* ... */) {
    match (&schema.field_type, value) {
        (FieldType::Bool, ConfigValue::Bool(v)) => render_toggle(...),
        (FieldType::Int { .. }, ConfigValue::Int(v)) => render_int_input(...),
        (FieldType::Optional { inner }, ConfigValue::Optional(opt)) => {
            render_optional_toggle(...);
            if let Some(inner_value) = opt {
                render_field(inner, inner_value, ...);
            }
        }
        (FieldType::Enum { variants }, ConfigValue::Enum { variant_index, data }) => {
            render_dropdown(variants, *variant_index, ...);
            if let Some(nested) = data {
                // Render nested config indented based on variant's VariantData
            }
        }
        // ...
    }
}
```

### 2.2 Input Handling

Generic handler for any Configure type:

```rust
fn on_field_changed<T: Configure>(
    config: &mut T, 
    field_index: usize, 
    new_value: ConfigValue
) -> Result<(), ConfigError> {
    let mut values = config.to_values();
    values.values[field_index] = new_value;
    config.apply(&values)
}
```

### 2.3 Helper Methods

Utility methods for frontend convenience:

```rust
impl ConfigValue {
    /// Get the label for an enum variant's current selection
    pub fn enum_variant_label<'a>(&self, field_type: &'a FieldType) -> Option<&'a str> {
        match (self, field_type) {
            (
                ConfigValue::Enum { variant_index, .. },
                FieldType::Enum { variants }
            ) => variants.get(*variant_index).map(|v| v.label),
            _ => None,
        }
    }
}
```

---

## Phase 3: Migration

### 3.1 Add Configure to serial-core Types
- Add `#[derive(Configure)]` to all config structs
- Implement `Configure` manually for external types (DataBits, Parity, etc.)

### 3.2 Replace TUI Config Panels
- Replace existing config rendering with generic `ConfigPanel`
- Remove old boilerplate (`get_config_option_strings`, `apply_dropdown_selection`, etc.)

---

## Implementation Checklist

### Phase 1: Config Crate + Macro ✅ COMPLETE
- [x] 1.1 Create `config/src/schema.rs` with schema types
- [x] 1.2 Create `config/src/value.rs` with value types  
- [x] 1.3 Create `config/src/error.rs` with error types
- [x] 1.4 Create `config/src/traits.rs` with Configure trait
- [x] 1.5 Wire up `config/src/lib.rs` with re-exports
- [x] 1.6 Add `thiserror` dependency to `config/Cargo.toml`
- [x] 1.7 Manually implement Configure for a simple test struct
- [x] 1.8 Write unit tests for manual implementation
- [x] 1.9 Create `config-derive` crate with Cargo.toml
- [x] 1.10 Implement struct derive (basic fields: String, bool, integers, floats)
- [x] 1.11 Add attribute parsing (label, desc, skip)
- [x] 1.12 Add numeric constraints (min, max)
- [x] 1.13 Add `Option<T>` support
- [x] 1.14 Add `PathBuf` support (as string)
- [x] 1.15 Add `Vec<T>` / List support
- [x] 1.16 Implement unit enum derive
- [x] 1.17 Implement tuple enum variants (primitives)
- [x] 1.18 Implement tuple enum variants (Configure types)
- [x] 1.19 Implement struct enum variants
- [x] 1.20 Integration tests with both crates

### Phase 2: TUI Integration
- [ ] 2.1 Create generic ConfigPanel widget
- [ ] 2.2 Implement field renderers (bool, int, string, char)
- [ ] 2.3 Implement Optional field renderer
- [ ] 2.4 Implement enum dropdown renderer
- [ ] 2.5 Implement nested config rendering (enum variants with data)
- [ ] 2.6 Implement input handling (navigation, edit, apply)
- [ ] 2.7 Add helper methods (enum_variant_label, etc.)
- [ ] 2.8 Replace one existing config panel as proof-of-concept

### Phase 3: Full Migration
- [ ] 3.1 Add Configure derives to all serial-core configs
- [ ] 3.2 Implement Configure for external types (DataBits, Parity, StopBits, FlowControl)
- [ ] 3.3 Replace all TUI config panels
- [ ] 3.4 Remove old boilerplate code
- [ ] 3.5 Update documentation

---

## Future Improvements (Post-Migration)

These are explicitly deferred until the core system is working:

1. **Label auto-inference** - Generate "Baud Rate" from `baud_rate`
2. **Custom validators** - `#[config(validate = "my_fn")]`
3. **Field grouping** - `#[config(group = "Advanced")]`
4. **Conditional visibility** - Show/hide fields based on other values
5. **Read-only fields** - Display-only fields for debugging
6. **Path picker hint** - `FieldType::Path` for file/directory picker UIs
7. **Duration support** - Proper `Duration` type with configurable display units (ms, s, min). Requires understanding real usage patterns first. For now, use `u64` with appropriate field names (e.g., `delay_ms`).

---

## Design Decisions Summary

| Issue | Decision | Rationale |
|-------|----------|-----------|
| `Option<T>` | `FieldType::Optional` + `ConfigValue::Optional` | Explicit representation; frontend decides rendering |
| `Duration` | Deferred - use `u64` for now | Frontend needs control over display units |
| `PathBuf` | Map to `String` | Keeps it simple; file picker can be added later |
| Skipped fields | Use `Default::default()` | Requires `Default` bound but keeps macro simple |
| Struct enum variants | `VariantData::Struct` with inline schema | Full support for `Foo { x: u8, y: String }` |
| Tuple enum variants | `VariantData::Single` with field type | Works for primitives and Configure types |
| Trait bound | `Configure: Sized + Default` | Enables `default_values()` and skipped field handling |
| Validation | In `from_values()` + standalone `validate()` | Allows checking before applying |

---

## Notes

- Types implementing `Configure` must also implement `Default`
- The derive macro is the most complex part - expect iteration
- External type support requires manual `Configure` implementations in serial-core
- Keep the runtime types simple; complexity should be in the macro
- Skipped field types must implement `Default`
