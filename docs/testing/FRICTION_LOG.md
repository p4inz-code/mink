# Friction log

Every problem found while building the test apps. One entry each: what happened, the smallest
program that shows it, and whether it is a bug, a missing feature or a rough edge.

| # | App | What happened | Kind | Status |
|---|---|---|---|---|
| 1 | field check | `rt_sys_call` with zero arguments always returned 0 | bug | fixed on next-1.0.6 |
| 2 | field check | A program that does not free every string ends with E-R06 and exit 106, even when its output is right | rough edge | open |
| 3 | field check | `mink run` takes no program arguments | missing feature | open |
| 4 | field check | A repository build cannot find the standard library when run from another folder | rough edge | open |
| 5 | widget (spike) | A real Windows window works: create a STATIC window through `rt_sys_call`, pump messages by hand, update text, close. A window found by title, exit 0, no leaks | finding | works |
| 6 | widget (spike) | MINK cannot hand Windows a callback, so no custom window procedure, and button clicks and painting must be polled | missing feature | open (runtime milestone) |
| 7 | widget | `rt_to_cstr` returns `Ptr<Int>`, and `rt_mem_store` wants `Int`, so every pointer stored needs `rt_ptr_to_int` | rough edge | open |
| 8 | https (spike) | HTTPS works: `https_client_get(url, "")` reached github.com (200, 468,710 bytes) using the Windows trust store. The reference wrongly said a CA bundle was required; a non-empty path pins one certificate. `example.com` did not resolve from this PC, which looked like a TLS failure at first | docs bug | fixed in the reference |
| 9 | jsonq, sitegen | A user function that takes `Str` by value moves the string, so using it afterwards is E-S10. Intrinsics (`rt_str_len`, `rt_str_byte`) will not take `&Str`, so a borrowing helper cannot read a string. The workaround is to pass a copy (`rt_str_concat(s, "")`) and free it inside the function | rough edge | open |
| 10 | jsonq | `json_parse_in` needs about four slots (32 bytes) per input byte, because the arena holds one word per byte plus the tree | rough edge | documented in the app; open |
