//! Reading a file that another program has open for writing.
//!
//! A log file is the usual case: the program that writes it keeps it open. `rt_fs_read` used to ask Windows for
//! `FILE_SHARE_READ` only, which fails with a sharing violation when another handle already has the file
//! open for writing, so a MINK program could not read a live log.

use std::io::Write;
use std::process::Command;

#[test]
fn a_file_another_program_is_writing_can_still_be_read() {
    let dir = std::env::temp_dir().join(format!("mink_share_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("live.log");
    let mut writer = std::fs::File::create(&log).unwrap();
    writer.write_all(b"first line\nsecond line\n").unwrap();
    writer.flush().unwrap();
    // `writer` stays open for the whole test, as a logging program's handle would.

    let source = dir.join("prog.mink");
    let path = log.to_string_lossy().replace('\\', "/");
    std::fs::write(
        &source,
        format!(
            "fn main() -> Int {{\n    let text = rt_fs_read(\"{path}\");\n    rt_print_int(rt_str_len(text));\n    rt_str_free(text);\n    return 0;\n}}\n"
        ),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mink")).arg("run").arg(&source).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "23", "the whole file was read");
    drop(writer);
    let _ = std::fs::remove_dir_all(&dir);
}
