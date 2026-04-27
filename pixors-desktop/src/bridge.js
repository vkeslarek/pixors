(function() {
  function post(action) {
    try { if (window.ipc) window.ipc.postMessage(action); } catch(e) {}
  }
  window.addEventListener('pixors:window', function(e) { post(e.detail); });

  var maybeDrag = false;

  var barTimer = setInterval(function() {
    var bar = document.querySelector('.menubar');
    if (!bar) return;
    clearInterval(barTimer);

    bar.addEventListener('mousedown', function(e) {
      if (e.target.closest('button')) return;
      maybeDrag = true;
    });

    bar.addEventListener('mousemove', function(e) {
      if (maybeDrag) {
        maybeDrag = false;
        post('drag_window');
      }
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
