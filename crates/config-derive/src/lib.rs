//! Derive macro for the `Configure` trait.
//!
//! This crate provides `#[derive(Configure)]` for automatic implementation
//! of the `Configure` trait from the `config` crate.
//!
//! # Example
//!
//! ```ignore
//! use config::Configure;
//! use config_derive::Configure;
//!
//! #[derive(Debug, Clone, Default, Configure)]
//! pub struct MyConfig {
//!     #[config(label = "Name", desc = "The user's name")]
//!     pub name: String,
//!
//!     #[config(label = "Count", min = 0, max = 100)]
//!     pub count: u32,
//!
//!     #[config(label = "Enabled")]
//!     pub enabled: bool,
//!
//!     #[config(skip)]
//!     pub internal_state: i32,
//! }
//! ```
//!
//! # Attributes
//!
//! ## Struct/Enum level
//! - `#[config(desc = "...")]` - Description for tooltips
//!
//! ## Field level
//! - `#[config(label = "...")]` - Pretty name for UI (required unless skipped)
//! - `#[config(desc = "...")]` - Description for tooltips
//! - `#[config(skip)]` - Skip this field (not configurable, uses Default)
//! - `#[config(min = N)]` - Minimum value (numeric types)
//! - `#[config(max = N)]` - Maximum value (numeric types)
//! - `#[config(min_len = N)]` - Minimum length (Vec types)
//! - `#[config(max_len = N)]` - Maximum length (Vec types)
//!
//! ## Variant level (enums)
//! - `#[config(label = "...")]` - Pretty name for UI (required)
//! - `#[config(desc = "...")]` - Description for tooltips

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, format_ident};
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Expr, ExprLit, Fields, 
    Ident, Lit, Type, GenericArgument, PathArguments,
};

/// Derive macro for the `Configure` trait.
#[proc_macro_derive(Configure, attributes(config))]
pub fn derive_configure(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    
    let expanded = match &input.data {
        Data::Struct(data) => derive_struct(&input, data),
        Data::Enum(data) => derive_enum(&input, data),
        Data::Union(_) => {
            syn::Error::new_spanned(&input, "Configure cannot be derived for unions")
                .to_compile_error()
        }
    };
    
    TokenStream::from(expanded)
}

/// Parse attributes from #[config(...)]
#[derive(Debug, Default)]
struct ConfigAttrs {
    label: Option<String>,
    desc: Option<String>,
    skip: bool,
    min: Option<String>,  // Store as string, parse later based on type
    max: Option<String>,
    min_len: Option<usize>,
    max_len: Option<usize>,
}

impl ConfigAttrs {
    fn from_attrs(attrs: &[Attribute]) -> Self {
        let mut result = Self::default();
        
        for attr in attrs {
            if !attr.path().is_ident("config") {
                continue;
            }
            
            // Parse using syn 2.x API
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("skip") {
                    result.skip = true;
                    return Ok(());
                }
                
                if meta.path.is_ident("label") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    result.label = Some(value.value());
                    return Ok(());
                }
                
                if meta.path.is_ident("desc") {
                    let value: syn::LitStr = meta.value()?.parse()?;
                    result.desc = Some(value.value());
                    return Ok(());
                }
                
                if meta.path.is_ident("min") {
                    let value: Expr = meta.value()?.parse()?;
                    if let Expr::Lit(ExprLit { lit, .. }) = value {
                        match lit {
                            Lit::Int(i) => result.min = Some(i.base10_digits().to_string()),
                            Lit::Float(f) => result.min = Some(f.base10_digits().to_string()),
                            _ => {}
                        }
                    }
                    return Ok(());
                }
                
                if meta.path.is_ident("max") {
                    let value: Expr = meta.value()?.parse()?;
                    if let Expr::Lit(ExprLit { lit, .. }) = value {
                        match lit {
                            Lit::Int(i) => result.max = Some(i.base10_digits().to_string()),
                            Lit::Float(f) => result.max = Some(f.base10_digits().to_string()),
                            _ => {}
                        }
                    }
                    return Ok(());
                }
                
                if meta.path.is_ident("min_len") {
                    let value: syn::LitInt = meta.value()?.parse()?;
                    result.min_len = value.base10_parse().ok();
                    return Ok(());
                }
                
                if meta.path.is_ident("max_len") {
                    let value: syn::LitInt = meta.value()?.parse()?;
                    result.max_len = value.base10_parse().ok();
                    return Ok(());
                }
                
                // Ignore unknown attributes
                Ok(())
            });
        }
        
        result
    }
}

/// Information about a struct field
struct FieldInfo {
    name: Ident,
    ty: Type,
    attrs: ConfigAttrs,
}

/// Derive Configure for a struct
fn derive_struct(input: &DeriveInput, data: &syn::DataStruct) -> TokenStream2 {
    let name = &input.ident;
    let type_attrs = ConfigAttrs::from_attrs(&input.attrs);
    
    // Only handle named fields (struct { ... })
    let fields: Vec<FieldInfo> = match &data.fields {
        Fields::Named(named) => {
            named.named.iter().map(|f| {
                FieldInfo {
                    name: f.ident.clone().expect("named field"),
                    ty: f.ty.clone(),
                    attrs: ConfigAttrs::from_attrs(&f.attrs),
                }
            }).collect()
        }
        _ => {
            return syn::Error::new_spanned(
                input, 
                "Configure can only be derived for structs with named fields"
            ).to_compile_error();
        }
    };
    
    // Separate skipped and configurable fields
    let config_fields: Vec<_> = fields.iter().filter(|f| !f.attrs.skip).collect();
    
    // Validate that all non-skipped fields have labels
    for field in &config_fields {
        if field.attrs.label.is_none() {
            return syn::Error::new_spanned(
                &field.name,
                format!("field '{}' requires #[config(label = \"...\")]", field.name)
            ).to_compile_error();
        }
    }
    
    // Check if any field is a Configure type (needs runtime initialization)
    let has_configure_fields = config_fields.iter().any(|f| {
        matches!(analyze_type(&f.ty), TypeInfo::Configure | TypeInfo::Unknown)
    });
    
    // Generate static schema
    let schema_name = format_ident!("__{}_SCHEMA", name.to_string().to_uppercase());
    let field_type_name = format_ident!("__{}_FIELD_TYPE", name.to_string().to_uppercase());
    let type_name = name.to_string();
    let type_desc = type_attrs.desc.as_ref().map(|d| quote!(Some(#d))).unwrap_or(quote!(None));
    
    // Generate to_values() body
    let to_values_body = generate_to_values(&config_fields);
    
    // Generate from_values() body
    let from_values_body = generate_from_values(&fields);
    
    if has_configure_fields {
        // Use LazyLock for structs with Configure-type fields
        // We create the FieldSchemas at runtime and leak them to get 'static lifetime
        let field_schemas = generate_field_schemas_lazy(&config_fields);
        
        quote! {
            #[doc(hidden)]
            static #schema_name: ::std::sync::LazyLock<::config::ConfigSchema> = 
                ::std::sync::LazyLock::new(|| {
                    // Create the field schemas vector and leak it to get a static slice
                    let fields_vec: ::std::vec::Vec<::config::FieldSchema> = vec![#field_schemas];
                    let fields_slice: &'static [::config::FieldSchema] = ::std::boxed::Box::leak(fields_vec.into_boxed_slice());
                    
                    ::config::ConfigSchema {
                        name: #type_name,
                        description: #type_desc,
                        fields: fields_slice,
                    }
                });
            
            #[doc(hidden)]
            static #field_type_name: ::std::sync::LazyLock<::config::FieldType> = 
                ::std::sync::LazyLock::new(|| {
                    ::config::FieldType::Struct {
                        schema: ::std::sync::LazyLock::force(&#schema_name),
                    }
                });
            
            impl ::config::Configure for #name {
                fn schema() -> &'static ::config::ConfigSchema {
                    ::std::sync::LazyLock::force(&#schema_name)
                }
                
                fn field_type() -> &'static ::config::FieldType {
                    ::std::sync::LazyLock::force(&#field_type_name)
                }
                
                fn to_values(&self) -> ::config::ConfigValues {
                    ::config::ConfigValues::new(vec![#to_values_body])
                }
                
                fn from_values(values: &::config::ConfigValues) -> Result<Self, ::config::ConfigError> {
                    // Validate first
                    let errors = ::config::validate(Self::schema(), values);
                    if !errors.is_empty() {
                        return Err(errors.into_iter().next().unwrap());
                    }
                    
                    #from_values_body
                }
            }
        }
    } else {
        // Simple case - all fields are primitive types, can use const statics
        let fields_static_name = format_ident!("__{}_FIELDS", name.to_string().to_uppercase());
        let field_schemas = generate_field_schemas(&config_fields);
        
        quote! {
            #[doc(hidden)]
            #[allow(dead_code)]
            static #fields_static_name: &[::config::FieldSchema] = &[#field_schemas];
            
            #[doc(hidden)]
            static #schema_name: ::config::ConfigSchema = ::config::ConfigSchema {
                name: #type_name,
                description: #type_desc,
                fields: #fields_static_name,
            };
            
            #[doc(hidden)]
            static #field_type_name: ::config::FieldType = ::config::FieldType::Struct {
                schema: &#schema_name,
            };
            
            impl ::config::Configure for #name {
                fn schema() -> &'static ::config::ConfigSchema {
                    &#schema_name
                }
                
                fn field_type() -> &'static ::config::FieldType {
                    &#field_type_name
                }
                
                fn to_values(&self) -> ::config::ConfigValues {
                    ::config::ConfigValues::new(vec![#to_values_body])
                }
                
                fn from_values(values: &::config::ConfigValues) -> Result<Self, ::config::ConfigError> {
                    // Validate first
                    let errors = ::config::validate(Self::schema(), values);
                    if !errors.is_empty() {
                        return Err(errors.into_iter().next().unwrap());
                    }
                    
                    #from_values_body
                }
            }
        }
    }
}

/// Generate static FieldSchema items (for const static initialization)
fn generate_field_schemas(fields: &[&FieldInfo]) -> TokenStream2 {
    let schemas: Vec<_> = fields.iter().map(|field| {
        let name = field.name.to_string();
        let label = field.attrs.label.as_ref().unwrap(); // Validated earlier
        let desc = field.attrs.desc.as_ref()
            .map(|d| quote!(Some(#d)))
            .unwrap_or(quote!(None));
        let field_type = generate_field_type(&field.ty, &field.attrs);
        
        quote! {
            ::config::FieldSchema {
                name: #name,
                label: #label,
                description: #desc,
                field_type: #field_type,
            }
        }
    }).collect();
    
    quote! { #(#schemas),* }
}

/// Generate FieldSchema items for lazy initialization (can call field_type())
fn generate_field_schemas_lazy(fields: &[&FieldInfo]) -> TokenStream2 {
    let schemas: Vec<_> = fields.iter().map(|field| {
        let name = field.name.to_string();
        let label = field.attrs.label.as_ref().unwrap(); // Validated earlier
        let desc = field.attrs.desc.as_ref()
            .map(|d| quote!(Some(#d)))
            .unwrap_or(quote!(None));
        let field_type = generate_field_type_lazy(&field.ty, &field.attrs);
        
        quote! {
            ::config::FieldSchema {
                name: #name,
                label: #label,
                description: #desc,
                field_type: #field_type,
            }
        }
    }).collect();
    
    quote! { #(#schemas),* }
}

/// Generate a reference to a FieldType for lazy initialization (can call functions)
fn generate_field_type_lazy(ty: &Type, attrs: &ConfigAttrs) -> TokenStream2 {
    let type_info = analyze_type(ty);
    
    match type_info {
        TypeInfo::String => quote!(&::config::FieldType::String),
        TypeInfo::Char => quote!(&::config::FieldType::Char),
        TypeInfo::Bool => quote!(&::config::FieldType::Bool),
        TypeInfo::SignedInt => {
            let min = attrs.min.as_ref().map(|v| {
                let v: i64 = v.parse().expect("invalid min");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            let max = attrs.max.as_ref().map(|v| {
                let v: i64 = v.parse().expect("invalid max");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            // For lazy init, we need to leak to get 'static lifetime
            quote! {{
                static FT: ::config::FieldType = ::config::FieldType::Int { min: #min, max: #max };
                &FT
            }}
        }
        TypeInfo::UnsignedInt => {
            let min = attrs.min.as_ref().map(|v| {
                let v: u64 = v.parse().expect("invalid min");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            let max = attrs.max.as_ref().map(|v| {
                let v: u64 = v.parse().expect("invalid max");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            quote! {{
                static FT: ::config::FieldType = ::config::FieldType::UInt { min: #min, max: #max };
                &FT
            }}
        }
        TypeInfo::Float => {
            let min = attrs.min.as_ref().map(|v| {
                let v: f64 = v.parse().expect("invalid min");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            let max = attrs.max.as_ref().map(|v| {
                let v: f64 = v.parse().expect("invalid max");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            quote! {{
                static FT: ::config::FieldType = ::config::FieldType::Float { min: #min, max: #max };
                &FT
            }}
        }
        TypeInfo::PathBuf => quote!(&::config::FieldType::String),
        TypeInfo::Option(inner) => {
            let inner_attrs = ConfigAttrs {
                min: attrs.min.clone(),
                max: attrs.max.clone(),
                min_len: attrs.min_len,
                max_len: attrs.max_len,
                ..Default::default()
            };
            let inner_type = generate_field_type_value(inner, &inner_attrs);
            quote! {{
                static INNER: ::config::FieldType = #inner_type;
                static FT: ::config::FieldType = ::config::FieldType::Optional { inner: &INNER };
                &FT
            }}
        }
        TypeInfo::Vec(inner) => {
            let inner_attrs = ConfigAttrs::default();
            let inner_type = generate_field_type_value(inner, &inner_attrs);
            let min_len = attrs.min_len.map(|v| quote!(Some(#v))).unwrap_or(quote!(None));
            let max_len = attrs.max_len.map(|v| quote!(Some(#v))).unwrap_or(quote!(None));
            quote! {{
                static ELEMENT: ::config::FieldType = #inner_type;
                static FT: ::config::FieldType = ::config::FieldType::List {
                    element: &ELEMENT,
                    min_len: #min_len,
                    max_len: #max_len,
                };
                &FT
            }}
        }
        TypeInfo::Configure | TypeInfo::Unknown => {
            // In lazy context, we CAN call field_type()
            quote! {
                <#ty as ::config::Configure>::field_type()
            }
        }
    }
}

/// Generate a reference to a FieldType for a Rust type.
/// 
/// This generates `&'static FieldType` expressions suitable for use in static FieldSchema.
/// For primitive types, we wrap them in inline static blocks.
/// For Configure types, we directly use their `field_type()` method which returns `&'static FieldType`.
fn generate_field_type(ty: &Type, attrs: &ConfigAttrs) -> TokenStream2 {
    // Extract the type path
    let type_info = analyze_type(ty);
    
    match type_info {
        TypeInfo::String => quote! {{
            static FT: ::config::FieldType = ::config::FieldType::String;
            &FT
        }},
        TypeInfo::Char => quote! {{
            static FT: ::config::FieldType = ::config::FieldType::Char;
            &FT
        }},
        TypeInfo::Bool => quote! {{
            static FT: ::config::FieldType = ::config::FieldType::Bool;
            &FT
        }},
        TypeInfo::SignedInt => {
            let min = attrs.min.as_ref().map(|v| {
                let v: i64 = v.parse().expect("invalid min");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            let max = attrs.max.as_ref().map(|v| {
                let v: i64 = v.parse().expect("invalid max");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            quote! {{
                static FT: ::config::FieldType = ::config::FieldType::Int { min: #min, max: #max };
                &FT
            }}
        }
        TypeInfo::UnsignedInt => {
            let min = attrs.min.as_ref().map(|v| {
                let v: u64 = v.parse().expect("invalid min");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            let max = attrs.max.as_ref().map(|v| {
                let v: u64 = v.parse().expect("invalid max");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            quote! {{
                static FT: ::config::FieldType = ::config::FieldType::UInt { min: #min, max: #max };
                &FT
            }}
        }
        TypeInfo::Float => {
            let min = attrs.min.as_ref().map(|v| {
                let v: f64 = v.parse().expect("invalid min");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            let max = attrs.max.as_ref().map(|v| {
                let v: f64 = v.parse().expect("invalid max");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            quote! {{
                static FT: ::config::FieldType = ::config::FieldType::Float { min: #min, max: #max };
                &FT
            }}
        }
        TypeInfo::Option(inner) => {
            let inner_attrs = ConfigAttrs {
                min: attrs.min.clone(),
                max: attrs.max.clone(),
                min_len: attrs.min_len,
                max_len: attrs.max_len,
                ..Default::default()
            };
            let inner_type = generate_field_type_value(inner, &inner_attrs);
            quote! {{
                static INNER: ::config::FieldType = #inner_type;
                static FT: ::config::FieldType = ::config::FieldType::Optional { inner: &INNER };
                &FT
            }}
        }
        TypeInfo::Vec(inner) => {
            let inner_attrs = ConfigAttrs::default();
            let inner_type = generate_field_type_value(inner, &inner_attrs);
            let min_len = attrs.min_len.map(|v| quote!(Some(#v))).unwrap_or(quote!(None));
            let max_len = attrs.max_len.map(|v| quote!(Some(#v))).unwrap_or(quote!(None));
            quote! {{
                static ELEMENT: ::config::FieldType = #inner_type;
                static FT: ::config::FieldType = ::config::FieldType::List {
                    element: &ELEMENT,
                    min_len: #min_len,
                    max_len: #max_len,
                };
                &FT
            }}
        }
        TypeInfo::PathBuf => quote! {{
            static FT: ::config::FieldType = ::config::FieldType::String;
            &FT
        }},
        TypeInfo::Configure | TypeInfo::Unknown => {
            // Type implements Configure, use its field_type() to get the appropriate FieldType
            // This works for both enums (returns FieldType::Enum) and structs (returns FieldType::Struct)
            // field_type() returns &'static FieldType, so we can use it directly
            quote! {
                <#ty as ::config::Configure>::field_type()
            }
        }
    }
}

/// Generate a FieldType value (not a reference) for use in static declarations.
/// This is used for inner types in Option/Vec where we need to declare the static ourselves.
fn generate_field_type_value(ty: &Type, attrs: &ConfigAttrs) -> TokenStream2 {
    let type_info = analyze_type(ty);
    
    match type_info {
        TypeInfo::String => quote!(::config::FieldType::String),
        TypeInfo::Char => quote!(::config::FieldType::Char),
        TypeInfo::Bool => quote!(::config::FieldType::Bool),
        TypeInfo::SignedInt => {
            let min = attrs.min.as_ref().map(|v| {
                let v: i64 = v.parse().expect("invalid min");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            let max = attrs.max.as_ref().map(|v| {
                let v: i64 = v.parse().expect("invalid max");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            quote!(::config::FieldType::Int { min: #min, max: #max })
        }
        TypeInfo::UnsignedInt => {
            let min = attrs.min.as_ref().map(|v| {
                let v: u64 = v.parse().expect("invalid min");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            let max = attrs.max.as_ref().map(|v| {
                let v: u64 = v.parse().expect("invalid max");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            quote!(::config::FieldType::UInt { min: #min, max: #max })
        }
        TypeInfo::Float => {
            let min = attrs.min.as_ref().map(|v| {
                let v: f64 = v.parse().expect("invalid min");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            let max = attrs.max.as_ref().map(|v| {
                let v: f64 = v.parse().expect("invalid max");
                quote!(Some(#v))
            }).unwrap_or(quote!(None));
            quote!(::config::FieldType::Float { min: #min, max: #max })
        }
        TypeInfo::PathBuf => quote!(::config::FieldType::String),
        // For nested Option/Vec or Configure types in Option/Vec, we need special handling
        // For now, only support simple types in Option/Vec
        TypeInfo::Option(_) | TypeInfo::Vec(_) => {
            quote!(compile_error!("Nested Option/Vec not supported in field types"))
        }
        TypeInfo::Configure | TypeInfo::Unknown => {
            // For Configure types inside Option/Vec, we can't easily get a static FieldType value
            // because field_type() returns a reference. We'd need the type to expose the static directly.
            // For now, generate a compile error with a helpful message.
            quote!(compile_error!("Configure types inside Option/Vec not yet supported - consider using a wrapper struct"))
        }
    }
}

/// Analyzed type information
enum TypeInfo<'a> {
    String,
    Char,
    Bool,
    SignedInt,
    UnsignedInt,
    Float,
    Option(&'a Type),
    Vec(&'a Type),
    PathBuf,
    Configure,
    Unknown,
}

/// Analyze a type to determine its kind
fn analyze_type(ty: &Type) -> TypeInfo<'_> {
    if let Type::Path(type_path) = ty {
        let segments = &type_path.path.segments;
        
        // Get the last segment (handles paths like std::string::String)
        if let Some(last) = segments.last() {
            let ident = last.ident.to_string();
            
            match ident.as_str() {
                "String" => return TypeInfo::String,
                "char" => return TypeInfo::Char,
                "bool" => return TypeInfo::Bool,
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => return TypeInfo::SignedInt,
                "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => return TypeInfo::UnsignedInt,
                "f32" | "f64" => return TypeInfo::Float,
                "PathBuf" => return TypeInfo::PathBuf,
                "Option" => {
                    if let PathArguments::AngleBracketed(args) = &last.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            return TypeInfo::Option(inner);
                        }
                    }
                }
                "Vec" => {
                    if let PathArguments::AngleBracketed(args) = &last.arguments {
                        if let Some(GenericArgument::Type(inner)) = args.args.first() {
                            return TypeInfo::Vec(inner);
                        }
                    }
                }
                _ => {
                    // Assume it's a type that implements Configure
                    return TypeInfo::Configure;
                }
            }
        }
    }
    
    TypeInfo::Unknown
}

/// Generate to_values() body for configurable fields
fn generate_to_values(fields: &[&FieldInfo]) -> TokenStream2 {
    let conversions: Vec<_> = fields.iter().map(|field| {
        let name = &field.name;
        generate_to_value(&field.ty, quote!(self.#name))
    }).collect();
    
    quote! { #(#conversions),* }
}

/// Generate ConfigValue from a Rust value
fn generate_to_value(ty: &Type, accessor: TokenStream2) -> TokenStream2 {
    generate_to_value_impl(ty, accessor, false)
}

/// Generate ConfigValue from a Rust value, with a flag indicating if the accessor is a reference
fn generate_to_value_impl(ty: &Type, accessor: TokenStream2, is_ref: bool) -> TokenStream2 {
    let type_info = analyze_type(ty);
    
    match type_info {
        TypeInfo::String => {
            if is_ref {
                quote!(::config::ConfigValue::String(#accessor.clone()))
            } else {
                quote!(::config::ConfigValue::String(#accessor.clone()))
            }
        }
        TypeInfo::Char => {
            if is_ref {
                quote!(::config::ConfigValue::Char(*#accessor))
            } else {
                quote!(::config::ConfigValue::Char(#accessor))
            }
        }
        TypeInfo::Bool => {
            if is_ref {
                quote!(::config::ConfigValue::Bool(*#accessor))
            } else {
                quote!(::config::ConfigValue::Bool(#accessor))
            }
        }
        TypeInfo::SignedInt => {
            if is_ref {
                quote!(::config::ConfigValue::Int(*#accessor as i64))
            } else {
                quote!(::config::ConfigValue::Int(#accessor as i64))
            }
        }
        TypeInfo::UnsignedInt => {
            if is_ref {
                quote!(::config::ConfigValue::UInt(*#accessor as u64))
            } else {
                quote!(::config::ConfigValue::UInt(#accessor as u64))
            }
        }
        TypeInfo::Float => {
            if is_ref {
                quote!(::config::ConfigValue::Float(*#accessor as f64))
            } else {
                quote!(::config::ConfigValue::Float(#accessor as f64))
            }
        }
        TypeInfo::PathBuf => quote!(::config::ConfigValue::String(#accessor.to_string_lossy().to_string())),
        TypeInfo::Option(inner) => {
            // accessor.as_ref() gives Option<&T>, so v in the closure is &T
            let inner_conv = generate_to_value_impl(inner, quote!(v), true);
            quote! {
                ::config::ConfigValue::Optional(
                    #accessor.as_ref().map(|v| Box::new(#inner_conv))
                )
            }
        }
        TypeInfo::Vec(inner) => {
            // iter() gives us &T, so we need to handle the reference
            let inner_conv = generate_to_value_impl(inner, quote!(item), true);
            quote! {
                ::config::ConfigValue::List(
                    #accessor.iter().map(|item| #inner_conv).collect()
                )
            }
        }
        TypeInfo::Configure | TypeInfo::Unknown => {
            // Type implements Configure, use its to_values()
            quote!(::config::ConfigValue::Struct(#accessor.to_values()))
        }
    }
}

/// Generate from_values() body
fn generate_from_values(fields: &[FieldInfo]) -> TokenStream2 {
    let mut index = 0usize;
    
    let field_extractions: Vec<_> = fields.iter().map(|field| {
        let name = &field.name;
        
        if field.attrs.skip {
            // Skipped fields use Default
            quote! {
                #name: Default::default()
            }
        } else {
            let extraction = generate_from_value(&field.ty, &field.name.to_string(), index);
            index += 1;
            quote! {
                #name: #extraction
            }
        }
    }).collect();
    
    quote! {
        Ok(Self {
            #(#field_extractions),*
        })
    }
}

/// Generate code to extract a value from ConfigValues at an index
fn generate_from_value(ty: &Type, field_name: &str, index: usize) -> TokenStream2 {
    let type_info = analyze_type(ty);
    
    match type_info {
        TypeInfo::String => quote! {
            match &values.values[#index] {
                ::config::ConfigValue::String(s) => s.clone(),
                other => return Err(::config::ConfigError::type_mismatch(#field_name, "String", other.type_name())),
            }
        },
        TypeInfo::Char => quote! {
            match &values.values[#index] {
                ::config::ConfigValue::Char(c) => *c,
                other => return Err(::config::ConfigError::type_mismatch(#field_name, "Char", other.type_name())),
            }
        },
        TypeInfo::Bool => quote! {
            match &values.values[#index] {
                ::config::ConfigValue::Bool(b) => *b,
                other => return Err(::config::ConfigError::type_mismatch(#field_name, "Bool", other.type_name())),
            }
        },
        TypeInfo::SignedInt => quote! {
            match &values.values[#index] {
                ::config::ConfigValue::Int(v) => *v as _,
                other => return Err(::config::ConfigError::type_mismatch(#field_name, "Int", other.type_name())),
            }
        },
        TypeInfo::UnsignedInt => quote! {
            match &values.values[#index] {
                ::config::ConfigValue::UInt(v) => *v as _,
                other => return Err(::config::ConfigError::type_mismatch(#field_name, "UInt", other.type_name())),
            }
        },
        TypeInfo::Float => quote! {
            match &values.values[#index] {
                ::config::ConfigValue::Float(v) => *v as _,
                other => return Err(::config::ConfigError::type_mismatch(#field_name, "Float", other.type_name())),
            }
        },
        TypeInfo::PathBuf => quote! {
            match &values.values[#index] {
                ::config::ConfigValue::String(s) => std::path::PathBuf::from(s),
                other => return Err(::config::ConfigError::type_mismatch(#field_name, "String", other.type_name())),
            }
        },
        TypeInfo::Option(inner) => {
            let inner_extraction = generate_inner_value_extraction(inner, field_name);
            quote! {
                match &values.values[#index] {
                    ::config::ConfigValue::Optional(opt) => {
                        opt.as_ref().map(|inner| {
                            let inner: &::config::ConfigValue = inner.as_ref();
                            #inner_extraction
                        }).transpose()?
                    },
                    other => return Err(::config::ConfigError::type_mismatch(#field_name, "Optional", other.type_name())),
                }
            }
        },
        TypeInfo::Vec(inner) => {
            let inner_extraction = generate_inner_value_extraction(inner, field_name);
            quote! {
                match &values.values[#index] {
                    ::config::ConfigValue::List(items) => {
                        items.iter().map(|inner| {
                            #inner_extraction
                        }).collect::<Result<Vec<_>, _>>()?
                    },
                    other => return Err(::config::ConfigError::type_mismatch(#field_name, "List", other.type_name())),
                }
            }
        },
        TypeInfo::Configure | TypeInfo::Unknown => {
            quote! {
                match &values.values[#index] {
                    ::config::ConfigValue::Struct(inner_values) => {
                        <#ty as ::config::Configure>::from_values(inner_values)?
                    },
                    other => return Err(::config::ConfigError::type_mismatch(#field_name, "Struct", other.type_name())),
                }
            }
        },
    }
}

/// Generate code to extract a value from a ConfigValue (for nested contexts)
/// `inner` is expected to be a reference to ConfigValue
fn generate_inner_value_extraction(ty: &Type, field_name: &str) -> TokenStream2 {
    let type_info = analyze_type(ty);
    
    match type_info {
        TypeInfo::String => quote! {
            match inner {
                ::config::ConfigValue::String(s) => Ok(s.clone()),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "String", other.type_name())),
            }
        },
        TypeInfo::Char => quote! {
            match inner {
                ::config::ConfigValue::Char(c) => Ok(*c),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Char", other.type_name())),
            }
        },
        TypeInfo::Bool => quote! {
            match inner {
                ::config::ConfigValue::Bool(b) => Ok(*b),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Bool", other.type_name())),
            }
        },
        TypeInfo::SignedInt => quote! {
            match inner {
                ::config::ConfigValue::Int(v) => Ok(*v as _),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Int", other.type_name())),
            }
        },
        TypeInfo::UnsignedInt => quote! {
            match inner {
                ::config::ConfigValue::UInt(v) => Ok(*v as _),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "UInt", other.type_name())),
            }
        },
        TypeInfo::Float => quote! {
            match inner {
                ::config::ConfigValue::Float(v) => Ok(*v as _),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Float", other.type_name())),
            }
        },
        TypeInfo::PathBuf => quote! {
            match inner {
                ::config::ConfigValue::String(s) => Ok(std::path::PathBuf::from(s)),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "String", other.type_name())),
            }
        },
        TypeInfo::Configure | TypeInfo::Unknown => {
            quote! {
                match inner {
                    ::config::ConfigValue::Struct(inner_values) => {
                        <#ty as ::config::Configure>::from_values(inner_values)
                    },
                    other => Err(::config::ConfigError::type_mismatch(#field_name, "Struct", other.type_name())),
                }
            }
        },
        // For nested Option/Vec in Option/Vec, this gets complex - leave as unsupported for now
        TypeInfo::Option(_) | TypeInfo::Vec(_) => quote! {
            compile_error!("Nested Option/Vec not yet supported")
        },
    }
}

/// Derive Configure for an enum
fn derive_enum(input: &DeriveInput, data: &syn::DataEnum) -> TokenStream2 {
    let name = &input.ident;
    let type_attrs = ConfigAttrs::from_attrs(&input.attrs);
    
    // Collect variant info
    let variants: Vec<_> = data.variants.iter().map(|v| {
        let attrs = ConfigAttrs::from_attrs(&v.attrs);
        (v, attrs)
    }).collect();
    
    // Validate all variants have labels
    for (variant, attrs) in &variants {
        if attrs.label.is_none() {
            return syn::Error::new_spanned(
                &variant.ident,
                format!("enum variant '{}' requires #[config(label = \"...\")]", variant.ident)
            ).to_compile_error();
        }
    }
    
    // Generate schema
    let schema_name = format_ident!("__{}_SCHEMA", name.to_string().to_uppercase());
    let field_type_name = format_ident!("__{}_FIELD_TYPE", name.to_string().to_uppercase());
    let type_name = name.to_string();
    let type_desc = type_attrs.desc.as_ref().map(|d| quote!(Some(#d))).unwrap_or(quote!(None));
    
    // For enums, the schema has no fields - the enum itself is the value
    // But we need to store variant info in a FieldType::Enum
    let variant_schemas = generate_variant_schemas(&variants);
    let variants_name = format_ident!("__{}_VARIANTS", name.to_string().to_uppercase());
    
    // Generate to_values() - enum becomes a single Enum value
    let to_values_arms = generate_enum_to_values_arms(name, &variants);
    
    // Generate from_values()
    let from_values_arms = generate_enum_from_values_arms(name, &variants);
    let variant_count = variants.len();
    
    quote! {
        #[doc(hidden)]
        #[allow(dead_code)]
        static #variants_name: &[::config::VariantSchema] = &[#variant_schemas];
        
        #[doc(hidden)]
        static #field_type_name: ::config::FieldType = ::config::FieldType::Enum {
            variants: #variants_name,
        };
        
        #[doc(hidden)]
        static #schema_name: ::config::ConfigSchema = ::config::ConfigSchema {
            name: #type_name,
            description: #type_desc,
            fields: &[],  // Enums have no fields, they ARE the value
        };
        
        impl ::config::Configure for #name {
            fn schema() -> &'static ::config::ConfigSchema {
                &#schema_name
            }
            
            fn field_type() -> &'static ::config::FieldType {
                &#field_type_name
            }
            
            fn to_values(&self) -> ::config::ConfigValues {
                let variant_value = match self {
                    #to_values_arms
                };
                ::config::ConfigValues::new(vec![variant_value])
            }
            
            fn from_values(values: &::config::ConfigValues) -> Result<Self, ::config::ConfigError> {
                if values.len() != 1 {
                    return Err(::config::ConfigError::wrong_value_count(1, values.len()));
                }
                
                match &values.values[0] {
                    ::config::ConfigValue::Enum { variant_index, data } => {
                        match (*variant_index, data) {
                            #from_values_arms
                            (idx, _) => Err(::config::ConfigError::invalid_variant(stringify!(#name), idx, #variant_count - 1)),
                        }
                    },
                    other => Err(::config::ConfigError::type_mismatch(stringify!(#name), "Enum", other.type_name())),
                }
            }
        }
    }
}

/// Generate VariantSchema items
fn generate_variant_schemas(variants: &[(&syn::Variant, ConfigAttrs)]) -> TokenStream2 {
    let schemas: Vec<_> = variants.iter().map(|(variant, attrs)| {
        let name = variant.ident.to_string();
        let label = attrs.label.as_ref().unwrap();
        let desc = attrs.desc.as_ref()
            .map(|d| quote!(Some(#d)))
            .unwrap_or(quote!(None));
        
        let variant_data = match &variant.fields {
            Fields::Unit => quote!(::config::VariantData::None),
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field = fields.unnamed.first().unwrap();
                let field_attrs = ConfigAttrs::from_attrs(&field.attrs);
                // For VariantData::Single, we need &'static FieldType
                // generate_field_type() returns exactly that
                let field_type_ref = generate_field_type(&field.ty, &field_attrs);
                quote! {
                    ::config::VariantData::Single(#field_type_ref)
                }
            }
            Fields::Unnamed(_) => {
                // Multi-value tuple variants not supported
                quote!(compile_error!("Tuple variants with multiple values not supported"))
            }
            Fields::Named(fields) => {
                // Struct variant - generate inline schema
                let field_schemas: Vec<_> = fields.named.iter().map(|f| {
                    let field_attrs = ConfigAttrs::from_attrs(&f.attrs);
                    let field_name = f.ident.as_ref().unwrap().to_string();
                    let field_label = field_attrs.label.as_ref()
                        .cloned()
                        .unwrap_or_else(|| field_name.clone());
                    let field_desc = field_attrs.desc.as_ref()
                        .map(|d| quote!(Some(#d)))
                        .unwrap_or(quote!(None));
                    let field_type = generate_field_type(&f.ty, &field_attrs);
                    
                    quote! {
                        ::config::FieldSchema {
                            name: #field_name,
                            label: #field_label,
                            description: #field_desc,
                            field_type: #field_type,
                        }
                    }
                }).collect();
                
                let variant_schema_name = format!("{}_SCHEMA", name.to_uppercase());
                quote! {
                    ::config::VariantData::Struct({
                        static FIELDS: &[::config::FieldSchema] = &[#(#field_schemas),*];
                        static SCHEMA: ::config::ConfigSchema = ::config::ConfigSchema {
                            name: #variant_schema_name,
                            description: None,
                            fields: FIELDS,
                        };
                        &SCHEMA
                    })
                }
            }
        };
        
        quote! {
            ::config::VariantSchema {
                name: #name,
                label: #label,
                description: #desc,
                data: #variant_data,
            }
        }
    }).collect();
    
    quote! { #(#schemas),* }
}

/// Generate to_values() match arms for enum
fn generate_enum_to_values_arms(enum_name: &Ident, variants: &[(&syn::Variant, ConfigAttrs)]) -> TokenStream2 {
    let arms: Vec<_> = variants.iter().enumerate().map(|(idx, (variant, _attrs))| {
        let variant_name = &variant.ident;
        
        match &variant.fields {
            Fields::Unit => {
                quote! {
                    #enum_name::#variant_name => ::config::ConfigValue::Enum {
                        variant_index: #idx,
                        data: None,
                    }
                }
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field = fields.unnamed.first().unwrap();
                // v will be a reference since we're matching on &self
                let value_conv = generate_to_value_impl(&field.ty, quote!(v), true);
                quote! {
                    #enum_name::#variant_name(v) => ::config::ConfigValue::Enum {
                        variant_index: #idx,
                        data: Some(::config::ConfigValues::new(vec![#value_conv])),
                    }
                }
            }
            Fields::Unnamed(_) => {
                quote!(compile_error!("Tuple variants with multiple values not supported"))
            }
            Fields::Named(fields) => {
                let field_names: Vec<_> = fields.named.iter()
                    .map(|f| f.ident.as_ref().unwrap())
                    .collect();
                // Fields will be references since we're matching on &self
                let field_convs: Vec<_> = fields.named.iter()
                    .map(|f| {
                        let name = f.ident.as_ref().unwrap();
                        generate_to_value_impl(&f.ty, quote!(#name), true)
                    })
                    .collect();
                
                quote! {
                    #enum_name::#variant_name { #(#field_names),* } => ::config::ConfigValue::Enum {
                        variant_index: #idx,
                        data: Some(::config::ConfigValues::new(vec![#(#field_convs),*])),
                    }
                }
            }
        }
    }).collect();
    
    quote! { #(#arms),* }
}

/// Generate from_values() match arms for enum
fn generate_enum_from_values_arms(enum_name: &Ident, variants: &[(&syn::Variant, ConfigAttrs)]) -> TokenStream2 {
    let arms: Vec<_> = variants.iter().enumerate().map(|(idx, (variant, _attrs))| {
        let variant_name = &variant.ident;
        let variant_name_str = variant_name.to_string();
        
        match &variant.fields {
            Fields::Unit => {
                quote! {
                    (#idx, None) => Ok(#enum_name::#variant_name),
                    (#idx, Some(_)) => Err(::config::ConfigError::validation_failed(
                        stringify!(#enum_name),
                        format!("unit variant '{}' should not have data", #variant_name_str)
                    ))
                }
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field = fields.unnamed.first().unwrap();
                let extraction = generate_inner_value_extraction_from_config_value(&field.ty, &variant_name_str);
                quote! {
                    (#idx, Some(inner_values)) if inner_values.len() == 1 => {
                        let inner = &inner_values.values[0];
                        let value = #extraction?;
                        Ok(#enum_name::#variant_name(value))
                    },
                    (#idx, Some(inner_values)) => Err(::config::ConfigError::wrong_value_count(1, inner_values.len())),
                    (#idx, None) => Err(::config::ConfigError::validation_failed(
                        stringify!(#enum_name),
                        format!("variant '{}' requires data", #variant_name_str)
                    ))
                }
            }
            Fields::Unnamed(_) => {
                quote!(compile_error!("Tuple variants with multiple values not supported"))
            }
            Fields::Named(fields) => {
                let field_count = fields.named.len();
                let field_extractions: Vec<_> = fields.named.iter().enumerate().map(|(fidx, f)| {
                    let field_name = f.ident.as_ref().unwrap();
                    let field_name_str = field_name.to_string();
                    let extraction = generate_field_extraction_indexed(&f.ty, &field_name_str, fidx);
                    quote! {
                        #field_name: #extraction?
                    }
                }).collect();
                
                quote! {
                    (#idx, Some(inner_values)) if inner_values.len() == #field_count => {
                        Ok(#enum_name::#variant_name {
                            #(#field_extractions),*
                        })
                    },
                    (#idx, Some(inner_values)) => Err(::config::ConfigError::wrong_value_count(#field_count, inner_values.len())),
                    (#idx, None) => Err(::config::ConfigError::validation_failed(
                        stringify!(#enum_name),
                        format!("variant '{}' requires data", #variant_name_str)
                    ))
                }
            }
        }
    }).collect();
    
    quote! { #(#arms,)* }
}

/// Generate extraction from a ConfigValue (not indexed)
fn generate_inner_value_extraction_from_config_value(ty: &Type, field_name: &str) -> TokenStream2 {
    let type_info = analyze_type(ty);
    
    match type_info {
        TypeInfo::String => quote! {
            match inner {
                ::config::ConfigValue::String(s) => Ok(s.clone()),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "String", other.type_name())),
            }
        },
        TypeInfo::Char => quote! {
            match inner {
                ::config::ConfigValue::Char(c) => Ok(*c),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Char", other.type_name())),
            }
        },
        TypeInfo::Bool => quote! {
            match inner {
                ::config::ConfigValue::Bool(b) => Ok(*b),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Bool", other.type_name())),
            }
        },
        TypeInfo::SignedInt => quote! {
            match inner {
                ::config::ConfigValue::Int(v) => Ok(*v as _),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Int", other.type_name())),
            }
        },
        TypeInfo::UnsignedInt => quote! {
            match inner {
                ::config::ConfigValue::UInt(v) => Ok(*v as _),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "UInt", other.type_name())),
            }
        },
        TypeInfo::Float => quote! {
            match inner {
                ::config::ConfigValue::Float(v) => Ok(*v as _),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Float", other.type_name())),
            }
        },
        TypeInfo::PathBuf => quote! {
            match inner {
                ::config::ConfigValue::String(s) => Ok(std::path::PathBuf::from(s)),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "String", other.type_name())),
            }
        },
        TypeInfo::Configure | TypeInfo::Unknown => {
            quote! {
                match inner {
                    ::config::ConfigValue::Struct(inner_values) => {
                        <#ty as ::config::Configure>::from_values(inner_values)
                    },
                    other => Err(::config::ConfigError::type_mismatch(#field_name, "Struct", other.type_name())),
                }
            }
        },
        TypeInfo::Option(_) | TypeInfo::Vec(_) => quote! {
            compile_error!("Option/Vec in enum variants not yet fully supported")
        },
    }
}

/// Generate extraction from ConfigValues at an index, returning Result
fn generate_field_extraction_indexed(ty: &Type, field_name: &str, index: usize) -> TokenStream2 {
    let type_info = analyze_type(ty);
    
    match type_info {
        TypeInfo::String => quote! {
            match &inner_values.values[#index] {
                ::config::ConfigValue::String(s) => Ok(s.clone()),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "String", other.type_name())),
            }
        },
        TypeInfo::Char => quote! {
            match &inner_values.values[#index] {
                ::config::ConfigValue::Char(c) => Ok(*c),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Char", other.type_name())),
            }
        },
        TypeInfo::Bool => quote! {
            match &inner_values.values[#index] {
                ::config::ConfigValue::Bool(b) => Ok(*b),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Bool", other.type_name())),
            }
        },
        TypeInfo::SignedInt => quote! {
            match &inner_values.values[#index] {
                ::config::ConfigValue::Int(v) => Ok(*v as _),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Int", other.type_name())),
            }
        },
        TypeInfo::UnsignedInt => quote! {
            match &inner_values.values[#index] {
                ::config::ConfigValue::UInt(v) => Ok(*v as _),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "UInt", other.type_name())),
            }
        },
        TypeInfo::Float => quote! {
            match &inner_values.values[#index] {
                ::config::ConfigValue::Float(v) => Ok(*v as _),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "Float", other.type_name())),
            }
        },
        TypeInfo::PathBuf => quote! {
            match &inner_values.values[#index] {
                ::config::ConfigValue::String(s) => Ok(std::path::PathBuf::from(s)),
                other => Err(::config::ConfigError::type_mismatch(#field_name, "String", other.type_name())),
            }
        },
        TypeInfo::Configure | TypeInfo::Unknown => {
            quote! {
                match &inner_values.values[#index] {
                    ::config::ConfigValue::Struct(iv) => {
                        <#ty as ::config::Configure>::from_values(iv)
                    },
                    other => Err(::config::ConfigError::type_mismatch(#field_name, "Struct", other.type_name())),
                }
            }
        },
        TypeInfo::Option(_) | TypeInfo::Vec(_) => quote! {
            compile_error!("Option/Vec in struct variants not yet fully supported")
        },
    }
}
