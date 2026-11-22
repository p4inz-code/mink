# Security

## Reporting a problem

If you find a security problem in MINK, please do not post the details in a public issue.

1. Use GitHub's private reporting: open the repository's **Security** tab and choose
   **Report a vulnerability**.
2. If that is not available, open an issue that says you have a security report and gives no details.
   The maintainer will reply with a private way to send it.

Please include the MINK version (`mink --version`), your Windows version and the steps to reproduce.

## What MINK touches

- Setup and `mink setup` only change your own account (the `HKCU` registry hive and your user PATH).
  They never ask for administrator rights.
- The setup window listens on `127.0.0.1` only, requires a per-run token, checks the `Host` header and
  sets a strict content security policy.
- `mink upgrade` contacts GitHub only when you run it, and checks the download's SHA-256 before starting
  an installer.
- The VS Code extension does not start `mink` or run files in a folder you have not trusted.

## Known gaps

- `mink.exe` and the installer are unsigned for now, so Windows may show a warning. Signing is coming.
