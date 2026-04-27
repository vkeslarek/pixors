(function() {
  function post(action) {
    try { if (window.ipc) window.ipc.postMessage(action); } catch(e) {}
  }
  window.addEventListener('pixors:window', function(e) { post(e.detail); });

  // ── Resize cursor + hit-test on edges ──────────────────
  document.addEventListener('mousemove', function(e) {
    post('mousemove:' + Math.round(e.clientX) + ',' + Math.round(e.clientY));
  });

  document.addEventListener('mousedown', function(e) {
    if (e.button !== 0) return;
    post('mousedown:' + Math.round(e.clientX) + ',' + Math.round(e.clientY));
  });

  // ── Menubar drag + double-click maximize ───────────────
  var maybeDrag = false;

  var barTimer = setInterval(function() {
    var bar = document.querySelector('.menubar');
    if (!bar) return;
    clearInterval(barTimer);

    bar.addEventListener('mousedown', function(e) {
      if (e.target.closest('button')) return;
      // Top 5px = resize edge, let document handler deal with it
      if (e.clientY < 5) return;
      e.stopPropagation();
      maybeDrag = true;
    });

    bar.addEventListener('mousemove', function(e) {
      if (maybeDrag) { maybeDrag = false; post('drag_window'); }
    });

    bar.addEventListener('mouseup', function() { maybeDrag = false; });
    bar.addEventListener('mouseleave', function() { maybeDrag = false; });

    bar.addEventListener('dblclick', function(e) {
      maybeDrag = false;
      if (e.target.closest('button')) return;
      post('maximize');
    });
  }, 200);
})();
