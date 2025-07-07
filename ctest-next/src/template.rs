use std::ops::Deref;

use askama::Template;
use quote::ToTokens;

use crate::{
    ffi_items::FfiItems, generator::GenerationError, translator::Translator, Field, MapInput,
    Result, Struct, TestGenerator, TranslationError, TyKind, VolatileItemKind,
};

/// Represents the Rust side of the generated testing suite.
#[derive(Template, Clone)]
#[template(path = "test.rs")]
pub(crate) struct RustTestTemplate<'a> {
    ffi_items: &'a FfiItems,
    #[expect(unused)]
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
    pub(crate) fn c_ident(&self, item: impl Into<MapInput<'a>>) -> Result<String, GenerationError> {
        self.generator.map(item)
    }

    /// Returns the equivalent C/Cpp type of the Rust item.
    pub(crate) fn c_type(&self, item: impl Into<MapInput<'a>>) -> Result<String, GenerationError> {
        let item: MapInput<'a> = item.into();

        let (ident, ty) = match item {
            MapInput::Const(c) => (c.ident().to_string(), self.translator.translate_type(&c.ty)),
            MapInput::Alias(a) => (a.ident().to_string(), Ok(a.ident().to_string())),
            MapInput::Field(_, f) => (f.ident().to_string(), self.translator.translate_type(&f.ty)),
            MapInput::Static(s) => (s.ident().to_string(), self.translator.translate_type(&s.ty)),
            MapInput::Fn(_) => unimplemented!(),
            MapInput::Struct(s) => (s.ident().to_string(), Ok(s.ident().to_string())),
            MapInput::Type(_, _) => panic!("MapInput::Type is not allowed!"),
            MapInput::FieldType(_, _) => panic!("MapInput::FieldType is not allowed!"),
        };

        let ty = ty.map_err(|e| GenerationError::TemplateRender("C".to_string(), e.to_string()))?;

        let kind = if self.ffi_items.contains_struct(&ident) {
            TyKind::Struct
        } else if self.ffi_items.contains_union(&ident) {
            TyKind::Union
        } else {
            TyKind::Other
        };
        self.generator.map(MapInput::Type(&ty, kind))
    }

    pub(crate) fn volatile(&self, v: VolatileItemKind) -> &str {
        if self.generator.volatile_item.is_none() {
            return "";
        }
        if self.generator.volatile_item.as_ref().unwrap()(v) {
            "volatile"
        } else {
            ""
        }
    }

    pub(crate) fn c_signature(
        &self,
        ty: &syn::Type,
        signature: &str,
    ) -> Result<String, TranslationError> {
        let sig = match ty {
            syn::Type::BareFn(f) => {
                assert!(f.lifetimes.is_none());
                let (ret, mut args, variadic) = decl2rust(&f)?;
                let abi = f
                    .abi
                    .clone()
                    .unwrap()
                    .name
                    .map(|s| s.value())
                    .unwrap_or("C".to_string());
                if variadic {
                    args.push("...".to_string());
                } else if args.is_empty() {
                    args.push("void".to_string());
                }
                format!("{}({}**{})({})", ret, abi, signature, args.join(", "))
            }
            syn::Type::Array(a) => match a.elem.deref() {
                syn::Type::Array(a2) => format!(
                    "{}(*{})[{}][{}]",
                    self.translator.translate_type(a2.elem.deref())?,
                    signature,
                    a.len.to_token_stream().to_string(),
                    a2.len.to_token_stream().to_string()
                ),
                _ => format!(
                    "{}(*{})[{}]",
                    self.translator.translate_type(a.elem.deref())?,
                    signature,
                    a.len.to_token_stream().to_string()
                ),
            },
            _ => format!(
                "{}* {}",
                self.generator
                    .map(MapInput::Type(&self.translator.translate_type(ty)?, {
                        if self
                            .ffi_items
                            .contains_struct(&ty.to_token_stream().to_string())
                        {
                            TyKind::Struct
                        } else if self
                            .ffi_items
                            .contains_union(&ty.to_token_stream().to_string())
                        {
                            TyKind::Union
                        } else {
                            TyKind::Other
                        }
                    }))
                    .unwrap(),
                signature
            ),
        };

        Ok(sig)
    }
}

/// Determine whether a C type has a sign.
pub(crate) fn has_sign(ffi_items: &FfiItems, ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Path(path) => {
            let ident = path.path.segments.last().unwrap().ident.clone();
            if let Some(aliased) = ffi_items
                .aliases()
                .iter()
                .find(|a| a.ident() == ident.to_string())
            {
                return has_sign(ffi_items, &aliased.ty);
            }
            match Translator::new().translate_primitive_type(&ident).as_str() {
                "char" | "short" | "int" | "long" | "long long" | "int8_t" | "int16_t"
                | "int32_t" | "int64_t" | "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t"
                | "size_t" | "ssize_t" => true,
                s => s.starts_with("signed ") || s.starts_with("unsigned "),
            }
        }
        _ => false,
    }
}

/// Determine whether a Rust alias/struct/union should have a round trip test.
///
/// By default all alias/struct/unions are roundtripped. Aliases or fields with arrays should
/// not be part of the roundtrip.
pub(crate) fn should_roundtrip(gen: &TestGenerator, ident: &str) -> bool {
    if gen.skip_roundtrip.is_none() {
        return true;
    }
    !(gen.skip_roundtrip.as_ref().unwrap())(ident)
}

pub(crate) fn skip_field(gen: &TestGenerator, s: &Struct, field: &Field) -> bool {
    gen.skips.iter().any(|f| f(&MapInput::Field(s, field)))
}

pub(crate) fn skip_field_type(gen: &TestGenerator, s: &Struct, field: &Field) -> bool {
    gen.skips.iter().any(|f| f(&MapInput::FieldType(s, field)))
}

fn decl2rust(decl: &syn::TypeBareFn) -> Result<(String, Vec<String>, bool), TranslationError> {
    let args = decl
        .inputs
        .iter()
        .map(|arg| Translator::new().translate_type(&arg.ty))
        .collect::<Result<Vec<_>, TranslationError>>()?;
    let ret = match &decl.output {
        syn::ReturnType::Default => "void".to_string(),
        syn::ReturnType::Type(_, ty) => match ty.deref() {
            syn::Type::Never(_) => "void".to_string(),
            syn::Type::Tuple(tuple) if tuple.elems.is_empty() => "void".to_string(),
            _ => Translator::new().translate_type(ty.deref())?,
        },
    };
    Ok((ret, args, decl.variadic.is_some()))
}
