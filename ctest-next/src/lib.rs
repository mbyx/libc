#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(missing_debug_implementations)]

//! # ctest2 - an FFI binding validator
//!
//! This library is intended to be used as a build dependency in a separate
//! project from the main repo to generate tests which can be used to validate
//! FFI bindings in Rust against the headers from which they come from.

#[cfg(test)]
mod tests;

mod ast;
mod ffi_items;
mod generator;
mod macro_expansion;
mod runner;
mod rustc_version;
mod template;
mod translator;

pub use ast::{Abi, Const, Field, Fn, Parameter, Static, Struct, Type, Union};
pub use generator::TestGenerator;
pub use macro_expansion::expand;
pub use runner::{compile_test, run_test};
pub use rustc_version::{rustc_version, RustcVersion};

/// A possible error that can be encountered in our library.
pub type Error = Box<dyn std::error::Error>;
/// A type alias for `std::result::Result` that defaults to our error type.
pub type Result<T, E = Error> = std::result::Result<T, E>;
/// A boxed string for representing identifiers.
type BoxStr = Box<str>;

/// The programming language being compiled into a library.
#[derive(Default, Debug, PartialEq, Eq)]
pub enum Language {
    /// The C programming language.
    #[default]
    C,
    /// The C++ programming language.
    CXX,
}

/// A kind of item to which the C volatile qualifier could apply.
#[derive(Debug)]
#[non_exhaustive]
pub enum VolatileItemKind {
    /// A struct field.
    StructField(Struct, Field),
    /// An extern static.
    Static(Static),
    /// A function argument.
    FnArgument(Fn, Box<Parameter>),
    /// Function return type.
    FnReturnType(Fn),
}
