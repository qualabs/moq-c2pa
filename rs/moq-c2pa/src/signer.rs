use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use base64::Engine as _;
use bytes::Bytes;
use sha2::{Digest, Sha256};

use crate::Error;

/// Signs a complete CMAF segment (moof+mdat) by piping it through the `c2pa-live-cbc` binary.
pub struct SegmentSigner {
    /// Path to the C2PA manifest JSON file.
    pub manifest_path: PathBuf,
    /// Path to the `c2pa-live-cbc` binary.
    pub signer_path: PathBuf,
    /// Stream ID used for deterministic IV derivation.
    pub stream_id: String,
    /// Base64-encoded first-16-bytes of SHA-256 of the signing cert.
    /// Required by c2pa-live-cbc as the CERT_HASH_B64 environment variable.
    cert_hash_b64: String,
}

impl SegmentSigner {
    /// Create a new signer, computing `CERT_HASH_B64` from the cert referenced in the manifest.
    pub fn new(manifest_path: PathBuf, signer_path: PathBuf, stream_id: String) -> Result<Self, Error> {
        let cert_hash_b64 = compute_cert_hash(&manifest_path)?;
        Ok(Self { manifest_path, signer_path, stream_id, cert_hash_b64 })
    }

    /// Sign a complete CMAF segment (moof+mdat bytes).
    ///
    /// Runs: `c2pa-live-cbc sign-stream --manifest <path> --segment-index <n> --stream-id <id>`
    /// with `CERT_HASH_B64` set in the environment.
    pub fn sign(&self, segment: &[u8], segment_index: usize) -> Result<Bytes, Error> {
        let manifest = self.manifest_path.to_str().ok_or(Error::InvalidPath)?;
        let tool = self.signer_path.to_str().ok_or(Error::InvalidPath)?;

        let mut child = Command::new(tool)
            .args([
                "sign-stream",
                "--manifest",
                manifest,
                "--segment-index",
                &segment_index.to_string(),
                "--stream-id",
                &self.stream_id,
            ])
            .env("CERT_HASH_B64", &self.cert_hash_b64)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(Error::Spawn)?;

        child
            .stdin
            .take()
            .expect("stdin was piped")
            .write_all(segment)
            .map_err(Error::Io)?;

        let output = child.wait_with_output().map_err(Error::Io)?;

        if !output.status.success() {
            return Err(Error::ExitStatus(output.status));
        }

        Ok(Bytes::from(output.stdout))
    }
}

/// Parse the manifest JSON, resolve `sign_cert` relative to the manifest's directory,
/// read the cert, and return `base64(sha256(cert)[0..16])`.
fn compute_cert_hash(manifest_path: &std::path::Path) -> Result<String, Error> {
    let manifest_str = std::fs::read_to_string(manifest_path).map_err(Error::Io)?;
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_str).map_err(Error::ManifestParse)?;

    let sign_cert_rel = manifest["sign_cert"]
        .as_str()
        .ok_or(Error::ManifestMissingField("sign_cert"))?;

    let manifest_dir = manifest_path.parent().ok_or(Error::InvalidPath)?;
    let cert_path = manifest_dir.join(sign_cert_rel);

    let cert_bytes = std::fs::read(&cert_path).map_err(Error::Io)?;
    let hash = Sha256::digest(&cert_bytes);
    Ok(base64::engine::general_purpose::STANDARD.encode(&hash[..16]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// Generate a minimal fragmented MP4 segment using ffmpeg.
    fn generate_cmaf_segment() -> Vec<u8> {
        let out = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "lavfi",
                "-i", "testsrc=duration=2:size=320x240:rate=30",
                "-c:v", "libx264",
                "-f", "mp4",
                "-movflags", "frag_keyframe+empty_moov+default_base_moof",
                "pipe:1",
            ])
            .output()
            .expect("ffmpeg not found — required for this test");
        assert!(!out.stdout.is_empty(), "ffmpeg produced no output");
        out.stdout
    }

    #[test]
    fn test_sign_cmaf_segment() {
        let segment = generate_cmaf_segment();
        let original_size = segment.len();

        let signer = SegmentSigner::new(
            PathBuf::from(r"C:\Users\santi\moq-c2pa\rs\moq-c2pa\segment_manifest.json"),
            PathBuf::from(r"C:\Users\santi\moq-c2pa\rs\moq-c2pa\bin\c2pa-live-cbc.exe"),
            "test-stream".to_string(),
        ).expect("failed to create signer");

        let signed = signer.sign(&segment, 0).expect("signing failed");

        // Signed output must be larger (C2PA manifest was embedded).
        assert!(
            signed.len() > original_size,
            "signed output ({}) should be larger than original ({})",
            signed.len(),
            original_size
        );

        // C2PA JUMBF marker must be present.
        assert!(
            signed.windows(4).any(|w| w == b"c2pa"),
            "c2pa marker not found in signed output"
        );
    }
}
