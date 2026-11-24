use happy_cracking::crypto::httpload::{self, LoadRequest, OutputFormat};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

fn req(url: String) -> LoadRequest {
    LoadRequest {
        url,
        method: "GET".to_string(),
        requests: Some(20),
        connections: 4,
        duration: None,
        qps: None,
        headers: Vec::new(),
        body: Vec::new(),
        timeout: Duration::from_secs(2),
        insecure: false,
        disable_keepalive: false,
        redirects: 0,
    }
}

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    accepts: Arc<AtomicUsize>,
    join: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn spawn(handler: fn(&[u8]) -> Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let accepts = Arc::new(AtomicUsize::new(0));
        let stop_bg = Arc::clone(&stop);
        let accepts_bg = Arc::clone(&accepts);
        listener.set_nonblocking(true).unwrap();
        let join = thread::spawn(move || {
            while !stop_bg.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        accepts_bg.fetch_add(1, Ordering::Relaxed);
                        stream.set_nonblocking(false).unwrap();
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(200)));
                        thread::spawn(move || {
                            let mut buf = [0u8; 8192];
                            loop {
                                let n = match stream.read(&mut buf) {
                                    Ok(0) | Err(_) => return,
                                    Ok(n) => n,
                                };
                                let resp = handler(&buf[..n]);
                                if stream.write_all(&resp).is_err() {
                                    return;
                                }
                            }
                        });
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url: format!("http://127.0.0.1:{port}/"),
            stop,
            accepts,
            join: Some(join),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn ok_body(_req: &[u8]) -> Vec<u8> {
    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\nok".to_vec()
}

#[test]
fn parses_duration_units() {
    assert_eq!(
        httpload::parse_duration("10s").unwrap(),
        Duration::from_secs(10)
    );
    assert_eq!(
        httpload::parse_duration("500ms").unwrap(),
        Duration::from_millis(500)
    );
    assert_eq!(
        httpload::parse_duration("2m").unwrap(),
        Duration::from_secs(120)
    );
    assert!(httpload::parse_duration("0s").is_err());
    assert!(httpload::parse_duration("3 fortnight").is_err());
}

#[test]
fn rejects_bad_urls_and_limits() {
    let mut bad = req("ftp://example.com/".to_string());
    assert!(httpload::run_load(&bad).is_err());
    bad = req("http://127.0.0.1/".to_string());
    bad.requests = Some(0);
    assert!(httpload::run_load(&bad).is_err());
    bad.requests = Some(10);
    bad.connections = 0;
    assert!(httpload::run_load(&bad).is_err());
}

#[test]
fn completes_keepalive_requests() {
    let server = Server::spawn(ok_body);
    let mut load = req(server.url.clone());
    load.requests = Some(40);
    load.connections = 4;
    let report = httpload::run_load(&load).unwrap();
    assert_eq!(report.success, 40);
    assert_eq!(report.errors, 0);
    assert_eq!(report.status_codes.get(&200), Some(&40));
    assert_eq!(report.bytes, 80);
    assert!(report.requests_per_sec() > 0.0);
    assert!(server.accepts.load(Ordering::Relaxed) <= 4);
}

#[test]
fn sends_method_body_and_header() {
    let server = Server::spawn(|raw| {
        let text = String::from_utf8_lossy(raw);
        if text.starts_with("POST ") && text.contains("X-Test: yes") && text.contains("\r\n\r\nhi")
        {
            b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n".to_vec()
        } else {
            b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n"
                .to_vec()
        }
    });
    let mut load = req(server.url.clone());
    load.method = "POST".to_string();
    load.body = b"hi".to_vec();
    load.headers = vec![("X-Test".to_string(), "yes".to_string())];
    load.requests = Some(5);
    load.connections = 1;
    let report = httpload::run_load(&load).unwrap();
    assert_eq!(report.status_codes.get(&201), Some(&5));
}

#[test]
fn counts_connection_failures() {
    let mut load = req("http://127.0.0.1:1/".to_string());
    load.requests = Some(4);
    load.connections = 2;
    load.timeout = Duration::from_millis(200);
    let report = httpload::run_load(&load).unwrap();
    assert_eq!(report.success, 0);
    assert_eq!(report.errors, 4);
}

#[test]
fn load_report_omits_url_userinfo() {
    let server = Server::spawn(|raw| {
        let text = String::from_utf8_lossy(raw);
        if text.contains("Authorization: Basic ") {
            ok_body(raw)
        } else {
            b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
        }
    });
    let password = "s3cret-for-redact-test";
    let url = server
        .url
        .replacen("http://", &format!("http://loaduser:{password}@"), 1);
    let mut load = req(url);
    load.requests = Some(2);
    load.connections = 1;
    let report = httpload::run_load(&load).unwrap();
    assert_eq!(report.success, 2);
    assert!(!report.url.contains(password));
    assert!(!report.url.contains("loaduser"));
    assert!(report.url.starts_with("http://127.0.0.1:"));
    let text = httpload::format_text(&report);
    let json = httpload::format_json(&report).unwrap();
    assert!(!text.contains(password));
    assert!(!json.contains(password));
}

#[test]
fn json_summary_contains_rate() {
    let server = Server::spawn(ok_body);
    let mut load = req(server.url.clone());
    load.requests = Some(3);
    load.connections = 1;
    let report = httpload::run_load(&load).unwrap();
    let json = httpload::format_json(&report).unwrap();
    assert!(json.contains("\"requests_per_sec\""));
    assert!(json.contains("\"success\": 3"));
    let text = httpload::format_text(&report);
    assert!(text.contains("Status codes:"));
    let _ = OutputFormat::Json;
}

#[test]
fn duration_mode_stops() {
    let server = Server::spawn(ok_body);
    let mut load = req(server.url.clone());
    load.requests = None;
    load.duration = Some(Duration::from_millis(200));
    load.connections = 2;
    let report = httpload::run_load(&load).unwrap();
    assert!(report.success > 0);
    assert!(report.elapsed >= Duration::from_millis(150));
    assert!(report.elapsed < Duration::from_secs(2));
}

fn redirect_cross_origin(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    if text.starts_with("GET / HTTP/") {
        return b"HTTP/1.1 302 Found\r\nLocation: http://example.com/steal\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n".to_vec();
    }
    ok_body(raw)
}

fn redirect_same_origin(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    if text.starts_with("GET / HTTP/") {
        return b"HTTP/1.1 302 Found\r\nLocation: /ok\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n".to_vec();
    }
    if text.starts_with("GET /ok ") {
        return ok_body(raw);
    }
    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n".to_vec()
}

fn redirect_userinfo(raw: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(raw);
    if text.starts_with("GET /ok ") {
        return ok_body(raw);
    }
    if text.starts_with("GET / HTTP/") {
        let host = text
            .lines()
            .find_map(|line| {
                line.strip_prefix("Host: ")
                    .or_else(|| line.strip_prefix("host: "))
            })
            .unwrap_or("127.0.0.1")
            .trim();
        return format!(
            "HTTP/1.1 302 Found\r\nLocation: http://evil:pass@{host}/ok\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n"
        )
        .into_bytes();
    }
    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n".to_vec()
}

fn one_redirect(url: String) -> LoadRequest {
    let mut load = req(url);
    load.requests = Some(1);
    load.connections = 1;
    load.redirects = 5;
    load
}

#[test]
fn follows_same_origin_redirect() {
    let server = Server::spawn(redirect_same_origin);
    let report = httpload::run_load(&one_redirect(server.url.clone())).unwrap();
    assert_eq!(report.success, 1);
    assert_eq!(report.status_codes.get(&200), Some(&1));
}

#[test]
fn does_not_follow_cross_origin_redirect() {
    let server = Server::spawn(redirect_cross_origin);
    let report = httpload::run_load(&one_redirect(server.url.clone())).unwrap();
    assert_eq!(report.success, 1);
    assert_eq!(report.status_codes.get(&302), Some(&1));
    assert!(!report.status_codes.contains_key(&200));
}

#[test]
fn does_not_follow_redirect_with_userinfo() {
    let server = Server::spawn(redirect_userinfo);
    let report = httpload::run_load(&one_redirect(server.url.clone())).unwrap();
    assert_eq!(report.success, 1);
    assert_eq!(report.status_codes.get(&302), Some(&1));
    assert!(!report.status_codes.contains_key(&200));
}
