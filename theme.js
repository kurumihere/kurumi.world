(function () {
  var root = document.documentElement;
  try {
    var saved = localStorage.getItem("theme");
    if (saved === "light" || saved === "dark") {
      root.setAttribute("data-theme", saved);
    }
  } catch (e) {}

  var mq = window.matchMedia("(prefers-color-scheme: dark)");
  var modes = ["auto", "light", "dark"];

  function current() {
    var t = root.getAttribute("data-theme");
    return t === "light" || t === "dark" ? t : "auto";
  }
  function effective(mode) {
    return mode === "auto" ? (mq.matches ? "dark" : "light") : mode;
  }
  function render() {
    var btn = document.getElementById("theme-toggle");
    if (!btn) return;
    var mode = current();
    btn.textContent = "[theme: " + mode + "]";
    btn.title = "theme: " + mode + " (" + effective(mode) + " now), click to change";
    btn.setAttribute("aria-label", btn.title);
  }
  function apply(mode) {
    if (mode === "auto") {
      root.removeAttribute("data-theme");
      try { localStorage.removeItem("theme"); } catch (e) {}
    } else {
      root.setAttribute("data-theme", mode);
      try { localStorage.setItem("theme", mode); } catch (e) {}
    }
    render();
  }
  function init() {
    var btn = document.getElementById("theme-toggle");
    if (!btn) return;
    btn.addEventListener("click", function () {
      apply(modes[(modes.indexOf(current()) + 1) % modes.length]);
    });
    if (mq.addEventListener) mq.addEventListener("change", render);
    else if (mq.addListener) mq.addListener(render);
    render();
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
