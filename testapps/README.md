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
| [widget](widget/main.mink) | A real Windows window with a live clock | 133 | see build | Window found by title, closes by itself, exit 0, no leaks. Not in the automated tests, since it opens a window |

Numbers were measured on the author's PC (Windows 11) with the 1.0.5 release build. More apps are added as
they are finished.
