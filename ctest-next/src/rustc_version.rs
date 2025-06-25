use std::{env, fmt::Display, num::ParseIntError, process::Command};

use crate::Result;

/// Represents the current version of the rustc compiler globally in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RustcVersion {
    major: u8,
    minor: u8,
    patch: u8,
}

impl RustcVersion {
    /// Define a rustc version with the given major.minor.patch.
    pub fn new(major: u8, minor: u8, patch: u8) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl Default for RustcVersion {
    fn default() -> Self {
        rustc_version().unwrap()
    }
}

impl Display for RustcVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RustcVersion({}, {}, {})",
            self.major, self.minor, self.patch
        )
    }
}

/// Return the global rustc version.
pub fn rustc_version() -> Result<RustcVersion> {
    let rustc = env::var("RUSTC").unwrap_or_else(|_| String::from("rustc"));

    let output = Command::new(rustc).arg("--version").output()?;

    if !output.status.success() {
        let error = std::str::from_utf8(&output.stderr)?;
        return Err(error.into());
    }

    // eg: rustc 1.87.0-(optionally nightly) (17067e9ac 2025-05-09)
    // Assume the format does not change.
    let [major, minor, patch] = std::str::from_utf8(&output.stdout)?
        .split_whitespace()
        .nth(1)
        .unwrap()
        .split('.')
        .take(3)
        .map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .trim()
                .parse::<u8>()
        })
        .collect::<Result<Vec<u8>, ParseIntError>>()?
        .try_into()
        .unwrap();

    Ok(RustcVersion::new(major, minor, patch))
}
