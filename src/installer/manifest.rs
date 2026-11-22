//! The list of files an installer payload carries, with their SHA-256 digests.
//!
//! A payload folder holds `bin\mink.exe`, `stdlib\`, `windows\` and `LICENSE`
//! next to `payload-manifest.txt`. The manifest is plain text so it can be
//! written by a build script and read without a JSON parser:
//!
//! ```text
//! mink-payload 1
//! version 1.0.5
//! <sha256> <size in bytes> <relative path>
//! ```
//!
//! The installer checks every file against it before copying anything, so a
//! damaged or half-downloaded payload is refused instead of installed.

use std::fs;
use std::path::{Path, PathBuf};

use crate::package::sha256::hex_digest;

/// The manifest's file name inside a payload.
pub const MANIFEST_NAME: &str = "payload-manifest.txt";

/// One file of the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadFile {
    /// The path relative to the payload folder, with `/` separators.
    pub path: String,
    /// The size in bytes.
    pub size: u64,
    /// The SHA-256 digest as lowercase hex.
    pub sha256: String,
}

/// A payload's manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The MINK version the payload installs.
    pub version: String,
    /// Every file, in path order.
    pub files: Vec<PayloadFile>,
}

/// Whether a relative path is safe to join onto a folder: not absolute, no `..`,
/// no drive letter, no empty parts.
pub fn is_safe_relative(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') || path.contains(':') {
        return false;
    }
    path.split(['/', '\\'])
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

impl Manifest {
    /// Reads a manifest from its text form.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines().map(str::trim_end);
        if lines.next() != Some("mink-payload 1") {
            return Err("the payload manifest is not a MINK payload manifest (version 1)".to_string());
        }
        let version = lines
            .next()
            .and_then(|line| line.strip_prefix("version "))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or("the payload manifest has no version line")?
            .to_string();
        if !super::engine::is_version_name(&version) {
            return Err(format!("the payload manifest names an unsafe version '{version}'"));
        }
        let version = version
            .to_string();
        let mut files = Vec::new();
        for (number, line) in lines.enumerate() {
            if line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(3, ' ');
            let (Some(sha256), Some(size), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
                return Err(format!("manifest line {} is not '<sha256> <size> <path>'", number + 3));
            };
            if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!("manifest line {} has a bad digest", number + 3));
            }
            let size: u64 = size
                .parse()
                .map_err(|_| format!("manifest line {} has a bad size", number + 3))?;
            if !is_safe_relative(path) {
                return Err(format!("manifest line {} names an unsafe path '{path}'", number + 3));
            }
            files.push(PayloadFile {
                path: path.replace('\\', "/"),
                size,
                sha256: sha256.to_ascii_lowercase(),
            });
        }
        if files.is_empty() {
            return Err("the payload manifest lists no files".to_string());
        }
        Ok(Self { version, files })
    }

    /// Writes the manifest in its text form.
    pub fn to_text(&self) -> String {
        let mut out = format!("mink-payload 1\nversion {}\n", self.version);
        for file in &self.files {
            out.push_str(&format!("{} {} {}\n", file.sha256, file.size, file.path));
        }
        out
    }

    /// Builds the manifest of a folder, skipping the manifest file itself.
    pub fn from_directory(root: &Path, version: &str) -> Result<Self, String> {
        let mut files = Vec::new();
        collect(root, root, &mut files)?;
        files.retain(|f| f.path != MANIFEST_NAME);
        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self {
            version: version.to_string(),
            files,
        })
    }

    /// Total size of every file in bytes.
    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Checks every file under `root` against the manifest.
    ///
    /// `each` is told how many files have been checked so far, so the installer
    /// can show progress. The first mismatch is returned as an error that names
    /// the file.
    pub fn verify(&self, root: &Path, each: &mut dyn FnMut(usize, &str)) -> Result<(), String> {
        for (index, file) in self.files.iter().enumerate() {
            each(index, &file.path);
            let path = root.join(file.path.replace('/', "\\"));
            let bytes = fs::read(&path).map_err(|error| format!("{}: cannot read ({error})", file.path))?;
            if bytes.len() as u64 != file.size {
                return Err(format!("{}: expected {} bytes, found {}", file.path, file.size, bytes.len()));
            }
            if hex_digest(&bytes) != file.sha256 {
                return Err(format!("{}: the contents do not match the manifest", file.path));
            }
        }
        Ok(())
    }
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<PayloadFile>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read '{}': {e}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect(root, &path, out)?;
        } else {
            let bytes = fs::read(&path).map_err(|e| format!("cannot read '{}': {e}", path.display()))?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "a file lies outside the payload folder".to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            out.push(PayloadFile {
                path: relative,
                size: bytes.len() as u64,
                sha256: hex_digest(&bytes),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mink_manifest_{label}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn safe_relative_paths() {
        assert!(is_safe_relative("bin/mink.exe"));
        assert!(is_safe_relative("stdlib\\json.mink"));
        for bad in ["", "/etc", "\\x", "C:\\x", "..\\x", "a/../b", "a//b", "./a"] {
            assert!(!is_safe_relative(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_manifest_round_trips() {
        let dir = scratch("round");
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(dir.join("bin").join("mink.exe"), b"exe bytes").unwrap();
        fs::write(dir.join("LICENSE"), b"license").unwrap();
        fs::write(dir.join(MANIFEST_NAME), b"ignored").unwrap();
        let manifest = Manifest::from_directory(&dir, "1.0.5").unwrap();
        assert_eq!(manifest.files.len(), 2, "the manifest file is skipped");
        assert_eq!(manifest.files[0].path, "LICENSE");
        assert_eq!(manifest.files[1].path, "bin/mink.exe");
        assert_eq!(Manifest::parse(&manifest.to_text()).unwrap(), manifest);
        assert_eq!(manifest.total_size(), 16);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verify_accepts_an_intact_payload_and_names_the_bad_file() {
        let dir = scratch("verify");
        fs::write(dir.join("a.txt"), b"alpha").unwrap();
        fs::write(dir.join("b.txt"), b"beta").unwrap();
        let manifest = Manifest::from_directory(&dir, "1.0.5").unwrap();
        let mut seen = Vec::new();
        manifest.verify(&dir, &mut |i, p| seen.push((i, p.to_string()))).unwrap();
        assert_eq!(seen, vec![(0, "a.txt".to_string()), (1, "b.txt".to_string())]);

        fs::write(dir.join("b.txt"), b"betA").unwrap();
        let err = manifest.verify(&dir, &mut |_, _| {}).unwrap_err();
        assert!(err.starts_with("b.txt") && err.contains("do not match"), "{err}");

        fs::write(dir.join("b.txt"), b"longer than before").unwrap();
        assert!(manifest.verify(&dir, &mut |_, _| {}).unwrap_err().contains("expected 4 bytes"));

        fs::remove_file(dir.join("b.txt")).unwrap();
        assert!(manifest.verify(&dir, &mut |_, _| {}).unwrap_err().contains("cannot read"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn bad_manifests_are_refused_with_a_reason() {
        let digest = "ab".repeat(32);
        let cases = [
            ("not a manifest", "not a MINK payload manifest"),
            ("mink-payload 1\n", "no version line"),
            ("mink-payload 1\nversion 1.0.5\n", "lists no files"),
            ("mink-payload 1\nversion 1.0.5\nshort 1 a\n", "bad digest"),
            (&format!("mink-payload 1\nversion 1.0.5\n{digest} x a\n"), "bad size"),
            (&format!("mink-payload 1\nversion 1.0.5\n{digest} 1 ..\\evil\n"), "unsafe path"),
            (&format!("mink-payload 1\nversion 1.0.5\n{digest} 1\n"), "is not '<sha256> <size> <path>'"),
        ];
        for (text, expected) in cases {
            let err = Manifest::parse(text).unwrap_err();
            assert!(err.contains(expected), "{text:?} gave {err:?}");
        }
    }

    #[test]
    fn paths_with_spaces_survive() {
        let digest = "cd".repeat(32);
        let manifest = Manifest::parse(&format!("mink-payload 1\nversion 2.0.0\n{digest} 7 stdlib/my file.mink\n")).unwrap();
        assert_eq!(manifest.files[0].path, "stdlib/my file.mink");
    }
}
