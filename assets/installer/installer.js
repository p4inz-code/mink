/* MINK Setup window. Plain DOM; every piece of text goes in through textContent, never innerHTML.
   Screens: Welcome, Location, Options, Installing, Finish (install) and Remove, Removing, Removed (uninstall). */
(function () {
  'use strict';

  const token = document.querySelector('meta[name="mink-token"]').content;
  const page = document.getElementById('page');
  const stepsEl = document.getElementById('steps');
  const sideNote = document.getElementById('side-note');

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
      if (disabled) return;
      const next = el.getAttribute('aria-checked') !== 'true';
      el.setAttribute('aria-checked', String(next));
      onChange(next);
    });
    return el;
  }

  function radio(label, note, name, value, selected, onSelect) {
    const el = h(
      'button',
      { class: 'radio', type: 'button', role: 'radio', 'data-name': name, 'aria-checked': String(selected) },
      h('span', { class: 'circle' }),
      h('span', { class: 'label' }, h('span', { text: label }), note ? h('small', { text: note }) : null),
    );
    el.addEventListener('click', () => {
      for (const other of page.querySelectorAll('[data-name="' + name + '"]')) other.setAttribute('aria-checked', 'false');
      el.setAttribute('aria-checked', 'true');
      onSelect(value);
    });
    return el;
  }

  function setSteps(names, current) {
    stepsEl.replaceChildren();
    names.forEach((name, index) => {
      const cls = index < current ? 'done' : index === current ? 'current' : '';
      stepsEl.append(
        h('li', { class: cls }, h('span', { class: 'dot', text: index < current ? '✓' : String(index + 1) }), h('span', { text: name })),
      );
    });
  }

  function show(name, steps, index, children, note) {
    screen = name;
    setSteps(steps, index);
    sideNote.textContent = note || '';
    page.replaceChildren(...children);
    const first = page.querySelector('input, .btn.primary');
    if (first) first.focus({ preventScroll: true });
  }

  const INSTALL_STEPS = ['Welcome', 'Location', 'Options', 'Install', 'Finish'];
  const REMOVE_STEPS = ['Remove', 'Removing', 'Finished'];

  // ---------------------------------------------------------------- install screens
  function welcome() {
    const lines = [
      h('h1', { text: 'Install MINK ' + state.version }),
      h('p', {
        class: 'strong',
        text: 'MINK builds native Windows programs from small, readable source files.',
      }),
      h('p', {
        text: 'This sets MINK up for your account only. It does not need administrator rights, and you can remove it any time from Apps & features.',
      }),
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
    lines.push(
      h('div', { class: 'footer' }, button('Cancel', closeWindow), button('Continue', location, 'primary')),
    );
    show('welcome', INSTALL_STEPS, 0, lines, 'Everything is installed under one folder in your profile.');
    if (state.elevated) page.querySelector('.btn.primary').disabled = true;
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
      1,
      [
        h('h1', { text: 'Where should MINK go?' }),
        h('p', { text: 'Pick a folder only MINK will use. Nothing of yours is ever placed in it or removed from it.' }),
        field,
        hint,
        h('div', { class: 'spacer' }),
        h('div', { class: 'footer' }, useDefault, button('Back', welcome), next),
      ],
    );
    validate();
  }

  function options() {
    const ft = (value) => {
      choice.fileType = value;
      for (const el of page.querySelectorAll('[data-needs="file-type"]')) el.setAttribute('aria-disabled', String(!value));
    };
    const needs = (el) => { el.dataset.needs = 'file-type'; if (!choice.fileType) el.setAttribute('aria-disabled', 'true'); return el; };
    const editorChildren = [h('h2', { text: 'When you double-click a .mink file, open it in' })];
    const editors = [{ name: 'Notepad', path: '' }, ...state.editors];
    const defaultEditor = state.editors.length ? state.editors[0].path : '';
    if (choice.editor === '' && defaultEditor) choice.editor = defaultEditor;
    for (const editor of editors) {
      editorChildren.push(needs(radio(editor.name, editor.path || 'Always available', 'editor', editor.path, editor.path === choice.editor, (value) => { choice.editor = value; })));
    }
    const pathNote = state.onPath ? 'MINK is already on your PATH' : 'So typing "mink" works in any new terminal';
    show(
      'options',
      INSTALL_STEPS,
      2,
      [
        h('h1', { text: 'Set up Windows' }),
        h('p', { text: 'These only change your own account, and mink setup changes them again later. Double-click only opens an editor; it never runs a file.' }),
        h('div', { class: 'group' },
          checkbox('Give .mink files the MINK icon and name', 'Shows up in Explorer at every size', choice.fileType, ft),
          needs(checkbox('Right-click entries: Run, Check, Build, Edit', null, choice.contextMenu, (v) => { choice.contextMenu = v; })),
          needs(checkbox('New > MINK Source File', null, choice.newMenu, (v) => { choice.newMenu = v; })),
          needs(checkbox('Preview pane and search for .mink files', null, choice.preview, (v) => { choice.preview = v; })),
          needs(checkbox('List MINK under Settings > Apps > Default apps', null, choice.defaultApps, (v) => { choice.defaultApps = v; })),
          checkbox('Add MINK to your PATH', pathNote, choice.addToPath && !state.onPath, (v) => { choice.addToPath = v; }, state.onPath),
        ),
        h('div', { class: 'group' }, ...editorChildren),
        h('div', { class: 'spacer' }),
        h('div', { class: 'footer' }, button('Back', location), button('Install', install, 'primary')),
      ],
    );
  }

  async function install() {
    if (busy) return;
    busy = true;
    const bar = h('div', { class: 'bar' }, h('div', { id: 'bar-fill' }));
    const log = h('ul', { class: 'log', id: 'log', 'aria-label': 'Progress' });
    show('installing', INSTALL_STEPS, 3, [
      h('h1', { text: 'Installing MINK ' + state.version }),
      h('p', { id: 'now', text: 'Starting' }),
      bar,
      log,
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
      const fill = document.getElementById('bar-fill');
      if (fill) fill.style.width = p.percent + '%';
      const now = document.getElementById('now');
      const log = document.getElementById('log');
      if (p.log.length && now) now.textContent = p.log[p.log.length - 1];
      if (log) {
        log.replaceChildren(...p.log.slice(-40).map((line) => h('li', { text: line })));
        log.scrollTop = log.scrollHeight;
      }
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
      h('h1', { text: 'MINK ' + (s.version || state.version) + ' is installed' }),
      h('p', { class: 'strong', text: 'Open a new terminal and type:' }),
      h('p', {}, h('code', { text: 'mink --version' })),
      h('dl', { class: 'summary' }, ...rows),
    ];
    if (s.upgradedFrom) children.push(h('p', { text: 'The previous version, ' + s.upgradedFrom + ', is still installed. "mink versions" lists them.' }));
    if (p.warnings && p.warnings.length) {
      children.push(h('div', { class: 'notice' }, h('span', { text: 'Some things did not work, but MINK is installed:' }), h('ul', {}, ...p.warnings.map((w) => h('li', { text: w })))));
    }
    children.push(h('div', { class: 'spacer' }), h('div', { class: 'footer' }, button('Finish', closeWindow, 'primary')));
    show('finish', INSTALL_STEPS, 5, children, 'To change these later, run mink setup. To remove MINK, use Apps & features.');
  }

  function failed(message) {
    show('failed', state.task === 'uninstall' ? REMOVE_STEPS : INSTALL_STEPS, state.task === 'uninstall' ? 1 : 3, [
      h('h1', { text: 'MINK was not installed' }),
      h('div', { class: 'notice bad' }, h('span', { text: message || 'Something went wrong.' })),
      h('p', { text: 'Nothing that was already on your PC was changed or removed.' }),
      h('div', { class: 'spacer' }),
      h('div', { class: 'footer' },
        button('Close', closeWindow),
        state.task === 'uninstall' ? null : button('Back', options, 'primary')),
    ]);
  }

  // ---------------------------------------------------------------- uninstall screens
  function confirmRemove() {
    show('remove', REMOVE_STEPS, 0, [
      h('h1', { text: 'Remove MINK ' + state.version }),
      h('p', { class: 'strong', text: 'This removes MINK and what Setup added for it:' }),
      h('div', { class: 'group' },
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
      h('p', { id: 'now', text: 'Starting' }),
      h('div', { class: 'bar' }, h('div', { id: 'bar-fill' })),
      h('ul', { class: 'log', id: 'log', 'aria-label': 'Progress' }),
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
      const fill = document.getElementById('bar-fill');
      if (fill) fill.style.width = p.percent + '%';
      const now = document.getElementById('now');
      const log = document.getElementById('log');
      if (p.log.length && now) now.textContent = p.log[p.log.length - 1];
      if (log) log.replaceChildren(...p.log.slice(-40).map((line) => h('li', { text: line })));
      if (p.state === 'done') {
        busy = false;
        const children = [h('h1', { text: 'MINK was removed' }), h('p', { text: 'Open terminals may still show the old PATH until you close them.' })];
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
