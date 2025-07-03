use askama::Template;
use quote::ToTokens;

use crate::{
    ffi_items::FfiItems, generator::GenerationError, translator::Translator, MapInput, Result,
    TestGenerator,
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
            MapInput::Const(c) => (c.ident(), self.translator.translate_type(&c.ty)),
            MapInput::Alias(a) => (a.ident(), self.translator.translate_type(&a.ty)),
            MapInput::Field(_, f) => (f.ident(), self.translator.translate_type(&f.ty)),
            MapInput::Static(s) => (s.ident(), self.translator.translate_type(&s.ty)),
            MapInput::Fn(_) => unimplemented!(),
            MapInput::Struct(_) => unimplemented!(),
            MapInput::Type(_, _, _) => panic!("MapInput::Type is not allowed!"),
        };

        let ty = ty.map_err(|e| {
            GenerationError::TemplateRender(
                self.generator.language.extension().to_string(),
                e.to_string(),
            )
        })?;

        self.generator.map(MapInput::Type(
            &ty,
            self.ffi_items.contains_struct(ident),
            self.ffi_items.contains_union(ident),
        ))
    }
}
