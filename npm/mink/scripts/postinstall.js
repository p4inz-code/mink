'use strict';

// Runs after `npm install -g @p4inz-code/mink`.
//
// It asks mink.exe to set up the .mink file type for this Windows account:
// `mink setup --auto`. That command never asks a question, never takes .mink
// away from another program, and keeps the choices of an earlier setup, so an
// upgrade only renews the stored paths.
//
// Nothing here may break an install. npm hides this script's output, so the
// result is checked with `mink setup --status`; every failure is swallowed and
// the install still succeeds.
//
// Skipped on purpose:
//   - anything that is not Windows;
//   - local installs (only a global install belongs to the whole account);
//   - when MINK_SKIP_SETUP is set, for packagers and CI.

const { spawnSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function main() {
  if (process.platform !== 'win32') return;
  if (process.env.npm_config_global !== 'true') return;
  if (process.env.MINK_SKIP_SETUP) return;

  const exe = path.join(__dirname, '..', 'bin', 'mink.exe');
  if (!fs.existsSync(exe)) return;

  spawnSync(exe, ['setup', '--auto'], {
    stdio: 'ignore',
    timeout: 90000,
    windowsHide: true,
  });
}

try {
  main();
} catch (error) {
  // Never fail the install over file-type setup.
}
process.exit(0);
