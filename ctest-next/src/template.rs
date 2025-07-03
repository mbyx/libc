use askama::Template;
use quote::ToTokens;

use crate::{
    ffi_items::FfiItems, generator::GenerationError, translator::Translator, MapInput, Result,
    TestGenerator, TyKind,
};

/// Represents the Rust side of the generated testing suite.
#[derive(Template, Clone)]
#[template(path = "test.rs")]
pub(crate) struct RustTestTemplate<'a> {
    ffi_items: &'a FfiItems,
    translator: Translator,
    #[expect(unused)]
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
        };

        let ty = ty.map_err(|e| {
            GenerationError::TemplateRender(
                self.generator.language.extension().to_string(),
                e.to_string(),
            )
        })?;

        let kind = if self.ffi_items.contains_struct(&ident) {
            TyKind::Struct
        } else if self.ffi_items.contains_union(&ident) {
            TyKind::Union
        } else {
            TyKind::Other
        };
        self.generator.map(MapInput::Type(&ty, kind))
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
