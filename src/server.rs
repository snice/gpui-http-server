//! A small multi-threaded static file server built on `tiny_http`.

use std::fs::{self, File};
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context as _, Result};
use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

/// A running server. Dropping it stops the listener and frees the port.
pub struct ServerHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    pub fn start(root: PathBuf, port: u16) -> Result<Self> {
        let root = root
            .canonicalize()
            .with_context(|| format!("文件夹不可用：{}", root.display()))?;
        let server = Server::http(("0.0.0.0", port))
            .map_err(|e| anyhow::anyhow!("无法监听端口 {port}：{e}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match server.recv_timeout(Duration::from_millis(150)) {
                    Ok(Some(request)) => {
                        let root = root.clone();
                        thread::spawn(move || handle(request, &root));
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        });
        Ok(Self { stop, thread: Some(thread) })
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Returns true when nothing is listening on `port`.
pub fn port_is_free(port: u16) -> bool {
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok()
}

/// First free port at or above `start`, skipping any in `taken`.
pub fn find_free_port(start: u16, taken: &[u16]) -> u16 {
    (start..=u16::MAX)
        .find(|p| !taken.contains(p) && port_is_free(*p))
        .unwrap_or(start)
}

/// The address phones on the same network can reach this machine at.
pub fn lan_ip() -> IpAddr {
    local_ip_address::local_ip().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

fn handle(request: Request, root: &Path) {
    let head = *request.method() == Method::Head;
    if !matches!(request.method(), Method::Get | Method::Head) {
        let _ = request.respond(Response::empty(StatusCode(405)));
        return;
    }
    let url = request.url().to_string();
    let raw_path = url.split(['?', '#']).next().unwrap_or("/");
    let decoded = percent_decode_str(raw_path).decode_utf8_lossy().into_owned();

    // Resolve inside the root only; reject any parent-dir component.
    let rel = Path::new(decoded.trim_start_matches('/'));
    if rel.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_))) {
        let _ = request.respond(error_page(403, "403 禁止访问"));
        return;
    }
    let target = root.join(rel);
    let Ok(target) = target.canonicalize() else {
        let _ = request.respond(error_page(404, "404 未找到"));
        return;
    };
    if !target.starts_with(root) {
        let _ = request.respond(error_page(403, "403 禁止访问"));
        return;
    }

    if target.is_dir() {
        if !decoded.ends_with('/') {
            let location = format!("{raw_path}/");
            let response = Response::empty(StatusCode(301))
                .with_header(header("Location", &location));
            let _ = request.respond(response);
            return;
        }
        for index in ["index.html", "index.htm"] {
            let candidate = target.join(index);
            if candidate.is_file() {
                serve_file(request, &candidate, head);
                return;
            }
        }
        let html = directory_listing(&target, &decoded);
        let response = Response::from_string(html)
            .with_header(header("Content-Type", "text/html; charset=utf-8"))
            .with_header(header("Cache-Control", "no-cache"));
        let _ = request.respond(response);
    } else {
        serve_file(request, &target, head);
    }
}

fn serve_file(request: Request, path: &Path, head: bool) {
    let Ok(file) = File::open(path) else {
        let _ = request.respond(error_page(404, "404 未找到"));
        return;
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut content_type = mime.to_string();
    if mime.type_() == "text" || mime.subtype() == "javascript" || mime.subtype() == "json" {
        content_type.push_str("; charset=utf-8");
    }
    let len = file.metadata().map(|m| m.len() as usize).ok();
    let body: Box<dyn std::io::Read + Send> =
        if head { Box::new(std::io::empty()) } else { Box::new(file) };
    let response = Response::new(StatusCode(200), vec![], body, if head { Some(0) } else { len }, None)
    .with_header(header("Content-Type", &content_type))
    .with_header(header("Cache-Control", "no-cache"));
    let _ = request.respond(response);
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn error_page(code: u16, message: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let body = format!(
        "<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'>\
         <body style='font:16px system-ui;display:grid;place-items:center;height:100vh;margin:0;color:#666'>\
         <div>{message}</div>"
    );
    Response::from_string(body)
        .with_status_code(StatusCode(code))
        .with_header(header("Content-Type", "text/html; charset=utf-8"))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn directory_listing(dir: &Path, url_path: &str) -> String {
    let mut entries: Vec<(String, bool)> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    (!name.starts_with('.')).then(|| (name, e.path().is_dir()))
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));

    let mut rows = String::new();
    if url_path != "/" {
        rows.push_str("<li><a href=\"../\">⬆︎ ..</a></li>");
    }
    for (name, is_dir) in entries {
        let href = utf8_percent_encode(&name, NON_ALPHANUMERIC);
        let slash = if is_dir { "/" } else { "" };
        let icon = if is_dir { "📁" } else { "📄" };
        rows.push_str(&format!(
            "<li><a href=\"{href}{slash}\">{icon} {}{slash}</a></li>",
            escape(&name)
        ));
    }
    format!(
        "<!doctype html><meta charset=utf-8>\
         <meta name=viewport content='width=device-width,initial-scale=1'>\
         <title>{title}</title>\
         <style>body{{font:15px/1.6 system-ui;max-width:720px;margin:0 auto;padding:24px 20px;color:#1f2328}}\
         h1{{font-size:18px;font-weight:600;word-break:break-all}}ul{{list-style:none;padding:0}}\
         li{{border-bottom:1px solid #eee}}a{{display:block;padding:10px 4px;color:inherit;text-decoration:none}}\
         a:hover{{background:#f6f8fa}}@media(prefers-color-scheme:dark){{body{{background:#161618;color:#e6e6e6}}\
         li{{border-color:#2a2a2d}}a:hover{{background:#222225}}}}</style>\
         <h1>{title}</h1><ul>{rows}</ul>",
        title = escape(url_path)
    )
}
