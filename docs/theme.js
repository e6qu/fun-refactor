/* Theme toggle. The page follows the system by default; a click pins a choice and
   remembers it, because someone reading a terminal at night has already decided. */
(function () {
  var root = document.documentElement;
  var saved = null;
  try { saved = localStorage.getItem('fr-theme'); } catch (e) { /* private mode */ }
  if (saved === 'dark' || saved === 'light') root.setAttribute('data-theme', saved);

  var button = document.getElementById('theme');
  if (!button) return;
  button.addEventListener('click', function () {
    var systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    var current = root.getAttribute('data-theme') || (systemDark ? 'dark' : 'light');
    var next = current === 'dark' ? 'light' : 'dark';
    root.setAttribute('data-theme', next);
    try { localStorage.setItem('fr-theme', next); } catch (e) { /* ignore */ }
  });
})();
