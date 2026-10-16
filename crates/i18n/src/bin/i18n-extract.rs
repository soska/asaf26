//! Writes `locales/en.json` and `locales/en.meta.json` from the source;
//! `--check` only compares them and fails when they're out of date.
//!
//! ```sh
//! cargo run -p asaf-i18n --bin i18n-extract
//! cargo run -p asaf-i18n --bin i18n-extract -- --check
//! ```

use std::path::Path;
use std::process::ExitCode;

use asaf_i18n::extract;

fn main() -> ExitCode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let check = std::env::args().skip(1).any(|a| a == "--check");
    let result = if check { extract::check(&root) } else { extract::write(&root) };
    match result {
        Ok(x) => {
            let verb = if check { "up to date" } else { "written" };
            println!(
                "{} and {} {verb}: {} files, {} calls, {} messages",
                extract::CATALOG,
                extract::META,
                x.files,
                x.calls,
                x.catalog.len()
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
