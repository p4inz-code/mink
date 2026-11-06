# M6 — GitHub Synchronization

Operation `M6-RC1`, 2026-11-05. Remote: `https://github.com/p4inz-code/mink`
(public). Branch: `main`.

Pushing the recovered commits to the already-configured `origin/main` was authorized for
this operation. Nothing else was authorized, and nothing else was done: no tag was
created, moved or deleted; no release was created; nothing was published to npm; no
force-push was used.

## 1. Commit groups pushed

The recovery was committed as named-path groups, never with broad staging:

| Commit | Scope | Files |
| --- | --- | --- |
| `54ccfd8` | recovered M3 Windows runtime, stdlib and tests (Sessions 140–149) | 23 |
| `3b7b2c5` | recovered roadmap lock and Sessions 128–149 records | 34 |
| `b6b164c` | preserved LICENSE and README worktree state | 2 |
| `13b1834` | merge of `origin/main` into local `main` (`--no-ff`) | 5 conflicts resolved |
| `ab0e80c` | environment-module ownership fix and its regression | 3 |
| `a640773` | 1.0.5 candidate manifests, refreshed binary, package LICENSE and release guard | 6 |
| `0d7a880` | README release state and M6-RC1 records | 5 |

Sessions 140–149 were committed as one unit because Session 141's recorded hunks depend
on the same CFG-aware path in `src/mir/lower.rs`; splitting them would have produced a
tree that neither builds nor passes.

## 2. Integration of the remote side

At the start of the operation the local branch was 44 commits ahead and 1 behind. Six
doc-only commits had landed on `origin/main` since the last local record. The repository
was not reset; the remote side was merged instead, and five conflicts were resolved by
taking the `HEAD` blobs, because `HEAD` carried the newer authoritative content (the
remote plain-language pass had, for example, dropped `Vec<T>`, `Map<K,V>` and `Set<T>`
from the README type list).

## 3. Push result

```text
$ git rev-list --left-right --count origin/main...main
0       51
$ git push origin main
To https://github.com/p4inz-code/mink
   fbcc6dc..0d7a880  main -> main
```

Exit status 0. Fast-forward, 51 commits, no forced update.

## 4. Post-push verification

After `git fetch origin --tags`:

```text
$ git rev-parse main origin/main
0d7a880b41393c90f071c8279467a7f090fe2281
0d7a880b41393c90f071c8279467a7f090fe2281
$ git rev-list --left-right --count origin/main...main
0       0
```

Local `main` and `origin/main` are the same commit — `0d7a880`. The branch is neither
ahead nor behind.

## 5. Tags

Tags were read before and after the push with `git ls-remote --tags origin` and each
remote tag's commit was compared with the local target:

| Tag | Commit | Match |
| --- | --- | --- |
| `v1.0.0` | `aaf88666a18c2cc23cdd81ff058847be41e97226` | yes |
| `v1.0.1` | `3e6a7a64c2d56f680a40578b43dfe7839fff8907` | yes |
| `v1.0.2` | `2a9316962ee19621960debd836e7337332782bf9` | yes |
| `v1.0.3` | `488e4746f445eadadb21f9ed92b54534373d41c2` | yes |
| `v1.0.4` | `dd556a692b8290416e14703471307cc311995d2f` | yes |

All five are unchanged and still point at their original commits. No `v1.0.5` tag exists:
the candidate is not released, so tagging it would have claimed a release that does not
exist.

## 6. What is not on the remote, by design

Four untracked files were preserved on disk and deliberately not committed:

- `MINK UI SETS OFFICIAL.zip`
- `freebuff-chat-2026-10-01T19-08-00.600Z.md`
- `nl1.mink`
- `nl1.exe`

They are not part of the recovery, and a binary archive and a chat export do not belong
in the repository history.

## 7. Registry state at the end of the operation

```text
$ npm view @p4inz-code/mink version
1.0.4
```

The published package remains 1.0.4 (published 2026-09-30). The verified 1.0.5 candidate
exists only as a local tarball
(`p4inz-code-mink-1.0.5.tgz`, SHA-256
`cfafc7a810d38e205475eb24a7009f1dfd0fc05feaad4beecfd24c50efc7df6f`) plus its source in
this repository. Its publication requires a maintainer with a valid npm token:

```bash
npm publish ./npm/mink --access public
```
