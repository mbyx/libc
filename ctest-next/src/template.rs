use std::ops::Deref;

use askama::Template;
use either::Either;
use quote::ToTokens;

use crate::ffi_items::FfiItems;
use crate::translator::{translate_abi, translate_expr, Translator};
use crate::{
    Field, MapInput, Result, Struct, TestGenerator, TranslationError, Union, VolatileItemKind,
};

/// Represents the Rust side of the generated testing suite.
#[derive(Template, Clone)]
#[template(path = "test.rs")]
pub(crate) struct RustTestTemplate<'a> {
    ffi_items: &'a FfiItems,
    translator: Translator,
    generator: &'a TestGenerator,
}

/// Represents the C side of the generated testing suite.
#[derive(Template, Clone)]
#[template(path = "test.c")]
pub(crate) struct CTestTemplate<'a> {
    translator: Translator,
    ffi_items: &'a FfiItems,
    generator: &'a TestGenerator,
}

impl<'a> RustTestTemplate<'a> {
    /// Create a new test template to test the given items.
    pub(crate) fn new(ffi_items: &'a FfiItems, generator: &'a TestGenerator) -> Self {
        Self {
            ffi_items,
            translator: Translator::new(),
            generator,
        }
    }
}

impl<'a> CTestTemplate<'a> {
    /// Create a new test template to test the given items.
    pub(crate) fn new(ffi_items: &'a FfiItems, generator: &'a TestGenerator) -> Self {
        Self {
            ffi_items,
            translator: Translator::new(),
            generator,
        }
    }

    /// Returns the equivalent C/Cpp identifier of the Rust item.
    pub(crate) fn c_ident(&self, item: impl Into<MapInput<'a>>) -> String {
        self.generator.map(item)
    }

    /// Returns the equivalent C/Cpp type of the Rust item.
    pub(crate) fn c_type(&self, item: impl Into<MapInput<'a>>) -> Result<String, TranslationError> {
        let item: MapInput<'a> = item.into();

        let (_ident, ty) = match item {
            MapInput::Const(c) => (c.ident(), self.translator.translate_type(&c.ty)?),
            MapInput::Field(_, f) => (f.ident(), self.translator.translate_type(&f.ty)?),
            MapInput::Static(s) => (s.ident(), self.translator.translate_type(&s.ty)?),
            MapInput::Fn(_) => unimplemented!(),
            // For structs/unions/aliases, their type is the same as their identifier.
            MapInput::Alias(a) => (
                a.ident(),
                self.translator.translate_primitive_type(&syn::Ident::new(
                    a.ident(),
                    proc_macro2::Span::call_site(),
                )),
            ),
            MapInput::Struct(s) => (
                s.ident(),
                self.translator.translate_primitive_type(&syn::Ident::new(
                    s.ident(),
                    proc_macro2::Span::call_site(),
                )),
            ),
            MapInput::Union(u) => (
                u.ident(),
                self.translator.translate_primitive_type(&syn::Ident::new(
                    u.ident(),
                    proc_macro2::Span::call_site(),
                )),
            ),

            MapInput::StructType(_) => panic!("MapInput::StructType is not allowed!"),
            MapInput::UnionType(_) => panic!("MapInput::UnionType is not allowed!"),
            MapInput::FieldType(_, _) => panic!("MapInput::FieldType is not allowed!"),
            MapInput::Type(_) => panic!("MapInput::Type is not allowed!"),
        };

        let item = if self.ffi_items.contains_struct(&ty) {
            MapInput::StructType(&ty)
        } else if self.ffi_items.contains_union(&ty) {
            MapInput::UnionType(&ty)
        } else {
            MapInput::Type(&ty)
        };

        Ok(self.generator.map(item))
    }

    /// Modify a C function `signature` that returns a ptr `ty` to be correctly translated.
    ///
    /// Arrays and Function types in C have different rules for placement, such as array lengths
    /// being placed after the parameter list.
    pub(crate) fn c_signature(
        &self,
        ty: &syn::Type,
        signature: &str,
    ) -> Result<String, TranslationError> {
        let new_signature = match ty {
            syn::Type::Path(p) => {
                // Check if this is an Option<fn_ptr> and recurse
                if let Some(last_segment) = p.path.segments.last() {
                    if last_segment.ident == "Option" {
                        if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                                // Recurse with the inner type
                                return self.c_signature(inner_ty, signature);
                            }
                        }
                    }
                }

                // Regular path type handling
                let mapped_type = self.get_mapped_type(ty)?;
                format!("{mapped_type}* {signature}")
            }
            syn::Type::BareFn(f) => {
                let (ret, mut args, variadic) = self.translator.translate_signature_partial(f)?;
                let abi = if let Some(abi) = &f.abi {
                    let target = self
                        .generator
                        .target
                        .clone()
                        .or_else(|| std::env::var("TARGET").ok())
                        .or_else(|| std::env::var("TARGET_PLATFORM").ok())
                        .unwrap();
                    translate_abi(abi, &target)
                } else {
                    ""
                };

                if variadic {
                    args.push("...".to_string());
                } else if args.is_empty() {
                    args.push("void".to_string());
                }

                format!("{}({}**{})({})", ret, abi, signature, args.join(", "))
            }
            syn::Type::Array(outer) => match outer.elem.deref() {
                syn::Type::Array(inner) => {
                    let inner_type = self.get_mapped_type(inner.elem.deref())?;
                    format!(
                        "{}(*{})[{}][{}]",
                        inner_type,
                        signature,
                        translate_expr(&outer.len),
                        translate_expr(&inner.len)
                    )
                }
                _ => {
                    let elem_type = self.get_mapped_type(outer.elem.deref())?;
                    format!(
                        "{}(*{})[{}]",
                        elem_type,
                        signature,
                        translate_expr(&outer.len)
                    )
                }
            },
            _ => {
                let mapped_type = self.get_mapped_type(ty)?;
                format!("{mapped_type}* {signature}")
            }
        };

        Ok(new_signature)
    }

    /// Recursively get the properly mapped type for any syn::Type
    fn get_mapped_type(&self, ty: &syn::Type) -> Result<String, TranslationError> {
        let type_name = match ty {
            syn::Type::Path(p) => p.path.segments.last().unwrap().ident.to_string(),
            syn::Type::Ptr(p) => match p.elem.deref() {
                syn::Type::Path(p) => p.path.segments.last().unwrap().ident.to_string(),
                _ => p.to_token_stream().to_string(),
            },
            _ => ty.to_token_stream().to_string(),
        };

        let unmapped_c_type = self.translator.translate_type(ty)?;
        let map_input = if self.ffi_items.contains_struct(&type_name) {
            MapInput::StructType(&unmapped_c_type)
        } else if self.ffi_items.contains_union(&type_name) {
            MapInput::UnionType(&unmapped_c_type)
        } else {
            MapInput::Type(&unmapped_c_type)
        };

        Ok(self.generator.map(map_input))
    }

    /// Returns the volatile keyword if the given item is volatile.
    pub(crate) fn emit_volatile(&self, v: VolatileItemKind) -> &str {
        if !self.generator.volatile_items.is_empty()
            && self.generator.volatile_items.iter().any(|f| f(v.clone()))
        {
            "volatile "
        } else {
            ""
        }
    }
}

/* Helper functions to make the template code readable. */

/// Determine whether a Rust alias/struct/union should have a round trip test.
///
/// By default all alias/struct/unions are roundtripped. Aliases or fields with arrays should
/// not be part of the roundtrip.
pub(crate) fn should_roundtrip(gen: &TestGenerator, ident: &str) -> bool {
    gen.skip_roundtrip.as_ref().is_none_or(|skip| !skip(ident))
}

/// Determine whether a Rust alias should have a signededness test.
///
/// By default all aliases are tested if they alias to a signed/unsigned type.
pub(crate) fn should_test_sign(gen: &TestGenerator, ident: &str) -> bool {
    gen.skip_signededness
        .as_ref()
        .is_none_or(|skip| !skip(ident))
}

/// Determine whether a Rust ffi function should have a fn ptr check.
pub(crate) fn should_test_fn_ptr(gen: &TestGenerator, ident: &str) -> bool {
    gen.skip_fn_ptr_check
        .as_ref()
        .is_none_or(|skip| !skip(ident))
}

/// Determine whether a struct field should be skipped for tests.
pub(crate) fn should_skip_field(
    gen: &TestGenerator,
    e: Either<&Struct, &Union>,
    field: &Field,
) -> bool {
    gen.skips.iter().any(|f| f(&MapInput::Field(e, field))) || !field.public
}

/// Determine whether a struct field type should be skipped for tests.
pub(crate) fn should_skip_field_type(
    gen: &TestGenerator,
    e: Either<&Struct, &Union>,
    field: &Field,
) -> bool {
    gen.skips.iter().any(|f| f(&MapInput::FieldType(e, field))) || !field.public
}

fn parse_signature_to_type(signature: &str) -> syn::Result<syn::Type> {
    let (_, s) = signature.split_once('(').unwrap();
    let type_str = format!("type T = unsafe extern \"C\" fn({};", s);
    eprintln!("type_str: {type_str}");
    let item: syn::ItemType = syn::parse_str(&type_str)?;
    Ok(*item.ty)
}
