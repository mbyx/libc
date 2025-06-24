use std::env;

use ctest_next::{compile_test, run_test, Result, TestGenerator};

// Headers are found relevative to the include directory, all files are generated
// relative to the output directory.

/// Create a test generator configured to useful settings.
///
/// The files will be generated in a unique temporary directory that gets
/// deleted when it goes out of scope.
fn default_generator(opt_level: u8) -> Result<(TestGenerator, tempfile::TempDir)> {
    env::set_var("OPT_LEVEL", opt_level.to_string());
    let temp_dir = tempfile::tempdir()?;
    let mut generator = TestGenerator::new();

    generator.out_dir(&temp_dir).include("tests/input");

    Ok((generator, temp_dir))
}

#[test]
fn test_entrypoint_hierarchy() {
    let crate_path = "tests/input/hierarchy/lib.rs";

    let (mut gen, out_dir) = default_generator(1).unwrap();
    gen.header("hierarchy.h")
        .generate(crate_path, "hierarchy_out")
        .unwrap();

    let test_binary = compile_test(
        out_dir.path().to_str().unwrap(),
        crate_path,
        "hierarchy_out",
    )
    .unwrap();

    assert!(run_test(test_binary.to_str().unwrap()).is_ok());
}

#[test]
fn test_entrypoint_simple() {
    let crate_path = "tests/input/simple.rs";

    let (mut gen, out_dir) = default_generator(1).unwrap();
    gen.header("simple.h")
        .generate(crate_path, "simple_out")
        .unwrap();

    let test_binary =
        compile_test(out_dir.path().to_str().unwrap(), crate_path, "simple_out").unwrap();

    assert!(run_test(test_binary.to_str().unwrap()).is_ok());
}

#[test]
fn test_entrypoint_macro() {
    let crate_path = "tests/input/macro.rs";

    let (mut gen, out_dir) = default_generator(1).unwrap();
    gen.header("macro.h")
        .generate(crate_path, "macro_out")
        .unwrap();

    let test_binary =
        compile_test(out_dir.path().to_str().unwrap(), crate_path, "macro_out").unwrap();

    assert!(run_test(test_binary.to_str().unwrap()).is_ok());
}

#[test]
fn test_entrypoint_invalid_syntax() {
    let crate_path = "tests/input/invalid_syntax.rs";
    let mut generator = TestGenerator::new();

    let fails = generator
        .generate(crate_path, "invalid_syntax_out")
        .is_err();

    assert!(fails)
}
