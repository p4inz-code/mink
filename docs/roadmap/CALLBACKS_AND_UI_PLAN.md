# Plan: callbacks, a `ui` module and an HTML builder (MINK 1.1)

**Status:** Plan, for the owner to approve. Written 2026-11-22. Nothing here is built yet.

## Why

A field check found two limits on what MINK can build for people:

- **Native windows.** A MINK program can create and run a real Windows window (see
  `testapps/widget`), but only with Windows' built-in `STATIC` window class, because MINK cannot hand Windows
  a **callback**. There is no custom window procedure, so clicks, painting and resizing have to be polled.
- **Web front ends.** MINK can serve and generate pages, but it has no way to write the HTML except by
  gluing strings together.

Callbacks are the single change that unlocks the rest.

## What a callback needs

Windows calls back into a program with the Windows x64 calling convention: the first four arguments in
`rcx`, `rdx`, `r8` and `r9`. MINK functions use their own stack convention. The runtime already bridges the
two for threads (`emit_thread_trampoline`): it loads one argument from a block and calls the MINK function.
A callback needs the same bridge in the other direction, with more arguments, and without a parameter block,
because Windows does not pass one for most callbacks (a window procedure gets only `hwnd`, `msg`, `wParam`,
`lParam`).

### Design: a fixed pool of thunks

- The compiler emits a fixed number of small thunks (16, to start) in the program's code section. Each one
  knows its own slot number, loads the first four register arguments, and jumps to one shared trampoline.
- A table in the program's data area maps slot number to the MINK function and its argument count.
- The trampoline pushes the arguments on the MINK stack in order, keeps the stack 16-byte aligned, calls
  the MINK function, and returns its integer result in `rax`.
- New intrinsic: `rt_callback(fn, nargs) -> Int` claims a free slot and returns the thunk's address, which
  can be passed to any Windows function. `rt_callback_free(address)` returns the slot.
- No executable memory is allocated at run time, so this does not depend on turning off data execution
  prevention, and a security scanner has nothing unusual to see.

### Rules

- Arguments and result are integers only. Pointers are passed as integers, as with `rt_sys_call` today.
- A callback runs on the thread Windows calls it on. Programs that use threads must lock their own data.
- The leak check counts unreleased slots like any other allocation, so a program that forgets
  `rt_callback_free` is told.
- More than four arguments is refused with a clear message at compile time; the stack arguments beyond four
  can follow in a later change if a real API needs them.

### Tests

| Test | Proves |
|---|---|
| `EnumWindows` with a callback that counts windows | A two-argument callback is called many times and returns a value |
| `SetTimer` with a callback that stops after three ticks | A four-argument callback, called from the message loop |
| A real window class with a window procedure that handles `WM_DESTROY` and calls `PostQuitMessage` | The full path used by every Windows program |
| Sixteen callbacks at once, then one more | The pool runs out cleanly, with an error and not a crash |
| A callback that raises an error inside | The program stops with a normal MINK error |
| Alignment checked with an SSE-using Windows function from inside a callback | The stack is aligned |

## The `ui` module

On top of callbacks, a small standard-library module for Windows programs. It wraps the window class, the
message loop and the common controls so a program does not write raw Windows calls.

Sketch of the API, so the owner can judge the feel (names are proposals):

```
mod ui;

fn on_click(arg: Int) -> Int { ui_set_text(arg, "Clicked"); return 0; }

fn main() -> Int {
    let w = ui_window("Hello", 360, 200);
    let label = ui_label(w, "Press the button", 20, 20, 300, 24);
    let btn = ui_button(w, "Press me", 20, 60, 120, 32);
    ui_on_click(btn, on_click, label);
    return ui_run(w);
}
```

- **Controls:** label, button, text box, checkbox, list box, progress bar, timer.
- **Layout:** fixed positions first. Rows and columns later.
- **Drawing:** a `paint` hook with a few GDI calls (lines, rectangles, text) after the controls work.
- **Look:** the system controls. No custom theming in 1.1.
- **Safety:** every handle is freed by `ui_run` when the window closes, so programs stay leak-free.

## The HTML builder

A small module for writing pages without gluing strings:

- `html_open(tag)`, `html_attr(name, value)`, `html_text(text)`, `html_close()` build into one string and
  escape text and attribute values, so a page cannot contain an injected tag by accident.
- A template function generalises what `testapps/sitegen` does today (`{{NAME}}` replaced from a data file).
- Used by the `todo`, `paste` and `livelog` test apps first, then promoted into the standard library.

## Steps

| Step | Work | Done when |
|---|---|---|
| C1 | Thunk pool and trampoline in the runtime, `rt_callback` and `rt_callback_free` | `EnumWindows` and `SetTimer` tests pass; leak check counts slots |
| C2 | A real window procedure test with `PostQuitMessage` | `testapps/widget` rewritten with a window procedure and a clickable button |
| C3 | `ui` module: window, label, button, text box, timer | The hello example above runs; closing the window exits 0 with no leaks |
| C4 | `ui`: checkbox, list box, progress bar, paint hook | A small settings window and a simple chart run |
| H1 | HTML builder with escaping, tests for injection | Escaping tests pass; `todo` uses it |
| H2 | Templating in the standard library | `sitegen` becomes a short program on top of it |
| D1 | Documentation, examples, the website's Examples page | Each module has a reference entry marked as run |

C1 is the risky part and is done first. If it takes more than a few days, the rest of 1.1 can ship without
`ui`, since the HTML builder does not depend on callbacks.

## What this does not include

- A cross-platform UI or a different look from the system's. That is a separate project.
- A WebAssembly or JavaScript target for running MINK in a browser.
- Anything for macOS, Linux or mobile.

## Decisions needed from the owner

1. Is a fixed pool of 16 callbacks enough to start?
2. Is the `ui` sketch the feel you want, or should it be closer to something you know?
3. Should `ui` ship in 1.1 even if only the basic controls are ready, or wait until C4?
