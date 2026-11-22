//! Icons and version information in Windows executables (1.0.5 sprint, M4).
//!
//! `mink build` adds the MINK icon and version strings to every Windows
//! program, `--icon` and `--no-icon` change the icon, and `mink resource`
//! brands an executable that has none (this is how `mink.exe` gets its icon).
//! Most checks read the finished PE; the Windows-only ones then ask Windows
//! itself what it sees, since that is what Explorer shows.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn mink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mink"))
}

struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("mink_res_{label}_{}_{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const HELLO: &str = "fn main() {\n    rt_print_str(\"hi\\n\");\n}\n";

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

/// The (resource type, id, bytes) of every resource in a PE image, or an empty
/// list when the image has no resource directory.
fn resources(pe: &[u8]) -> Vec<(u32, u32, Vec<u8>)> {
    let pe_at = u32_at(pe, 0x3C) as usize;
    let optional = pe_at + 4 + 20;
    let rva = u32_at(pe, optional + 128);
    let size = u32_at(pe, optional + 132);
    if size == 0 {
        return Vec::new();
    }
    // Find the section that holds the resource directory.
    let sections = u16_at(pe, pe_at + 4 + 2) as usize;
    let table = optional + u16_at(pe, pe_at + 4 + 16) as usize;
    let mut base_rva = 0;
    let mut base_file = 0;
    for i in 0..sections {
        let at = table + i * 40;
        let va = u32_at(pe, at + 12);
        let raw = u32_at(pe, at + 16);
        if rva >= va && rva < va + raw.max(u32_at(pe, at + 8)) {
            base_rva = va;
            base_file = u32_at(pe, at + 20) as usize;
        }
    }
    let at_rva = |r: u32| base_file + (r - base_rva) as usize;
    let root = at_rva(rva);
    let entries = |dir: usize| -> Vec<(u32, u32)> {
        let n = u16_at(pe, dir + 12) as usize + u16_at(pe, dir + 14) as usize;
        (0..n)
            .map(|i| (u32_at(pe, dir + 16 + i * 8), u32_at(pe, dir + 20 + i * 8)))
            .collect()
    };
    let mut out = Vec::new();
    for (kind, kind_target) in entries(root) {
        for (id, id_target) in entries(root + (kind_target & 0x7FFF_FFFF) as usize) {
            for (_, data_target) in entries(root + (id_target & 0x7FFF_FFFF) as usize) {
                let entry = root + data_target as usize;
                let data = at_rva(u32_at(pe, entry));
                let len = u32_at(pe, entry + 4) as usize;
                out.push((kind, id, pe[data..data + len].to_vec()));
            }
        }
    }
    out
}

fn build(args: &[&str]) -> std::process::Output {
    mink().arg("build").args(args).output().unwrap()
}

fn icon_frames(ico: &[u8]) -> Vec<Vec<u8>> {
    let n = u16_at(ico, 4) as usize;
    (0..n)
        .map(|i| {
            let e = 6 + i * 16;
            let size = u32_at(ico, e + 8) as usize;
            let off = u32_at(ico, e + 12) as usize;
            ico[off..off + size].to_vec()
        })
        .collect()
}

/// Windows' own report of an executable: product, description, comments, file
/// version, and the size of the icon Explorer would draw.
#[cfg(windows)]
fn windows_view(exe: &Path) -> Vec<String> {
    let script = format!(
        "$f = Get-Item -LiteralPath '{p}'; $v = $f.VersionInfo; \
         Add-Type -AssemblyName System.Drawing; \
         $i = [System.Drawing.Icon]::ExtractAssociatedIcon('{p}'); \
         'product=' + $v.ProductName; 'description=' + $v.FileDescription; \
         'comments=' + $v.Comments; 'version=' + $v.FileVersion; \
         'original=' + $v.OriginalFilename; 'icon=' + $i.Width + 'x' + $i.Height",
        p = exe.display().to_string().replace('\'', "''")
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("run powershell");
    assert!(
        out.status.success(),
        "powershell failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .collect()
}

#[test]
fn a_built_program_carries_the_mink_icon_and_version_strings() {
    let dir = Scratch::new("default");
    let source = dir.write("tool.mink", HELLO);
    let out = build(&[source.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let exe = std::fs::read(dir.path("tool.exe")).unwrap();
    let found = resources(&exe);
    assert_eq!(found.iter().filter(|r| r.0 == 3).count(), 7, "one RT_ICON per frame");
    assert_eq!(found.iter().filter(|r| r.0 == 14).count(), 1, "one icon group");
    assert_eq!(found.iter().filter(|r| r.0 == 16).count(), 1, "one version resource");
    let shipped = std::fs::read("npm/mink/windows/mink-app.ico").unwrap();
    let frames = icon_frames(&shipped);
    for (i, frame) in frames.iter().enumerate() {
        let leaf = found.iter().find(|r| r.0 == 3 && r.1 == (i + 1) as u32).unwrap();
        assert_eq!(&leaf.2, frame, "frame {i} is the shipped MINK icon");
    }
}

#[cfg(windows)]
#[test]
fn windows_reports_the_icon_and_the_version_strings() {
    let dir = Scratch::new("winview");
    let source = dir.write("tool.mink", HELLO);
    assert!(build(&[source.to_str().unwrap()]).status.success());
    let view = windows_view(&dir.path("tool.exe"));
    assert!(view.contains(&"product=tool".to_string()), "{view:?}");
    assert!(view.contains(&"description=tool".to_string()), "{view:?}");
    assert!(view.contains(&"original=tool.exe".to_string()), "{view:?}");
    assert!(
        view.iter().any(|l| l.starts_with("comments=Built with MINK ")),
        "{view:?}"
    );
    assert!(view.contains(&"icon=32x32".to_string()), "Windows draws the embedded icon: {view:?}");
}

#[test]
fn the_program_still_runs_with_its_resources() {
    let dir = Scratch::new("runs");
    let source = dir.write("tool.mink", HELLO);
    assert!(build(&[source.to_str().unwrap()]).status.success());
    let out = Command::new(dir.path("tool.exe")).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("hi"));
}

#[test]
fn no_icon_keeps_the_version_information_but_drops_the_icon() {
    let dir = Scratch::new("noicon");
    let source = dir.write("tool.mink", HELLO);
    assert!(build(&[source.to_str().unwrap(), "--no-icon"]).status.success());
    let found = resources(&std::fs::read(dir.path("tool.exe")).unwrap());
    assert!(found.iter().all(|r| r.0 != 3 && r.0 != 14), "no icon resources");
    assert_eq!(found.iter().filter(|r| r.0 == 16).count(), 1);
}

#[test]
fn a_chosen_icon_replaces_the_default() {
    let dir = Scratch::new("custom");
    let source = dir.write("tool.mink", HELLO);
    let other = std::fs::read("npm/mink/windows/mink-file.ico").unwrap();
    std::fs::write(dir.path("mine.ico"), &other).unwrap();
    let out = build(&[source.to_str().unwrap(), "--icon", dir.path("mine.ico").to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let found = resources(&std::fs::read(dir.path("tool.exe")).unwrap());
    let frames = icon_frames(&other);
    let first = found.iter().find(|r| r.0 == 3 && r.1 == 1).unwrap();
    assert_eq!(first.2, frames[0], "the custom icon's first frame");
}

#[test]
fn a_bad_icon_fails_the_build_with_a_clear_message() {
    let dir = Scratch::new("bad");
    let source = dir.write("tool.mink", HELLO);
    dir.write("junk.ico", "this is not an icon");
    let out = build(&[source.to_str().unwrap(), "--icon", dir.path("junk.ico").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not a valid .ico file"), "{stderr}");

    let out = build(&[source.to_str().unwrap(), "--icon", dir.path("missing.ico").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot read the icon"));
}

#[test]
fn the_icon_options_belong_to_build_only() {
    let dir = Scratch::new("opts");
    let source = dir.write("tool.mink", HELLO);
    let out = mink().args(["check", source.to_str().unwrap(), "--no-icon"]).output().unwrap();
    assert_ne!(out.status.code(), Some(0));
    let out = mink().args(["test", source.to_str().unwrap(), "--icon", "x.ico"]).output().unwrap();
    assert_ne!(out.status.code(), Some(0));
}

#[cfg(windows)]
#[test]
fn a_project_build_takes_its_name_and_version_from_the_manifest() {
    let dir = Scratch::new("project");
    dir.write("mink.toml", "[package]\nname = \"demo\"\nversion = \"2.3.4\"\n");
    let source = dir.write("main.mink", HELLO);
    assert!(build(&[source.to_str().unwrap()]).status.success());
    let view = windows_view(&dir.path("main.exe"));
    assert!(view.contains(&"product=demo".to_string()), "{view:?}");
    assert!(view.contains(&"version=2.3.4".to_string()), "{view:?}");
}

/// A Windows program straight from the backend, with no resources.
fn plain_executable(dir: &Scratch) -> PathBuf {
    let source = dir.write("plain.mink", HELLO);
    let mut sources = mink::source::SourceMap::new();
    let report = mink::driver::check(&mut sources, &source).expect("check");
    assert!(report.errors.is_empty());
    let mir = report.mir.expect("mir");
    let image = mink::backend::compile(&mir, &sources, mink::backend::Target::native()).expect("compile");
    let path = dir.path("plain.exe");
    std::fs::write(&path, image.bytes).unwrap();
    path
}

#[cfg(windows)]
#[test]
fn mink_resource_brands_an_executable_that_has_none() {
    let dir = Scratch::new("brand");
    let exe = plain_executable(&dir);
    assert!(resources(&std::fs::read(&exe).unwrap()).is_empty());
    let out = mink()
        .arg("resource")
        .arg(&exe)
        .args(["--icon", "npm/mink/windows/mink-app.ico"])
        .args(["--product", "Plain Tool", "--version", "1.2.3", "--description", "A plain tool"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let view = windows_view(&exe);
    assert!(view.contains(&"product=Plain Tool".to_string()), "{view:?}");
    assert!(view.contains(&"description=A plain tool".to_string()), "{view:?}");
    assert!(view.contains(&"version=1.2.3".to_string()), "{view:?}");
    assert!(view.contains(&"icon=32x32".to_string()), "{view:?}");
    // It still runs.
    let run = Command::new(&exe).output().unwrap();
    assert_eq!(run.status.code(), Some(0));
    // No temporary file is left behind.
    assert!(!dir.path("plain.exe.mink-resource.tmp").exists());
}

#[test]
fn mink_resource_refuses_an_executable_that_already_has_resources() {
    let dir = Scratch::new("twice");
    let source = dir.write("tool.mink", HELLO);
    assert!(build(&[source.to_str().unwrap()]).status.success());
    let before = std::fs::read(dir.path("tool.exe")).unwrap();
    let out = mink()
        .arg("resource")
        .arg(dir.path("tool.exe"))
        .args(["--product", "Again"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("already has resources"));
    assert_eq!(std::fs::read(dir.path("tool.exe")).unwrap(), before, "the file is untouched");
}

#[test]
fn mink_resource_validates_its_arguments() {
    let out = mink().args(["resource"]).output().unwrap();
    assert_ne!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("missing executable path"));
    let out = mink().args(["resource", "x.exe"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stderr).contains("nothing to add"));
    let out = mink().args(["resource", "x.exe", "--bogus"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown option '--bogus'"));
    let out = mink().args(["resource", "does-not-exist.exe", "--product", "X"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot read"));
}

#[cfg(windows)]
#[test]
fn the_compiler_itself_can_be_branded_and_still_works() {
    // The release step for mink.exe: copy it, add the icon and version, run it.
    let dir = Scratch::new("compiler");
    let copy = dir.path("mink.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_mink"), &copy).unwrap();
    let out = Command::new(&copy)
        .arg("resource")
        .arg(&copy)
        .args(["--icon", "npm/mink/windows/mink-app.ico", "--product", "MINK"])
        .args(["--version", env!("CARGO_PKG_VERSION"), "--description", "MINK compiler"])
        .output()
        .unwrap();
    // A running executable cannot be replaced; branding is done from another copy.
    if out.status.success() {
        panic!("replacing a running executable should not succeed");
    }
    let driver = dir.path("driver.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_mink"), &driver).unwrap();
    let out = Command::new(&driver)
        .arg("resource")
        .arg(&copy)
        .args(["--icon", "npm/mink/windows/mink-app.ico", "--product", "MINK"])
        .args(["--version", env!("CARGO_PKG_VERSION"), "--description", "MINK compiler"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let version = Command::new(&copy).arg("--version").output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        format!("mink {}", env!("CARGO_PKG_VERSION"))
    );
    let view = windows_view(&copy);
    assert!(view.contains(&"product=MINK".to_string()), "{view:?}");
    assert!(view.contains(&"icon=32x32".to_string()), "{view:?}");
}
