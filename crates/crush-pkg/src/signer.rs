use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

const PRIVATE_KEY_SIZE: usize = 64;
const PUBLIC_KEY_SIZE: usize = 32;
const SIGNATURE_SIZE: usize = 64;

pub fn generate_keys(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;

    let mut csprng = rand::rngs::OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let verifying_key = signing_key.verifying_key();

    let private_path = dir.join("private_key.pem");
    File::create(&private_path)?.write_all(&signing_key.to_keypair_bytes())?;

    let public_path = dir.join("public_key.pem");
    File::create(&public_path)?.write_all(&verifying_key.to_bytes())?;

    println!("  wrote {}", private_path.display());
    println!("  wrote {}", public_path.display());
    Ok(())
}

pub fn sign_package(package_path: &Path, private_key_path: &Path) -> anyhow::Result<()> {
    let mut key_bytes = [0u8; PRIVATE_KEY_SIZE];
    let mut f = File::open(private_key_path)?;
    f.read_exact(&mut key_bytes)?;

    let signing_key = SigningKey::from_keypair_bytes(&key_bytes)?;

    let mut package_data = Vec::new();
    File::open(package_path)?.read_to_end(&mut package_data)?;

    let signature = signing_key.sign(&package_data);

    let sig_path = package_path.with_extension("cap.sig");
    File::create(&sig_path)?.write_all(&signature.to_bytes())?;
    println!("  wrote {}", sig_path.display());
    Ok(())
}

pub fn verify_package(package_path: &Path, public_key_path: &Path) -> anyhow::Result<bool> {
    let mut pk_bytes = [0u8; PUBLIC_KEY_SIZE];
    let mut f = File::open(public_key_path)?;
    f.read_exact(&mut pk_bytes)?;

    let verifying_key = VerifyingKey::from_bytes(&pk_bytes)?;

    let mut package_data = Vec::new();
    File::open(package_path)?.read_to_end(&mut package_data)?;

    let sig_path = package_path.with_extension("cap.sig");
    let mut sig_bytes = [0u8; SIGNATURE_SIZE];
    let mut f = File::open(&sig_path)?;
    f.read_exact(&mut sig_bytes)?;

    let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);

    match verifying_key.verify(&package_data, &sig) {
        Ok(()) => {
            println!("  signature verified for {}", package_path.display());
            Ok(true)
        }
        Err(e) => {
            println!("  signature verification FAILED: {}", e);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_roundtrip() {
        let dir = tempfile::tempdir().unwrap();

        // Generate keys
        let key_dir = dir.path().join("keys");
        generate_keys(&key_dir).unwrap();

        // Create a test file
        let test_file = dir.path().join("test.cap");
        std::fs::write(&test_file, b"hello world, this is a capsule").unwrap();

        // Sign
        let priv_key = key_dir.join("private_key.pem");
        sign_package(&test_file, &priv_key).unwrap();
        assert!(dir.path().join("test.cap.sig").exists());

        // Verify
        let pub_key = key_dir.join("public_key.pem");
        assert!(verify_package(&test_file, &pub_key).unwrap());
    }

    #[test]
    fn generate_keys_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        generate_keys(dir.path()).unwrap();
        assert!(dir.path().join("private_key.pem").exists());
        assert!(dir.path().join("public_key.pem").exists());

        let pk = std::fs::read(dir.path().join("public_key.pem")).unwrap();
        assert_eq!(pk.len(), 32);
    }
}
