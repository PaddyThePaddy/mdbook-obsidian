/// Appends a self-contained image lightbox (CSS + JS) to chapters that
/// contain at least one image. Supports click-to-open, drag-to-pan,
/// scroll/pinch-to-zoom, double-tap to reset, and Escape / backdrop to close.

const LIGHTBOX_CSS: &str = r#"<style>
.lb-overlay{position:fixed;inset:0;background:rgba(0,0,0,.87);z-index:9999;
  display:none;align-items:center;justify-content:center;
  overscroll-behavior:contain;}
.lb-overlay.lb-open{display:flex;}
.lb-img{max-width:90vw;max-height:90vh;object-fit:contain;
  cursor:grab;border-radius:2px;box-shadow:0 4px 24px rgba(0,0,0,.6);
  user-select:none;-webkit-user-select:none;
  touch-action:none;-webkit-user-drag:none;}
.lb-close{position:fixed;top:12px;right:12px;width:44px;height:44px;
  border:none;border-radius:50%;background:rgba(255,255,255,.15);
  color:#fff;font-size:24px;line-height:44px;text-align:center;
  cursor:pointer;padding:0;transition:background .15s;}
.lb-close:hover,.lb-close:focus{background:rgba(255,255,255,.3);outline:none;}
</style>
"#;

const LIGHTBOX_JS: &str = r#"
<script>
(function () {
  document.addEventListener('DOMContentLoaded', function () {
    if (document.querySelector('.lb-overlay')) return;

    // Collect every image on the page except future lightbox images.
    var pageImgs = Array.from(document.querySelectorAll('img'))
      .filter(function (el) { return !el.classList.contains('lb-img'); });
    if (!pageImgs.length) return;

    // Build the overlay DOM.
    var overlay = document.createElement('div');
    overlay.className = 'lb-overlay';
    overlay.setAttribute('role', 'dialog');
    overlay.setAttribute('aria-modal', 'true');

    var lbImg = document.createElement('img');
    lbImg.className = 'lb-img';

    var closeBtn = document.createElement('button');
    closeBtn.className = 'lb-close';
    closeBtn.innerHTML = '&times;';
    closeBtn.setAttribute('aria-label', 'Close image');

    overlay.appendChild(lbImg);
    overlay.appendChild(closeBtn);
    document.body.appendChild(overlay);

    // Transform state.
    var scale = 1, tx = 0, ty = 0, lastDist = 0;
    var activePointers = new Map();

    function apply() {
      lbImg.style.transform =
        'translate(' + tx + 'px,' + ty + 'px) scale(' + scale + ')';
    }
    function reset() { scale = 1; tx = 0; ty = 0; apply(); }

    function open(src, alt) {
      lbImg.src = src;
      lbImg.alt = alt || '';
      reset();
      overlay.classList.add('lb-open');
      document.body.style.overflow = 'hidden';
      closeBtn.focus();
    }

    function close() {
      overlay.classList.remove('lb-open');
      document.body.style.overflow = '';
      lbImg.src = '';
      activePointers.clear();
      lastDist = 0;
    }

    // Add zoom-in cursor and click-to-open on every content image.
    pageImgs.forEach(function (el) {
      el.style.cursor = 'zoom-in';
      el.addEventListener('click', function (e) {
        e.stopPropagation();
        open(el.src, el.alt);
      });
    });

    // Close on backdrop click; double-click / double-tap on image resets zoom.
    var lastTap = 0;
    overlay.addEventListener('click', function (e) {
      if (e.target === overlay) { close(); return; }
      var now = Date.now();
      if (now - lastTap < 300) reset();
      lastTap = now;
    });

    closeBtn.addEventListener('click', close);

    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && overlay.classList.contains('lb-open')) close();
    });

    // --- Pointer events: single-pointer drag, two-pointer pinch zoom --------

    lbImg.addEventListener('pointerdown', function (e) {
      e.preventDefault();
      lbImg.setPointerCapture(e.pointerId);
      activePointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (activePointers.size === 1) lbImg.style.cursor = 'grabbing';
    });

    lbImg.addEventListener('pointermove', function (e) {
      if (!activePointers.has(e.pointerId)) return;
      e.preventDefault();
      var prev = activePointers.get(e.pointerId);
      activePointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

      if (activePointers.size === 1) {
        // Drag.
        tx += e.clientX - prev.x;
        ty += e.clientY - prev.y;
        apply();
      } else if (activePointers.size >= 2) {
        // Pinch zoom: measure distance between the two oldest pointers.
        var pts = Array.from(activePointers.values());
        var dist = Math.hypot(pts[1].x - pts[0].x, pts[1].y - pts[0].y);
        if (lastDist > 0) {
          scale = Math.max(0.5, Math.min(10, scale * (dist / lastDist)));
          apply();
        }
        lastDist = dist;
      }
    });

    function onPointerEnd(e) {
      activePointers.delete(e.pointerId);
      if (activePointers.size < 2) lastDist = 0;
      if (activePointers.size === 0) lbImg.style.cursor = 'grab';
    }
    lbImg.addEventListener('pointerup', onPointerEnd);
    lbImg.addEventListener('pointercancel', onPointerEnd);

    // --- Scroll-wheel zoom (desktop) ----------------------------------------

    overlay.addEventListener('wheel', function (e) {
      e.preventDefault();
      scale = Math.max(0.5, Math.min(10, scale * (e.deltaY < 0 ? 1.12 : 0.9)));
      apply();
    }, { passive: false });
  });
}());
</script>"#;

pub(crate) fn process(content: &str) -> String {
    if !has_images(content) {
        return content.to_string();
    }
    let mut out = content.to_string();
    out.push_str(LIGHTBOX_CSS);
    out.push_str(LIGHTBOX_JS);
    out
}

fn has_images(content: &str) -> bool {
    content.contains("![") || content.contains("<img")
}
