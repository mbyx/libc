use std::{
    env,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use askama::Template;
use syn::visit::Visit;

use crate::{
    expand,
    ffi_items::FfiItems,
    template::{CTestTemplate, RustTestTemplate},
    Const, Field, Language, Result, Static, Struct, Type,
};
/// Inputs needed to rename or skip a field.
#[expect(dead_code)]
#[derive(Debug, Clone)]
pub(crate) enum MapInput<'a> {
    Struct(&'a Struct),
    Fn(&'a crate::Fn),
    Field(&'a Struct, &'a Field),
    Alias(&'a Type),
    Const(&'a Const),
    Static(&'a Static),
    Type(&'a str, bool, bool),
}

type MappedName = Box<dyn Fn(&MapInput) -> Option<String>>;
type Skip = Box<dyn Fn(&MapInput) -> bool>;

/// A builder used to generate a test suite.
#[non_exhaustive]
#[derive(Default)]
#[expect(missing_debug_implementations)]
pub struct TestGenerator {
    headers: Vec<String>,
    target: Option<String>,
    host: Option<String>,
    includes: Vec<PathBuf>,
    out_dir: Option<PathBuf>,
    language: Language,
    flags: Vec<String>,
    defines: Vec<(String, Option<String>)>,
    mapped_names: Vec<MappedName>,
    skips: Vec<Skip>,
}

impl TestGenerator {
    /// Creates a new blank test generator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a header to be included as part of the generated C file.
    pub fn header(&mut self, header: &str) -> &mut Self {
        self.headers.push(header.to_string());
        self
    }

    /// Configures the target to compile C code for.
    pub fn target(&mut self, target: &str) -> &mut Self {
        self.target = Some(target.to_string());
        self
    }

    /// Configures the host.
    pub fn host(&mut self, host: &str) -> &mut Self {
        self.host = Some(host.to_string());
        self
    }

    /// Add a path to the C compiler header lookup path.
    ///
    /// This is useful for if the C library is installed to a nonstandard
    /// location to ensure that compiling the C file succeeds.
    pub fn include<P: AsRef<Path>>(&mut self, p: P) -> &mut Self {
        self.includes.push(p.as_ref().to_owned());
        self
    }

    /// Configures the output directory of the generated Rust and C code.
    pub fn out_dir<P: AsRef<Path>>(&mut self, p: P) -> &mut Self {
        self.out_dir = Some(p.as_ref().to_owned());
        self
    }

    /// Configures whether the tests for a struct are emitted.
    pub fn skip_struct(&mut self, f: impl Fn(&Struct) -> bool + 'static) -> &mut Self {
        self.skips.push(Box::new(move |item| {
            if let MapInput::Struct(struct_) = item {
                f(struct_)
            } else {
                false
            }
        }));
        self
    }

    /// Configures whether all tests for a field are skipped or not.
    pub fn skip_field(&mut self, f: impl Fn(&Struct, &Field) -> bool + 'static) -> &mut Self {
        self.skips.push(Box::new(move |item| {
            if let MapInput::Field(struct_, field) = item {
                f(struct_, field)
            } else {
                false
            }
        }));
        self
    }

    /// Configures whether all tests for a typedef are skipped or not.
    pub fn skip_alias(&mut self, f: impl Fn(&Type) -> bool + 'static) -> &mut Self {
        self.skips.push(Box::new(move |item| {
            if let MapInput::Alias(alias) = item {
                f(alias)
            } else {
                false
            }
        }));
        self
    }

    /// Configures whether the tests for a constant's value are generated.
    pub fn skip_const(&mut self, f: impl Fn(&Const) -> bool + 'static) -> &mut Self {
        self.skips.push(Box::new(move |item| {
            if let MapInput::Const(constant) = item {
                f(constant)
            } else {
                false
            }
        }));
        self
    }

    /// Configures whether the tests for a static definition are generated.
    pub fn skip_static(&mut self, f: impl Fn(&Static) -> bool + 'static) -> &mut Self {
        self.skips.push(Box::new(move |item| {
            if let MapInput::Static(static_) = item {
                f(static_)
            } else {
                false
            }
        }));
        self
    }

    /// Configures whether tests for a function definition are generated.
    pub fn skip_fn(&mut self, f: impl Fn(&crate::Fn) -> bool + 'static) -> &mut Self {
        self.skips.push(Box::new(move |item| {
            if let MapInput::Fn(func) = item {
                f(func)
            } else {
                false
            }
        }));
        self
    }

    /// Sets the programming language.
    pub fn language(&mut self, language: Language) -> &mut Self {
        self.language = language;
        self
    }

    /// Add a flag to the C compiler invocation.
    pub fn flag(&mut self, flag: &str) -> &mut Self {
        self.flags.push(flag.to_string());
        self
    }

    /// Set a `-D` flag for the C compiler being called.
    ///
    /// This can be used to define various variables to configure how header
    /// files are included or what APIs are exposed from header files.
    pub fn define(&mut self, k: &str, v: Option<&str>) -> &mut Self {
        self.defines
            .push((k.to_string(), v.map(std::string::ToString::to_string)));
        self
    }

    /// Configures how Rust `const`s names are translated to C.
    pub fn map_constant(&mut self, f: impl Fn(&Const) -> Option<String> + 'static) -> &mut Self {
        self.mapped_names.push(Box::new(move |item| {
            if let MapInput::Const(c) = item {
                f(c)
            } else {
                None
            }
        }));
        self
    }

    /// Configures how a Rust struct field is translated to a C struct field.
    pub fn map_field(
        &mut self,
        f: impl Fn(&Struct, &Field) -> Option<String> + 'static,
    ) -> &mut Self {
        self.mapped_names.push(Box::new(move |item| {
            if let MapInput::Field(s, c) = item {
                f(s, c)
            } else {
                None
            }
        }));
        self
    }

    /// Configures the name of a function in the generated C code.
    pub fn map_fn(&mut self, f: impl Fn(&crate::Fn) -> Option<String> + 'static) -> &mut Self {
        self.mapped_names.push(Box::new(move |item| {
            if let MapInput::Fn(func) = item {
                f(func)
            } else {
                None
            }
        }));
        self
    }

    /// Configures how a Rust type is translated to a C type.
    pub fn map_type(
        &mut self,
        f: impl Fn(&str, bool, bool) -> Option<String> + 'static,
    ) -> &mut Self {
        self.mapped_names.push(Box::new(move |item| {
            if let MapInput::Type(ty, is_struct, is_union) = item {
                f(ty, *is_struct, *is_union)
            } else {
                None
            }
        }));
        self
    }

    /// Generate all tests for the given crate and output the Rust side to a file.
    pub fn generate<P: AsRef<Path>>(&mut self, crate_path: P, output_file_path: P) -> Result<()> {
        let output_file_path = self.generate_files(crate_path, output_file_path)?;

        let target = self
            .target
            .clone()
            .unwrap_or(env::var("TARGET_PLATFORM").unwrap());
        let host = self
            .host
            .clone()
            .unwrap_or(env::var("HOST_PLATFORM").unwrap());

        let mut cfg = cc::Build::new();
        if let Language::CXX = self.language {
            cfg.cpp(true);
        }

        let extension = match self.language {
            Language::C => "c",
            Language::CXX => "cpp",
        };

        cfg.file(output_file_path.with_extension(extension));
        cfg.host(&host);
        if target.contains("msvc") {
            cfg.flag("/W3")
                .flag("/Wall")
                .flag("/WX")
                // ignored warnings
                .flag("/wd4820") // warning about adding padding?
                .flag("/wd4100") // unused parameters
                .flag("/wd4996") // deprecated functions
                .flag("/wd4296") // '<' being always false
                .flag("/wd4255") // converting () to (void)
                .flag("/wd4668") // using an undefined thing in preprocessor?
                .flag("/wd4366") // taking ref to packed struct field might be unaligned
                .flag("/wd4189") // local variable initialized but not referenced
                .flag("/wd4710") // function not inlined
                .flag("/wd5045") // compiler will insert Spectre mitigation
                .flag("/wd4514") // unreferenced inline function removed
                .flag("/wd4711"); // function selected for automatic inline
        } else {
            cfg.flag("-Wall")
                .flag("-Wextra")
                .flag("-Werror")
                .flag("-Wno-unused-parameter")
                .flag("-Wno-type-limits")
                // allow taking address of packed struct members:
                .flag("-Wno-address-of-packed-member")
                .flag("-Wno-unknown-warning-option")
                .flag("-Wno-deprecated-declarations"); // allow deprecated items
        }

        for flag in &self.flags {
            cfg.flag(flag);
        }

        for (a, b) in &self.defines {
            cfg.define(a, b.as_ref().map(|s| &s[..]));
        }

        for p in &self.includes {
            cfg.include(p);
        }

        let stem: &str = output_file_path.file_stem().unwrap().to_str().unwrap();
        cfg.target(&target)
            .out_dir(output_file_path.parent().unwrap())
            .compile(stem);

        Ok(())
    }

    /// Generate the Rust and C testing files.
    ///
    /// Returns the path to the generated file.
    pub(crate) fn generate_files<P: AsRef<Path>>(
        &mut self,
        crate_path: P,
        output_file_path: P,
    ) -> Result<PathBuf> {
        let expanded = expand(crate_path)?;
        let ast = syn::parse_file(&expanded)?;

        let mut ffi_items = FfiItems::new();
        ffi_items.visit_file(&ast);

        // FIXME: Does not filter out tests for fields.
        self.filter_ffi_items(&mut ffi_items);

        let output_directory = self
            .out_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(env::var_os("OUT_DIR").unwrap()));
        let output_file_path = output_directory.join(output_file_path);

        // Generate the Rust side of the tests.
        File::create(output_file_path.with_extension("rs"))?.write_all(
            RustTestTemplate::new(&ffi_items, self)?
                .render()?
                .as_bytes(),
        )?;

        let extension = match self.language {
            Language::C => "c",
            Language::CXX => "cpp",
        };

        // Generate the C/Cxx side of the tests.
        let c_output_path = output_file_path.with_extension(extension);
        let headers = self.headers.iter().map(|h| h.as_str()).collect();
        File::create(&c_output_path)?.write_all(
            CTestTemplate::new(headers, &ffi_items, self)
                .render()?
                .as_bytes(),
        )?;

        Ok(output_file_path)
    }

    /// Skips entire items such as structs, constants and aliases from being tested.
    ///
    /// This method is not responsible for skipping any specific tests or for skipping tests for
    /// specific fields.
    fn filter_ffi_items(&self, ffi_items: &mut FfiItems) {
        ffi_items
            .aliases
            .retain(|ty| !self.skips.iter().any(|f| f(&MapInput::Alias(ty))));
        ffi_items
            .constants
            .retain(|ty| !self.skips.iter().any(|f| f(&MapInput::Const(ty))));
        ffi_items
            .structs
            .retain(|ty| !self.skips.iter().any(|f| f(&MapInput::Struct(ty))));
        ffi_items
            .foreign_functions
            .retain(|ty| !self.skips.iter().any(|f| f(&MapInput::Fn(ty))));
        ffi_items
            .foreign_statics
            .retain(|ty| !self.skips.iter().any(|f| f(&MapInput::Static(ty))));
    }

    /// Maps Rust identifiers or types of items to their C counterparts if specified, otherwise defaults to original.
    pub(crate) fn map(&self, item: MapInput) -> String {
        let found = self.mapped_names.iter().find_map(|f| f(&item));
        if let Some(s) = found {
            return s;
        }
        match item {
            MapInput::Const(c) => c.ident().to_string(),
            MapInput::Fn(f) => f.ident().to_string(),
            MapInput::Static(s) => s.ident().to_string(),
            MapInput::Struct(s) => s.ident().to_string(),
            MapInput::Alias(t) => t.ident().to_string(),
            MapInput::Field(_, f) => f.ident().to_string(),
            MapInput::Type(ty, is_struct, is_union) => {
                if is_struct {
                    format!("struct {ty}")
                } else if is_union {
                    format!("union {ty}")
                } else {
                    ty.to_string()
                }
            }
        }
    }
}
