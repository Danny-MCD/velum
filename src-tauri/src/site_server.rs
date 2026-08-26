//! A tiny local static file server. Each published "static folder" site gets
//! one of these bound to 127.0.0.1 on a random port, which Tor's onion
//! service then points at.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct RunningServer {
    stop_flag: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl RunningServer {
    pub fn stop(mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Serve `root` on `127.0.0.1:port` until `RunningServer::stop` is called.
pub fn serve_folder(root: PathBuf, port: u16) -> anyhow::Result<RunningServer> {
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|e| anyhow::anyhow!("failed to bind local server: {e}"))?;
    let stop_flag = Arc::new(AtomicBool::new(false));
    let flag = stop_flag.clone();

    let handle = std::thread::spawn(move || {
        while !flag.load(Ordering::SeqCst) {
            match server.recv_timeout(Duration::from_millis(200)) {
                Ok(Some(request)) => handle_request(&root, request),
                Ok(None) => continue, // timed out, loop back and check the flag
                Err(_) => break,
            }
        }
    });

    Ok(RunningServer {
        stop_flag,
        handle: Some(handle),
    })
}

fn handle_request(root: &Path, request: tiny_http::Request) {
    let requested = request.url().split(['?', '#']).next().unwrap_or("/");
    let response = match resolve_path(root, requested) {
        Some(path) => match std::fs::read(&path) {
            Ok(bytes) => {
                let mime = guess_mime(&path);
                let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], mime.as_bytes())
                    .expect("valid header");
                tiny_http::Response::from_data(bytes)
                    .with_header(header)
                    .boxed()
            }
            Err(_) => not_found(),
        },
        None => not_found(),
    };
    let _ = request.respond(response);
}

fn not_found() -> tiny_http::ResponseBox {
    tiny_http::Response::from_string("404 Not Found")
        .with_status_code(404)
        .boxed()
}

/// Turns a request path into a real file under `root`, defaulting to
/// `index.html` for directories and refusing to walk outside `root` via `..`.
fn resolve_path(root: &Path, url_path: &str) -> Option<PathBuf> {
    let relative = url_path.trim_start_matches('/');
    let relative = if relative.is_empty() { "index.html" } else { relative };

    let mut resolved = root.to_path_buf();
    for part in Path::new(relative).components() {
        match part {
            Component::Normal(seg) => resolved.push(seg),
            Component::CurDir => {}
            // Anything trying to go up (ParentDir) or reference a root/prefix
            // is rejected outright - no path traversal outside the folder.
            _ => return None,
        }
    }

    if resolved.is_dir() {
        resolved = resolved.join("index.html");
    }

    if resolved.starts_with(root) && resolved.is_file() {
        Some(resolved)
    } else {
        None
    }
}

fn guess_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
