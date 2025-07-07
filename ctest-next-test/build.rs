use ctest_next::{generate_test, Fn, Parameter, TestGenerator, TyKind, VolatileItemKind};
use std::env;

fn main() {
    let opt_level = env::var("OPT_LEVEL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let profile = env::var("PROFILE").unwrap_or_default();
    if profile == "release" || opt_level >= 2 {
        println!("cargo:rustc-cfg=optimized");
    }

    // FIXME(ctest): Entirely possible that .c files are ignored right now.
    // cc::Build::new()
    //     .include("src")
    //     .warnings(false)
    //     .file("src/t1.c")
    //     .compile("libt1.a");
    // println!("cargo:rerun-if-changed=src/t1.c");
    // println!("cargo:rerun-if-changed=src/t1.h");

    // cc::Build::new()
    //     .warnings(false)
    //     .file("src/t2.c")
    //     .compile("libt2.a");
    // println!("cargo:rerun-if-changed=src/t2.c");
    // println!("cargo:rerun-if-changed=src/t2.h");

    let mut gen = TestGenerator::new();
    gen.header("t1.h")
        .include("src")
        .rename_fn(|f| f.link_name().unwrap_or(f.ident()).to_string().into())
        .rename_type(|ty, kind| {
            Some(match (ty, kind) {
                ("T1Union", _) => ty.to_string(),
                ("Transparent", _) => ty.to_string(),
                ("timeval", _) => ty.to_string(),
                ("log_record_t", _) => ty.to_string(),
                ("LongDoubleWrap", _) => ty.to_string(),
                (t, TyKind::Union) => format!("union {t}"),
                (t, TyKind::Struct) => format!("struct {t}"),
                (t, _) => t.to_string(),
            })
        })
        // FIXME(ctest): This could be removed if we filter by `pub` in ffi_items.
        .skip_const(|c| c.ident() == "NOT_PRESENT")
        .skip_struct(|s| s.ident() == "timeval")
        .skip_struct(|s| s.ident() == "log_record_t")
        .volatile_item(t1_volatile)
        .array_arg(t1_arrays)
        .skip_roundtrip(|n| n == "Arr");
    generate_test(&mut gen, "src/t1.rs", "t1gen.rs").unwrap();

    let mut gen = TestGenerator::new();
    gen.header("t2.h")
        .include("src")
        .rename_type(|ty, kind| {
            Some(match (ty, kind) {
                ("T2Union", _) => ty.to_string(),
                (t, TyKind::Struct) => format!("struct {t}"),
                (t, TyKind::Union) => format!("union {t}"),
                (t, _) => t.to_string(),
            })
        })
        .skip_roundtrip(|_| true);
    generate_test(&mut gen, "src/t2.rs", "t2gen.rs").unwrap();
}

fn t1_volatile(item: VolatileItemKind) -> bool {
    use VolatileItemKind::*;
    match item {
        StructField(s, f) if s.ident() == "V" && f.ident() == "v" => true,
        Static(s) if s.ident() == "vol_ptr" => true,
        Static(s) if s.ident() == "T1_fn_ptr_vol" => true,
        FnArgument(f, p) if f.ident() == "T1_vol0" && p.ident() == "arg0" => true,
        FnArgument(f, p) if f.ident() == "T1_vol2" && p.ident() == "arg1" => true,
        FnReturnType(f) if f.ident() == "T1_vol1" || f.ident() == "T1_vol2" => true,
        _ => false,
    }
}

fn t1_arrays(f: Fn, p: Parameter) -> bool {
    // The parameter `a` of the functions `T1r`, `T1s`, `T1t`, `T1v` is an array.
    p.ident() == "a" && matches!(f.ident(), "T1r" | "T1s" | "T1t" | "T1v")
}
