use hbb_common::{
    anyhow::Context,
    base64, log,
    sha2::{Digest, Sha256},
    ResultType,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const API_PORT: u16 = 21114;

const PERSONAL_AB_GUID: &str = "personal";

lazy_static::lazy_static! {
    static ref RUNTIME: Mutex<Option<ApiRuntime>> = Default::default();
    static ref STORE: Mutex<ApiStore> = Mutex::new(load_store());
}

struct ApiRuntime {
    stop: Arc<AtomicBool>,
}

impl Drop for ApiRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct ApiStore {
    users: HashMap<String, StoredUser>,
    tokens: HashMap<String, String>,
    legacy_ab: Option<String>,
    personal_peers: Vec<Value>,
    personal_tags: Vec<Value>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct StoredUser {
    password_hash: String,
    display_name: String,
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

pub fn start() -> ResultType<()> {
    let mut runtime = RUNTIME.lock().unwrap();
    if runtime.is_some() {
        return Ok(());
    }

    let listener = match TcpListener::bind(("0.0.0.0", API_PORT)) {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            log::warn!(
                "Beyond Remote account API port {} is already in use; assuming an API is already running",
                API_PORT
            );
            return Ok(());
        }
        Err(err) => return Err(err.into()),
    };
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    std::thread::spawn(move || {
        while !stop_thread.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    std::thread::spawn(move || {
                        if let Err(err) = handle_stream(stream) {
                            log::debug!("Beyond Remote account API request failed: {err}");
                        }
                    });
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(err) => {
                    log::warn!("Beyond Remote account API listener failed: {err}");
                    std::thread::sleep(Duration::from_millis(250));
                }
            }
        }
    });
    *runtime = Some(ApiRuntime { stop });
    Ok(())
}

pub fn local_url() -> String {
    format!("http://127.0.0.1:{API_PORT}")
}

fn handle_stream(mut stream: TcpStream) -> ResultType<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let request = read_request(&mut stream)?;
    let response = route(request);
    write_response(&mut stream, response)
}

fn read_request(stream: &mut TcpStream) -> ResultType<Request> {
    let mut data = Vec::new();
    let mut buf = [0_u8; 4096];
    let mut header_end = None;
    while header_end.is_none() {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
        header_end = find_header_end(&data);
        if data.len() > 1024 * 1024 {
            return Err(hbb_common::anyhow::anyhow!("request too large"));
        }
    }
    let header_end = header_end.context("missing HTTP headers")?;
    let header_text = String::from_utf8_lossy(&data[..header_end]);
    let mut lines = header_text.lines();
    let first = lines.next().context("missing request line")?;
    let mut first_parts = first.split_whitespace();
    let method = first_parts.next().unwrap_or_default().to_owned();
    let raw_path = first_parts.next().unwrap_or_default();
    let path = raw_path.split('?').next().unwrap_or_default().to_owned();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            headers.insert(key.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    while data.len() < body_start + content_length {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    let body = data
        .get(body_start..body_start + content_length.min(data.len().saturating_sub(body_start)))
        .unwrap_or_default()
        .to_vec();
    Ok(Request {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|w| w == b"\r\n\r\n")
}

fn route(request: Request) -> HttpResponse {
    if request.method == "OPTIONS" {
        return HttpResponse::json(204, json!(null));
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/api/beyondremote/ping") => HttpResponse::json(200, json!({"ok": true})),
        ("GET", "/api/login-options") => HttpResponse::json(200, json!([])),
        ("POST", "/api/login") => login(request),
        ("POST", "/api/currentUser") => current_user(request),
        ("POST", "/api/logout") => HttpResponse::json(200, json!(null)),
        ("GET", "/api/ab") | ("POST", "/api/ab/get") => legacy_ab_get(request),
        ("POST", "/api/ab") => legacy_ab_set(request),
        ("POST", "/api/ab/settings") => HttpResponse::json(200, json!({"max_peer_one_ab": 0})),
        ("POST", "/api/ab/personal") => HttpResponse::json(200, json!({"guid": PERSONAL_AB_GUID})),
        ("POST", "/api/ab/shared/profiles") => {
            HttpResponse::json(200, json!({"total": 0, "data": []}))
        }
        ("POST", "/api/ab/peers") => paged(personal_peers()),
        ("POST", path) if path.starts_with("/api/ab/tags/") => {
            HttpResponse::json(200, Value::Array(personal_tags()))
        }
        ("POST", path) if path.starts_with("/api/ab/peer/add/") => peer_add(request),
        ("PUT", path) if path.starts_with("/api/ab/peer/update/") => peer_update(request),
        ("DELETE", path) if path.starts_with("/api/ab/peer/") => peer_delete(request),
        ("POST", path) if path.starts_with("/api/ab/tag/add/") => tag_add(request),
        ("PUT", path) if path.starts_with("/api/ab/tag/rename/") => tag_rename(request),
        ("PUT", path) if path.starts_with("/api/ab/tag/update/") => tag_update(request),
        ("DELETE", path) if path.starts_with("/api/ab/tag/") => tag_delete(request),
        ("GET", "/api/device-group/accessible") => paged(Vec::new()),
        ("GET", "/api/users") => users(request),
        ("GET", "/api/peers") => paged(Vec::new()),
        _ => HttpResponse::json(404, json!({"error": "not found"})),
    }
}

fn login(request: Request) -> HttpResponse {
    let body = json_body(&request);
    let username = body
        .get("username")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("local");
    let password_hash = hash_password(body.get("password").and_then(Value::as_str).unwrap_or(""));
    let mut store = STORE.lock().unwrap();
    if let Some(user) = store.users.get(username) {
        if user.password_hash != password_hash {
            return HttpResponse::json(401, json!({"error": "Invalid username or password"}));
        }
    } else {
        store.users.insert(
            username.to_owned(),
            StoredUser {
                password_hash,
                display_name: username.to_owned(),
            },
        );
    }
    let token = new_token(username);
    store.tokens.insert(token.clone(), username.to_owned());
    save_store(&store);
    HttpResponse::json(
        200,
        json!({
            "type": "access_token",
            "access_token": token,
            "user": user_payload(username, store.users.get(username)),
        }),
    )
}

fn current_user(request: Request) -> HttpResponse {
    let Some(username) = request_user(&request) else {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    };
    let store = STORE.lock().unwrap();
    HttpResponse::json(200, user_payload(&username, store.users.get(&username)))
}

fn users(request: Request) -> HttpResponse {
    let Some(username) = request_user(&request) else {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    };
    let store = STORE.lock().unwrap();
    paged(vec![user_payload(&username, store.users.get(&username))])
}

fn legacy_ab_get(request: Request) -> HttpResponse {
    if request_user(&request).is_none() {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    }
    let store = STORE.lock().unwrap();
    HttpResponse::json(
        200,
        json!({
            "data": store.legacy_ab.clone().unwrap_or_else(|| json!({"tags":[],"peers":[],"tag_colors":"{}"}).to_string()),
            "updated_at": now_millis(),
        }),
    )
}

fn legacy_ab_set(request: Request) -> HttpResponse {
    if request_user(&request).is_none() {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    }
    let body = json_body(&request);
    let data = body
        .get("data")
        .and_then(Value::as_str)
        .unwrap_or(r#"{"tags":[],"peers":[],"tag_colors":"{}"}"#);
    let mut store = STORE.lock().unwrap();
    store.legacy_ab = Some(data.to_owned());
    if let Ok(v) = serde_json::from_str::<Value>(data) {
        if let Some(peers) = v.get("peers").and_then(Value::as_array) {
            store.personal_peers = peers.clone();
        }
        if let Some(tags) = v.get("tags").and_then(Value::as_array) {
            store.personal_tags = tags
                .iter()
                .filter_map(Value::as_str)
                .map(|name| json!({"name": name, "color": tag_color(name)}))
                .collect();
        }
    }
    save_store(&store);
    HttpResponse::json(200, json!(null))
}

fn peer_add(request: Request) -> HttpResponse {
    if request_user(&request).is_none() {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    }
    let peer = json_body(&request);
    let Some(id) = peer.get("id").and_then(Value::as_str).map(str::to_owned) else {
        return HttpResponse::json(400, json!({"error": "Missing peer id"}));
    };
    let mut store = STORE.lock().unwrap();
    if let Some(existing) = store
        .personal_peers
        .iter_mut()
        .find(|p| p.get("id").and_then(Value::as_str) == Some(id.as_str()))
    {
        merge_object(existing, &peer);
    } else {
        store.personal_peers.push(peer);
    }
    save_store(&store);
    HttpResponse::empty_ok()
}

fn peer_update(request: Request) -> HttpResponse {
    if request_user(&request).is_none() {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    }
    let patch = json_body(&request);
    let Some(id) = patch.get("id").and_then(Value::as_str).map(str::to_owned) else {
        return HttpResponse::json(400, json!({"error": "Missing peer id"}));
    };
    let mut store = STORE.lock().unwrap();
    if let Some(existing) = store
        .personal_peers
        .iter_mut()
        .find(|p| p.get("id").and_then(Value::as_str) == Some(id.as_str()))
    {
        merge_object(existing, &patch);
    }
    save_store(&store);
    HttpResponse::empty_ok()
}

fn peer_delete(request: Request) -> HttpResponse {
    if request_user(&request).is_none() {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    }
    let ids = json_body(&request);
    let ids: Vec<String> = ids
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mut store = STORE.lock().unwrap();
    store.personal_peers.retain(|p| {
        !ids.iter()
            .any(|id| p.get("id").and_then(Value::as_str) == Some(id))
    });
    save_store(&store);
    HttpResponse::empty_ok()
}

fn tag_add(request: Request) -> HttpResponse {
    if request_user(&request).is_none() {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    }
    let tag = json_body(&request);
    let Some(name) = tag.get("name").and_then(Value::as_str).map(str::to_owned) else {
        return HttpResponse::json(400, json!({"error": "Missing tag name"}));
    };
    let mut store = STORE.lock().unwrap();
    if !store
        .personal_tags
        .iter()
        .any(|t| t.get("name").and_then(Value::as_str) == Some(name.as_str()))
    {
        store.personal_tags.push(json!({
            "name": name,
            "color": tag.get("color").and_then(Value::as_i64).unwrap_or_else(|| tag_color(&name) as i64),
        }));
    }
    save_store(&store);
    HttpResponse::empty_ok()
}

fn tag_rename(request: Request) -> HttpResponse {
    if request_user(&request).is_none() {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    }
    let body = json_body(&request);
    let old = body.get("old").and_then(Value::as_str).unwrap_or_default();
    let new = body.get("new").and_then(Value::as_str).unwrap_or_default();
    let mut store = STORE.lock().unwrap();
    for tag in &mut store.personal_tags {
        if tag.get("name").and_then(Value::as_str) == Some(old) {
            tag["name"] = json!(new);
        }
    }
    for peer in &mut store.personal_peers {
        rename_peer_tag(peer, old, new);
    }
    save_store(&store);
    HttpResponse::empty_ok()
}

fn tag_update(request: Request) -> HttpResponse {
    if request_user(&request).is_none() {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    }
    let body = json_body(&request);
    let name = body.get("name").and_then(Value::as_str).unwrap_or_default();
    let color = body.get("color").and_then(Value::as_i64).unwrap_or(0);
    let mut store = STORE.lock().unwrap();
    for tag in &mut store.personal_tags {
        if tag.get("name").and_then(Value::as_str) == Some(name) {
            tag["color"] = json!(color);
        }
    }
    save_store(&store);
    HttpResponse::empty_ok()
}

fn tag_delete(request: Request) -> HttpResponse {
    if request_user(&request).is_none() {
        return HttpResponse::json(401, json!({"error": "Unauthorized"}));
    }
    let names = json_body(&request);
    let names: Vec<String> = names
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mut store = STORE.lock().unwrap();
    store.personal_tags.retain(|t| {
        !names
            .iter()
            .any(|n| t.get("name").and_then(Value::as_str) == Some(n))
    });
    for peer in &mut store.personal_peers {
        remove_peer_tags(peer, &names);
    }
    save_store(&store);
    HttpResponse::empty_ok()
}

fn paged(data: Vec<Value>) -> HttpResponse {
    HttpResponse::json(200, json!({"total": data.len(), "data": data}))
}

fn personal_peers() -> Vec<Value> {
    STORE.lock().unwrap().personal_peers.clone()
}

fn personal_tags() -> Vec<Value> {
    STORE.lock().unwrap().personal_tags.clone()
}

fn request_user(request: &Request) -> Option<String> {
    let token = request
        .headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)?;
    STORE.lock().unwrap().tokens.get(token).cloned()
}

fn user_payload(username: &str, user: Option<&StoredUser>) -> Value {
    json!({
        "name": username,
        "display_name": user.map(|u| u.display_name.as_str()).unwrap_or(username),
        "avatar": "",
        "email": "",
        "note": "",
        "status": 1,
        "is_admin": true,
        "verifier": "",
    })
}

fn json_body(request: &Request) -> Value {
    serde_json::from_slice(&request.body).unwrap_or_else(|_| json!({}))
}

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    #[allow(deprecated)]
    base64::encode(hasher.finalize())
}

fn new_token(username: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(username.as_bytes());
    hasher.update(now_millis().to_string().as_bytes());
    hasher.update(format!("{:?}", std::thread::current().id()).as_bytes());
    #[allow(deprecated)]
    base64::encode(hasher.finalize())
        .chars()
        .filter(|c| !matches!(c, '/' | '+' | '='))
        .collect()
}

fn merge_object(target: &mut Value, patch: &Value) {
    if let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn rename_peer_tag(peer: &mut Value, old: &str, new: &str) {
    if let Some(tags) = peer.get_mut("tags").and_then(Value::as_array_mut) {
        for tag in tags {
            if tag.as_str() == Some(old) {
                *tag = json!(new);
            }
        }
    }
}

fn remove_peer_tags(peer: &mut Value, names: &[String]) {
    if let Some(tags) = peer.get_mut("tags").and_then(Value::as_array_mut) {
        tags.retain(|tag| !names.iter().any(|name| tag.as_str() == Some(name)));
    }
}

fn tag_color(name: &str) -> i32 {
    let mut hash = 0_i32;
    for b in name.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(b as i32);
    }
    (0xff000000_u32.wrapping_add((hash as u32) & 0x00ff_ffff)) as i32
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn store_path() -> PathBuf {
    data_root().join("managed-server").join("account-api.json")
}

fn load_store() -> ApiStore {
    fs::read_to_string(store_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_store(store: &ApiStore) {
    let path = store_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    if let Ok(data) = serde_json::to_string_pretty(store) {
        fs::write(path, data).ok();
    }
}

fn data_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(v) = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")) {
            return PathBuf::from(v).join("BeyondRemote");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("BeyondRemote");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(v) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(v).join("beyondremote");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("beyondremote");
        }
    }
    std::env::temp_dir().join("beyondremote")
}

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

impl HttpResponse {
    fn json(status: u16, value: Value) -> Self {
        let body = if status == 204 {
            Vec::new()
        } else {
            serde_json::to_vec(&value).unwrap_or_default()
        };
        Self { status, body }
    }

    fn empty_ok() -> Self {
        Self {
            status: 200,
            body: Vec::new(),
        }
    }
}

fn write_response(stream: &mut TcpStream, response: HttpResponse) -> ResultType<()> {
    let reason = match response.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Error",
    };
    let headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Authorization, Content-Type\r\nAccess-Control-Allow-Methods: GET, POST, PUT, DELETE, OPTIONS\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.body.len()
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(&response.body)?;
    Ok(())
}
