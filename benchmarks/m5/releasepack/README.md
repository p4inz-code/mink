# releasepack — M5 Application 3

Deterministic Windows release ZIP archives from a JSON configuration, with a
sorted SHA-256 CSV manifest, verification and safe extraction. The frozen
contract is section 7 of
[`docs/benchmarks/M5/M5_APPLICATION_SELECTION.md`](../../../../docs/benchmarks/M5/M5_APPLICATION_SELECTION.md).

## Usage

```text
releasepack build <config.json>
releasepack verify <archive.zip> <manifest.csv>
releasepack extract <archive.zip> <dest-dir>
releasepack --help
```

Build config (UTF-8 JSON object, exactly these five keys):

```json
{
  "root": "tree",
  "archive": "out/release.zip",
  "manifest": "out/manifest.csv",
  "include_extensions": [".txt", ".json"],
  "exclude_dirs": ["out", "skip"]
}
```

- `root` must be an existing directory; all paths are relative to the current
  working directory.
- `archive` and `manifest` parents must already exist, must not be inside
  `root` (prevents self-inclusion), and must be different paths.
- `include_extensions`: 1–8 entries matching `^\.[A-Za-z0-9]{1,8}$`,
  case-insensitive.
- `exclude_dirs`: 0–8 non-empty base names, matched case-insensitively at any
  depth.

Compression is fixed at level 6 and timestamps are the stdlib's fixed DOS epoch
(1980-01-01), so identical trees produce byte-identical archives. Entries are
sorted by relative path in byte order and use `/` separators.

| Artifact | Contents |
| --- | --- |
| `archive` | ZIP with one entry per selected file |
| `manifest` | `path,size,sha256` with LF records and RFC-4180 quoting |
| build stdout | `files=N bytes=N archive_bytes=N`, CRLF |

An empty match set is exit 5 and writes no archive or manifest.

`verify` reads the CSV manifest and the ZIP central directory and requires the
same file set, sizes and SHA-256 values. It prints `verified files=N` on
success. Mismatches print sorted lines and exit 6:

```text
MISMATCH <path> extra      archive entry absent from the manifest
MISMATCH <path> missing    manifest row absent from the archive
MISMATCH <path> size       byte sizes differ
MISMATCH <path> sha256     digests differ
MISMATCH <path> corrupt    the entry cannot be decoded
```

A corrupt or unsupported archive is a `releasepack:` stderr diagnostic and
exit 6 rather than a MISMATCH line (there is no meaningful path to report).

`extract` creates `dest-dir` if missing (one level; its parent must exist) and
uses the stdlib's validated extraction: absolute, drive-qualified, UNC, `..`
traversal, NUL and ADS-shaped entry names are rejected before anything is
written. stdout on success is `extracted files=N`, CRLF.

| Exit | Meaning |
| ---: | --- |
| 0 | Command completed successfully |
| 1 | Usage/command/arity error |
| 2 | Config, archive or manifest missing / not a regular file |
| 3 | Config or manifest is not valid JSON/CSV or fails schema/type validation |
| 4 | Root/output/destination filesystem failure, output inside root, or a non-relative path |
| 5 | Build matched no files |
| 6 | Corrupt/unsupported/unsafe archive, or manifest/archive mismatch |

Diagnostics are `releasepack: <label>` on stderr with no stdout before a fatal
error.

## Build

From the repository root:

```bash
target/release/mink.exe build benchmarks/m5/releasepack/app/main.mink
mkdir -p benchmarks/m5/releasepack/build
cp benchmarks/m5/releasepack/app/main.exe benchmarks/m5/releasepack/build/releasepack.exe
```

One native PE executable with no sidecar files. Build outputs are ignored.

## Test

```bash
python benchmarks/m5/releasepack/tests/driver.py
```

`--no-build` reuses the executable, `--update-goldens` rewrites `expected/`
from the oracle, and `--only <text>` runs a subset.

`tests/oracle.py` is an independent contract oracle: it walks the fixture tree
itself, computes sizes and SHA-256 with `hashlib`, writes the expected CSV with
Python's `csv`, and inspects produced archives with Python's `zipfile`
(entry order, sizes, digests, fixed epoch, DEFLATE, UTF-8 name flag). It does
not reproduce MINK's DEFLATE bytes; determinism is checked by comparing two
independent builds byte-for-byte.

The suite covers the committed fixture tree, byte-exact manifest/stdout/help
goldens, all exit classes, malformed and boundary configs, output-inside-root
and unsafe paths, a Windows sharing-lock read failure, verify mismatches
(extra/missing/size/sha256, sorted), corrupt archives, traversal/absolute/drive
entry names, extraction round-trips, three repeated builds, and execution from
outside the repository.

## Semantics and recorded boundaries

- Manifest rows are sorted by path in byte order, the same order as the ZIP
  entries. CSV records end in LF; stdout ends in CRLF.
- `archive_bytes` is the size of the produced ZIP.
- `size` is the uncompressed file size; `sha256` is the full 64 lowercase hex
  digest of the file content.
- `verify` accepts stored and DEFLATE entries, and entries in any order; it
  compares by name, not position.
- `extracted files=N` counts extracted archive entries. This application's
  builds emit exactly one entry per file; a foreign archive with directory
  entries counts those entries too.
- Output paths are required to be plain relative paths: absolute paths, drive
  qualifiers, `:` and `..` segments are rejected with exit 4. This is stricter
  than the frozen text (which fixes `root` as relative but does not spell out
  output paths) and it makes the inside-root check sound.
- Build writes the archive first and the manifest second. A failure on the
  second write returns 4; the already-written archive is not rolled back.
- Extraction validates every entry name before writing, so an unsafe archive
  leaves no extracted files. A filesystem failure *during* extraction can leave
  the files written before the failure; that is reported as exit 4, not
  claimed as atomic.
- No signing, encryption, ZIP64, multi-disk archives, comments, symlinks,
  permissions, timestamps, incremental archives, compression-level option or
  remote storage. No Unicode path guarantee; fixtures are ASCII.
- Everything is buffered: the stdlib ZIP reader/writer and the 4 MiB runtime
  heap are comfortable for the frozen target (≤ 200 KiB uncompressed), not for
  arbitrarily large trees.
- No MINK compiler, runtime or stdlib source is modified by this application.
