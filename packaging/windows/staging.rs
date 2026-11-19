//! File operations for the Windows launcher. CMD owns publication and cleanup.
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::process::ExitCode;

// Keep application statuses distinct from common CMD process-launch failures.
const OCCUPIED: u8 = 20;
const FILE_ERROR: u8 = 21;
const NO_TEXT: u8 = 22;

fn has_text(path: &Path) -> io::Result<bool> {
    for line in BufReader::new(File::open(path)?).lines() {
        let line = line?;
        let page_marker = line
            .strip_prefix("<!-- Page ")
            .and_then(|tail| tail.strip_suffix(" -->"))
            .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()));
        if !page_marker && !line.trim().is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let (Some(command), Some(path), None) = (args.next(), args.next(), args.next()) else {
        return ExitCode::from(2);
    };
    let code = match command.to_str() {
        Some("reserve") => match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(_) => 0,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => OCCUPIED,
            Err(_) => FILE_ERROR,
        },
        Some("has-text") => match has_text(Path::new(&path)) {
            Ok(true) => 0,
            Ok(false) => NO_TEXT,
            Err(_) => FILE_ERROR,
        },
        _ => 2,
    };
    ExitCode::from(code)
}
