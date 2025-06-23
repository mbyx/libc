use crate::Result;
use std::env;
use std::fs::{canonicalize, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Compiles a Rust source file and links it against a static library.
///
/// Returns the path to the generated binary.
pub fn compile_test<P: AsRef<Path>>(
    output_dir: P,
    crate_path: P,
    library_file: P,
) -> Result<PathBuf> {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let output_dir = output_dir.as_ref();
    let crate_path = crate_path.as_ref();
    let library_file = library_file.as_ref();

    let rust_file = output_dir
        .join(crate_path.file_stem().unwrap())
        .with_extension("rs");
    let binary_path = output_dir.join(rust_file.file_stem().unwrap());

    File::create(&rust_file)?.write_all(
        format!(
            "include!(r#\"{}\"#);\ninclude!(r#\"{}.rs\"#);",
            canonicalize(crate_path)?.display(),
            library_file.display()
        )
        .as_bytes(),
    )?;

    let mut cmd = Command::new(rustc);
    cmd.arg(&rust_file)
        .arg(format!("-Lnative={}", output_dir.display()))
        .arg(format!(
            "-lstatic={}",
            library_file.file_stem().unwrap().to_str().unwrap()
        ))
        .arg("--target")
        .arg(env::var("TARGET_PLATFORM").unwrap())
        .arg("-o")
        .arg(&binary_path)
        .arg("-Aunused");

    let linker = env::var("LINKER").unwrap_or_default();
    if !linker.is_empty() {
        cmd.arg(format!("-Clinker={linker}"));
    }

    let flags = env::var("FLAGS").unwrap_or_default();
    if !flags.is_empty() {
        cmd.args(flags.split_whitespace());
    }

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(std::str::from_utf8(&output.stderr)?.into());
    }

    Ok(binary_path)
}

/// Executes the compiled test binary and returns its output.
pub fn run_test(test_binary: &str) -> Result<String> {
    let runner = env::var("RUNNER").unwrap_or_default();
    let output = if runner.is_empty() {
        Command::new(test_binary).output()?
    } else {
        let mut args = runner.split_whitespace();
        let mut cmd = Command::new(args.next().unwrap());
        cmd.args(args).arg(test_binary).output()?
    };

    if !output.status.success() {
        return Err(std::str::from_utf8(&output.stderr)?.into());
    }

    // The template prints to stderr regardless.
    Ok(std::str::from_utf8(&output.stderr)?.to_string())
}
