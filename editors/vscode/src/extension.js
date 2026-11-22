'use strict';
// MINK for VS Code: starts `mink lsp` and adds Run, Check and Build commands.
// Everything else (errors, completion, hover, rename, formatting) comes from the language server.

const vscode = require('vscode');
const { LanguageClient, TransportKind } = require('vscode-languageclient/node');

let client;
let output;

function minkPath() {
  return vscode.workspace.getConfiguration('mink').get('path') || 'mink';
}

async function startServer() {
  if (!vscode.workspace.isTrusted) {
    output.appendLine('This folder is not trusted, so the MINK language server was not started.');
    return;
  }
  const command = minkPath();
  const run = { command, args: ['lsp'], transport: TransportKind.stdio };
  client = new LanguageClient(
    'mink',
    'MINK',
    { run, debug: run },
    {
      documentSelector: [{ scheme: 'file', language: 'mink' }, { scheme: 'untitled', language: 'mink' }],
      outputChannel: output,
    },
  );
  try {
    await client.start();
  } catch (error) {
    client = undefined;
    const choice = await vscode.window.showErrorMessage(
      `MINK could not start "${command}". Is MINK installed and on your PATH? (${error.message})`,
      'Open settings',
      'Install MINK',
    );
    if (choice === 'Open settings') vscode.commands.executeCommand('workbench.action.openSettings', 'mink.path');
    if (choice === 'Install MINK') vscode.env.openExternal(vscode.Uri.parse('https://github.com/p4inz-code/mink#install'));
  }
}

async function stopServer() {
  if (client) {
    const old = client;
    client = undefined;
    try { await old.stop(); } catch (error) { /* it may already be gone */ }
  }
}

function quote(text) {
  return '"' + text.replace(/"/g, '""') + '"';
}

async function runOnFile(verb) {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== 'mink') {
    vscode.window.showInformationMessage('Open a .mink file first.');
    return;
  }
  if (editor.document.isUntitled) {
    vscode.window.showInformationMessage('Save the file first, so MINK has something to ' + verb + '.');
    return;
  }
  if (!vscode.workspace.isTrusted) {
    vscode.window.showWarningMessage('MINK will not run programs in an untrusted folder.');
    return;
  }
  await editor.document.save();
  let terminal = vscode.window.terminals.find((t) => t.name === 'MINK');
  if (!terminal) terminal = vscode.window.createTerminal('MINK');
  terminal.show(true);
  const exe = minkPath();
  const needsCall = /\s/.test(exe) && String(vscode.env.shell).toLowerCase().includes('powershell');
  terminal.sendText(`${needsCall ? '& ' : ''}${quote(exe)} ${verb} ${quote(editor.document.fileName)}`);
}

async function activate(context) {
  output = vscode.window.createOutputChannel('MINK');
  context.subscriptions.push(
    output,
    vscode.commands.registerCommand('mink.run', () => runOnFile('run')),
    vscode.commands.registerCommand('mink.check', () => runOnFile('check')),
    vscode.commands.registerCommand('mink.build', () => runOnFile('build')),
    vscode.commands.registerCommand('mink.showOutput', () => output.show(true)),
    vscode.commands.registerCommand('mink.restartServer', async () => {
      await stopServer();
      await startServer();
    }),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration('mink.path')) {
        await stopServer();
        await startServer();
      }
    }),
    vscode.workspace.onDidGrantWorkspaceTrust(() => startServer()),
    vscode.workspace.onWillSaveTextDocument((event) => {
      if (event.document.languageId !== 'mink') return;
      if (!vscode.workspace.getConfiguration('mink').get('formatOnSave')) return;
      event.waitUntil(
        Promise.resolve(vscode.commands.executeCommand('vscode.executeFormatDocumentProvider', event.document.uri)).then(
          (edits) => (Array.isArray(edits) ? edits : []),
          () => [],
        ),
      );
    }),
  );
  await startServer();
}

async function deactivate() {
  await stopServer();
}

module.exports = { activate, deactivate };
