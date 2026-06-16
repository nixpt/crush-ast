use std::path::{Path, PathBuf};
use std::io::{Read, Write, Cursor};

const EXCLUDED_DIRS: &[&str] = &["target", ".git", "node_modules", "__pycache__", "dist"];

fn is_excluded_path(path: &Path) -> bool {
    path.components().any(|c| {
        if let std::path::Component::Normal(s) = c {
            if let Some(s) = s.to_str() {
                return EXCLUDED_DIRS.contains(&s);
            }
        }
        false
    })
}

pub fn pack(source_dir: &Path, output_path: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::create(output_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);

    let base = source_dir.canonicalize()?;

    for entry in walkdir::WalkDir::new(&base).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if is_excluded_path(path) {
            continue;
        }

        let relative = path.strip_prefix(&base)?;
        let name = relative.to_string_lossy().replace('\\', "/");

        if path.is_dir() {
            zip.add_directory(&name, options)?;
        } else {
            zip.start_file(&name, options)?;
            let mut data = Vec::new();
            std::fs::File::open(path)?.read_to_end(&mut data)?;
            zip.write_all(&data)?;
        }
    }

    zip.finish()?;
    println!("  packed {} -> {}", source_dir.display(), output_path.display());
    Ok(())
}

pub fn unpack(pack_path: &Path, output_dir: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(pack_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    std::fs::create_dir_all(output_dir)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let target = output_dir.join(&name);

        if entry.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            std::fs::write(&target, data)?;

            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode))?;
            }
        }
    }

    println!("  unpacked {} -> {} ({} entries)", pack_path.display(), output_dir.display(), archive.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_roundtrip() {
        let dir = tempfile::tempdir().unwrap();

        // Create fixture
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.crush"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join("crush.toml"), "[package]\nname = \"test\"\nversion = \"0.1.0\"\n").unwrap();

        // Create excluded dir (should be skipped)
        std::fs::create_dir_all(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("target/debug"), "junk\n").unwrap();

        let pack_path = dir.path().join("test.crush-pack");
        pack(dir.path(), &pack_path).unwrap();
        assert!(pack_path.exists());

        let out_dir = dir.path().join("out");
        unpack(&pack_path, &out_dir).unwrap();

        assert!(out_dir.join("src/main.crush").exists());
        assert!(out_dir.join("crush.toml").exists());
        // Excluded dir should not be present
        assert!(!out_dir.join("target").exists());
    }

    #[test]
    fn test_is_excluded_path() {
        assert!(is_excluded_path(Path::new("foo/target/bar")));
        assert!(is_excluded_path(Path::new(".git/config")));
        assert!(is_excluded_path(Path::new("node_modules/foo")));
        assert!(!is_excluded_path(Path::new("src/main.crush")));
        assert!(!is_excluded_path(Path::new("crush.toml")));
    }
}
