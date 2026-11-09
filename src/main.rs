//! svgo-rs CLI: `svgo-rs [INPUT] [-o OUTPUT]`, `svgo-rs -f DIR -o DIR`,
//! or stdin -> stdout when INPUT is `-` or omitted. Output is the
//! preset-default optimized SVG with no trailing newline added.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn optimize_bytes(data: &[u8]) -> Result<Vec<u8>, String> {
    let input = String::from_utf8_lossy(data);
    Ok(svgo_rs::optimize_preset(&input)?.into_bytes())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut folder = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                output = args.get(i).cloned();
            }
            "-f" | "--folder" => folder = true,
            "-i" | "--input" => {
                i += 1;
                input = args.get(i).cloned();
            }
            "--version" => {
                println!("svgo-rs {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            other if !other.starts_with('-') || other == "-" => input = Some(other.to_string()),
            other => {
                eprintln!("svgo-rs: unknown option {other}");
                return ExitCode::from(2);
            }
        }
        i += 1;
    }

    if folder {
        let (Some(dir), Some(out)) = (input, output) else {
            eprintln!("usage: svgo-rs -f INPUT_DIR -o OUTPUT_DIR");
            return ExitCode::from(2);
        };
        return run_folder(Path::new(&dir), Path::new(&out));
    }

    let data: Vec<u8> = match input.as_deref() {
        None | Some("-") => {
            let mut buf = Vec::new();
            if std::io::stdin().read_to_end(&mut buf).is_err() {
                return ExitCode::FAILURE;
            }
            buf
        }
        Some(path) => match std::fs::read(path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("svgo-rs: {path}: {e}");
                return ExitCode::FAILURE;
            }
        },
    };
    let out = match optimize_bytes(&data) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("svgo-rs: {e}");
            return ExitCode::FAILURE;
        }
    };
    match output.as_deref() {
        None | Some("-") => {
            let _ = std::io::stdout().write_all(&out);
        }
        Some(path) => {
            if std::fs::write(path, &out).is_err() {
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

fn run_folder(dir: &Path, out_dir: &Path) -> ExitCode {
    if std::fs::create_dir_all(out_dir).is_err() {
        return ExitCode::FAILURE;
    }
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "svg")).collect(),
        Err(e) => {
            eprintln!("svgo-rs: {}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    };
    entries.sort();
    let mut failures = 0;
    for path in entries {
        let data = match std::fs::read(&path) { Ok(d) => d, Err(_) => { failures += 1; continue; } };
        match optimize_bytes(&data) {
            Ok(out) => {
                let dest = out_dir.join(path.file_name().unwrap());
                if std::fs::write(&dest, out).is_err() { failures += 1; }
            }
            Err(e) => {
                eprintln!("svgo-rs: {}: {e}", path.display());
                failures += 1;
            }
        }
    }
    if failures > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}
