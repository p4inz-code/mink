# Third-party notices

MINK's compiler uses no third-party code: it is written in Rust with no dependencies. The items below ship
with or are used by parts of MINK.

## Fonts

Bundled with the setup window and the website under the SIL Open Font License 1.1. The license texts are in
`assets/installer/fonts/` and `site/fonts/`, and in the `licenses` folder of the setup file and portable zip.

| Font | Copyright |
|---|---|
| Atkinson Hyperlegible Next | The Atkinson Hyperlegible Next Project Authors, https://github.com/googlefonts/atkinson-hyperlegible-next |
| Familjen Grotesk | The Familjen Grotesk Project Authors, https://github.com/Familjen-Sthlm/Familjen-Grotesk |
| IBM Plex Mono | IBM Corp., https://github.com/IBM/plex |

## VS Code extension

`editors/vscode` depends on `vscode-languageclient` (MIT License, Microsoft Corporation) and its own
dependencies, which are listed in `editors/vscode/package-lock.json`. They are not part of the compiler or
the setup file.

## MINK's own artwork

The MINK logo, symbol and icons in `assets/brand/` belong to the creator, Atharva Patil, and are used under
the brand rules in `assets/brand/README.md`. They are not covered by the Apache License's patent grant or
by any permission to use the MINK name or logo.
