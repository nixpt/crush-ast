use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::manifest::Manifest;

pub struct CapsuleBundle {
    pub manifest: Manifest,
    pub payload: Vec<u8>,
    pub readme: Option<String>,
}

impl CapsuleBundle {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if name.ends_with(".cap") || name.ends_with(".ecap") || name.ends_with(".zip") {
            Self::from_zip(path)
        } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
            Self::from_tar_gz(path)
        } else if name.ends_with(".tar") {
            Self::from_tar(path)
        } else {
            anyhow::bail!("Unknown bundle format: {}", name)
        }
    }

    fn from_zip(path: &Path) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)?;

        let mut manifest: Option<Manifest> = None;
        let mut payload: Option<Vec<u8>> = None;
        let mut readme: Option<String> = None;

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();

            if name == "Capsule.toml"
                || name == "capsule.toml"
                || name == "crush.toml"
                || name.ends_with("/Capsule.toml")
                || name.ends_with("/capsule.toml")
                || name.ends_with("/crush.toml")
            {
                let mut content = String::new();
                entry.read_to_string(&mut content)?;
                match Manifest::from_str(&content, Path::new("")) {
                    Ok(m) => manifest = Some(m),
                    Err(_) => continue,
                }
            } else if name.contains("payload/") && name.ends_with(".casm") {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                payload = Some(data);
            } else if name.ends_with(".cvm") {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                payload = Some(data);
            } else if name == "README.md" || name.ends_with("/README.md") {
                let mut content = String::new();
                entry.read_to_string(&mut content)?;
                readme = Some(content);
            }
        }

        if payload.is_none() {
            let file = File::open(path)?;
            let mut archive = zip::ZipArchive::new(file)?;
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i)?;
                let entry_name = entry.name();
                if entry_name.ends_with(".casm") || entry_name.ends_with(".cvm") {
                    let mut data = Vec::new();
                    entry.read_to_end(&mut data)?;
                    payload = Some(data);
                    break;
                }
            }
        }

        let manifest = manifest.ok_or_else(|| anyhow::anyhow!("Missing capsule/crush.toml in bundle"))?;
        let payload = payload.ok_or_else(|| anyhow::anyhow!("Missing .casm/.cvm payload in bundle"))?;

        Ok(Self { manifest, payload, readme })
    }

    fn from_tar_gz(path: &Path) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        let decoder = flate2::read::GzDecoder::new(file);
        Self::read_tar(decoder)
    }

    fn from_tar(path: &Path) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        Self::read_tar(file)
    }

    fn read_tar<R: Read>(reader: R) -> anyhow::Result<Self> {
        let mut archive = tar::Archive::new(reader);

        let mut manifest: Option<Manifest> = None;
        let mut payload: Option<Vec<u8>> = None;
        let mut readme: Option<String> = None;

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            let name = path.to_string_lossy().to_string();

            if name.ends_with("Capsule.toml")
                || name.ends_with("capsule.toml")
                || name.ends_with("crush.toml")
            {
                let mut content = String::new();
                entry.read_to_string(&mut content)?;
                match Manifest::from_str(&content, Path::new("")) {
                    Ok(m) => manifest = Some(m),
                    Err(_) => continue,
                }
            } else if name.ends_with(".casm") || name.ends_with(".cvm") {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                payload = Some(data);
            } else if name.ends_with("README.md") {
                let mut content = String::new();
                entry.read_to_string(&mut content)?;
                readme = Some(content);
            }
        }

        let manifest = manifest.ok_or_else(|| anyhow::anyhow!("Missing capsule/crush.toml in bundle"))?;
        let payload = payload.ok_or_else(|| anyhow::anyhow!("Missing .casm/.cvm payload in bundle"))?;

        Ok(Self { manifest, payload, readme })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_from_dir_as_zip_bundle() {
        let dir = tempfile::tempdir().unwrap();
        crate::manifest::scaffold_package(dir.path(), "bundle-test").unwrap();

        // Build to get .cvm output
        let manifest = crate::manifest::Manifest::from_file(&dir.path().join("capsule.toml")).unwrap();
        let builder = crate::builder::PackageBuilder::new(manifest, dir.path().to_path_buf());
        let output = builder.build().unwrap();
        builder.write_output(&output).unwrap();

        // Create a capsule dir with manifest + payload for packing
        let capsule_dir = dir.path().join("capsule");
        std::fs::create_dir_all(&capsule_dir).unwrap();
        std::fs::copy(
            dir.path().join("capsule.toml"),
            capsule_dir.join("capsule.toml"),
        ).unwrap();
        std::fs::copy(
            dir.path().join("target/bundle-test.cvm"),
            capsule_dir.join("payload.cvm"),
        ).unwrap();

        let pack_path = dir.path().join("bundle-test.cap");
        crate::packer::pack(&capsule_dir, &pack_path).unwrap();

        let bundle = CapsuleBundle::from_file(&pack_path).unwrap();
        assert_eq!(bundle.manifest.capsule.name, "bundle-test");
        assert!(!bundle.payload.is_empty());
    }
}
