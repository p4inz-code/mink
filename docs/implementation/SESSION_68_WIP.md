# SESSION_68 — Work-in-Progress Analysis

## Phase 0 Complete: Baseline Confirmed
- 12 tests pass, 14 tests IGNORED (all ACCESS_VIOLATION 0xC0000005)
- Exit code -1073741819 = 0xC0000005 = STATUS_ACCESS_VIOLATION

## Phase 1: Root-Cause Analysis (IN PROGRESS)

### Diagnostic Test Results (5 rounds)

#### Round 1 (net_diag.rs) — Pattern Discovery
- PASS: n01 (init), n02 (init idempotent), n03 (cleanup), n20-22 (bind/listen), n30 (connect refused), n41 (ntohs), n60-61 (helpers), n81 (double close), n92 (resolve)
- All 14 IGNORED tests fail with ACCESS_VIOLATION

#### Round 2 (net_diag2.rs) — Pattern Isolation
**PASS (normal exit):**
- d2_01: init + socket + cleanup → 0
- d2_02: init + htons(no cleanup) → 0
- d2_06: init + close(inline) → 0
- d2_08: init + return socket → 264
- d2_09: init + cleanup → 0
- d2_10: init + socket + cleanup + close → 0

**CRASH (0xC0000005):**
- d2_03: htons without init → CRASH
- d2_04: socket + close + cleanup without init → CRASH
- d2_05: init + close(inline) + cleanup → CRASH
- d2_07: init + close(inline) + cleanup → CRASH

#### Round 3 (net_diag3.rs) — Precise Pattern
**PASS:**
- d3_01: init + socket + cleanup (NO close) → 0
- d3_02: init + socket + cleanup + close → 0
- d3_05: init only → 0
- d3_06: init + socket → 0
- d3_07: init + socket + close → 0
- d3_10 (n20): init + socket + bind + close + cleanup → 0
- d3_11 (n81): init + socket + close + close + cleanup + if → 1
- d3_12: if-else only → 1

**CRASH:**
- d3_03: init + socket + close(stored) + cleanup → CRASH
- d3_04: init + close(inline) + cleanup → CRASH
- d3_08: init + socket + close(stored) + cleanup → CRASH
- d3_09 (n10): init + socket + close + cleanup + if → CRASH

#### Round 4 (net_diag4.rs) — Step-by-Step Markers
**CRITICAL FINDING:** Markers printed: 10 (before init), 11 (after init), 12 (after socket), 13 (after close), **14 NOT printed** → crash DURING net_cleanup(), not during net_close()

- d4_01: CRASH after last rt_print_int
- d4_02: stdout="10\r\n11\r\n12\r\n13\r\n" → CRASH in net_cleanup()
- d4_04: init + close(-1) + cleanup → CRASH (invalid socket also crashes)
- d4_05: init + close(0) + cleanup → CRASH

#### Round 5 (net_diag5.rs) — Function-by-Function
**PASS (works before cleanup):**
- d5_01: init + cleanup only → 0
- d5_03: init + socket + shutdown + cleanup → 0
- d5_04: init + socket + send + cleanup → 0
- d5_05: init + last_error + cleanup → 0
- d5_07: init + htons(no cleanup) → 0 (but returns 0, not 513!)

**CRASH (causes cleanup to crash):**
- d5_02: init + close(-1) + cleanup → CRASH (markers 1,2 printed, not 3)
- d5_06: init + htons(258) + cleanup → CRASH (htons prints "0", wrong!)

### Key Findings

1. **WSACleanup crashes AFTER certain function pointer calls (call_rax)**
   - WSACleanup works fine alone (d5_01) or when called BEFORE close (d3_02)
   - WSACleanup crashes when preceded by closesocket (d5_02, d3_08, d3_09)
   - WSACleanup also crashes when preceded by htons (d5_06)

2. **htons returns wrong value (0 instead of 513)**
   - d5_07: init + htons(258) → prints "0", returns 0
   - But native Rust reference test: htons(258) = 513 ✓
   - n41 passes only because both net_htons and net_ntohs return the same wrong value

3. **The crash is NOT in the MINK exit/leak-check code** — exit code is 0xC0000005, not 106 (E-R06)

4. **Functions that DON'T cause the crash when called before cleanup:**
   - socket (via net_tcp_socket)
   - shutdown
   - send
   - WSAGetLastError

5. **Functions that DO cause the crash when called before cleanup:**
   - closesocket (via net_close)
   - htons (via net_htons)

### Hypothesis (needs verification)

The crash pattern suggests that certain `call_rax` invocations to WinAPI functions may be corrupting stack state or callee-saved registers. The pattern is:
- Some call_rax functions work fine (socket, shutdown, send)
- Some call_rax functions cause WSACleanup to crash (closesocket, htons)

The root cause likely involves **stack alignment or shadow space handling** in the `call_net_func` helper or the individual networking service emitters. The MINK runtime uses its own calling convention (stack args, result in RAX) but the WinAPI functions use the Windows x64 ABI (register args, shadow space, 16-byte stack alignment).

### emit_net_htons BUG CONFIRMED
```rust
fn emit_net_htons(code: &mut Code) {
    prologue(code);
    code.sub_rsp(16);
    code.mov_r_mem(Reg::Rax, Reg::Rbp, 16);  // load arg
    code.mov_mem_r(Reg::Rsp, 0, Reg::Rax);    // BUG: stores on stack
    call_net_func(code, 16);                    // instead of: mov rcx, rax
    code.add_rsp(16);
    code.leave_ret();
}
```
The argument is placed on the stack (`mov [rsp], rax`) instead of in RCX. Windows x64 ABI requires first arg in RCX.

### Next Steps
1. Fix emit_net_htons to place argument in RCX
2. Audit ALL networking service emitters for correct register argument setup
3. Create a reusable `call_winapi` helper that ensures proper ABI compliance
4. Identify and fix the root cause of the WSACleanup-after-closesocket crash
5. Test all 14 failing tests

### Test Files Created
- tests/net_diag.rs (Round 1)
- tests/net_diag2.rs (Round 2)
- tests/net_diag3.rs (Round 3)
- tests/net_diag4.rs (Round 4)
- tests/net_diag5.rs (Round 5)
