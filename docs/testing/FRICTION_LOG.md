# Friction log

Every problem found while building the test apps. One entry each: what happened, the smallest
program that shows it, and whether it is a bug, a missing feature or a rough edge.

| # | App | What happened | Kind | Status |
|---|---|---|---|---|
| 1 | field check | `rt_sys_call` with zero arguments always returned 0 | bug | fixed on next-1.0.6 |
| 2 | field check | A program that does not free every string ends with E-R06 and exit 106, even when its output is right | rough edge | open |
| 3 | field check | `mink run` takes no program arguments | missing feature | open |
| 4 | field check | A repository build cannot find the standard library when run from another folder | rough edge | open |
