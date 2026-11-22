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
| 11 | todo | **SQL `UPDATE` with an expression updated every row.** `UPDATE t SET done = 1 - done WHERE id = 1` changed both rows; `SET done = done + 1` wrote the text `done` into every row; a misspelt `WHERE` in `DELETE` deleted every row. The engine ignored text after the value and then saw no WHERE. A trailing `;` was stored inside the value (`8;`) | **bug (data loss)** | **fixed on next-1.0.6**: such text is now an error and the row data is untouched; `;` ends a value. Regression test in `tests/sqlite_lib.rs` |
| 12 | todo | The SQL engine only supports `SELECT *` and `SELECT COUNT(*)`; `SELECT done FROM t` answers with an error text in the result string, which a program can mistake for data | missing feature | open (documented limit) |
| 13 | todo | The ownership check is not path-sensitive: a variable freed and then assigned in both branches of an `if` is still reported as moved. Assigning it in a straight line fixes it | rough edge | open |
| 14 | todo | A temporary passed straight into a call (`rt_str_concat(a, sql_quote(x))`) is never freed, so the leak check fires at exit; each temporary needs its own variable and a `rt_str_free` | rough edge | open |
| 15 | paste | **`base64_decode_alloc` decoded every lowercase letter as an uppercase one** (`_b64_val` returned `c - 97` instead of `c - 97 + 26`), so any text with a lowercase letter came back wrong while its length looked right. The existing tests only checked the length | **bug** | **fixed on next-1.0.6**; the new test checks the decoded bytes |
| 16 | livelog | **`rt_fs_read` could not read a file another program had open for writing** (it asked Windows for `FILE_SHARE_READ` only, so a live log gave a sharing violation and an empty string). The old compiler read 0 bytes of a file held open for writing; the fixed one reads all 23 | **bug** | **fixed on next-1.0.6** (share mode now read, write and delete); `tests/fs_share_mode.rs` |
| 17 | mini | Writing a parser over strings is awkward because every call moves its string. The workaround, used by the JSON library too, is to copy the text into an arena of integers and pass a pointer, which is not moved. Fine once known, but nothing in the docs says so | rough edge | open |
