//! Every runtime error a program can stop with (E-R01 to E-R15) has an explanation for `mink explain`.
//! The leak error E-R06 is the one people meet first, and it used to answer "unknown error code".

use std::process::Command;

#[test]
fn every_runtime_error_code_can_be_explained() {
    for n in 1..=15 {
        let code = format!("E-R{n:02}");
        let out = Command::new(env!("CARGO_BIN_EXE_mink")).args(["explain", &code]).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{code} has no explanation: {}", String::from_utf8_lossy(&out.stderr));
        assert!(text.contains(&format!("Error {code}:")) && text.contains("Category: runtime"), "{code}: {text}");
        assert!(text.contains("Common causes:") && text.contains("Suggested fixes:"), "{code}: {text}");
    }
    let leak = Command::new(env!("CARGO_BIN_EXE_mink")).args(["explain", "E-R06"]).output().unwrap();
    assert!(String::from_utf8_lossy(&leak.stdout).contains("exit code is 106"));
}
