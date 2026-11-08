document.getElementById("hi").textContent = "h" + "i".repeat(1 + Math.floor(Math.random() * 4));

(function () {
  var words = ["dumbass", "anime girl", "vibecoder", "being human", "c fanatic"];
  var el = document.getElementById("what");
  var cur = -1;
  function next() {
    var i;
    do { i = Math.floor(Math.random() * words.length); } while (i === cur);
    cur = i;
    el.textContent = words[i];
  }
  el.addEventListener("click", next);
  el.addEventListener("keydown", function (e) {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); next(); }
  });
  next();
})();

(function () {
  fetch("/api/views", { method: "POST" }).then(function (r) {
    if (!r.ok) throw new Error(r.status);
    return r.json();
  }).then(function (j) {
    document.getElementById("views").textContent = j.views;
    document.getElementById("views-line").hidden = false;
  }).catch(function () {});
})();

(function () {
  var d = new Date(document.lastModified);
  if (isNaN(d)) return;
  function p(n) { return n < 10 ? "0" + n : "" + n; }
  document.getElementById("updated").textContent =
    d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate());
})();
