// Small extras. The page works without this file: it only colours code and adds a soft highlight
// that follows the pointer on cards.
(function () {
  'use strict';
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
  if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    document.querySelectorAll('.card').forEach(function (card) {
      card.addEventListener('pointermove', function (event) {
        var box = card.getBoundingClientRect();
        card.style.setProperty('--mx', (event.clientX - box.left) + 'px');
        card.style.setProperty('--my', (event.clientY - box.top) + 'px');
      });
    });
  }
})();
