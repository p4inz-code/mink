//! `rt_sys_call` with no arguments: GetCurrentProcessId, GetTickCount64 and GetLastError take none,
//! and used to be unreachable because a zero-argument call was a silent no-op returning 0.

use std::process::Command;

fn run(label: &str, source: &str) -> (i32, String) {
    let dir = std::env::temp_dir().join(format!("mink_syscall_{label}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("prog.mink");
    std::fs::write(&path, source).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mink")).arg("run").arg(&path).output().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
fn zero_argument_windows_calls_return_real_values() {
    let (code, stdout) = run(
        "zero",
        "fn main() -> Int {
    let kern = rt_sys_load_lib(\"kernel32.dll\");
    let args = rt_alloc(32);
    let pid = rt_sys_call(rt_sys_get_proc(kern, \"GetCurrentProcessId\"), args, 0);
    let ticks = rt_sys_call(rt_sys_get_proc(kern, \"GetTickCount64\"), args, 0);
    rt_free(args);
    if pid == rt_process_id() && ticks > 0 {
        rt_print_str(\"ok\");
        return 0;
    }
    rt_print_str(\"wrong\");
    return 1;
}
",
    );
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("ok"), "{stdout}");
}

#[test]
fn a_missing_function_reports_zero_instead_of_crashing() {
    let (code, stdout) = run(
        "null",
        "fn main() -> Int {
    let kern = rt_sys_load_lib(\"kernel32.dll\");
    let args = rt_alloc(32);
    let missing = rt_sys_get_proc(kern, \"ThisFunctionDoesNotExist\");
    let r = rt_sys_call(missing, args, 0);
    rt_free(args);
    rt_print_int(r);
    return 0;
}
",
    );
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.trim() == "0", "{stdout}");
}
