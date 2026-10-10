#!/usr/bin/env python3
"""Milestone 1 memory / scaling battery.

Runs each completed validation app on progressively larger inputs, reporting
wall-clock time for the MINK build and for the Python reference, plus the exit
code (the runtime leak checker `E-R06` = 106 is the memory assertion).

    python validation/memory_battery.py

Sizes stay inside each app's documented limits (the JSON arena is ~8000 bytes,
the process stdin window is 4096 bytes). Everything runs from the repository
root.
"""
import os
import subprocess
import sys
import time

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MINK = os.path.join(REPO, "target", "debug", "mink.exe")
PY = sys.executable
TMP = os.path.join(REPO, "validation", "battery_tmp")


def run(exe_args, cwd=REPO):
    start = time.perf_counter()
    proc = subprocess.run(exe_args, cwd=cwd, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, timeout=300)
    elapsed = (time.perf_counter() - start) * 1000.0
    return proc.returncode, elapsed, proc.stdout, proc.stderr


def report(name, size, mink_res, py_res=None):
    code, ms, _out, err = mink_res
    mark = "ok" if code == 0 else f"exit {code}"
    if code == 106:
        mark = "LEAK (E-R06)"
    if err:
        mark += " [" + err.decode("utf-8", "replace").strip()[:60] + "]"
    line = f"{name:<26} {size:<10} mink {ms:8.1f} ms  {mark}"
    if py_res is not None:
        pcode, pms, _po, _pe = py_res
        line += f"   |  python {pms:8.1f} ms (exit {pcode})"
    print(line)


def write(path, data):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as handle:
        handle.write(data)
    return path


def main():
    os.makedirs(TMP, exist_ok=True)
    print("memory/scaling battery - repo root: %s\n" % REPO)

    # --- A3 textgrep: line count scaling -------------------------------
    for lines in (5_000, 25_000):
        body = "".join(
            f"2026-10-01T00:00:{i % 60:02d}Z {'ERROR' if i % 7 == 0 else 'INFO'} event {i}\n"
            for i in range(lines))
        log = write(os.path.join(TMP, f"a3_{lines}.log"), body.encode())
        report("a3_textgrep -c", f"{lines} lines",
               run([os.path.join(REPO, "validation", "a3_textgrep", "main.exe"),
                    "-c", "ERROR", log]),
               run([PY, os.path.join(REPO, "validation", "a3_textgrep", "python_ref.py"),
                    "-c", "ERROR", log]))

    # --- B csvdata: row count scaling ---------------------------------
    for rows in (2_000, 10_000):
        data = "id,customer,city,units,price\r\n" + "".join(
            f"{i},\"Acme, Inc.\",{['Berlin', 'Paris', 'Rome'][i % 3]},{i % 97},{i % 13}.50\r\n"
            for i in range(1, rows + 1))
        csv = write(os.path.join(TMP, f"b_{rows}.csv"), data.encode())
        report("b_csvdata agg", f"{rows} rows",
               run([os.path.join(REPO, "validation", "b_csvdata", "main.exe"),
                    "agg", csv, "2", "3"]),
               run([PY, os.path.join(REPO, "validation", "b_csvdata", "python_ref.py"),
                    "agg", csv, "2", "3"]))

    # --- C json: arena-bound scaling -----------------------------------
    for objs in (50, 150):
        doc = "[" + ",".join(
            f'{{"id":{i},"qty":{i % 31},"tag":"row {i}"}}' for i in range(objs)) + "]"
        path = write(os.path.join(TMP, f"c_{objs}.json"), doc.encode())
        report("c_json sum", f"{len(doc)} B, {objs} objs",
               run([os.path.join(REPO, "validation", "c_json", "main.exe"),
                    "sum", path, "qty"]),
               run([PY, os.path.join(REPO, "validation", "c_json", "python_ref.py"),
                    "sum", path, "qty"]))

    # --- D sqlite: statement count scaling -----------------------------
    for stmts in (500, 2_000):
        script = "CREATE TABLE t (id INT, name STR)\n" + "".join(
            f"INSERT INTO t VALUES ({i}, 'row{i}')\n" for i in range(stmts))
        script += "SELECT COUNT(*) FROM t\n"
        path = write(os.path.join(TMP, f"d_{stmts}.sql"), script.encode())
        report("d_sqlite script", f"{stmts} inserts",
               run([os.path.join(REPO, "validation", "d_sqlite", "main.exe"), path]),
               run([PY, os.path.join(REPO, "validation", "d_sqlite", "python_ref.py"), path]))

    # --- G threads: work scaling ---------------------------------------
    for n, workers in ((100_000, 4), (1_000_000, 16)):
        report("g_threads squares", f"n={n} w={workers}",
               run([os.path.join(REPO, "validation", "g_threads", "main.exe"),
                    "squares", str(n), str(workers)]),
               run([PY, os.path.join(REPO, "validation", "g_threads", "python_ref.py"),
                    "squares", str(n), str(workers)]))

    # --- H manifest: file count scaling --------------------------------
    for count in (100, 400):
        root = os.path.join(TMP, f"h_tree_{count}")
        if os.path.isdir(root):
            for base, _dirs, files in os.walk(root, topdown=False):
                for name in files:
                    os.remove(os.path.join(base, name))
                os.rmdir(base)
        for i in range(count):
            rel = f"d{i % 10}/f{i}.txt"
            write(os.path.join(root, *rel.split("/")), f"file {i}\n".encode())
        import json as _json
        cfg = {"root": root,
               "out": os.path.join(TMP, f"h_{count}_out.csv"),
               "extensions": [".txt"]}
        cfg_path = os.path.join(TMP, f"h_{count}.json")
        with open(cfg_path, "w", encoding="utf-8") as handle:
            _json.dump(cfg, handle)
        py_cfg = os.path.join(TMP, f"h_{count}_py.json")
        cfg = dict(cfg, out=os.path.join(TMP, f"h_{count}_out_py.csv"))
        with open(py_cfg, "w", encoding="utf-8") as handle:
            _json.dump(cfg, handle)
        report("h_manifest", f"{count} files",
               run([os.path.join(REPO, "validation", "h_manifest", "main.exe"), cfg_path]),
               run([PY, os.path.join(REPO, "validation", "h_manifest", "python_ref.py"), py_cfg]))

    print("\nAny non-zero exit other than the app's own status codes is a "
          "failure; exit 106 is the E-R06 leak checker.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
