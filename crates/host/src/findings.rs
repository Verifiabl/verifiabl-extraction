//! Keeps a file the module replay found a problem with, so the finding
//! outlives the corpus it came from.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::ReadKind;

/// Writes `bytes` to `findings`, named by its SHA-256 as libFuzzer names its
/// findings: corpus folders can hold different files with one name, and one
/// must not replace another. A note beside it says where it came from.
pub fn keep_finding(
    findings: &Path,
    source: &Path,
    bytes: &[u8],
    kind: ReadKind,
    found: &str,
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(findings)?;
    let name: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let kept = findings.join(&name);
    std::fs::write(&kept, bytes)?;
    std::fs::write(
        findings.join(format!("{name}.{}.txt", kind.name())),
        format!("{} ({}): {found}\n", source.display(), kind.name()),
    )?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_finding_by_its_content() {
        let findings = std::env::temp_dir().join(format!("vf-findings-{}", std::process::id()));
        let kept = keep_finding(
            &findings,
            Path::new("corpus/a"),
            b"abc",
            ReadKind::Pdf,
            "a fault",
        )
        .unwrap();
        assert_eq!(
            kept.file_name().unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let note = std::fs::read_to_string(findings.join(format!(
            "{}.pdf.txt",
            kept.file_name().unwrap().to_str().unwrap()
        )))
        .unwrap();
        assert_eq!(note, "corpus/a (pdf): a fault\n");
        std::fs::remove_dir_all(findings).unwrap();
    }
}
