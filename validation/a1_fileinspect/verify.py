"""Compare the MINK fileinspect against the Python reference.

Runs both binaries over the same cases and reports any difference in exit
code, stdout fields or stderr text. Usage: python verify.py
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MINK = os.path.join(HERE, "main.exe")
PY = [sys.executable, os.path.join(HERE, "python_ref.py")]

CASES = [
    (["fixtures/app.log", "--sha", "--lines"], {}),
    (["fixtures/app.log", "--grep", "ERROR"], {}),
    (["fixtures/app.log", "--grep", "pid="], {}),
    (["fixtures/app.log", "--grep", "nomatch-xyz"], {}),
    # Anchored-match positives: `2026` occurs on every line but only the first
    # one starts with it; `^2026` pins the anchor explicitly.
    (["fixtures/app.log", "--grep", "2026"], {}),
    (["fixtures/app.log", "--grep", "^2026"], {}),
    (["fixtures/utf8.txt", "--sha"], {}),
    (["fixtures/crlf.txt", "--lines"], {}),
    (["fixtures/no_trailing_newline.txt", "--lines"], {}),
    (["fixtures/empty.txt"], {}),
    (["fixtures/nope.txt"], {}),
    (["fixtures"], {}),
    (["fixtures/big.txt"], {}),
    (["fixtures/big.txt", "--max-bytes", "4194304", "--sha"], {}),
    (["fixtures/big.txt", "--max-bytes", "4194304"], {"FILEINSPECT_MAX_BYTES": "100"}),
    ([], {}),
    (["--help"], {}),
    (["fixtures/app.log", "--max-bytes"], {}),
    (["fixtures/app.log", "--max-bytes", "abc"], {}),
    (["a", "b"], {}),
]


def run(cmd, env_over):
    env = dict(os.environ)
    env.update(env_over)
    proc = subprocess.run(cmd, capture_output=True, env=env, cwd=HERE)
    return proc.returncode, proc.stdout.decode("utf-8", "replace"), proc.stderr.decode("utf-8", "replace")


def main():
    failures = 0
    for args, env_over in CASES:
        m_code, m_out, m_err = run([MINK] + args, env_over)
        p_code, p_out, p_err = run(PY + args, env_over)
        label = " ".join(args) or "(no args)"
        problems = []
        if m_code != p_code:
            problems.append("exit %d != %d" % (m_code, p_code))
        if m_out != p_out:
            problems.append("stdout differs")
        if m_err.strip() != p_err.strip():
            problems.append("stderr differs")
        status = "OK  " if not problems else "DIFF"
        if problems:
            failures += 1
        print("%s %-52s %s" % (status, label, "; ".join(problems)))
        if problems:
            print("  mink  out: %r" % m_out[:300])
            print("  python out: %r" % p_out[:300])
            print("  mink  err: %r" % m_err[:200])
            print("  python err: %r" % p_err[:200])
    print("\n%d/%d cases differ" % (failures, len(CASES)))
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
