(function () {
  var list = document.getElementById("gb-list");
  var form = document.getElementById("gb-form");
  var status = document.getElementById("gb-status");
  var nameInput = form.elements.name;
  var urlInput = form.elements.url;
  var linkBtn = document.getElementById("gb-link");
  var sendBtn = form.querySelector("[type=submit]");

  var adj = ["sleepy", "shy", "lost", "tiny", "silly", "grumpy", "cozy", "sneaky",
    "lazy", "curious", "dizzy", "quiet", "feral", "smol", "anxious", "chaotic"];
  var noun = ["daemon", "penguin", "cat", "fox", "ghost", "raccoon", "frog", "moth",
    "otter", "crow", "hamster", "kernel", "process", "packet", "goblin", "bunny"];
  function pick(a) { return a[Math.floor(Math.random() * a.length)]; }
  function randomName() { return pick(adj) + " " + pick(noun); }
  function fitName() { nameInput.style.width = Math.max(4, nameInput.value.length + 1) + "ch"; }

  var placeholders = [
    "meow meow maow purr",
    "mrrp mrrrow nya~",
    "purr purr *knocks your cup off the table*",
    "*sits on your keyboard* fffffffffffffff",
    "мяу мяу мур мур",
    "тут был даня",
    "hiii kurumi :3",
    "sudo leave a nice message",
    "kurumi ur site is cool",
    "i use arch btw",
    "its so renakover",
    "кто прочитал тот здохнет",
    "nyaaa~ :3",
    "mrow? mrrp? MROW!",
    "*headbutts your screen*",
    "*slow blink*",
    "hiss hiss >:(",
    "feed me or i scream at 4am",
    "*brings you a dead mouse as a gift*",
    "кис кис кис",
    "мяу. это всё что я хотел сказать",
    "мурмурмурмурмур",
    "котик был здесь =^.^=",
    "а у меня свой сервер на докере",
    "сломал прод в пятницу",
    "ctrl+c ctrl+v",
    ":wq",
    "how do i exit vim",
    "segmentation fault (core dumped)",
    "works on my machine",
    "have you tried turning it off and on again",
    "rewrite it in rust",
    "chmod 777 everything",
    "kernel panic - not syncing: too cute",
    "/dev/null is my diary",
    "sudo make me a sandwich",
    "404 thoughts not found"
  ];
  var curPh = -1;
  function nextPlaceholder() {
    var i;
    do { i = Math.floor(Math.random() * placeholders.length); } while (i === curPh);
    curPh = i;
    form.elements.message.placeholder = placeholders[i];
  }
  nextPlaceholder();

  var saved = null;
  try { saved = localStorage.getItem("gb-name"); } catch (e) {}
  nameInput.value = saved || randomName();
  fitName();
  nameInput.addEventListener("input", fitName);
  document.getElementById("gb-dice").addEventListener("click", function () {
    nameInput.value = randomName();
    fitName();
  });

  linkBtn.addEventListener("click", function () {
    urlInput.hidden = !urlInput.hidden;
    linkBtn.className = urlInput.hidden ? "gb-btn" : "gb-btn on";
    if (!urlInput.hidden) urlInput.focus();
  });

  function fmt(ts) {
    var d = new Date(ts * 1000);
    function p(n) { return n < 10 ? "0" + n : "" + n; }
    return d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate()) +
      " " + p(d.getHours()) + ":" + p(d.getMinutes());
  }
  function el(tag, cls, text) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text != null) e.textContent = text;
    return e;
  }

  function render(entries) {
    list.textContent = "";
    if (!entries.length) {
      list.appendChild(el("p", "gb-empty", "no entries yet. be the first :3"));
      return;
    }
    entries.forEach(function (e) {
      var box = el("div", "gb-entry");
      var head = el("div", "gb-head");
      var who;
      if (e.url) {
        who = el("a", null, e.name);
        who.href = e.url;
        who.rel = "nofollow ugc noopener noreferrer";
        who.target = "_blank";
      } else {
        who = el("b", null, e.name);
      }
      head.appendChild(who);
      head.appendChild(document.createTextNode(" #" + e.id + " " + fmt(e.created)));
      box.appendChild(head);
      box.appendChild(el("div", "gb-msg", e.message));
      if (e.reply) box.appendChild(el("div", "gb-reply", "> " + e.reply));
      if (e.private) box.appendChild(el("div", "gb-pending", "// private: only you and kurumi can see this"));
      else if (e.pending) box.appendChild(el("div", "gb-pending", "// waiting for a reply: only you can see this for now"));
      list.appendChild(box);
    });
    list.scrollTop = list.scrollHeight;
  }

  // Entries this browser posted: [{id, token}], so the author can see
  // their own message before it's approved.
  function getOwn() {
    try {
      var o = JSON.parse(localStorage.getItem("gb-own") || "[]");
      return Array.isArray(o) ? o : [];
    } catch (e) { return []; }
  }
  function setOwn(o) {
    try { localStorage.setItem("gb-own", JSON.stringify(o.slice(-50))); } catch (e) {}
  }

  function load() {
    var own = getOwn();
    var headers = {};
    if (own.length) {
      headers["X-Guestbook-Own"] = own.map(function (o) { return o.id + ":" + o.token; }).join(",");
    }
    return fetch("/api/guestbook", { headers: headers }).then(function (r) {
      if (!r.ok) throw new Error(r.status);
      return r.json();
    }).then(function (entries) {
      // Forget entries that were deleted.
      var seen = {};
      entries.forEach(function (e) { if (e.own) seen[e.id] = true; });
      var kept = own.filter(function (o) { return seen[o.id]; });
      if (kept.length !== own.length) setOwn(kept);
      render(entries);
    }).catch(function () {
      list.textContent = "";
      list.appendChild(el("p", "gb-empty", "guestbook is offline right now"));
    });
  }
  load();

  form.addEventListener("submit", function (ev) {
    ev.preventDefault();
    var msg = form.elements.message.value.trim();
    if (!msg) {
      status.textContent = "write something first";
      return;
    }
    var priv = form.elements.private.checked;
    sendBtn.disabled = true;
    status.textContent = "sending...";
    try { localStorage.setItem("gb-name", nameInput.value.trim()); } catch (e) {}
    fetch("/api/guestbook", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: nameInput.value,
        url: urlInput.hidden ? "" : urlInput.value,
        message: msg,
        private: priv,
        website: form.elements.website.value
      })
    }).then(function (r) {
      return r.json().catch(function () { return {}; }).then(function (j) {
        if (!r.ok) throw new Error(j.error || "something went wrong (" + r.status + ")");
        return j;
      });
    }).then(function (j) {
      form.elements.message.value = "";
      nextPlaceholder();
      status.textContent = priv
        ? "sent! only kurumi will see it"
        : "sent! others will see it once kurumi replies";
      if (j.id && j.token) {
        var own = getOwn();
        own.push({ id: j.id, token: j.token });
        setOwn(own);
        return load();
      }
    }).catch(function (e) {
      status.textContent = "error: " + e.message;
    }).then(function () {
      sendBtn.disabled = false;
    });
  });

  form.elements.message.addEventListener("keydown", function (e) {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) form.requestSubmit();
  });
})();
