// Small extras. Every page works without this file. It colours code, adds copy buttons, replays the terminal
// demo, and adds a soft highlight that follows the pointer on cards.
(function () {
  'use strict';
  var reduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  var KEYWORDS = /\b(fn|let|mut|for|in|while|if|else|return|mod|loop|match|struct|enum)\b/g;
  function escape(text) { return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
  function colour(source) {
    return source.split('\n').map(function (line) {
      var comment = '';
      var at = line.indexOf('//');
      if (at >= 0) { comment = '<span class="cm">' + escape(line.slice(at)) + '</span>'; line = line.slice(0, at); }
      var parts = line.split(/("(?:[^"\\]|\\.)*")/);
      var html = parts.map(function (part, i) {
        if (i % 2 === 1) return '<span class="st">' + escape(part) + '</span>';
        return escape(part).replace(KEYWORDS, '<span class="kw">$1</span>').replace(/\b(\d+)\b/g, '<span class="nu">$1</span>');
      }).join('');
      return html + comment;
    }).join('\n');
  }
  document.querySelectorAll('pre.code').forEach(function (pre) { pre.innerHTML = colour(pre.textContent); });
  document.querySelectorAll('.window > pre').forEach(function (pre) { pre.innerHTML = colour(pre.textContent); });

  // Copy buttons.
  function copyText(node) {
    var text = node.textContent;
    if (node.classList.contains('hash')) { var m = text.match(/[0-9a-f]{64}/i); if (m) text = m[0]; }
    return text.replace(/\s+$/, '');
  }
  function addCopy(node, host) {
    var btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'copy';
    btn.textContent = 'Copy';
    btn.setAttribute('aria-label', 'Copy to clipboard');
    btn.addEventListener('click', function () {
      var text = copyText(node);
      var done = function (ok) {
        btn.textContent = ok ? 'Copied' : 'Press Ctrl+C';
        setTimeout(function () { btn.textContent = 'Copy'; }, 1600);
      };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(function () { done(true); }, function () { done(false); });
      } else {
        var range = document.createRange(); range.selectNodeContents(node);
        var sel = window.getSelection(); sel.removeAllRanges(); sel.addRange(range); done(false);
      }
    });
    host.appendChild(btn);
  }
  document.querySelectorAll('pre.code').forEach(function (pre) { var host = pre.closest('.card'); if (host) { host.classList.add('has-copy'); addCopy(pre, host); } });
  document.querySelectorAll('.cmd, .hash').forEach(function (node) { var host = node.parentElement; host.classList.add('has-copy'); addCopy(node, host); });

  // Terminal demo: types the commands of the hero window once, as a replay. Without this the final state shows.
  var run = document.querySelector('.window .run pre');
  if (run && !reduce) {
    var lines = run.textContent.split('\n');
    var final = run.innerHTML;
    var bar = document.querySelector('.window .bar');
    var timer = null;
    var play = function () {
      clearTimeout(timer);
      run.textContent = '';
      var i = 0;
      var shown = '';
      var next = function () {
        if (i >= lines.length) { run.innerHTML = final; return; }
        var line = lines[i++];
        if (line.indexOf('PS>') === 0) {
          var typed = 'PS> ';
          var rest = line.slice(4);
          var c = 0;
          var type = function () {
            run.textContent = shown + typed + rest.slice(0, c);
            if (c < rest.length) { c++; timer = setTimeout(type, 35); }
            else { shown += line + '\n'; timer = setTimeout(next, 320); }
          };
          type();
        } else {
          shown += line + (i < lines.length ? '\n' : '');
          run.textContent = shown;
          timer = setTimeout(next, 220);
        }
      };
      next();
    };
    if (bar) {
      var replay = document.createElement('button');
      replay.type = 'button'; replay.className = 'replay'; replay.textContent = 'Replay';
      replay.setAttribute('aria-label', 'Replay the terminal demo');
      replay.addEventListener('click', play);
      bar.insertBefore(replay, bar.lastElementChild);
    }
    var seen = false;
    if ('IntersectionObserver' in window) {
      new IntersectionObserver(function (entries) {
        entries.forEach(function (e) { if (e.isIntersecting && !seen) { seen = true; play(); } });
      }, { threshold: 0.4 }).observe(document.querySelector('.window'));
    }
  }

  if (!reduce) {
    document.querySelectorAll('.card').forEach(function (card) {
      card.addEventListener('pointermove', function (event) {
        var box = card.getBoundingClientRect();
        card.style.setProperty('--mx', (event.clientX - box.left) + 'px');
        card.style.setProperty('--my', (event.clientY - box.top) + 'px');
      });
    });
  }
})();
