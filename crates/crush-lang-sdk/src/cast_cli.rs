//! `crushc cast …` (and `crush cast …`): the CAST file tools (#365).
//!
//! Ported from exosphere's `ant crush cast` handler
//! (`crates/exo/cli/src/handlers/cast.rs`), retargeted at this repo's
//! `crush_cast::Program`:
//!
//! ```text
//! crushc cast validate FILE            schema + version check (.cast.json, .cast, .castb)
//! crushc cast pack FILE [-o OUT]       CAST JSON → CBOR (.castb)
//! crushc cast unpack FILE [-o OUT]     CBOR → CAST JSON (stdout without -o)
//! crushc cast format FILE [--in-place] canonical JSON form
//! crushc cast diff LEFT RIGHT          semantic diff; exit 1 when they differ
//! ```
//!
//! Every load goes through the `cast_version` gate (`Program::deserialize`).
//! Exit codes: 0 ok, 1 invalid / different, 2 usage or I/O error.

use std::path::{Path, PathBuf};

use crush_cast::{Format, Program, validate_json};

const USAGE: &str = "\
usage: crushc cast <command> …
  validate FILE              check a CAST file (.cast.json, .cast, .castb): schema and cast_version
  pack FILE [-o OUT]         CAST JSON -> CBOR (default OUT: <stem>.castb)
  unpack FILE [-o OUT]       CBOR -> CAST JSON (default: stdout)
  format FILE [--in-place]   print (or rewrite) the canonical JSON form
  diff LEFT RIGHT            semantic diff; exit 1 when the programs differ";

/// Run `crushc cast ARGS…`; returns the process exit code.
pub fn main(args: &[String]) -> i32 {
    match run(args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("crushc cast: {e:#}");
            2
        }
    }
}

fn run(args: &[String]) -> anyhow::Result<i32> {
    let Some((cmd, rest)) = args.split_first() else {
        eprintln!("{USAGE}");
        return Ok(2);
    };
    match cmd.as_str() {
        "validate" => validate(&one_file(rest, "validate")?),
        "pack" => {
            let (file, out) = file_and_output(rest, "pack")?;
            pack(&file, out.as_deref())
        }
        "unpack" => {
            let (file, out) = file_and_output(rest, "unpack")?;
            unpack(&file, out.as_deref())
        }
        "format" => {
            let in_place = rest.iter().any(|a| a == "--in-place" || a == "-i");
            let files: Vec<&String> =
                rest.iter().filter(|a| *a != "--in-place" && *a != "-i").collect();
            match files.as_slice() {
                [file] => format(Path::new(file.as_str()), in_place),
                _ => anyhow::bail!("usage: crushc cast format FILE [--in-place]"),
            }
        }
        "diff" => match rest {
            [left, right] => diff(Path::new(left), Path::new(right)),
            // git's external-diff calling convention (7 args).
            [_path, old, _, _, new, _, _] => diff(Path::new(old), Path::new(new)),
            _ => anyhow::bail!("usage: crushc cast diff LEFT RIGHT"),
        },
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(0)
        }
        other => {
            eprintln!("crushc cast: unknown command '{other}'\n{USAGE}");
            Ok(2)
        }
    }
}

fn one_file(rest: &[String], cmd: &str) -> anyhow::Result<PathBuf> {
    match rest {
        [file] => Ok(PathBuf::from(file)),
        _ => anyhow::bail!("usage: crushc cast {cmd} FILE"),
    }
}

fn file_and_output(rest: &[String], cmd: &str) -> anyhow::Result<(PathBuf, Option<PathBuf>)> {
    let mut file = None;
    let mut out = None;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" | "--output" => {
                out = Some(PathBuf::from(
                    it.next().ok_or_else(|| anyhow::anyhow!("{a} needs a file"))?,
                ))
            }
            _ if file.is_none() => file = Some(PathBuf::from(a)),
            _ => anyhow::bail!("usage: crushc cast {cmd} FILE [-o OUT]"),
        }
    }
    Ok((file.ok_or_else(|| anyhow::anyhow!("usage: crushc cast {cmd} FILE [-o OUT]"))?, out))
}

fn read(path: &Path) -> anyhow::Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| anyhow::anyhow!("cannot read '{}': {e}", path.display()))
}

/// Load either encoding through the version gate.
fn load(path: &Path) -> anyhow::Result<Program> {
    Program::deserialize(&read(path)?, Format::from_path(path))
        .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))
}

fn validate(path: &Path) -> anyhow::Result<i32> {
    let data = read(path)?;
    if Format::from_path(path) == Format::Json {
        let text = String::from_utf8_lossy(&data);
        if let Err(errors) = validate_json(&text) {
            eprintln!("{}: invalid CAST JSON:", path.display());
            for e in &errors {
                eprintln!("{e}");
            }
            eprintln!("schema reference: docs/cast/schema-reference.md");
            return Ok(1);
        }
    }
    // Binary: decoding enforces the schema; both: the cast_version gate.
    match Program::deserialize(&data, Format::from_path(path)) {
        Ok(p) => {
            let kind = if Format::from_path(path) == Format::Binary { "binary" } else { "JSON" };
            println!(
                "{}: valid CAST {kind} (cast_version {}, {} function(s))",
                path.display(),
                p.cast_version,
                p.functions.len()
            );
            Ok(0)
        }
        Err(e) => {
            eprintln!("{}: invalid CAST: {e}", path.display());
            Ok(1)
        }
    }
}

/// `foo.cast.json` → `foo.castb`; anything else gets its extension swapped.
fn default_pack_output(file: &Path) -> PathBuf {
    crate::formats::artifact_path(file, "castb")
}

fn pack(file: &Path, output: Option<&Path>) -> anyhow::Result<i32> {
    let data = read(file)?;
    let program = Program::deserialize(&data, Format::Json)
        .map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    let cbor = program.serialize(Format::Binary).map_err(|e| anyhow::anyhow!("{e}"))?;
    let out = output.map(Path::to_path_buf).unwrap_or_else(|| default_pack_output(file));
    std::fs::write(&out, &cbor).map_err(|e| anyhow::anyhow!("cannot write '{}': {e}", out.display()))?;
    println!("{} ({} bytes) → {} ({} bytes)", file.display(), data.len(), out.display(), cbor.len());
    Ok(0)
}

fn unpack(file: &Path, output: Option<&Path>) -> anyhow::Result<i32> {
    let program = Program::deserialize(&read(file)?, Format::Binary)
        .map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    let json = program.serialize(Format::Json).map_err(|e| anyhow::anyhow!("{e}"))?;
    match output {
        Some(out) if out.as_os_str() != "-" => {
            std::fs::write(out, &json)
                .map_err(|e| anyhow::anyhow!("cannot write '{}': {e}", out.display()))?;
            println!("{} → {} ({} bytes)", file.display(), out.display(), json.len());
        }
        _ => println!("{}", String::from_utf8_lossy(&json)),
    }
    Ok(0)
}

fn format(file: &Path, in_place: bool) -> anyhow::Result<i32> {
    let canonical = crush_cast::format::canonical_form(&load(file)?);
    if in_place {
        std::fs::write(file, &canonical)
            .map_err(|e| anyhow::anyhow!("cannot write '{}': {e}", file.display()))?;
    } else {
        println!("{canonical}");
    }
    Ok(0)
}

fn diff(left: &Path, right: &Path) -> anyhow::Result<i32> {
    let changes = crush_cast::diff::diff_programs(&load(left)?, &load(right)?);
    for change in &changes {
        println!("{change}");
    }
    Ok(if changes.is_empty() { 0 } else { 1 })
}
