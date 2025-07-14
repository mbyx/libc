use std::env;
use std::fs::canonicalize;
use std::path::Path;
use std::process::Command;

use crate::{Result, TestGenerator};

/// Use rustc to expand all macros and pretty print the crate into a single file.
pub fn expand<P: AsRef<Path>>(crate_path: P, gen: &TestGenerator) -> Result<String> {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| String::from("rustc"));

    let mut cmd = Command::new(rustc);
    cmd.env("RUSTC_BOOTSTRAP", "1")
        .arg("-Zunpretty=expanded")
        .arg("--edition")
        .arg("2021") // By default, -Zunpretty=expanded uses 2015 edition.
        .arg(canonicalize(crate_path)?);

    for (k, v) in &gen.cfg {
        match v {
            None => {
                cmd.arg("--cfg").arg(k);
            }
            Some(val) => {
                cmd.arg("--cfg").arg(format!("{k}=\"{val}\""));
            }
        }
    }

    let output = cmd.output()?;

    if !output.status.success() {
        let error = std::str::from_utf8(&output.stderr)?;
        return Err(error.into());
    }

    let expanded = std::str::from_utf8(&output.stdout)?.to_string();

    Ok(expanded)
}
