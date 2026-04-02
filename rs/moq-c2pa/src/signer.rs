use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use bytes::Bytes;

use crate::Error;

/// Signs a complete CMAF segment (moof+mdat) by piping it through the `c2patool` binary.
pub struct SegmentSigner {
    /// Path to the C2PA manifest JSON file.
    pub manifest_path: PathBuf,
    /// Path to the `c2patool` binary. Defaults to `"c2patool"` (searched on PATH).
    pub c2patool_path: PathBuf,
}

impl SegmentSigner {
    /// Sign a complete CMAF segment (moof+mdat bytes).
    ///
    /// Runs: `c2patool - --format video/iso.segment --manifest <path> --output - --no_signing_verify`
    ///
    /// The signed output may contain additional ISO BMFF boxes (e.g. a `uuid` box carrying
    /// the C2PA manifest) appended after the original mdat.
    pub fn sign(&self, segment: &[u8]) -> Result<Bytes, Error> {
        let manifest = self.manifest_path.to_str().ok_or(Error::InvalidPath)?;
        let tool = self.c2patool_path.to_str().ok_or(Error::InvalidPath)?;

        let mut child = Command::new(tool)
            .args([
                "-",
                "--format",
                "video/iso.segment",
                "--manifest",
                manifest,
                "--output",
                "-",
                "--no_signing_verify",
            ])
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

        let signer = SegmentSigner {
            manifest_path: PathBuf::from(r"C:\Users\santi\Downloads\vod_image (1).json"),
            c2patool_path: PathBuf::from(r"C:\Users\santi\moq-c2pa\rs\moq-c2pa\bin\c2patool.exe"),
        };

        let signed = signer.sign(&segment).expect("signing failed");

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
