//! guestbook: tiny pre-moderated guestbook backend.
//!
//! Public:  GET/POST /api/guestbook
//! Admin:   GET /admin, GET /api/admin/entries, POST /api/admin/entries/{id}
//! Admin routes are behind HTTP basic auth (user `kurumi`, password from
//! GB_ADMIN_PASSWORD); without that variable the admin is disabled.

use std::{
    collections::HashMap,
    env, fs,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use base64::Engine;
use serde::{Deserialize, Serialize};

const NAME_MAX: usize = 32;
const URL_MAX: usize = 200;
const MESSAGE_MAX: usize = 500;
const REPLY_MAX: usize = 500;
const POST_COOLDOWN: Duration = Duration::from_secs(30);
const PENDING_MAX: usize = 300;

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    id: u64,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply: Option<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    approved: bool,
    created: u64,
    #[serde(default)]
    ip: String,
    /// Secret handed to the author so they can see their own entry before
    /// it is approved (or when it is private).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    token: String,
}

#[derive(Serialize, Deserialize)]
struct Db {
    next_id: u64,
    entries: Vec<Entry>,
    #[serde(default)]
    views: u64,
}

struct App {
    db: Mutex<Db>,
    path: PathBuf,
    last_post: Mutex<HashMap<String, Instant>>,
    last_view: Mutex<HashMap<String, Instant>>,
    admin_password: Option<String>,
}

type Shared = Arc<App>;

fn err(code: StatusCode, msg: &str) -> Response {
    (code, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Trim, normalise newlines, drop control characters, and collapse runs of
/// blank lines. Newlines are kept only when `multiline` is set.
fn clean(s: &str, multiline: bool) -> String {
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0;
    for c in s.trim().chars() {
        if c == '\n' {
            if multiline {
                newlines += 1;
                if newlines <= 2 {
                    out.push('\n');
                }
            } else {
                out.push(' ');
            }
            continue;
        }
        newlines = 0;
        if c.is_control() {
            continue;
        }
        out.push(c);
    }
    out.trim().to_string()
}

/// 128-bit random token, hex encoded.
fn new_token() -> std::io::Result<String> {
    let mut buf = [0u8; 16];
    std::io::Read::read_exact(&mut fs::File::open("/dev/urandom")?, &mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// Parse the `X-Guestbook-Own` header: comma-separated `id:token` pairs.
fn own_tokens(headers: &HeaderMap) -> Vec<(u64, String)> {
    headers
        .get("x-guestbook-own")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(',')
        .take(100)
        .filter_map(|pair| {
            let (id, tok) = pair.trim().split_once(':')?;
            let id = id.parse().ok()?;
            (tok.len() == 32).then(|| (id, tok.to_string()))
        })
        .collect()
}

impl App {
    fn load(path: PathBuf, admin_password: Option<String>) -> Self {
        let db = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Db {
                next_id: 1,
                entries: Vec::new(),
                views: 0,
            },
            Err(e) => panic!("cannot read {}: {e}", path.display()),
        };
        App {
            db: Mutex::new(db),
            path,
            last_post: Mutex::new(HashMap::new()),
            last_view: Mutex::new(HashMap::new()),
            admin_password,
        }
    }

    /// Atomically write the database: temp file, fsync, rename.
    fn save(&self, db: &Db) -> std::io::Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec_pretty(db)?)?;
        f.sync_all()?;
        fs::rename(&tmp, &self.path)
    }
}

#[derive(Serialize)]
struct PublicEntry<'a> {
    id: u64,
    name: &'a str,
    url: Option<&'a str>,
    message: &'a str,
    reply: Option<&'a str>,
    created: u64,
    /// Only set on the viewer's own entries.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    own: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pending: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    private: bool,
}

async fn list_public(State(app): State<Shared>, headers: HeaderMap) -> Response {
    let own = own_tokens(&headers);
    let is_own = |e: &Entry| {
        !e.token.is_empty()
            && own.iter().any(|(id, tok)| *id == e.id && ct_eq(tok.as_bytes(), e.token.as_bytes()))
    };
    let db = app.db.lock().unwrap();
    let out: Vec<PublicEntry> = db
        .entries
        .iter()
        .filter_map(|e| {
            let public = e.approved && !e.private;
            let own = is_own(e);
            (public || own).then(|| PublicEntry {
                id: e.id,
                name: &e.name,
                url: e.url.as_deref(),
                message: &e.message,
                reply: e.reply.as_deref(),
                created: e.created,
                own,
                pending: own && !e.approved && !e.private,
                private: own && e.private,
            })
        })
        .collect();
    Json(out).into_response()
}

#[derive(Deserialize)]
struct NewEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    private: bool,
    /// Honeypot: hidden from humans, bots tend to fill it.
    #[serde(default)]
    website: String,
}

async fn create(State(app): State<Shared>, headers: HeaderMap, Json(req): Json<NewEntry>) -> Response {
    let ip = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    if !req.website.is_empty() {
        // Pretend it worked so the bot moves on.
        return (StatusCode::CREATED, Json(serde_json::json!({ "id": 0 }))).into_response();
    }

    let message = clean(&req.message, true);
    let mut name = clean(&req.name, false);
    let url = clean(&req.url, false);

    if message.is_empty() {
        return err(StatusCode::BAD_REQUEST, "message is empty");
    }
    if message.chars().count() > MESSAGE_MAX {
        return err(StatusCode::BAD_REQUEST, "message is too long");
    }
    if name.is_empty() {
        name = "anonymous".into();
    }
    if name.chars().count() > NAME_MAX {
        return err(StatusCode::BAD_REQUEST, "name is too long");
    }
    let url = if url.is_empty() {
        None
    } else if url.chars().count() > URL_MAX {
        return err(StatusCode::BAD_REQUEST, "link is too long");
    } else if url.starts_with("https://") || url.starts_with("http://") {
        Some(url)
    } else {
        return err(StatusCode::BAD_REQUEST, "link must start with http:// or https://");
    };

    {
        let mut last = app.last_post.lock().unwrap();
        let t = Instant::now();
        if let Some(prev) = last.get(&ip) {
            if t.duration_since(*prev) < POST_COOLDOWN {
                return err(StatusCode::TOO_MANY_REQUESTS, "slow down a bit");
            }
        }
        if last.len() > 10_000 {
            last.retain(|_, p| t.duration_since(*p) < POST_COOLDOWN);
        }
        last.insert(ip.clone(), t);
    }

    let mut db = app.db.lock().unwrap();
    if db.entries.iter().filter(|e| !e.approved).count() >= PENDING_MAX {
        return err(StatusCode::SERVICE_UNAVAILABLE, "guestbook is full, try later");
    }
    let token = match new_token() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("token generation failed: {e}");
            return err(StatusCode::INTERNAL_SERVER_ERROR, "could not save");
        }
    };
    let id = db.next_id;
    db.next_id += 1;
    db.entries.push(Entry {
        id,
        name,
        url,
        message,
        reply: None,
        private: req.private,
        approved: false,
        created: now(),
        ip,
        token: token.clone(),
    });
    if let Err(e) = app.save(&db) {
        eprintln!("save failed: {e}");
        db.entries.pop();
        db.next_id -= 1;
        return err(StatusCode::INTERNAL_SERVER_ERROR, "could not save");
    }
    (StatusCode::CREATED, Json(serde_json::json!({ "id": id, "token": token }))).into_response()
}

const VIEW_COOLDOWN: Duration = Duration::from_secs(10);

/// Count a page view and return the total. Rapid reloads from the same IP
/// are not counted, so the number can't be pumped with a curl loop.
async fn view(State(app): State<Shared>, headers: HeaderMap) -> Response {
    let ip = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();
    let count = {
        let mut last = app.last_view.lock().unwrap();
        let t = Instant::now();
        let fresh = last.get(&ip).is_none_or(|p| t.duration_since(*p) >= VIEW_COOLDOWN);
        if last.len() > 10_000 {
            last.retain(|_, p| t.duration_since(*p) < VIEW_COOLDOWN);
        }
        last.insert(ip, t);
        fresh
    };
    let mut db = app.db.lock().unwrap();
    if count {
        db.views += 1;
        if let Err(e) = app.save(&db) {
            eprintln!("save failed: {e}");
        }
    }
    Json(serde_json::json!({ "views": db.views })).into_response()
}

async fn list_admin(State(app): State<Shared>) -> Response {
    let db = app.db.lock().unwrap();
    let mut all = db.entries.clone();
    all.reverse();
    Json(all).into_response()
}

#[derive(Deserialize)]
struct AdminAction {
    action: String,
    #[serde(default)]
    reply: String,
}

async fn admin_action(
    State(app): State<Shared>,
    Path(id): Path<u64>,
    Json(req): Json<AdminAction>,
) -> Response {
    let mut db = app.db.lock().unwrap();
    let Some(pos) = db.entries.iter().position(|e| e.id == id) else {
        return err(StatusCode::NOT_FOUND, "no such entry");
    };
    match req.action.as_str() {
        "approve" => {
            if db.entries[pos].private {
                return err(StatusCode::BAD_REQUEST, "private entries cannot be published");
            }
            db.entries[pos].approved = true;
        }
        "hide" => db.entries[pos].approved = false,
        "reply" => {
            let reply = clean(&req.reply, true);
            if reply.chars().count() > REPLY_MAX {
                return err(StatusCode::BAD_REQUEST, "reply is too long");
            }
            // Replying to a public entry also publishes it.
            if !reply.is_empty() && !db.entries[pos].private {
                db.entries[pos].approved = true;
            }
            db.entries[pos].reply = (!reply.is_empty()).then_some(reply);
        }
        "delete" => {
            db.entries.remove(pos);
        }
        _ => return err(StatusCode::BAD_REQUEST, "unknown action"),
    }
    if let Err(e) = app.save(&db) {
        eprintln!("save failed: {e}");
        return err(StatusCode::INTERNAL_SERVER_ERROR, "could not save");
    }
    StatusCode::NO_CONTENT.into_response()
}

const ADMIN_USER: &str = "kurumi";

/// Constant-time byte comparison, so the password can't be guessed by timing.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn require_admin(State(app): State<Shared>, req: Request, next: Next) -> Response {
    let Some(password) = app.admin_password.as_deref() else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "admin is disabled");
    };
    let ok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|b64| base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok())
        .is_some_and(|creds| {
            let expected = format!("{ADMIN_USER}:{password}");
            ct_eq(&creds, expected.as_bytes())
        });
    if ok {
        return next.run(req).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"guestbook admin\", charset=\"UTF-8\"")],
        "unauthorized",
    )
        .into_response()
}

async fn admin_page() -> Html<&'static str> {
    Html(include_str!("admin.html"))
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let path = PathBuf::from(env::var("GB_DB").unwrap_or_else(|_| "guestbook.json".into()));
    let addr = env::var("GB_ADDR").unwrap_or_else(|_| "127.0.0.1:8737".into());
    let admin_password = env::var("GB_ADMIN_PASSWORD").ok().filter(|p| !p.is_empty());
    if admin_password.is_none() {
        eprintln!("GB_ADMIN_PASSWORD is not set: admin is disabled");
    }
    let app = Arc::new(App::load(path, admin_password));

    let admin = Router::new()
        .route("/api/admin/entries", get(list_admin))
        .route("/api/admin/entries/{id}", post(admin_action))
        .route("/admin", get(admin_page))
        .route_layer(middleware::from_fn_with_state(app.clone(), require_admin));

    let mut router = Router::new()
        .route("/api/guestbook", get(list_public).post(create))
        .route("/api/views", post(view))
        .merge(admin);

    // Local preview only: serve the site's index.html from GB_STATIC.
    // In production nginx serves static files and this stays unset.
    if let Ok(dir) = env::var("GB_STATIC") {
        let keys = PathBuf::from(&dir).join("keys");
        router = router.route(
            "/keys",
            get(move || {
                let keys = keys.clone();
                async move {
                    match fs::read_to_string(&keys) {
                        Ok(s) => ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], s).into_response(),
                        Err(_) => StatusCode::NOT_FOUND.into_response(),
                    }
                }
            }),
        );
        let index = PathBuf::from(dir).join("index.html");
        router = router.route(
            "/",
            get(move || {
                let index = index.clone();
                async move {
                    match fs::read_to_string(&index) {
                        Ok(s) => Html(s).into_response(),
                        Err(_) => StatusCode::NOT_FOUND.into_response(),
                    }
                }
            }),
        );
    }

    let router = router
        .layer(DefaultBodyLimit::max(16 * 1024))
        .with_state(app);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {addr}: {e}"));
    eprintln!("guestbook listening on {addr}");
    axum::serve(listener, router).await.unwrap();
}
