use std::{
    env,
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use askama::Template;
use syn::visit::Visit;
use thiserror::Error;

use crate::{
    expand,
    ffi_items::FfiItems,
    template::{CTestTemplate, RustTestTemplate},
    Const, Field, Language, MapInput, Parameter, Result, Static, Struct, TyKind, Type,
    VolatileItemKind,
};

/// A function that takes a mappable input and returns its mapping as Some, otherwise
/// use the default name if None.
type MappedName = Box<dyn Fn(&MapInput) -> Option<String>>;
/// A function that determines whether to skip an item or not.
type Skip = Box<dyn Fn(&MapInput) -> bool>;
/// A function that determines whether a variable or field is volatile.
type VolatileItem = Box<dyn Fn(VolatileItemKind) -> bool>;
/// A function that determines whether a function arument is an array.
type ArrayArg = Box<dyn Fn(crate::Fn, Parameter) -> bool>;

/// A builder used to generate a test suite.
#[derive(Default)]
#[expect(missing_debug_implementations)]
pub struct TestGenerator {
    pub(crate) headers: Vec<String>,
    pub(crate) target: Option<String>,
    pub(crate) includes: Vec<PathBuf>,
    out_dir: Option<PathBuf>,
    /// The language chosen for testing bindings.
    pub language: Language,
    flags: Vec<String>,
    defines: Vec<(String, Option<String>)>,
    mapped_names: Vec<MappedName>,
    skips: Vec<Skip>,
    verbose_skip: bool,
    volatile_item: Option<VolatileItem>,
    array_arg: Option<ArrayArg>,
}

#[derive(Debug, Error)]
pub enum GenerationError {
    #[error("unable to expand crate {0}: {1}")]
    MacroExpansion(PathBuf, String),
    #[error("unable to parse expanded crate {0}: {1}")]
    RustSyntax(String, String),
    #[error("unable to render {0} template: {1}")]
    TemplateRender(String, String),
    #[error("unable to create or write template file: {0}")]
    OsError(std::io::Error),
    #[error("unable to map Rust identifier or type")]
    ItemMap,
}

impl TestGenerator {
    /// Creates a new blank test generator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a header to be included as part of the generated C file.
    ///
    /// The generate C test will be compiled by a C compiler, and this can be
    /// used to ensure that all the necessary header files are included to test
    /// all FFI definitions.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.header("foo.h")
    ///    .header("bar.h");
    /// ```
    pub fn header(&mut self, header: &str) -> &mut Self {
        self.headers.push(header.to_string());
        self
    }

    /// Configures the target to compile C code for.
    ///
    /// Note that for Cargo builds this defaults to `$TARGET` and it's not
    /// necessary to call.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.target("x86_64-unknown-linux-gnu");
    /// ```
    pub fn target(&mut self, target: &str) -> &mut Self {
        self.target = Some(target.to_string());
        self
    }

    /// Add a path to the C compiler header lookup path.
    ///
    /// This is useful for if the C library is installed to a nonstandard
    /// location to ensure that compiling the C file succeeds.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::env;
    /// use std::path::PathBuf;
    ///
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    /// cfg.include(out_dir.join("include"));
    /// ```
    pub fn include<P: AsRef<Path>>(&mut self, p: P) -> &mut Self {
        self.includes.push(p.as_ref().to_owned());
        self
    }

    /// Configures the output directory of the generated Rust and C code.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.out_dir("path/to/output");
    /// ```
    pub fn out_dir<P: AsRef<Path>>(&mut self, p: P) -> &mut Self {
        self.out_dir = Some(p.as_ref().to_owned());
        self
    }

    /// Skipped item names are printed to `stderr` if `skip` is `true`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.verbose_skip(true);
    /// ```
    pub fn verbose_skip(&mut self, skip: bool) -> &mut Self {
        self.verbose_skip = skip;
        self
    }

    /// Is volatile?
    ///
    /// The closure given takes a `VolatileKind` denoting a particular item that
    /// could be volatile, and returns whether this is the case. This is used to
    /// make sure that the generated test code also has the volatile keyword.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::{TestGenerator, VolatileItemKind};
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.volatile_item(|item| {
    ///     match item {
    ///         VolatileItemKind::StructField(s, f)
    ///             if s.ident() == "foo_struct" && f.ident() == "foo_field"
    ///              => true,
    ///         _ => false,
    /// }});
    /// ```
    pub fn volatile_item(&mut self, f: impl Fn(VolatileItemKind) -> bool + 'static) -> &mut Self {
        self.volatile_item = Some(Box::new(f));
        self
    }

    /// Is argument of function an array?
    ///
    /// The closure denotes whether particular argument of a function is an array.
    /// This is used to figure out which pointer argument is actually an array. For
    /// example, `uint8_t*` could be a pointer to an integer or pointer to the start
    /// of an array.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.array_arg(|func, arg| {
    ///     match (func.ident(), arg.ident()) {
    ///         ("foo", "bar") => true,
    ///         _ => false,
    /// }});
    /// ```
    pub fn array_arg(&mut self, f: impl Fn(crate::Fn, Parameter) -> bool + 'static) -> &mut Self {
        self.array_arg = Some(Box::new(f));
        self
    }

    /// Configures whether the tests for a struct are emitted.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.skip_struct(|s| {
    ///     s.ident().starts_with("foo_")
    /// });
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.skip_field(|s, f| {
    ///     s.ident() == "foo_t" || (s.ident() == "bar_t" && f.ident() == "bar")
    /// });
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.skip_alias(|a| {
    ///     a.ident().starts_with("foo_")
    /// });
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.skip_const(|s| {
    ///     s.ident().starts_with("FOO_")
    /// });
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.skip_static(|s| {
    ///     s.ident().starts_with("foo_")
    /// });
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.skip_fn(|s| {
    ///     s.ident().starts_with("foo_")
    /// });
    /// ```
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
    ///
    /// This is used to generate C++ versions of the test that can be compiled
    /// and ran in the same way as C.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::env;
    /// use std::path::PathBuf;
    ///
    /// use ctest_next::{TestGenerator, Language};
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.language(Language::CXX);
    /// ```
    pub fn language(&mut self, language: Language) -> &mut Self {
        self.language = language;
        self
    }

    /// Add a flag to the C compiler invocation.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::env;
    /// use std::path::PathBuf;
    ///
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.flag("-Wno-type-limits");
    /// ```
    pub fn flag(&mut self, flag: &str) -> &mut Self {
        self.flags.push(flag.to_string());
        self
    }

    /// Set a `-D` flag for the C compiler being called.
    ///
    /// This can be used to define various variables to configure how header
    /// files are included or what APIs are exposed from header files.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.define("_GNU_SOURCE", None)
    ///    .define("_WIN32_WINNT", Some("0x8000"));
    /// ```
    pub fn define(&mut self, k: &str, v: Option<&str>) -> &mut Self {
        self.defines
            .push((k.to_string(), v.map(std::string::ToString::to_string)));
        self
    }

    /// Configures how Rust `const`s names are translated to C.
    pub fn rename_constant(&mut self, f: impl Fn(&Const) -> Option<String> + 'static) -> &mut Self {
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.rename_field(|_s, field| {
    ///     Some(field.ident().replace("foo", "bar"))
    /// });
    /// ```
    pub fn rename_field(
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::TestGenerator;
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.rename_fn(|f| Some(format!("{}_c", f.ident())));
    /// ```
    pub fn rename_fn(&mut self, f: impl Fn(&crate::Fn) -> Option<String> + 'static) -> &mut Self {
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
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ctest_next::{TestGenerator, TyKind};
    ///
    /// let mut cfg = TestGenerator::new();
    /// cfg.rename_type(|ty, kind| {
    ///     if kind == TyKind::Struct {
    ///         Some(format!("{}_t", ty))
    ///     } else {
    ///         None
    ///     }
    /// });
    /// ```
    pub fn rename_type(
        &mut self,
        f: impl Fn(&str, TyKind) -> Option<String> + 'static,
    ) -> &mut Self {
        self.mapped_names.push(Box::new(move |item| {
            if let MapInput::Type(ty, kind) = item {
                f(ty, *kind)
            } else {
                None
            }
        }));
        self
    }

    /// Generate the Rust and C testing files.
    ///
    /// Returns the path to the generated file.
    pub fn generate_files(
        &mut self,
        crate_path: impl AsRef<Path>,
        output_file_path: impl AsRef<Path>,
    ) -> Result<PathBuf, GenerationError> {
        let expanded = expand(&crate_path).map_err(|e| {
            GenerationError::MacroExpansion(crate_path.as_ref().to_path_buf(), e.to_string())
        })?;
        let ast = syn::parse_file(&expanded)
            .map_err(|e| GenerationError::RustSyntax(expanded, e.to_string()))?;

        let mut ffi_items = FfiItems::new();
        ffi_items.visit_file(&ast);

        // FIXME(ctest): Does not filter out tests for fields.
        self.filter_ffi_items(&mut ffi_items);

        let output_directory = self
            .out_dir
            .clone()
            .unwrap_or_else(|| env::var("OUT_DIR").unwrap().into());
        let output_file_path = output_directory.join(output_file_path);

        // Generate the Rust side of the tests.
        File::create(output_file_path.with_extension("rs"))
            .map_err(GenerationError::OsError)?
            .write_all(
                RustTestTemplate::new(&ffi_items, self)
                    .render()
                    .map_err(|e| {
                        GenerationError::TemplateRender("Rust".to_string(), e.to_string())
                    })?
                    .as_bytes(),
            )
            .map_err(GenerationError::OsError)?;

        // Generate the C/Cxx side of the tests.
        let c_output_path = output_file_path.with_extension(self.language.extension());
        File::create(&c_output_path)
            .map_err(GenerationError::OsError)?
            .write_all(
                CTestTemplate::new(&ffi_items, self)
                    .render()
                    .map_err(|e| {
                        GenerationError::TemplateRender(
                            self.language.display_name().to_string(),
                            e.to_string(),
                        )
                    })?
                    .as_bytes(),
            )
            .map_err(GenerationError::OsError)?;

        Ok(output_file_path)
    }

    /// Skips entire items such as structs, constants, and aliases from being tested.
    /// Does not skip specific tests or specific fields.
    fn filter_ffi_items(&self, ffi_items: &mut FfiItems) {
        let verbose = self.verbose_skip;

        macro_rules! filter {
            ($field:ident, $variant:ident, $label:literal) => {{
                let (retained, skipped): (Vec<_>, Vec<_>) = ffi_items
                    .$field
                    .drain(..)
                    .partition(|item| !self.skips.iter().any(|f| f(&MapInput::$variant(item))));
                ffi_items.$field = retained;
                if verbose {
                    skipped
                        .iter()
                        .for_each(|item| eprintln!("Skipping {} \"{}\"", $label, item.ident()));
                }
            }};
        }

        filter!(aliases, Alias, "alias");
        filter!(constants, Const, "const");
        filter!(structs, Struct, "struct");
        filter!(foreign_functions, Fn, "fn");
        filter!(foreign_statics, Static, "static");
    }

    /// Maps Rust identifiers or types to C counterparts, or defaults to the original name.
    pub(crate) fn map<'a>(&self, item: impl Into<MapInput<'a>>) -> Result<String, GenerationError> {
        let item = item.into();
        if let Some(mapped) = self.mapped_names.iter().find_map(|f| f(&item)) {
            return Ok(mapped);
        }
        Ok(match item {
            MapInput::Const(c) => c.ident().to_string(),
            MapInput::Fn(f) => f.ident().to_string(),
            MapInput::Static(s) => s.ident().to_string(),
            MapInput::Struct(s) => s.ident().to_string(),
            MapInput::Alias(t) => t.ident().to_string(),
            MapInput::Field(_, f) => f.ident().to_string(),
            MapInput::Type(ty, TyKind::Struct) => format!("struct {ty}"),
            MapInput::Type(ty, TyKind::Union) => format!("union {ty}"),
            MapInput::Type(ty, TyKind::Other) => ty.to_string(),
        })
    }
}
