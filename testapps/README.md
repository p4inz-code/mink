# Test apps

Small programs written in MINK to find out what works. Each one is built with the release compiler and run
on known input; the automated ones are in `tests/testapps.rs`. The plan is in
[`docs/testing/TEST_APPS_PLAN.md`](../docs/testing/TEST_APPS_PLAN.md) and what went wrong is in
[`docs/testing/FRICTION_LOG.md`](../docs/testing/FRICTION_LOG.md).

Build and run any of them:

```
mink build testapps/jsonq/main.mink
.\testapps\jsonq\main.exe data.json .items[0].name
```

| App | What it does | Lines | Exe size | Result |
|---|---|---|---|---|
| [sitegen](sitegen/main.mink) | Builds the MINK website from a template and a data file | 106 | see build | Builds `site/index.html`; tested |
| [jsonq](jsonq/main.mink) | Picks a value out of a JSON file, like a small `jq` | 146 | 121 KB | 551 KB file queried in 28 ms; built in 17 ms; tested |
| [fgrep](fgrep/main.mink) | Finds text in the files under a folder, with `-i` and binary-file skipping | 171 | see build | Matched PowerShell `Select-String` exactly on 3.1 MB of source (2,622 case-sensitive lines, 2,664 ignoring case). 181 ms against 42 ms for `Select-String`; tested |
| [dupfind](dupfind/main.mink) | Finds duplicate files by size and SHA-256 | 136 | see build | Matched `Get-FileHash` exactly on 303 files (134 groups, 147 extra copies, 4,648,639 bytes). 1,549 ms against 217 ms; tested |
| [serve](serve/main.mink) | A static file server for one folder on 127.0.0.1, with content types and refusal of `..` paths | 198 | see build | 300 requests for a 19 KB file, all correct, server exit 0 with no leaks. Serves this website; tested |
| [todo](todo/main.mink) | A whole web app in one .exe: REST API, SQLite database in a file, and the page itself | 289 | see build | Add, toggle, delete and restart verified over HTTP, JSON escaping correct, exit 0 with no leaks; tested |
| [paste](paste/main.mink) | A tiny pastebin: post text, get a six-character link | 181 | see build | Newlines, tabs, quotes and backslashes come back byte for byte; exit 0 with no leaks; tested. Found a base64 decoding bug (fixed) |
| [widget](widget/main.mink) | A real Windows window with a live clock | 133 | see build | Window found by title, closes by itself, exit 0, no leaks. Not in the automated tests, since it opens a window |

Numbers were measured on the author's PC (Windows 11) with the 1.0.5 release build. More apps are added as
they are finished.
