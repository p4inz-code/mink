# M6 Session 176 exercises

Small language-learning programs written from developer-facing documentation
only. Each case is built and run in a disposable directory so generated
executables and data files never enter the source tree.

Run all cases with the repository compiler:

```bash
python benchmarks/m6/session176/verify.py
```

Run with another compiler, for example the npm-installed one:

```bash
python benchmarks/m6/session176/verify.py --compiler <path-to-mink.exe>
```

The verifier copies the repository `stdlib/` into each disposable case
directory, so it works with a repository build (`target/release/mink.exe`),
the npm package binary, or the M6 reference binary. A compiler only searches
`<exe>/../stdlib` and `<cwd>/stdlib`; the case directory is temporary, so the
standard library has to be supplied there for `mod` imports to resolve.

The verifier checks exact stdout (after CRLF normalization) and exit codes.
Program B accepts `empty` and `single` arguments to exercise dataset
boundaries. Program C handles a missing file and an unwritable path.
