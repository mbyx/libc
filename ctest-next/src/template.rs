use askama::Template;
use quote::ToTokens;

use crate::{
    ffi_items::FfiItems, generator::MapInput, rustc_version, translator::Translator, Result,
    RustcVersion, TestGenerator,
};

/// Represents the Rust side of the generated testing suite.
#[derive(Template, Clone)]
#[template(path = "test.rs")]
pub(crate) struct RustTestTemplate<'a> {
    rustc_version: RustcVersion,
    ffi_items: &'a FfiItems,
    generator: &'a TestGenerator,
}

/// Represents the C side of the generated testing suite.
#[derive(Template, Clone)]
#[template(path = "test.c")]
pub(crate) struct CTestTemplate<'a> {
    headers: Vec<&'a str>,
    ffi_items: &'a FfiItems,
    generator: &'a TestGenerator,
}

impl<'a> RustTestTemplate<'a> {
    /// Create a new test template to test the given items.
    pub(crate) fn new(ffi_items: &'a FfiItems, generator: &'a TestGenerator) -> Result<Self> {
        Ok(Self {
            ffi_items,
            rustc_version: rustc_version()?,
            generator,
        })
    }
}

impl<'a> CTestTemplate<'a> {
    /// Create a new test template to test the given items.
    pub(crate) fn new(
        headers: Vec<&'a str>,
        ffi_items: &'a FfiItems,
        generator: &'a TestGenerator,
    ) -> Self {
        Self {
            headers,
            ffi_items,
            generator,
        }
    }
}
