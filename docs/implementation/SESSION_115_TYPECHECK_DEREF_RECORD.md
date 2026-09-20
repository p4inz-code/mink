# Session 115 — Deref place records and stale parser-hardening claims

**Parity blockers closed:** none (defect fix + stale-test correction)
**Parity-blocking set:** 5 (S62, P04, P05, P06, P09) — unchanged
**Linux:** frozen.

## Defect 1 — `*r` places kept reading as `unknown`

**Symptom.** `tests/typecheck.rs::references_flow_through_calls` failed:

```
fn bump(p) { *p = *p + 1; } fn main() { let v = 10; let m = &mut v; bump(m); }
assertion `left == right` failed: type of `*p`
  left: "unknown"
 right: "Int"
```

The failure reproduced on clean `ae1aeba` with the Session 114 work stashed,
so it was a long-standing defect (the test has been red since the deferred
re-type pass landed), not an async regression. No error was reported — only
the recorded expression type was wrong.

**Root cause.** Two independent problems in the same place
(`src/typecheck/checker.rs`):

1. `check_assign`'s `ExprKind::Deref` arm never typed its target through
   `expr_type`; it read the *operand*'s type and, when the operand was still
   an unresolved inference variable (an unannotated parameter no call site has
   pinned yet), pushed a raw `Error` entry at the target's span and returned
   `Error`. Because the deref place was never typed through `check_expr`, it
   was also never registered in `deferred`, so the re-type pass skipped it
   (`operand_recomputed` was false and `deferred` did not contain the span).
2. `resolve_deferred_expr` recomputed a subtree's type but only wrote it back
   for the `Try`, `Group` and `Tuple` arms. `recorded_ty` and
   `expr_type_exact` match the **first** entry for a span, so the forward
   pass's stale entry shadowed every later, correct entry — the recomputed
   type was visible to callers of `resolve_deferred_expr` but not to anyone
   reading the recorded table (which is what HIR lowering and the tests use).

**Fix.**

- `check_assign`'s deref arm now types the place through `expr_type(target)`,
  mirroring the `Ident`/`Member`/`Index` arms. That records the referent type
  for HIR lowering, and when the operand is unresolved `check_expr` registers
  the place in `deferred` as it does for a deref read. Mutability is decided
  from the **operand's** reference type (looked up with `recorded_ty`, since
  the place's own type is the referent), so writes through `&T` still report
  E-T21.
- `resolve_deferred_expr` is now a thin wrapper that writes the recomputed type
  back over the forward pass's entry (`update_recorded`) whenever anything in
  the subtree was re-typed; the old body is `resolve_deferred_expr_inner`.
  This makes the write-back uniform instead of arm-by-arm, which is what the
  `update_recorded` doc comment already described as the intended design.

**Verification.** `tests/typecheck.rs` 163/163 (was 162 passed / 1 failed);
`references` 58, `ownership` 43, `hir` 25, `mir` 34, `backend` 46,
`semantics` 75, `try_operator` 12, `adversarial` 93 (1 ignored),
`optimization` 38, `parser` 98, `source` 12, `scalar_types` 27, `runtime` 24,
`smoke` 13, `strings` 63, `session99` 13, `session100` 14, `aggregate` 59,
`release` 68, `cli` 74 (1 ignored), `packages` 5, `s69_hardening` 11,
`test_runner` 12, `cargo test --lib` 62 — all green. The previously failing
test is the permanent regression for defect 1.

## Defect 2 — stale parser-hardening claims about `async` / `await`

`tests/parser_hardening.rs` asserted that `async` and `await` are *excluded*
keywords that must be rejected, which Session 114 made false: they are real
keywords now. Three tests were corrected to state the truth instead of being
deleted:

- `excluded_declarations_at_top_level_are_rejected` no longer lists
  `async fn f() {}` (the `enum` note is now joined by an `async fn` note).
- `excluded_constructs_inside_functions_are_rejected` uses the genuinely
  rejected bare `await;` (E-P32) instead of the now-legal `await x;`.
- `excluded_keywords_are_never_silently_accepted` drops `async`/`await` from
  the rejected keyword list.
- New `async_fn_and_await_are_accepted` pins the positive grammar:
  `async fn` with and without a return annotation and with a parameter,
  `pub async fn`, `await` of a handle, and `await` inside a larger expression.

`tests/parser_hardening.rs` 64/64 (was 60 passed / 3 failed).

## Final state

- Parity-blocking set: **5** (S62, P04, P05, P06, P09) — unchanged by this
  session; the fix is a correctness fix on the shared typechecker, not a new
  capability.
- `cargo fmt --check` clean.
- Linux frozen.
