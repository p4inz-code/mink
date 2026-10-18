MINK v1.0.4 Release
====================

Release Identity
-----------------
- Tag: v1.0.4 (annotated; created locally. Not pushed, see "Registry state")
- Branch: main
- Platform: x86-64 Windows PE (Windows x86_64 is the frozen baseline)
- npm: `@p4inz-code/mink@1.0.4`: **prepared and independently verified, publish pending a
  valid token**. The registry still serves 1.0.3 (`npm dist-tag ls` → `latest: 1.0.3`).
- npm tarball: 28 files (`bin/mink.exe`, `package.json`, and all 26 `stdlib/*.mink`
  modules); `p4inz-code-mink-1.0.4.tgz`, 1348748 bytes (4332882 unpacked)
- npm tarball sha1 (`shasum`): `fdea0ad977dea7c2b8734ac87a5ca970fd373132`
- npm tarball integrity: `sha512-mFmIdnD8qYnrtmCWwKE38Cv5E063uw2vYOzFZ6tZuQHD/bg1GDnmpSG+tyMRj77t/+M55MdNR8Oi/fVO8zs21g==`
- Compiler binary SHA-256 (`target/release/mink.exe` == `npm/mink/bin/mink.exe`):
  `2bc97c9210d0605705e63b4ae448dc3554a1b4819bdbccf70386b0c09b13bf0c`
- Bundled `stdlib/`: byte-identical to the repository `stdlib/` (`diff -r` clean)

Starting commit: `8193ecb` (== `origin/main` at the start of the work) · ending commit:
the commit carrying this record.

This release closes the one public defect reported against 1.0.3. It is a defect closure,
not a feature release: no language surface, standard-library source, backend or
monomorphizer change is included.

The Defect
----------
A program that declared the bundled `collections` module built by 1.0.3 failed whenever
the module's `vec_min` / `vec_max` were never called:

```mink
mod collections;
mod filesystem;

fn main() -> Int {
    let v = rt_vec_new(4);
    rt_vec_free(v);
    rt_print_str("clean exit\n");
    return 0;
}
```

```text
$ mink check repro.mink      # passed (no output)
$ mink build repro.mink      # 1.0.3
mink: error[E-B07]: invalid backend IR (function `vec_min`: binary operand type does not
match the operator)
  --> <compiler>\stdlib\collections.mink:142:12
mink: error[E-B07]: invalid backend IR (function `vec_max`: ... )
  --> <compiler>\stdlib\collections.mink:156:12
```

`mink check` passed and `mink build` failed because the failure happened after type
checking, in the backend. Calling `vec_max`/`vec_min` from the program masked it (the call
site unified the parameters), which is why the module shipped unnoticed.

Root Cause
----------
One wrong layer, three symptoms:

1. An untyped function parameter starts as `TypeKind::Infer(None)`
   (`src/typecheck/checker.rs`, `pre_register`).
2. The checker's binary-operator rule deliberately linked two unconstrained operands
   without pinning them for `==`/`!=` and `<`/`>`-class operators ("any scalar is
   valid"). With no call site to unify the linked variables, `val < result` inside
   `vec_min` / `vec_max` stayed unresolved for the whole program.
3. The backend classified unresolved `Infer` as `BType::Unit` (`src/backend/lower.rs`,
   `classify`), and the backend verifier requires `Int` operands for `Lt/Le/Gt/Ge`
   (`src/backend/verify.rs`), hence `E-B07` for a function the program never called.

The first incorrect layer is the type checker's contract: unresolved operands must not
reach the backend as `unit`. The fix is placed there.

The Fix
-------
`src/typecheck/checker.rs` (+68 lines), standard library untouched:

- a `pending_scalar_operands: Vec<(TypeId, TypeId)>` field on the checker records the
  operand pairs of arithmetic, comparison, shift and bitwise operators, and of the
  compound-assignment path;
- `pin_remaining_operator_operands()` runs once at the end of type checking (after
  `resolve_deferred_members()`) and unifies each recorded pair with `Int`;
- unification only succeeds for still-unresolved operands, so call-site-resolved `Int`,
  `Float` and other concrete types are never overwritten. `vec_max`/`vec_min` therefore
  keep working for `Float` vectors when a call site pins them.

Regression Coverage
-------------------
Five permanent native-executing tests were added to `tests/collections_lib.rs` (which
builds and runs real executables) and assert values, not just exit codes:

| Test | Holds |
| --- | --- |
| `collections_module_import_builds_without_calling_aggregates` | the exact original failure: import only, build, run, "clean exit" |
| `collections_vec_min_max_int_runtime_values` | `[5,2,9]` → `vec_max` 9, `vec_min` 2 |
| `collections_vec_sum_int_runtime_value` | `[10,20,30]` → 60 |
| `collections_untyped_min_helper_runtime_values` | untyped `smaller(a, b)`: `2, -7` (including negatives) |
| `untyped_min_max_float_runtime_values` | direct `Float` comparisons through untyped functions are not re-pinned to `Int` |

`tests/typecheck.rs`'s `genuinely_ambiguous_types_stay_unresolved` asserted the old
contract that produced the defect; it was replaced by
`arithmetic_on_unconstrained_operands_resolves_to_int`, which documents the defect and
asserts the corrected result. (`Vec<Float>` element equality is a separate pre-existing
1.0.3 runtime defect, reproduced unchanged on the public 1.0.3 compiler, and is
deliberately not covered by this release.)

Full Regression
---------------
Run on the fixed source, all 71 integration targets plus the unit tests:

- 2973 passed · 0 failed · 1 ignored: 2852 integration passes over all 71 test
  targets, plus 121 unit tests. The one ignored test
  (`s62_system_store_public_https`) requires public internet access and says so.
- `tests/process_lib.rs` needs `NoDefaultCurrentDirectoryInExePath` unset on this
  machine: with it set, `cmd.exe` refuses a bare executable name from the current
  directory and 8 of its 80 tests fail, identically under the shipped 1.0.3 binary, so
  this is an environment property, not a compiler behaviour. With the variable unset the
  target is 80 passed · 0 failed.
- After the version-only bump to 1.0.4, the version-sensitive and defect-adjacent targets
  were re-run: 11 targets, 483 passed · 0 failed.
- `cargo build --release` clean; `cargo fmt --check` clean; `cargo clippy --all-targets`
  reports exactly the pre-existing baseline warning count (89 with the fix, 89 with the
  fix stashed, no new warnings).

Verified From the Exact Tarball
-------------------------------
The publish gate below was verified by installing the built tarball, not the source tree:

- `npm pack` → 28 files, 26 `stdlib/` modules, version 1.0.4;
- fresh isolated install (`npm install -g --prefix <empty dir> p4inz-code-mink-1.0.4.tgz`),
  the directory not on any PATH → `mink --version` prints `mink 1.0.4`;
- with that install only: the original `mod collections;` repro builds and runs (exit 0,
  "clean exit"), `vec_max`/`vec_min` on `[5,2,9]` print `9` and `2`, the untyped `smaller`
  helper prints `2` and `-7`, `vec_sum` prints `60`;
- CLI workflow through the same install: `mink init` writes a manifest, `mink test`
  discovers and passes a `fn test_*`, `mink build` produces the exe, `mink run` prints the
  expected output, `mink repl` evaluates piped statements.

Downstream Re-verification (mink-scout)
---------------------------------------
The frozen showcase application was rebuilt from the 1.0.4 install and re-verified
unchanged:

- `mink check main.mink` clean, `mink build main.mink` exit 0;
- fixture report identical to its 1.0.3 build (byte-for-byte after CRLF normalisation):
  Files 13, Total bytes 4479, Extensions 4, Unique contents 10, Duplicate files 6,
  Largest file `fixture/demo-project/assets/model.bin`, Largest size 4096, SHA-256
  `db7d832d8e0891e5522d29dd93130bbb1a41b123f0d2f9a1169c2cfc1b98b8cf`, extension table
  `.txt 6 169 | .bin 3 4160 | .json 2 80 | .mink 2 70`; `verify_fixture.py` → `VERIFY: PASS`;
- exit codes: no argument 1, two arguments 1, missing directory 2, file-instead-of-directory
  2, empty directory 0 (Files 0 / Largest size 0), trailing slash 0, spaces in the path 0,
  unreadable file 2 (verified with an exclusive handle; an ACL deny entry has no effect for
  this account's token);
- repeatability: 5 runs, exit 0, empty stderr, one distinct stdout hash;
- standalone: copy of the exe plus fixture in a clean directory with `PATH` reduced to
  `C:\Windows\System32` reproduces the report and `VERIFY: PASS`;
- generated artifact: PE32+ x86-64, 4 sections, imports only `kernel32.dll`.

Registry State
--------------
Publishing was attempted and is blocked by credentials, exactly as recorded for 1.0.3:

```text
$ npm whoami
npm error code E401 Unauthorized - GET https://registry.npmjs.org/-/whoami

$ npm publish --access public          # from npm/mink
npm notice Publishing to https://registry.npmjs.org/ with tag latest and public access
npm error code E404
npm error 404 The requested resource '@p4inz-code/mink@1.0.4' could not be found or you
do not have permission to access it.

$ npm view @p4inz-code/mink version    # after the attempt
1.0.3
```

The token in `C:\Users\Admin\.npmrc` is rejected by the registry. Nothing was published
and the registry is unchanged. The maintainer command, once a valid token is stored, is:

```bash
cd npm/mink
npm publish --access public
```

After that, the release gate is: `npm view @p4inz-code/mink version` → `1.0.4`,
`npm dist-tag ls @p4inz-code/mink` → `latest: 1.0.4`, a fresh install in a directory that
is not on any PATH printing `mink 1.0.4`, and the tarball/compiler hashes above matching
the registry metadata. By the decision recorded with this release, the commit and the
annotated tag `v1.0.4` are local; `origin/main` is intentionally left at `8193ecb` until
the package can be published.

Observations Not Fixed Here (Out of Scope)
------------------------------------------
- `mink explain E-B07` reports an unknown code on 1.0.3 and 1.0.4 alike; the documented
  code table lists only `E-B08`/`E-B11` for backend diagnostics.
- `Vec<Float>` element equality (`rt_vec_get(v, 1) == 7.5`) is false; reproduced
  identically on the public 1.0.3 compiler (see `tests/collections_lib.rs`).
- Generated executables are deterministic for a given install, but not byte-identical
  across install locations: the PE embeds stdlib path strings in its diagnostic location
  table (for example `...\stdlib\hashing.mink:108`), and a longer path can shift a section
  across a page boundary. Behaviour is unchanged.
- `use name;` parses and binds nothing (`mod name;` is what loads a module); recorded as a
  behaviour note in the mink-scout defect notes, unchanged by this release.

Windows remains the completed, frozen baseline; Linux remains frozen and unsupported. No
new feature cycle is started by this release.
