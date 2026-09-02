(function () {
  "use strict";

  /** @type {HTMLElement | null} */
  let modal = null;
  let previousFocus = null;
  /** @type {{ mermaidEl: Element, svg: SVGSVGElement, prevMaxWidth: string, prevWidth: string } | null} */
  let active = null;

  function overlayBg() {
    return document.documentElement.classList.contains("dark")
      ? "rgba(17, 17, 17, 0.98)"
      : "rgba(255, 255, 255, 0.98)";
  }

  function ensureModal() {
    if (!modal) {
      modal = document.getElementById("mermaid-zoom");
    }
    if (!modal) {
      return null;
    }

    modal.style.position = "fixed";
    modal.style.inset = "0";
    modal.style.zIndex = "9999";
    modal.querySelector(".mermaid-zoom__backdrop").style.background = overlayBg();
    return modal;
  }

  function bindModalEvents() {
    if (!modal || modal.dataset.mermaidZoomEvents === "true") {
      return;
    }
    modal.dataset.mermaidZoomEvents = "true";

    modal.addEventListener("click", (event) => {
      if (event.target.closest("[data-mermaid-zoom-close]")) {
        closeModal();
      }
    });

    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && modal && !modal.hidden) {
        closeModal();
      }
    });
  }

  function openModal(mermaidEl) {
    const host = ensureModal();
    if (!host) {
      return;
    }

    const svg = mermaidEl.querySelector("svg");
    if (!svg) {
      return;
    }

    const stage = host.querySelector(".mermaid-zoom__stage");

    active = {
      mermaidEl,
      svg,
      prevMaxWidth: svg.style.maxWidth,
      prevWidth: svg.style.width,
    };

    // Move the live SVG (do not clone): Mermaid embeds #id-scoped styles and
    // foreignObject labels that break when cloned or when id is stripped.
    stage.appendChild(svg);
    svg.style.maxWidth = "95vw";
    svg.style.width = "95vw";

    host.removeAttribute("hidden");
    host.setAttribute("aria-hidden", "false");
    previousFocus = document.activeElement;
    document.body.classList.add("mermaid-zoom-open");
    host.querySelector(".mermaid-zoom__close").focus();
  }

  function closeModal() {
    if (!modal || modal.hidden) {
      return;
    }

    if (active) {
      active.svg.style.maxWidth = active.prevMaxWidth;
      active.svg.style.width = active.prevWidth;
      active.mermaidEl.appendChild(active.svg);
      active = null;
    }

    modal.setAttribute("hidden", "");
    modal.setAttribute("aria-hidden", "true");
    document.body.classList.remove("mermaid-zoom-open");
    if (previousFocus && typeof previousFocus.focus === "function") {
      previousFocus.focus();
    }
  }

  function bindDiagram(el) {
    if (el.dataset.mermaidZoomBound === "true") {
      return;
    }
    if (!el.querySelector("svg")) {
      return;
    }

    el.dataset.mermaidZoomBound = "true";
    el.classList.add("mermaid--zoomable");
    el.addEventListener("click", () => {
      if (modal && !modal.hidden && active && active.mermaidEl === el) {
        return;
      }
      openModal(el);
    });
  }

  function bindAll() {
    document.querySelectorAll(".mermaid").forEach(bindDiagram);
  }

  function resetBindings() {
    closeModal();
    document.querySelectorAll(".mermaid").forEach((el) => {
      el.dataset.mermaidZoomBound = "false";
      el.classList.remove("mermaid--zoomable");
    });
  }

  function init() {
    modal = document.getElementById("mermaid-zoom");
    bindModalEvents();
    bindAll();

    let attempts = 0;
    const retry = window.setInterval(() => {
      bindAll();
      attempts += 1;
      if (attempts >= 40) {
        window.clearInterval(retry);
      }
    }, 100);
  }

  document.addEventListener("DOMContentLoaded", init);

  // Hextra re-renders mermaid on dark/light toggle (~150ms debounce).
  let themeTimeout;
  new MutationObserver(() => {
    window.clearTimeout(themeTimeout);
    themeTimeout = window.setTimeout(() => {
      resetBindings();
      bindAll();
    }, 220);
  }).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["class"],
  });
})();
