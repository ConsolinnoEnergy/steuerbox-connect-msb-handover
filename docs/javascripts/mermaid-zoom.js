/* Zoom button for mermaid diagrams: opens the rendered diagram fullscreen. */
(function () {
  "use strict";

  function setButtonState(btn, wrap) {
    var isFs = document.fullscreenElement === wrap;
    btn.textContent = isFs ? "✕" : "⤢";
    btn.title = isFs ? "Verkleinern" : "Diagramm vergrößern";
    btn.setAttribute("aria-label", btn.title);
  }

  function enhance(el) {
    if (el.closest(".mermaid-zoom-wrap")) {
      // Already wrapped — only make sure the wrap really contains it.
      return;
    }
    var wrap = document.createElement("div");
    wrap.className = "mermaid-zoom-wrap";
    el.parentNode.insertBefore(wrap, el);
    wrap.appendChild(el);

    var btn = document.createElement("button");
    btn.type = "button";
    btn.className = "mermaid-zoom-btn";
    setButtonState(btn, wrap);
    btn.addEventListener("click", function () {
      if (document.fullscreenElement === wrap) {
        document.exitFullscreen();
      } else {
        wrap.requestFullscreen && wrap.requestFullscreen();
      }
    });
    document.addEventListener("fullscreenchange", function () {
      setButtonState(btn, wrap);
    });
    wrap.appendChild(btn);
  }

  function enhanceAll() {
    document.querySelectorAll(".mermaid").forEach(enhance);
  }

  // Mermaid renders asynchronously; also catch late-inserted diagrams.
  new MutationObserver(enhanceAll).observe(document.body, {
    childList: true,
    subtree: true,
  });

  enhanceAll();
})();