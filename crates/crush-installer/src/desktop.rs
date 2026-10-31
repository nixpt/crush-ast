//! Opt-in desktop integration (CRUSH-366): file-type identification for the
//! Crush formats on freedesktop systems.
//!
//! `install --desktop-integration` copies the shared-mime-info package to
//! `$XDG_DATA_HOME/mime/packages/crush.xml` (refreshing the database with
//! `update-mime-database` when it is on `PATH`) and the `magic(5)` rules to
//! `$XDG_DATA_HOME/crush/crush.magic`, then prints how to hook the latter into
//! `file(1)`. Nothing outside the user's own data dir is touched, and
//! `~/.magic` is deliberately not edited. What was written is recorded in
//! `<prefix>/.crush/desktop-integration.json` so `uninstall` can undo exactly it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// The files shipped to users. `contrib/` in the repository is the editable
/// source; a unit test keeps these copies identical to it.
pub const MIME_XML: &str = include_str!("../assets/crush.xml");
pub const MAGIC: &str = include_str!("../assets/crush.magic");

const RECORD_FILE: &str = "desktop-integration.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Record {
    files: Vec<PathBuf>,
    /// The `mime/` directory `update-mime-database` was run on.
    mime_root: Option<PathBuf>,
}

fn record_path(prefix: &Path) -> PathBuf {
    prefix.join(".crush").join(RECORD_FILE)
}

/// `$XDG_DATA_HOME`, else `$HOME/.local/share` (the XDG default).
pub fn data_home() -> Result<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_DATA_HOME").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(d));
    }
    let home = dirs::home_dir().context("cannot locate $HOME for the XDG data dir")?;
    Ok(home.join(".local/share"))
}

/// Run `update-mime-database <mime_root>`; `Ok(false)` when it is not installed.
fn refresh_mime_db(mime_root: &Path) -> Result<bool> {
    match Command::new("update-mime-database")
        .arg(mime_root)
        .stdout(Stdio::null())
        .status()
    {
        Ok(s) if s.success() => Ok(true),
        Ok(s) => anyhow::bail!("update-mime-database exited with {s}"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).context("cannot run update-mime-database"),
    }
}

/// Install the MIME package and magic file under `data_home`.
pub fn install(prefix: &Path, data_home: &Path, quiet: bool) -> Result<()> {
    let mime_root = data_home.join("mime");
    let mime_xml = mime_root.join("packages").join("crush.xml");
    let magic = data_home.join("crush").join("crush.magic");
    for (path, body) in [(&mime_xml, MIME_XML), (&magic, MAGIC)] {
        fs::create_dir_all(path.parent().expect("has a parent"))
            .with_context(|| format!("cannot create parent of '{}'", path.display()))?;
        fs::write(path, body).with_context(|| format!("cannot write '{}'", path.display()))?;
        if !quiet {
            eprintln!("  installed {}", path.display());
        }
    }

    let refreshed = refresh_mime_db(&mime_root)?;
    let record = Record {
        files: vec![mime_xml, magic.clone()],
        mime_root: refreshed.then_some(mime_root.clone()),
    };
    let rp = record_path(prefix);
    fs::create_dir_all(rp.parent().expect("has a parent"))?;
    fs::write(&rp, serde_json::to_string_pretty(&record)?)
        .with_context(|| format!("cannot write '{}'", rp.display()))?;

    if !quiet {
        if refreshed {
            eprintln!("  ran update-mime-database {}", mime_root.display());
        } else {
            eprintln!(
                "  update-mime-database not found; install shared-mime-info, then run:\n    update-mime-database {}",
                mime_root.display()
            );
        }
        eprintln!(
            "  to let file(1) identify Crush files, add the rules to your magic file:\n    cat {} >> ~/.magic\n  (or try them once with: file -m {} FILE)",
            magic.display(),
            magic.display()
        );
    }
    Ok(())
}

/// Remove what [`install`] wrote. A no-op when desktop integration was never installed.
pub fn uninstall(prefix: &Path, quiet: bool) -> Result<()> {
    let rp = record_path(prefix);
    let Ok(text) = fs::read_to_string(&rp) else {
        return Ok(());
    };
    let record: Record = serde_json::from_str(&text)
        .with_context(|| format!("corrupt desktop-integration record '{}'", rp.display()))?;
    for f in &record.files {
        if f.exists() {
            fs::remove_file(f).with_context(|| format!("cannot remove '{}'", f.display()))?;
            if !quiet {
                eprintln!("  removed {}", f.display());
            }
        }
        // Drop our now-empty `crush/` data dir; never anything shared.
        if f.file_name().is_some_and(|n| n == "crush.magic")
            && let Some(dir) = f.parent()
        {
            let _ = fs::remove_dir(dir);
        }
    }
    if let Some(root) = &record.mime_root
        && let Err(e) = refresh_mime_db(root)
    {
        eprintln!("  warning: {e:#}");
    }
    fs::remove_file(&rp)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The embedded copies must equal the editable sources in `contrib/`
    /// (skipped when built from a packaged tarball, which has no `contrib/`).
    #[test]
    fn assets_match_contrib() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contrib");
        let Ok(xml) = fs::read_to_string(root.join("mime/crush.xml")) else {
            return;
        };
        assert_eq!(
            xml, MIME_XML,
            "run: cp contrib/mime/crush.xml crates/crush-installer/assets/"
        );
        assert_eq!(
            fs::read_to_string(root.join("magic/crush.magic")).unwrap(),
            MAGIC,
            "run: cp contrib/magic/crush.magic crates/crush-installer/assets/"
        );
    }

    #[test]
    fn install_then_uninstall_round_trips() {
        let prefix = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        install(prefix.path(), data.path(), true).unwrap();
        assert!(data.path().join("mime/packages/crush.xml").exists());
        assert!(data.path().join("crush/crush.magic").exists());
        uninstall(prefix.path(), true).unwrap();
        assert!(!data.path().join("mime/packages/crush.xml").exists());
        assert!(!data.path().join("crush").exists());
        assert!(!record_path(prefix.path()).exists());
        // idempotent
        uninstall(prefix.path(), true).unwrap();
    }
}
