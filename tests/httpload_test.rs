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
