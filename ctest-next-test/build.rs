use ctest_next::{generate_test, Fn, Language, Parameter, TestGenerator, TyKind, VolatileItemKind};
use std::process::Command;

fn main() {
    use std::env;
    let opt_level = env::var("OPT_LEVEL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let profile = env::var("PROFILE").unwrap_or_default();
    if profile == "release" || opt_level >= 2 {
        println!("cargo:rustc-cfg=optimized");
    }

    cc::Build::new()
        .include("src")
        .warnings(false)
        .file("src/t1.c")
        .compile("libt1.a");
    println!("cargo:rerun-if-changed=src/t1.c");
    println!("cargo:rerun-if-changed=src/t1.h");

    cc::Build::new()
        .warnings(false)
        .file("src/t2.c")
        .compile("libt2.a");
    println!("cargo:rerun-if-changed=src/t2.c");
    println!("cargo:rerun-if-changed=src/t2.h");

    let mut gen = TestGenerator::new();
    gen.header("t1.h")
        .include("src")
        // link_name.unwrap_or(rust_name) was used, check if needed.
        .rename_fn(|a| Some(a.ident().to_string()))
        .rename_type(|ty, kind| {
            Some(match (ty, kind) {
                ("T1Union", _) => ty.to_string(),
                ("Transparent", _) => ty.to_string(),
                (t, TyKind::Union) => format!("union {t}"),
                (t, TyKind::Struct) => format!("struct {t}"),
                (t, k) => {
                    println!("{t} :: {k:?}");
                    t.to_string()
                }
            })
        })
        .volatile_item(t1_volatile)
        .array_arg(t1_arrays);
    // .skip_roundtrip(|n| n == "Arr")
    generate_test(&mut gen, "src/t1.rs", "t1gen.rs").unwrap();

    let mut gen = TestGenerator::new();
    gen.header("t2.h").include("src").rename_type(|ty, kind| {
        Some(match (ty, kind) {
            ("T2Union", _) => ty.to_string(),
            (t, TyKind::Struct) => format!("struct {t}"),
            (t, TyKind::Union) => format!("union {t}"),
            (t, _) => t.to_string(),
        })
    });
    // .skip_roundtrip(|_| true)
    generate_test(&mut gen, "src/t2.rs", "t2gen.rs").unwrap();

    println!("cargo::rustc-check-cfg=cfg(has_cxx)");
    if !cfg!(unix) || Command::new("c++").arg("v").output().is_ok() {
        // A C compiler is always available, but these are only run if a C++ compiler is
        // also available.
        println!("cargo::rustc-cfg=has_cxx");

        let mut gen = TestGenerator::new();
        gen.header("t1.h")
            .language(Language::CXX)
            .include("src")
            // link_name.unwrap_or(rust_name) was used, check if needed.
            .rename_fn(|a| Some(a.ident().to_string()))
            .rename_type(|ty, kind| {
                Some(match (ty, kind) {
                    ("T1Union", _) => ty.to_string(),
                    ("Transparent", _) => ty.to_string(),
                    (t, TyKind::Union) => format!("union {t}"),
                    (t, TyKind::Struct) => format!("struct {t}"),
                    (t, _) => t.to_string(),
                })
            })
            .volatile_item(t1_volatile)
            .array_arg(t1_arrays);
        // .skip_roundtrip(|n| n == "Arr")
        generate_test(&mut gen, "src/t1.rs", "t1gen_cxx.rs").unwrap();

        let mut gen = TestGenerator::new();
        gen.header("t2.h")
            .language(Language::CXX)
            .include("src")
            .rename_type(|ty, kind| {
                Some(match (ty, kind) {
                    ("T2Union", _) => ty.to_string(),
                    (t, TyKind::Struct) => format!("struct {t}"),
                    (t, TyKind::Union) => format!("union {t}"),
                    (t, _) => t.to_string(),
                })
            });
        // .skip_roundtrip(|_| true)
        generate_test(&mut gen, "src/t2.rs", "t2gen_cxx.rs").unwrap();
    } else {
        println!("cargo::warning=skipping C++ tests");
    }
}

fn t1_volatile(i: VolatileItemKind) -> bool {
    use VolatileItemKind::*;
    match i {
        StructField(n, f) if n.ident() == "V" && f.ident() == "v" => true,
        Static(n) if n.ident() == "vol_ptr" => true,
        // These were 0, 1 respectively, check if it's needed.
        FnArgument(n, _) if n.ident() == "T1_vol0" => true,
        FnArgument(n, _) if n.ident() == "T1_vol2" => true,
        FnReturnType(n) if n.ident() == "T1_vol1" || n.ident() == "T1_vol2" => true,
        Static(n) if n.ident() == "T1_fn_ptr_vol" => true,
        _ => false,
    }
}

fn t1_arrays(n: Fn, _i: Parameter) -> bool {
    // i == 0 && originally.
    matches!(n.ident(), "T1r" | "T1s" | "T1t" | "T1v")
}
