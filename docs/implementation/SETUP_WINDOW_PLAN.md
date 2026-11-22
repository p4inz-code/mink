# Plan: a custom setup window with no system title bar

**Status:** Plan. Nothing here is built yet. Written for the owner to approve or change.

## The problem

Setup is a web page shown in a Microsoft Edge `--app` window. The page is fully custom, but the frame
around it is Edge's: a system title bar in the user's Windows accent colour, system caption buttons
and a system border. The owner's rule for the ecosystem (Kinoz, OutlineDesk) is a custom title bar and
no system controls anywhere.

Edge gives no way to hide that frame. So the fix is to host the page in a window we own.

## What we need

1. A window with no system title bar, drawn by us: a dark bar with the MINK mark, the step name, and
   minimise and close buttons in the page's own style.
2. Dragging the window by that bar, and a soft shadow and rounded corners so it does not look flat.
3. No change to the page, the local server, the install engine or the tests behind them.
4. Windows 10 and newer, per-user, no administrator rights.
5. The compiler crate stays dependency-free and `#![forbid(unsafe_code)]`.

## Options

| Option | How it works | For | Against |
|---|---|---|---|
| A. Keep Edge, strip its frame from outside | A helper program removes the caption style from Edge's window | No new browser dependency | Fragile: Edge repaints the frame, dragging needs a second overlay window, breaks with Edge updates |
| B. Draw the whole window ourselves | Win32 and Direct2D, no browser | Nothing to depend on | Throws away the finished page; weeks of work for one window |
| C. Ship a browser | Electron, as Kinoz does | Full control | 80 MB or more for a 5 MB compiler; wrong for MINK |
| D. Our own window around the system WebView2 | A small second program creates a borderless window and puts the WebView2 control in it | Keeps the page as it is; real drag, shadow and corners; small | Needs the Microsoft WebView2 Runtime on the PC |

## Recommendation: option D, built with `wry` and `tao`

`tao` makes the window and `wry` hosts WebView2 in it. Both are maintained by the Tauri project and
dual-licensed MIT and Apache-2.0. The window is made with decorations off, and the page's title bar
sends a message to the host to start a drag, minimise or close.

This is a **separate crate and a separate program**, `mink-setup.exe`, so the decision to keep the
compiler free of dependencies holds. Dependencies, and the unsafe code inside them, live only in the
setup program.

### How it fits together

```
mink-Setup-1.0.5.exe            (Inno Setup wrapper, unchanged)
  -> unpacks payload.zip
  -> runs mink.exe installer     (starts the local server, as today)
       -> if mink-setup.exe is in the payload and WebView2 is installed:
            starts mink-setup.exe <address>   (our frameless window)
          otherwise:
            starts Edge --app                  (today's behaviour)
```

- `mink-setup.exe` takes the server address and token, opens one borderless window sized to the page,
  and loads only that address. It does nothing else.
- When the window closes, the host exits and `mink installer` carries on, exactly as it does when the
  Edge window closes today.
- The fallback keeps setup working on a PC without WebView2. It still has Edge's frame, which is
  documented as the reason to install the WebView2 Runtime.

### Changes to the page

- A title bar element at the top: the small MINK mark, "MINK Setup", and minimise and close buttons.
  It is hidden when the page runs in the fallback Edge window, because Edge already has its own.
- Mouse-down on the bar sends `drag`; the buttons send `minimize` and `close`. The messages go over
  the host's script channel and nowhere else.
- Resize is off. The window is a fixed size, which suits an installer.

### Security rules

- The host loads only `http://127.0.0.1:<port>/` with the run's token, and blocks every other
  navigation and every new window. Links open in the default browser, only if they start with
  `https://github.com/p4inz-code/`.
- Developer tools, context menu, zoom keys and browser accelerators are switched off.
- The host reads only its two arguments. It never reads or writes files, the registry or the network.
- The local server's existing protections stay: token, host check, strict content security policy.

## Steps

| Step | Work | Done when |
|---|---|---|
| S1 | New crate `setup-window/`, a window that opens a given address with no frame, drag, minimise and close | Opens, drags, closes, on Windows 10 and 11 |
| S2 | Title bar in the page, shown only in the host window; messages to the host | Same look as the page; fallback window unchanged |
| S3 | `mink installer` prefers the host when present and WebView2 is found, else Edge | Both paths tested, including WebView2 missing |
| S4 | Shadow, rounded corners on Windows 11, correct DPI scaling and multi-monitor placement | Looks right at 100%, 150% and 200% |
| S5 | Build script, payload manifest, setup file, NOTICE and licences for the new dependencies | Setup file installs with the host; hashes verified |

## Tests

- The existing installer tests keep running against the server, which does not change.
- New: choosing the host or Edge from what is installed; the title bar shows only in the host; the host
  refuses other addresses.
- By hand, on a clean Windows 10 and a clean Windows 11: open, drag, minimise, close, install, cancel,
  and run with WebView2 removed to check the fallback.

## Costs and risks

- **Build time and size.** `wry` and `tao` pull in a few dozen crates. Expect a few minutes for a first
  build and a setup program of roughly 2 to 3 MB more. Builds will run with limited jobs to stay inside
  the machine-heat rule.
- **WebView2 may be missing.** It ships with Windows 11. On Windows 10 it usually arrives with Edge or
  Office updates, but that is not guaranteed, and has not been checked on a clean Windows 10 yet. The
  fallback covers it.
- **Unsigned programs.** A second unsigned `.exe` can draw more SmartScreen and antivirus attention. It
  is inside the setup file, not a separate download.
- **Maintenance.** `wry` and `tao` move quickly. Pin exact versions and update on purpose.

## Decisions needed from the owner

1. Is a separate setup program with outside crates (`wry`, `tao`) acceptable, given the compiler itself
   stays dependency-free?
2. Is "needs the WebView2 Runtime, with an Edge-window fallback" acceptable on Windows 10?
3. Fixed window size, or allow resize? (The plan says fixed.)

## Not in this plan

Light theme, text-size controls and other options the owner removed stay removed. The page design is
unchanged.
