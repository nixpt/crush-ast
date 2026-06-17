use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use ed25519_dalek::{Signer, Verifier};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// ECAP types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EcapSignature {
    pub signer_did: String,
    pub algorithm: String,
    pub signature_bytes: Vec<u8>,
    pub signed_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EcapManifest {
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub required_capabilities: Vec<String>,
    pub optional_capabilities: Vec<String>,
    pub sections: Vec<ManifestSection>,
    pub metadata: ManifestMetadata,
    pub signature: Option<EcapSignature>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifestSection {
    pub name: String,
    pub section_type: String,
    pub size: u64,
    pub hash: String,
    pub encrypted: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManifestMetadata {
    pub created_at: String,
    pub modified_at: Option<String>,
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for ManifestMetadata {
    fn default() -> Self {
        Self {
            created_at: Utc::now().to_rfc3339(),
            modified_at: None,
            custom: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EcapPackage {
    pub manifest: EcapManifest,
    pub sections: Vec<EcapSection>,
    pub signature: Option<Vec<u8>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EcapSection {
    pub name: String,
    pub data: Vec<u8>,
    pub encrypted: bool,
    pub hash: [u8; 32],
}

// ---------------------------------------------------------------------------
// Impl
// ---------------------------------------------------------------------------

impl EcapManifest {
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            author: None,
            description: None,
            required_capabilities: Vec::new(),
            optional_capabilities: Vec::new(),
            sections: Vec::new(),
            metadata: ManifestMetadata::default(),
            signature: None,
        }
    }

    pub fn canonical_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.signature = None;
        serde_json::to_vec(&unsigned)
            .map_err(|e| anyhow::anyhow!("Failed to produce canonical bytes: {}", e))
    }

    pub fn sign_with(
        &mut self,
        signing_key: &ed25519_dalek::SigningKey,
        did: &str,
    ) -> anyhow::Result<()> {
        let canonical = self.canonical_bytes()?;
        let sig = signing_key.sign(&canonical);
        self.signature = Some(EcapSignature {
            signer_did: did.to_string(),
            algorithm: "Ed25519".to_string(),
            signature_bytes: sig.to_bytes().to_vec(),
            signed_at: Utc::now().to_rfc3339(),
        });
        Ok(())
    }

    pub fn verify_signature(
        &self,
        verifying_key: &ed25519_dalek::VerifyingKey,
    ) -> anyhow::Result<bool> {
        use ed25519_dalek::Signature;
        let sig_info = self
            .signature
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No signature on manifest"))?;
        let canonical = self.canonical_bytes()?;
        let sig = Signature::from_slice(&sig_info.signature_bytes)?;
        match verifying_key.verify(&canonical, &sig) {
            Ok(()) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub fn add_section(&mut self, section: ManifestSection) {
        self.sections.push(section);
    }
}

impl EcapSection {
    pub fn new(name: &str, data: Vec<u8>) -> Self {
        let hash = {
            let mut hasher = Sha256::new();
            hasher.update(&data);
            hasher.finalize().into()
        };
        Self {
            name: name.to_string(),
            data,
            encrypted: false,
            hash,
        }
    }

    pub fn verify_hash(&self) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(&self.data);
        let computed: [u8; 32] = hasher.finalize().into();
        computed == self.hash
    }
}

impl ManifestSection {
    pub fn new(name: &str, section_type: &str) -> Self {
        Self {
            name: name.to_string(),
            section_type: section_type.to_string(),
            size: 0,
            hash: String::new(),
            encrypted: false,
        }
    }
}

impl EcapPackage {
    pub fn new(manifest: EcapManifest) -> Self {
        Self {
            manifest,
            sections: Vec::new(),
            signature: None,
        }
    }

    pub fn add_section(&mut self, section: EcapSection) {
        self.sections.push(section);
    }

    pub fn write_to_file(&self, path: &Path) -> anyhow::Result<()> {
        let data = bincode::serialize(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn read_from_file(path: &Path) -> anyhow::Result<Self> {
        let data = std::fs::read(path)?;
        let pkg: Self = bincode::deserialize(&data)?;
        Ok(pkg)
    }
}

// ---------------------------------------------------------------------------
// Top-level helpers
// ---------------------------------------------------------------------------

pub fn create_ecap_package(manifest: &EcapManifest, sections: Vec<EcapSection>) -> EcapPackage {
    let mut pkg = EcapPackage::new(manifest.clone());
    for s in sections {
        pkg.add_section(s);
    }
    pkg
}

pub fn create_ecap_package_to_file(
    manifest: &EcapManifest,
    sections: Vec<EcapSection>,
    output: &Path,
) -> anyhow::Result<()> {
    let pkg = create_ecap_package(manifest, sections);
    pkg.write_to_file(output)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_hash_verify() {
        let s = EcapSection::new("code", b"fn main() {}".to_vec());
        assert!(s.verify_hash());
        let mut tampered = s.clone();
        tampered.data.push(0);
        assert!(!tampered.verify_hash());
    }

    #[test]
    fn ecap_roundtrip() {
        let mut manifest = EcapManifest::new("test-pkg", "0.1.0");
        manifest.author = Some("test".into());
        manifest.add_section(ManifestSection::new("code", "crush"));

        let sections = vec![EcapSection::new("main.crush", b"fn main() {}".to_vec())];
        let pkg = create_ecap_package(&manifest, sections);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ecap");
        pkg.write_to_file(&path).unwrap();

        let loaded = EcapPackage::read_from_file(&path).unwrap();
        assert_eq!(loaded.manifest.name, "test-pkg");
        assert_eq!(loaded.sections.len(), 1);
        assert!(loaded.sections[0].verify_hash());
    }

    #[test]
    fn sign_and_verify_manifest() {
        let mut csprng = rand::rngs::OsRng;
        let signing_key = ed25519_dalek::SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        let mut manifest = EcapManifest::new("signed-pkg", "1.0.0");
        manifest.sign_with(&signing_key, "did:key:test123").unwrap();
        assert!(manifest.signature.is_some());
        assert!(manifest.verify_signature(&verifying_key).unwrap());
    }
}
