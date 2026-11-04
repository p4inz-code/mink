# M6 Session 176 exercises

Small language-learning programs written from developer-facing documentation
only. Each case is built and run in a disposable directory so generated
executables and data files never enter the source tree.

Run all cases with the repository compiler:

```bash
python benchmarks/m6/session176/verify.py
```

Run with the installed npm compiler:

```bash
python benchmarks/m6/session176/verify.py --compiler tmp/m6_install_175/node_modules/@p4inz-code/mink/bin/mink.exe
```

The verifier checks exact stdout (after CRLF normalization) and exit codes.
Program B accepts `empty` and `single` arguments to exercise dataset
boundaries. Program C handles a missing file and an unwritable path.
