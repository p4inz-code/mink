# Releasing MINK 1.0.5

The steps, in order. Steps 1 to 4 are done by tools in this repository. Steps 5 to 7 publish to the
outside world and wait for the owner's go-ahead.

1. **Build.** `cargo build --release --bin mink`
2. **Test.** `cargo test` with the full suite, then `git status` to check nothing unexpected changed.
3. **Make the release files.** `powershell -File tools\build_installer.ps1 -Exe target\release\mink.exe`
   writes the setup file, the portable zip, the payload and a `.sha256` for each into `dist\`.
4. **Make the winget files.** `powershell -File tools\make_winget_manifest.ps1` reads the setup file's
   hash. Do this again if the setup file is rebuilt, because the hash changes.
5. **GitHub release.** Create the release `v1.0.5` and upload the setup file, the portable zip and the
   `.sha256` files unchanged.
6. **npm.** Copy `target\release\mink.exe` to `npm\mink\bin\mink.exe`, run `npm pack --dry-run` in
   `npm\mink` and check the file list, then `npm publish --access public`. Publishing cannot be undone.
7. **winget.** Open a pull request with the files from `packaging\winget\1.0.5\` against
   `microsoft/winget-pkgs`.

## Checks before step 5

| Check | How |
|---|---|
| Versions agree | `Cargo.toml` and `npm\mink\package.json` both say 1.0.5; `mink --version` prints 1.0.5 |
| The npm binary is the new one | `npm\mink\bin\mink.exe lsp --help` works |
| Credit travels | `LICENSE` and `NOTICE` are in the npm file list and the portable zip |
| Setup works silently | Run the setup file from PowerShell with `Start-Process -Wait` and `/VERYSILENT` into a test folder, check `mink --version`, then remove it |
| Setup works on screen | Open it, go through every page, go back and forward, check About and Help |
| Unsigned notice | The release notes say the file is unsigned for now |

## Release notes

Use the 1.0.5 section of [`CHANGELOG.md`](../CHANGELOG.md).
