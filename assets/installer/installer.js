/* MINK Setup window. Plain DOM; every piece of text goes in through textContent, never innerHTML.
   Install screens: Welcome, License, Location, Options, Installing, Finish.
   Remove screens: Remove, Removing, Removed. About and Help open over any screen. */
(function () {
  'use strict';

  const REPO = 'https://github.com/p4inz-code/mink';
  const ISSUES = REPO + '/issues';
  const NEW_ISSUE = REPO + '/issues/new';

  const token = document.querySelector('meta[name="mink-token"]').content;
  const page = document.getElementById('page');
  const stepsEl = document.getElementById('steps');
  const sideNote = document.getElementById('side-note');
  const overlay = document.getElementById('overlay');
  const root = document.documentElement;

  let state = null;
  let screen = '';
  let busy = false;
  let timer = null;
  const choice = {
    dir: '',
    fileType: true,
    contextMenu: true,
    newMenu: true,
    preview: true,
    defaultApps: true,
    addToPath: true,
    editor: '',
  };

  // ---------------------------------------------------------------- helpers
  function h(tag, attrs, ...children) {
    const el = document.createElement(tag);
    for (const [key, value] of Object.entries(attrs || {})) {
      if (value === false || value == null) continue;
      if (key === 'class') el.className = value;
      else if (key === 'text') el.textContent = value;
      else if (key.startsWith('on')) el.addEventListener(key.slice(2), value);
      else el.setAttribute(key, value === true ? '' : value);
    }
    for (const child of children) if (child != null) el.append(child);
    return el;
  }

  function link(text, href) {
    return h('a', { href, target: '_blank', rel: 'noopener noreferrer', text });
  }

  async function call(path, body) {
    const options = { method: body === undefined ? 'GET' : 'POST', headers: { 'X-Mink-Token': token } };
    if (body !== undefined) {
      options.headers['Content-Type'] = 'application/json';
      options.body = JSON.stringify(body);
    }
    const response = await fetch(path, options);
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'The request failed');
    return data;
  }

  function button(label, onClick, kind) {
    return h('button', { class: 'btn ' + (kind || ''), type: 'button', onclick: onClick, text: label });
  }

  function checkbox(label, note, checked, onChange, disabled) {
    const el = h(
      'button',
      { class: 'check', type: 'button', role: 'checkbox', 'aria-checked': String(checked), 'aria-disabled': disabled ? 'true' : null },
      h('span', { class: 'box' }),
      h('span', { class: 'label' }, h('span', { text: label }), note ? h('small', { text: note }) : null),
    );
    el.addEventListener('click', () => {
      if (el.getAttribute('aria-disabled') === 'true') return;
      const next = el.getAttribute('aria-checked') !== 'true';
      el.setAttribute('aria-checked', String(next));
      onChange(next);
    });
    return el;
  }

  function radio(label, note, name, value, selected, onSelect, disabled) {
    const el = h(
      'button',
      { class: 'radio', type: 'button', role: 'radio', 'data-name': name, 'aria-checked': String(selected), 'aria-disabled': disabled ? 'true' : null },
      h('span', { class: 'circle' }),
      h('span', { class: 'label' }, h('span', { text: label }), note ? h('small', { text: note }) : null),
    );
    el.addEventListener('click', () => {
      if (el.getAttribute('aria-disabled') === 'true') return;
      for (const other of page.querySelectorAll('[data-name="' + name + '"]')) other.setAttribute('aria-checked', 'false');
      el.setAttribute('aria-checked', 'true');
      onSelect(value);
    });
    return el;
  }

  // ---------------------------------------------------------------- the step list
  const INSTALL_STEPS = ['Welcome', 'License', 'Location', 'Options', 'Install', 'Finish'];
  const REMOVE_STEPS = ['Remove', 'Removing', 'Finished'];
  // Screens you can step back to before anything is installed.
  const GO_BACK = { Welcome: () => welcome(), License: () => license(), Location: () => location(), Options: () => options() };
  const BACKABLE = new Set(['welcome', 'license', 'location', 'options']);

  function setSteps(names, current) {
    stepsEl.replaceChildren();
    const canGo = !busy && BACKABLE.has(screen) && names === INSTALL_STEPS;
    names.forEach((name, index) => {
      const cls = index < current ? 'done' : index === current ? 'current' : '';
      const dot = h('span', { class: 'dot', text: index < current ? '✓' : String(index + 1) });
      const label = h('span', { class: 'step-name', text: name });
      const goes = canGo && index < current && GO_BACK[name];
      const item = goes
        ? h('button', { class: 'step', type: 'button', title: 'Go back to ' + name, onclick: GO_BACK[name] }, dot, label)
        : h('span', { class: 'step', 'aria-current': index === current ? 'step' : null }, dot, label);
      stepsEl.append(h('li', { class: cls }, item));
    });
  }

  function show(name, steps, index, children, note) {
    screen = name;
    setSteps(steps, index);
    sideNote.textContent = note || '';
    page.replaceChildren(...children);
    page.scrollTop = 0;
    const first = page.querySelector('input, .btn.primary');
    if (first) first.focus({ preventScroll: true });
  }

  // ---------------------------------------------------------------- display settings
  const SIZES = { normal: '17px', large: '20px', larger: '23px' };

  function remember(key, value) {
    try { window.localStorage.setItem(key, value); } catch (error) { /* private window: skip */ }
  }
  function recall(key) {
    try { return window.localStorage.getItem(key); } catch (error) { return null; }
  }

  function applySize(name) {
    if (!SIZES[name]) name = 'normal';
    root.style.fontSize = SIZES[name];
    remember('mink-setup-size', name);
    for (const b of document.querySelectorAll('[data-size]')) b.setAttribute('aria-pressed', String(b.dataset.size === name));
  }

  function applyTheme(name) {
    if (name !== 'light' && name !== 'dark') name = 'dark';
    root.setAttribute('data-theme', name);
    remember('mink-setup-theme', name);
    for (const b of document.querySelectorAll('[data-theme-choice]')) b.setAttribute('aria-pressed', String(b.dataset.themeChoice === name));
  }

  function buildTools() {
    const tools = document.getElementById('tools');
    const sizeButton = (name, text, label) => h('button', { class: 'chip', type: 'button', 'data-size': name, 'aria-label': label, onclick: () => applySize(name), text });
    const themeButton = (name, text) => h('button', { class: 'chip', type: 'button', 'data-theme-choice': name, onclick: () => applyTheme(name), text });
    tools.replaceChildren(
      h('div', { class: 'tool-row' }, h('span', { class: 'tool-label', text: 'Text size' }),
        h('span', { class: 'chips' }, sizeButton('normal', 'A', 'Normal text'), sizeButton('large', 'A+', 'Large text'), sizeButton('larger', 'A++', 'Largest text'))),
      h('div', { class: 'tool-row' }, h('span', { class: 'tool-label', text: 'Colours' }),
        h('span', { class: 'chips' }, themeButton('dark', 'Dark'), themeButton('light', 'Light'))),
      h('div', { class: 'tool-row links' }, button('About', openAbout, 'ghost'), button('Help & support', openHelp, 'ghost')),
    );
    applySize(recall('mink-setup-size') || 'normal');
    applyTheme(recall('mink-setup-theme') || 'dark');
  }

  // ---------------------------------------------------------------- about and help
  function closeOverlay() {
    overlay.hidden = true;
    overlay.replaceChildren();
    page.removeAttribute('inert');
  }

  function openOverlay(title, children) {
    overlay.replaceChildren(
      h('div', { class: 'sheet', role: 'dialog', 'aria-modal': 'true', 'aria-label': title },
        h('div', { class: 'sheet-body' }, ...children),
        h('div', { class: 'footer' }, button('Close', closeOverlay, 'primary'))),
    );
    overlay.hidden = false;
    page.setAttribute('inert', '');
    const close = overlay.querySelector('.btn.primary');
    if (close) close.focus({ preventScroll: true });
  }

  function openAbout() {
    const version = state ? state.version : '';
    openOverlay('About MINK', [
      h('img', { class: 'about-logo', src: '/logo.svg', alt: 'MINK', width: '150', height: '60' }),
      h('h1', { text: 'MINK ' + version }),
      h('p', { class: 'strong', text: 'A programming language that builds native Windows programs from small, readable source files.' }),
      h('dl', { class: 'summary' },
        h('dt', { text: 'Created by' }), h('dd', { text: 'Atharva Patil' }),
        h('dt', { text: 'Studio' }), h('dd', { text: 'P4inz Interactive Labs' }),
        h('dt', { text: 'Copyright' }), h('dd', { text: '© 2026 Atharva Patil | P4inz | P4inz Interactive Labs' }),
        h('dt', { text: 'License' }), h('dd', { text: 'Apache License 2.0. Free to use. Credit to the creator stays with every copy.' }),
        h('dt', { text: 'Windows' }), h('dd', { text: 'Windows 10 and newer' })),
      h('p', {}, h('span', { text: 'Source code, releases and docs: ' }), link('github.com/p4inz-code/mink', REPO)),
      h('p', { text: 'MINK is built by one person, over many months. If it helps you, a star on GitHub or a mention to a friend means a lot.' }),
    ]);
  }

  function diagnostics() {
    const lines = [
      'MINK Setup ' + (state ? state.version : '?'),
      'Task: ' + (state ? state.task : '?'),
      'Screen: ' + screen,
      'Window: ' + navigator.userAgent,
    ];
    return lines.join('\n');
  }

  function openHelp() {
    const status = h('span', { class: 'copy-status', role: 'status' });
    const copy = button('Copy these details', async () => {
      try {
        await navigator.clipboard.writeText(diagnostics());
        status.textContent = 'Copied. Paste it into your report.';
      } catch (error) {
        status.textContent = 'Could not copy. Select the text above and copy it by hand.';
      }
    });
    openOverlay('Help and support', [
      h('h1', { text: 'Help and support' }),
      h('p', { class: 'strong', text: 'Something not working, or a question? Here is how to get an answer.' }),
      h('ol', { class: 'points numbered' },
        h('li', {}, h('strong', { text: 'Look through existing reports. ' }), h('span', { text: 'Someone may have had the same problem. ' }), link('Open the issue list', ISSUES)),
        h('li', {}, h('strong', { text: 'Start a new report. ' }), h('span', { text: 'It is free, and you need a GitHub account. ' }), link('Report a problem', NEW_ISSUE)),
        h('li', {}, h('strong', { text: 'Say what happened. ' }), h('span', { text: 'What you clicked, what you expected, and what you saw instead. A screenshot helps.' }))),
      h('h2', { text: 'Details worth pasting into your report' }),
      h('pre', { class: 'details', text: diagnostics() }),
      h('p', {}, copy, status),
      h('p', { text: 'Nothing is sent anywhere unless you paste it yourself.' }),
    ]);
  }

  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && !overlay.hidden) closeOverlay();
  });

  // ---------------------------------------------------------------- install screens
  function welcome() {
    const lines = [
      h('h1', { text: 'Install MINK ' + state.version }),
      h('p', { class: 'strong', text: 'MINK builds native Windows programs from small, readable source files.' }),
      h('p', { text: 'This sets MINK up for your account only. It does not need administrator rights, and you can remove it any time from Apps & features.' }),
      h('ul', { class: 'facts' },
        h('li', { text: 'Takes about a minute' }),
        h('li', { text: 'Nothing leaves your PC' }),
        h('li', { text: 'You can go back at any step' })),
    ];
    if (state.existing) {
      lines.push(
        h('div', { class: 'notice' }, h('span', {
          text:
            'MINK ' + state.existing.version + ' is already installed in ' + state.existing.dir +
            '. This installs ' + state.version + ' next to it and makes it the one that runs. The old version stays until you remove it.',
        })),
      );
    }
    if (state.elevated) {
      lines.push(
        h('div', { class: 'notice bad' }, h('span', {
          text: 'This window is running as an administrator, so MINK would be installed for that account only. Close it and start the installer from a normal window.',
        })),
      );
    }
    lines.push(h('div', { class: 'spacer' }));
    lines.push(h('div', { class: 'footer' }, button('Cancel', closeWindow), button('Continue', license, 'primary')));
    show('welcome', INSTALL_STEPS, 0, lines, 'Everything is installed under one folder in your profile.');
    if (state.elevated) page.querySelector('.btn.primary').disabled = true;
  }

  function license() {
    const point = (title, text) => h('li', {}, h('strong', { text: title + ' ' }), h('span', { text }));
    show(
      'license',
      INSTALL_STEPS,
      1,
      [
        h('h1', { text: 'License and credit' }),
        h('p', { class: 'strong', text: 'MINK is free for everyone to use, for anything. Credit for it stays with its creator.' }),
        h('div', { class: 'credit' },
          h('span', { class: 'credit-label', text: 'Created by' }),
          h('span', { class: 'credit-name', text: 'Atharva Patil' }),
          h('span', { text: 'P4inz Interactive Labs  ·  © 2026' })),
        h('h2', { text: 'What you can do' }),
        h('ul', { class: 'points' },
          point('Use it for anything.', 'Personal, school, work or commercial projects. It costs nothing.'),
          point('Keep what you build.', 'Programs you write with MINK are yours.'),
          point('Change it and share it.', 'You may share MINK or a changed copy of it.')),
        h('h2', { text: 'What you must keep' }),
        h('ul', { class: 'points' },
          point('The credit.', 'When you share MINK or a changed copy, keep the copyright line, the license text and the NOTICE file. They name Atharva Patil and P4inz Interactive Labs as the creator.'),
          point('A note on changes.', 'A changed copy must say it was changed.'),
          point('The name.', 'You may not use the MINK name or logo to suggest your copy is the official one.')),
        h('h2', { text: 'Also good to know' }),
        h('ul', { class: 'points' },
          point('Patents.', 'Contributors license the patents their code needs. If you sue anyone over a patent claim about MINK, that license ends for you.'),
          point('No warranty.', 'MINK comes as it is. The authors are not liable for damage from using it.')),
        h('p', {}, h('span', { text: 'This is the Apache License 2.0. Full legal text: ' }), link('apache.org/licenses/LICENSE-2.0', 'https://www.apache.org/licenses/LICENSE-2.0'), h('span', { text: '.' })),
        h('div', { class: 'spacer' }),
        h('div', { class: 'footer' }, button('Back', welcome), button('Accept and continue', location, 'primary')),
      ],
      'Free and open source. The code is on GitHub.',
    );
  }

  function location() {
    if (!choice.dir) choice.dir = state.existing ? state.existing.dir : state.defaultDir;
    const input = h('input', { type: 'text', id: 'dir', value: choice.dir, spellcheck: 'false', autocomplete: 'off', 'aria-describedby': 'dir-hint' });
    const field = h('div', { class: 'field' }, h('label', { for: 'dir', text: 'Install folder' }), input);
    const sizeText = 'About ' + Math.max(1, Math.round(state.sizeBytes / 1048576)) + ' MB. Several versions can live in this folder side by side.';
    const hint = h('p', { class: 'hint', id: 'dir-hint', text: sizeText });
    const next = button('Continue', null, 'primary');
    const validate = () => {
      const value = input.value.trim().replace(/^"|"$/g, '');
      const ok = /^[A-Za-z]:\\.+/.test(value) || /^\\\\[^\\]+\\[^\\]+/.test(value);
      field.classList.toggle('bad', !ok && value.length > 0);
      hint.classList.toggle('bad', !ok && value.length > 0);
      hint.textContent = ok || !value
        ? sizeText
        : 'Type a full path, for example C:\\Users\\you\\MINK. If the folder already holds other files, you will be asked to pick another.';
      next.disabled = !ok;
      if (ok) choice.dir = value;
    };
    input.addEventListener('input', validate);
    next.addEventListener('click', options);
    const useDefault = button('Use the default', () => {
      input.value = state.existing ? state.existing.dir : state.defaultDir;
      validate();
    });
    show(
      'location',
      INSTALL_STEPS,
      2,
      [
        h('h1', { text: 'Where should MINK go?' }),
        h('p', { text: 'Pick a folder only MINK will use. Nothing of yours is ever placed in it or removed from it.' }),
        field,
        hint,
        h('div', { class: 'spacer' }),
        h('div', { class: 'footer' }, useDefault, button('Back', license), next),
      ],
    );
    validate();
  }

  function options() {
    const keys = [
      ['fileType', 'Give .mink files the MINK icon and name', 'Shows up in Explorer at every size'],
      ['contextMenu', 'Right-click entries: Run, Check, Build, Edit', 'Works on any .mink file'],
      ['newMenu', 'New > MINK Source File', 'Create a new file from the right-click New menu'],
      ['preview', 'Preview pane and search for .mink files', 'See the code in Explorer without opening it'],
      ['defaultApps', 'List MINK under Settings > Apps > Default apps', 'So you can pick it there later'],
      ['addToPath', 'Add MINK to your PATH', state.onPath ? 'MINK is already on your PATH' : 'So typing "mink" works in any new terminal'],
    ];
    const needsFileType = new Set(['contextMenu', 'newMenu', 'preview', 'defaultApps']);
    const boxes = new Map();
    const sync = () => {
      for (const [key, el] of boxes) {
        const locked = (needsFileType.has(key) && !choice.fileType) || (key === 'addToPath' && state.onPath);
        el.setAttribute('aria-disabled', String(locked));
        el.setAttribute('aria-checked', String(key === 'addToPath' ? choice.addToPath && !state.onPath : choice[key]));
      }
    };
    const setAll = (value) => {
      for (const [key] of keys) if (!(key === 'addToPath' && state.onPath)) choice[key] = value;
      sync();
    };
    const group = h('div', { class: 'group' });
    for (const [key, label, note] of keys) {
      const el = checkbox(label, note, choice[key], (v) => { choice[key] = v; sync(); });
      boxes.set(key, el);
      group.append(el);
    }

    // The editor for double-click is chosen for you: the first one found, else Notepad.
    // `mink setup` changes it later.
    if (choice.editor === '' && state.editors.length) choice.editor = state.editors[0].path;

    show(
      'options',
      INSTALL_STEPS,
      3,
      [
        h('h1', { text: 'Set up Windows' }),
        h('p', { text: 'Tick what you want. These only change your own account, and "mink setup" can change them again later. Double-clicking a .mink file only opens it in an editor. It never runs it.' }),
        h('div', { class: 'quick' }, button('Tick all', () => setAll(true), 'small'), button('Untick all', () => setAll(false), 'small')),
        group,
        h('div', { class: 'spacer' }),
        h('div', { class: 'footer' }, button('Back', location), button('Install', install, 'primary')),
      ],
    );
    sync();
  }

  function progressBlock() {
    return h('div', { class: 'progress' },
      h('div', { class: 'pct-row' }, h('span', { class: 'pct', id: 'pct', text: '0%' }), h('span', { class: 'now', id: 'now', text: 'Starting' })),
      h('div', { class: 'bar', role: 'progressbar', 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-valuenow': '0', id: 'bar', 'aria-label': 'Progress' }, h('div', { id: 'bar-fill' })),
      h('details', { class: 'more' }, h('summary', { text: 'Show details' }), h('ul', { class: 'log', id: 'log', 'aria-label': 'Details' })));
  }

  function paint(p) {
    const fill = document.getElementById('bar-fill');
    if (fill) fill.style.width = p.percent + '%';
    const bar = document.getElementById('bar');
    if (bar) bar.setAttribute('aria-valuenow', String(Math.round(p.percent)));
    const pct = document.getElementById('pct');
    if (pct) pct.textContent = Math.round(p.percent) + '%';
    const now = document.getElementById('now');
    const log = document.getElementById('log');
    if (p.log.length && now) now.textContent = p.log[p.log.length - 1];
    if (log) {
      log.replaceChildren(...p.log.slice(-40).map((line) => h('li', { text: line })));
      log.scrollTop = log.scrollHeight;
    }
  }

  async function install() {
    if (busy) return;
    busy = true;
    show('installing', INSTALL_STEPS, 4, [
      h('h1', { text: 'Installing MINK ' + state.version }),
      h('p', { text: 'This only takes a moment. Please keep this window open.' }),
      progressBlock(),
      h('div', { class: 'spacer' }),
    ], 'Please keep this window open.');
    try {
      await call('/api/install', {
        dir: choice.dir,
        fileType: choice.fileType,
        contextMenu: choice.contextMenu,
        newMenu: choice.newMenu,
        preview: choice.preview,
        defaultApps: choice.defaultApps,
        addToPath: choice.addToPath && !state.onPath,
        editor: choice.editor,
      });
      poll();
    } catch (error) {
      busy = false;
      failed(error.message);
    }
  }

  function poll() {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      let p;
      try { p = await call('/api/progress'); } catch (error) { poll(); return; }
      paint(p);
      if (p.state === 'done') { busy = false; finished(p); return; }
      if (p.state === 'error') { busy = false; failed(p.error); return; }
      poll();
    }, 200);
  }

  function finished(p) {
    const s = p.summary || {};
    const rows = [];
    const add = (name, value) => rows.push(h('dt', { text: name }), h('dd', { text: value }));
    add('Version', s.version || state.version);
    add('Folder', s.dir || choice.dir);
    add('Windows', choice.fileType ? 'MINK icon, name and entries are set up' : 'Left as it was');
    add('PATH', choice.addToPath && !state.onPath ? 'MINK was added; open a new terminal' : 'Unchanged');
    const children = [
      h('div', { class: 'success', 'aria-hidden': 'true' }),
      h('h1', { text: 'MINK ' + (s.version || state.version) + ' is installed' }),
      h('p', { class: 'strong', text: 'Open a new terminal and type:' }),
      h('p', {}, h('code', { text: 'mink --version' })),
      h('dl', { class: 'summary' }, ...rows),
    ];
    if (s.upgradedFrom) children.push(h('p', { text: 'The previous version, ' + s.upgradedFrom + ', is still installed. "mink versions" lists them.' }));
    if (p.warnings && p.warnings.length) {
      children.push(h('div', { class: 'notice' }, h('span', { text: 'Some things did not work, but MINK is installed:' }), h('ul', {}, ...p.warnings.map((w) => h('li', { text: w })))));
    }
    children.push(h('p', {}, h('span', { text: 'New here? ' }), link('Read the getting-started guide', REPO + '#readme')));
    children.push(h('div', { class: 'spacer' }), h('div', { class: 'footer' }, button('Finish', closeWindow, 'primary')));
    show('finish', INSTALL_STEPS, 6, children, 'To change these later, run mink setup. To remove MINK, use Apps & features.');
  }

  function failed(message) {
    const removing = state.task === 'uninstall';
    show('failed', removing ? REMOVE_STEPS : INSTALL_STEPS, removing ? 1 : 4, [
      h('h1', { text: removing ? 'MINK was not removed' : 'MINK was not installed' }),
      h('div', { class: 'notice bad' }, h('span', { text: message || 'Something went wrong.' })),
      h('p', { text: 'Nothing that was already on your PC was changed or removed.' }),
      h('p', { text: 'If this keeps happening, the Help page shows how to report it.' }),
      h('div', { class: 'spacer' }),
      h('div', { class: 'footer' },
        button('Close', closeWindow),
        button('Get help', openHelp),
        removing ? null : button('Back', options, 'primary')),
    ]);
  }

  // ---------------------------------------------------------------- uninstall screens
  function confirmRemove() {
    show('remove', REMOVE_STEPS, 0, [
      h('h1', { text: 'Remove MINK ' + state.version }),
      h('p', { class: 'strong', text: 'This removes MINK and what Setup added for it:' }),
      h('div', { class: 'group plain' },
        h('p', { text: '• every installed version of MINK and its launcher' }),
        h('p', { text: '• the .mink icon, right-click entries, New menu and preview settings' }),
        h('p', { text: '• MINK from your PATH, if Setup added it' })),
      h('p', { text: 'Your own programs and .mink files are not touched.' }),
      h('div', { class: 'spacer' }),
      h('div', { class: 'footer' }, button('Cancel', closeWindow), button('Remove', removeNow, 'primary')),
    ]);
  }

  async function removeNow() {
    if (busy) return;
    busy = true;
    show('removing', REMOVE_STEPS, 1, [
      h('h1', { text: 'Removing MINK' }),
      progressBlock(),
      h('div', { class: 'spacer' }),
    ]);
    try {
      await call('/api/uninstall', {});
      pollRemoval();
    } catch (error) {
      busy = false;
      failed(error.message);
    }
  }

  function pollRemoval() {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      let p;
      try { p = await call('/api/progress'); } catch (error) { pollRemoval(); return; }
      paint(p);
      if (p.state === 'done') {
        busy = false;
        const children = [h('div', { class: 'success', 'aria-hidden': 'true' }), h('h1', { text: 'MINK was removed' }), h('p', { text: 'Open terminals may still show the old PATH until you close them.' })];
        if (p.warnings && p.warnings.length) {
          children.push(h('div', { class: 'notice' }, h('span', { text: 'A few things need a look:' }), h('ul', {}, ...p.warnings.map((w) => h('li', { text: w })))));
        }
        children.push(h('div', { class: 'spacer' }), h('div', { class: 'footer' }, button('Close', closeWindow, 'primary')));
        show('removed', REMOVE_STEPS, 3, children);
        return;
      }
      if (p.state === 'error') { busy = false; failed(p.error); return; }
      pollRemoval();
    }, 200);
  }

  // ---------------------------------------------------------------- start and stop
  async function closeWindow() {
    try { await call('/api/finish', {}); } catch (error) { /* the server may already be gone */ }
    window.close();
  }

  async function start() {
    buildTools();
    setInterval(() => { call('/api/ping', {}).catch(() => {}); }, 2000);
    try {
      state = await call('/api/state');
    } catch (error) {
      page.replaceChildren(h('h1', { text: 'Setup could not start' }), h('p', { text: error.message }));
      return;
    }
    if (state.task === 'uninstall') confirmRemove();
    else welcome();
  }

  start();
})();
