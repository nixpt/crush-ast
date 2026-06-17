//! Static-site capsules.
//!
//! Bundle a directory of static web assets (html/css/js/media) into a single
//! signed ECAP capsule — a portable, tamper-evident, distributable artifact.
//! This is the "publish a site as a capsule" path; it needs no exosphere and no
//! Crush bytecode (unlike [`crate::builder`], which packages compiled `.cvm`).
//!
//! - [`build_site_capsule`] walks the asset tree, hashing each file into an
//!   [`EcapSection`] (SHA-256) and recording it in the manifest.
//! - [`write_site_capsule`] adds a reserved `__site__.json` metadata section
//!   (capsule type + entry), optionally Ed25519-signs the manifest, and writes
//!   the `.ecap`.
//! - [`extract_site_capsule`] verifies every section hash and unpacks the tree
//!   back out (for hosting / inspection), proving the round-trip.
//!
//! Note: site metadata lives in a dedicated section rather than
//! `manifest.metadata.custom`, because the ECAP package is bincode-serialized
//! and bincode cannot deserialize the `serde_json::Value` that `custom` holds
//! (it needs `deserialize_any`, which non-self-describing formats lack).

use std::path::Path;

use ed25519_dalek::SigningKey;
use walkdir::WalkDir;

use crate::ecap::{
    EcapManifest, EcapPackage, EcapSection, ManifestSection, create_ecap_package_to_file,
};

/// Capsule-type marker stored in the reserved metadata section.
pub const SITE_CAPSULE_TYPE: &str = "static-site";
/// Reserved section name carrying the site metadata JSON.
pub const SITE_META_SECTION: &str = "__site__.json";

/// Directory/file names skipped when walking the asset tree.
fn is_ignored(rel: &str) -> bool {
    rel.split('/').any(|c| {
        c == ".git" || c == ".DS_Store" || c == "node_modules" || c == "target" || c.is_empty()
    })
}

fn manifest_section_for(section: &EcapSection, section_type: &str) -> ManifestSection {
    let mut ms = ManifestSection::new(&section.name, section_type);
    ms.size = section.data.len() as u64;
    ms.hash = hex::encode(section.hash);
    ms
}

/// Build a static-site ECAP capsule (manifest + asset sections) from a directory
/// of web assets. `entry` is the relative path of the landing document (e.g.
/// `index.html`) and must exist among the bundled files. The returned sections
/// are the assets only; the metadata section is added by [`write_site_capsule`].
pub fn build_site_capsule(
    assets_dir: &Path,
    name: &str,
    version: &str,
    entry: &str,
) -> anyhow::Result<(EcapManifest, Vec<EcapSection>)> {
    if !assets_dir.is_dir() {
        anyhow::bail!("assets directory not found: {}", assets_dir.display());
    }

    let mut manifest = EcapManifest::new(name, version);
    let mut sections: Vec<EcapSection> = Vec::new();

    for dent in WalkDir::new(assets_dir).into_iter().filter_map(|e| e.ok()) {
        if !dent.file_type().is_file() {
            continue;
        }
        let rel = dent
            .path()
            .strip_prefix(assets_dir)?
            .to_string_lossy()
            .replace('\\', "/");
        if is_ignored(&rel) || rel == SITE_META_SECTION {
            continue;
        }
        let data = std::fs::read(dent.path())?;
        let section = EcapSection::new(&rel, data);
        manifest.add_section(manifest_section_for(&section, "static-asset"));
        sections.push(section);
    }

    if sections.is_empty() {
        anyhow::bail!("no files found under {}", assets_dir.display());
    }
    if !sections.iter().any(|s| s.name == entry) {
        anyhow::bail!(
            "entry '{}' not found among the {} bundled asset(s)",
            entry,
            sections.len()
        );
    }

    let total: u64 = sections.iter().map(|s| s.data.len() as u64).sum();
    manifest.description = Some(format!(
        "Static site capsule — {} asset(s), {} byte(s)",
        sections.len(),
        total
    ));

    Ok((manifest, sections))
}

/// Build the reserved metadata section describing a site capsule.
fn meta_section(entry: &str, asset_count: usize) -> anyhow::Result<EcapSection> {
    let meta = serde_json::json!({
        "capsule_type": SITE_CAPSULE_TYPE,
        "entry": entry,
        "asset_count": asset_count,
    });
    Ok(EcapSection::new(
        SITE_META_SECTION,
        serde_json::to_vec(&meta)?,
    ))
}

/// Load a 64-byte Ed25519 keypair file (the format `crush-pkg generate-keys`
/// writes to `private_key.pem`).
fn load_signing_key(key_path: &Path) -> anyhow::Result<SigningKey> {
    let bytes = std::fs::read(key_path)?;
    let arr: [u8; 64] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected a 64-byte keypair at {}", key_path.display()))?;
    Ok(SigningKey::from_keypair_bytes(&arr)?)
}

/// Build a static-site capsule and write it to `output` as an `.ecap` file.
/// When `sign` is provided, the manifest is Ed25519-signed (over its canonical
/// bytes) and attributed to `did`. Returns the number of bundled assets.
pub fn write_site_capsule(
    assets_dir: &Path,
    name: &str,
    version: &str,
    entry: &str,
    output: &Path,
    sign: Option<&Path>,
    did: Option<&str>,
) -> anyhow::Result<usize> {
    let (mut manifest, mut sections) = build_site_capsule(assets_dir, name, version, entry)?;
    let count = sections.len();

    let meta = meta_section(entry, count)?;
    manifest.add_section(manifest_section_for(&meta, "site-meta"));
    sections.push(meta);

    if let Some(key_path) = sign {
        let signing_key = load_signing_key(key_path)?;
        manifest.sign_with(&signing_key, did.unwrap_or("did:key:unknown"))?;
    }

    create_ecap_package_to_file(&manifest, sections, output)?;
    Ok(count)
}

/// Extract a static-site capsule back to `out_dir`, verifying every section's
/// SHA-256 hash. Returns the declared entry path. The reserved metadata section
/// is consumed (not written to disk), yielding a clean, servable tree.
pub fn extract_site_capsule(capsule: &Path, out_dir: &Path) -> anyhow::Result<String> {
    let pkg = EcapPackage::read_from_file(capsule)?;

    let meta = pkg
        .sections
        .iter()
        .find(|s| s.name == SITE_META_SECTION)
        .ok_or_else(|| anyhow::anyhow!("{} is not a static-site capsule", capsule.display()))?;
    if !meta.verify_hash() {
        anyhow::bail!("metadata section hash mismatch (capsule tampered?)");
    }
    let meta_json: serde_json::Value = serde_json::from_slice(&meta.data)?;
    if meta_json.get("capsule_type").and_then(|v| v.as_str()) != Some(SITE_CAPSULE_TYPE) {
        anyhow::bail!("{} is not a static-site capsule", capsule.display());
    }
    let entry = meta_json
        .get("entry")
        .and_then(|v| v.as_str())
        .unwrap_or("index.html")
        .to_string();

    std::fs::create_dir_all(out_dir)?;
    for sec in &pkg.sections {
        if sec.name == SITE_META_SECTION {
            continue;
        }
        if !sec.verify_hash() {
            anyhow::bail!(
                "hash mismatch for section '{}' (capsule tampered?)",
                sec.name
            );
        }
        let dest = out_dir.join(&sec.name);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &sec.data)?;
    }

    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_sample_site(dir: &Path) {
        std::fs::create_dir_all(dir.join("css")).unwrap();
        std::fs::write(dir.join("index.html"), b"<h1>hi</h1>").unwrap();
        std::fs::write(dir.join("css/site.css"), b"h1{color:red}").unwrap();
        std::fs::write(dir.join("app.js"), b"console.log(1)").unwrap();
    }

    #[test]
    fn build_collects_all_assets() {
        let dir = tempfile::tempdir().unwrap();
        write_sample_site(dir.path());
        let (manifest, sections) =
            build_site_capsule(dir.path(), "mysite", "1.0.0", "index.html").unwrap();
        assert_eq!(sections.len(), 3);
        assert_eq!(manifest.sections.len(), 3);
        assert!(sections.iter().all(|s| s.verify_hash()));
    }

    #[test]
    fn missing_entry_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        write_sample_site(dir.path());
        assert!(build_site_capsule(dir.path(), "s", "1.0.0", "nope.html").is_err());
    }

    #[test]
    fn write_extract_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        write_sample_site(dir.path());
        let out = dir.path().join("site.ecap");
        let n = write_site_capsule(
            dir.path(),
            "mysite",
            "1.0.0",
            "index.html",
            &out,
            None,
            None,
        )
        .unwrap();
        assert_eq!(n, 3);

        let extract_dir = dir.path().join("extracted");
        let entry = extract_site_capsule(&out, &extract_dir).unwrap();
        assert_eq!(entry, "index.html");
        // metadata section must not leak into the served tree
        assert!(!extract_dir.join(SITE_META_SECTION).exists());
        assert_eq!(
            std::fs::read(extract_dir.join("index.html")).unwrap(),
            b"<h1>hi</h1>"
        );
        assert_eq!(
            std::fs::read(extract_dir.join("css/site.css")).unwrap(),
            b"h1{color:red}"
        );
    }

    #[test]
    fn extract_rejects_non_site_capsule() {
        let dir = tempfile::tempdir().unwrap();
        // a bytecode-style capsule with no site metadata section
        let manifest = EcapManifest::new("notsite", "1.0.0");
        let out = dir.path().join("bytecode.ecap");
        create_ecap_package_to_file(
            &manifest,
            vec![EcapSection::new("main.cvm", vec![1, 2, 3])],
            &out,
        )
        .unwrap();
        assert!(extract_site_capsule(&out, &dir.path().join("x")).is_err());
    }

    #[test]
    fn signed_capsule_verifies() {
        let dir = tempfile::tempdir().unwrap();
        write_sample_site(dir.path());

        let mut csprng = rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let key_path = dir.path().join("private_key.pem");
        std::fs::write(&key_path, signing_key.to_keypair_bytes()).unwrap();

        let out = dir.path().join("signed.ecap");
        write_site_capsule(
            dir.path(),
            "s",
            "1.0.0",
            "index.html",
            &out,
            Some(&key_path),
            Some("did:key:test"),
        )
        .unwrap();

        let pkg = EcapPackage::read_from_file(&out).unwrap();
        let vk = signing_key.verifying_key();
        assert!(pkg.manifest.signature.is_some());
        assert!(pkg.manifest.verify_signature(&vk).unwrap());
    }
}
