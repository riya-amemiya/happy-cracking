use super::{LoadReport, Prepared, build_request, new_histogram};
use anyhow::{Context, Result};
use hdrhistogram::Histogram;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, Error as TlsError, SignatureScheme,
};
use socket2::SockRef;
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use url::Url;

const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const MAX_CONSECUTIVE_ERRORS: u64 = 32;

pub(crate) fn execute(prep: &Prepared) -> Result<LoadReport> {
    let addr = resolve(prep)?;
    let tls = if prep.url.scheme() == "https" {
        Some(tls_setup(prep)?)
    } else {
        None
    };
    let quotas = split_quota(prep.requests, prep.connections);
    let started = Instant::now();
    let deadline = prep.duration.map(|d| started + d);
    let pacer = prep.qps.map(Pacer::new);
    let pacer_ref = pacer.as_ref();
    let mut parts = Vec::with_capacity(prep.connections);
    std::thread::scope(|scope| {
        let mut joins = Vec::with_capacity(prep.connections);
        for quota in quotas {
            let tls = tls.clone();
            joins.push(
                scope.spawn(move || worker(prep, addr, tls.as_ref(), quota, deadline, pacer_ref)),
            );
        }
        for join in joins {
            parts.push(join.join().unwrap_or_else(|_| Partial::aborted()));
        }
    });
    let elapsed = started.elapsed();
    Ok(merge(prep, parts, elapsed))
}

fn worker(
    prep: &Prepared,
    addr: SocketAddr,
    tls: Option<&TlsSetup>,
    quota: Option<u64>,
    deadline: Option<Instant>,
    pacer: Option<&Pacer>,
) -> Partial {
    let mut stats = Partial::new();
    let mut buf = Vec::with_capacity(8 * 1024);
    let mut left = quota.unwrap_or(u64::MAX);
    let mut consecutive_errors = 0u64;
    let mut plain: Option<TcpStream> = None;
    let mut tls_conn: Option<TlsConn> = None;

    while left > 0 {
        if deadline.is_some_and(|end| Instant::now() >= end) {
            break;
        }
        if let Some(delay) = pacer.and_then(Pacer::reserve) {
            std::thread::sleep(delay);
            if deadline.is_some_and(|end| Instant::now() >= end) {
                break;
            }
        }
        let start = Instant::now();
        let exchange = if let Some(setup) = tls {
            match tls_conn.as_mut() {
                Some(conn) => conn.with_mut(|stream| exchange(stream, prep, &mut buf)),
                None => match TlsConn::connect(addr, setup, prep.timeout) {
                    Ok(mut conn) => {
                        let result = conn.with_mut(|stream| exchange(stream, prep, &mut buf));
                        tls_conn = Some(conn);
                        result
                    }
                    Err(kind) => Err(kind),
                },
            }
        } else {
            match plain.as_mut() {
                Some(stream) => exchange(stream, prep, &mut buf),
                None => match connect_plain(addr, prep.timeout) {
                    Ok(mut stream) => {
                        let result = exchange(&mut stream, prep, &mut buf);
                        plain = Some(stream);
                        result
                    }
                    Err(kind) => Err(kind),
                },
            }
        };
        match exchange {
            Ok(parsed) => {
                let follow = follow_redirect(prep, &parsed);
                if let Some(next) = follow {
                    match replay_redirect(
                        prep,
                        addr,
                        tls,
                        &mut plain,
                        &mut tls_conn,
                        &next,
                        &mut buf,
                    ) {
                        Ok(final_parsed) => {
                            stats.record(
                                final_parsed.status,
                                final_parsed.body_len,
                                start.elapsed(),
                            );
                            consecutive_errors = 0;
                            if final_parsed.close || !prep.keepalive {
                                plain = None;
                                tls_conn = None;
                                buf.clear();
                            }
                        }
                        Err(kind) => {
                            stats.fail(kind);
                            plain = None;
                            tls_conn = None;
                            buf.clear();
                            consecutive_errors += 1;
                        }
                    }
                } else {
                    stats.record(parsed.status, parsed.body_len, start.elapsed());
                    consecutive_errors = 0;
                    if parsed.close || !prep.keepalive {
                        plain = None;
                        tls_conn = None;
                        buf.clear();
                    }
                }
            }
            Err(kind) => {
                stats.fail(kind);
                plain = None;
                tls_conn = None;
                buf.clear();
                consecutive_errors += 1;
            }
        }
        left -= 1;
        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
            stats.fail_many(ErrKind::Connect, left);
            break;
        }
    }
    stats
}

fn replay_redirect(
    prep: &Prepared,
    addr: SocketAddr,
    tls: Option<&TlsSetup>,
    plain: &mut Option<TcpStream>,
    tls_conn: &mut Option<TlsConn>,
    target: &RedirectTarget,
    buf: &mut Vec<u8>,
) -> std::result::Result<Parsed, ErrKind> {
    let mut current = target.clone();
    let mut hops = 0usize;
    loop {
        let request = build_request(
            &current.method,
            &current.url,
            &prep.headers,
            &current.body,
            prep.keepalive,
        )
        .map_err(|_| ErrKind::Parse)?;
        let same_origin = same_socket(prep, &current.url);
        if !same_origin {
            *plain = None;
            *tls_conn = None;
        }
        let head = current.method == "HEAD";
        let parsed = if current.url.scheme() == "https" {
            let setup = tls.ok_or(ErrKind::Tls)?;
            if tls_conn.is_none() {
                *tls_conn = Some(TlsConn::connect(addr, setup, prep.timeout)?);
            }
            let conn = tls_conn.as_mut().ok_or(ErrKind::Tls)?;
            conn.with_mut(|stream| exchange_request(stream, &request, head, buf))?
        } else {
            if plain.is_none() {
                *plain = Some(connect_plain(addr, prep.timeout)?);
            }
            let stream = plain.as_mut().ok_or(ErrKind::Connect)?;
            exchange_request(stream, &request, head, buf)?
        };
        hops += 1;
        if hops >= prep.redirects {
            return Ok(parsed);
        }
        if let Some(next) = next_redirect(
            &current.method,
            &current.url,
            &current.body,
            prep.redirects - hops,
            &parsed,
        ) {
            current = next;
            continue;
        }
        return Ok(parsed);
    }
}

fn follow_redirect(prep: &Prepared, parsed: &Parsed) -> Option<RedirectTarget> {
    if prep.redirects == 0 {
        return None;
    }
    next_redirect(&prep.method, &prep.url, &prep.body, prep.redirects, parsed)
}

fn next_redirect(
    method: &str,
    current: &Url,
    prep_body: &[u8],
    remaining: usize,
    parsed: &Parsed,
) -> Option<RedirectTarget> {
    if remaining == 0 || parsed.location.is_empty() {
        return None;
    }
    if !matches!(parsed.status, 301 | 302 | 303 | 307 | 308) {
        return None;
    }
    let next = current.join(&parsed.location).ok()?;
    if next.scheme() != "http" && next.scheme() != "https" {
        return None;
    }
    let (method, body) = if matches!(parsed.status, 301..=303) && method != "HEAD" {
        ("GET".to_string(), Vec::new())
    } else {
        (method.to_string(), prep_body.to_vec())
    };
    Some(RedirectTarget {
        method,
        url: next,
        body,
    })
}

fn same_socket(prep: &Prepared, url: &Url) -> bool {
    url.scheme() == prep.url.scheme()
        && url.host_str() == prep.url.host_str()
        && url.port_or_known_default() == prep.url.port_or_known_default()
}

#[derive(Clone)]
struct RedirectTarget {
    method: String,
    url: Url,
    body: Vec<u8>,
}

fn exchange<S: Read + Write>(
    sock: &mut S,
    prep: &Prepared,
    buf: &mut Vec<u8>,
) -> std::result::Result<Parsed, ErrKind> {
    exchange_request(sock, &prep.request, prep.method_head, buf)
}

fn exchange_request<S: Read + Write>(
    sock: &mut S,
    request: &[u8],
    method_head: bool,
    buf: &mut Vec<u8>,
) -> std::result::Result<Parsed, ErrKind> {
    write_all(sock, request)?;
    sock.flush().map_err(map_io)?;
    read_response(sock, buf, method_head)
}

fn write_all<S: Write>(sock: &mut S, mut data: &[u8]) -> std::result::Result<(), ErrKind> {
    while !data.is_empty() {
        match sock.write(data) {
            Ok(0) => return Err(ErrKind::Write),
            Ok(n) => data = &data[n..],
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => return Err(map_io(err)),
        }
    }
    Ok(())
}

fn read_response<S: Read>(
    sock: &mut S,
    buf: &mut Vec<u8>,
    method_head: bool,
) -> std::result::Result<Parsed, ErrKind> {
    let mut tmp = [0u8; 8192];
    loop {
        if let Some(parsed) = try_parse(buf, method_head)? {
            let consumed = parsed.consumed;
            if consumed < buf.len() {
                buf.drain(..consumed);
            } else {
                buf.clear();
            }
            return Ok(parsed);
        }
        if buf.len() >= MAX_RESPONSE_BYTES {
            return Err(ErrKind::Parse);
        }
        let n = match sock.read(&mut tmp) {
            Ok(0) => return Err(ErrKind::Read),
            Ok(n) => n,
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(err) => return Err(map_io(err)),
        };
        if buf.len() + n > MAX_RESPONSE_BYTES {
            return Err(ErrKind::Parse);
        }
        buf.extend_from_slice(&tmp[..n]);
    }
}

struct Parsed {
    status: u16,
    consumed: usize,
    body_len: u64,
    close: bool,
    location: String,
}

fn try_parse(buf: &[u8], method_head: bool) -> std::result::Result<Option<Parsed>, ErrKind> {
    if buf.len() > MAX_HEADER_BYTES && !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        return Err(ErrKind::Parse);
    }
    let mut headers = [httparse::EMPTY_HEADER; 96];
    let mut resp = httparse::Response::new(&mut headers);
    let parsed = match resp.parse(buf) {
        Ok(httparse::Status::Complete(n)) => n,
        Ok(httparse::Status::Partial) => return Ok(None),
        Err(_) => return Err(ErrKind::Parse),
    };
    let status = resp.code.unwrap_or(0);
    let version = resp.version.unwrap_or(1);
    let mut content_length = None;
    let mut chunked = false;
    let mut close = version == 0;
    let mut location = String::new();
    for header in resp.headers {
        if header.name.eq_ignore_ascii_case("content-length") {
            let text = std::str::from_utf8(header.value).map_err(|_| ErrKind::Parse)?;
            let len = text.trim().parse::<u64>().map_err(|_| ErrKind::Parse)?;
            if content_length.is_some() {
                return Err(ErrKind::Parse);
            }
            content_length = Some(len);
        } else if header.name.eq_ignore_ascii_case("transfer-encoding")
            && header
                .value
                .windows(7)
                .any(|w| w.eq_ignore_ascii_case(b"chunked"))
        {
            chunked = true;
        } else if header.name.eq_ignore_ascii_case("connection") {
            if header
                .value
                .windows(5)
                .any(|w| w.eq_ignore_ascii_case(b"close"))
            {
                close = true;
            }
        } else if header.name.eq_ignore_ascii_case("location") {
            location = String::from_utf8_lossy(header.value).trim().to_string();
        }
    }
    let no_body = method_head || matches!(status, 100..=199 | 204 | 304);
    if no_body {
        return Ok(Some(Parsed {
            status,
            consumed: parsed,
            body_len: 0,
            close,
            location,
        }));
    }
    if chunked {
        let Some(end) = parse_chunked(buf, parsed)? else {
            return Ok(None);
        };
        return Ok(Some(Parsed {
            status,
            consumed: end,
            body_len: end.saturating_sub(parsed) as u64,
            close,
            location,
        }));
    }
    let len = usize::try_from(content_length.unwrap_or(0)).unwrap_or(usize::MAX);
    if len > MAX_RESPONSE_BYTES {
        return Err(ErrKind::Parse);
    }
    let total = parsed + len;
    if buf.len() < total {
        return Ok(None);
    }
    Ok(Some(Parsed {
        status,
        consumed: total,
        body_len: len as u64,
        close,
        location,
    }))
}

fn parse_chunked(buf: &[u8], mut index: usize) -> std::result::Result<Option<usize>, ErrKind> {
    for _ in 0..4096 {
        let Some(line_end) = find_crlf(buf, index) else {
            return Ok(None);
        };
        let hex = &buf[index..line_end];
        let hex = hex.split(|&b| b == b';').next().unwrap_or(hex);
        let size = parse_hex(hex)?;
        let data_start = line_end + 2;
        if size == 0 {
            let Some(trailer_end) = find_header_end(buf, data_start) else {
                return Ok(None);
            };
            return Ok(Some(trailer_end));
        }
        if size > MAX_RESPONSE_BYTES {
            return Err(ErrKind::Parse);
        }
        let after = data_start + size + 2;
        if buf.len() < after {
            return Ok(None);
        }
        if &buf[after - 2..after] != b"\r\n" {
            return Err(ErrKind::Parse);
        }
        index = after;
        if index > MAX_RESPONSE_BYTES {
            return Err(ErrKind::Parse);
        }
    }
    Err(ErrKind::Parse)
}

fn find_crlf(buf: &[u8], start: usize) -> Option<usize> {
    buf[start..]
        .windows(2)
        .position(|w| w == b"\r\n")
        .map(|pos| start + pos)
}

fn find_header_end(buf: &[u8], start: usize) -> Option<usize> {
    buf[start..]
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|pos| start + pos + 4)
}

fn parse_hex(bytes: &[u8]) -> std::result::Result<usize, ErrKind> {
    if bytes.is_empty() {
        return Err(ErrKind::Parse);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| ErrKind::Parse)?;
    usize::from_str_radix(text.trim(), 16).map_err(|_| ErrKind::Parse)
}

fn connect_plain(addr: SocketAddr, timeout: Duration) -> std::result::Result<TcpStream, ErrKind> {
    let stream = TcpStream::connect_timeout(&addr, timeout).map_err(map_io)?;
    tune(&stream);
    stream.set_read_timeout(Some(timeout)).map_err(map_io)?;
    stream.set_write_timeout(Some(timeout)).map_err(map_io)?;
    Ok(stream)
}

fn tune(stream: &TcpStream) {
    let _ = stream.set_nodelay(true);
    let sock = SockRef::from(stream);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let _ = sock.set_tcp_quickack(true);
    let _ = sock;
}

fn resolve(prep: &Prepared) -> Result<SocketAddr> {
    let host = prep.url.host_str().context("URL is missing a host")?;
    let port = prep
        .url
        .port_or_known_default()
        .context("URL is missing a port")?;
    if let Some(HostMatch::Ip(ip)) = ip_host(&prep.url) {
        return Ok(SocketAddr::new(ip, port));
    }
    let mut addrs = (host, port)
        .to_socket_addrs()
        .with_context(|| format!("failed to resolve {host}"))?;
    addrs.next().context("DNS returned no addresses")
}

enum HostMatch {
    Ip(IpAddr),
}

fn ip_host(url: &Url) -> Option<HostMatch> {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => Some(HostMatch::Ip(IpAddr::V4(ip))),
        Some(url::Host::Ipv6(ip)) => Some(HostMatch::Ip(IpAddr::V6(ip))),
        _ => None,
    }
}

#[derive(Clone)]
struct TlsSetup {
    config: Arc<ClientConfig>,
    name: ServerName<'static>,
}

fn tls_setup(prep: &Prepared) -> Result<TlsSetup> {
    ensure_provider();
    let host = prep.url.host_str().context("URL is missing a host")?;
    let name = match prep.url.host() {
        Some(url::Host::Ipv4(ip)) => {
            ServerName::IpAddress(rustls::pki_types::IpAddr::V4(ip.into()))
        }
        Some(url::Host::Ipv6(ip)) => {
            ServerName::IpAddress(rustls::pki_types::IpAddr::V6(ip.into()))
        }
        _ => ServerName::try_from(host.to_string()).context("invalid TLS server name")?,
    };
    let config = if prep.insecure {
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerify))
            .with_no_client_auth()
    } else {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth()
    };
    Ok(TlsSetup {
        config: Arc::new(config),
        name,
    })
}

fn ensure_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

struct TlsConn {
    tcp: TcpStream,
    session: ClientConnection,
}

impl TlsConn {
    fn connect(
        addr: SocketAddr,
        setup: &TlsSetup,
        timeout: Duration,
    ) -> std::result::Result<Self, ErrKind> {
        let tcp = connect_plain(addr, timeout)?;
        let session = ClientConnection::new(setup.config.clone(), setup.name.clone())
            .map_err(|_| ErrKind::Tls)?;
        Ok(Self { tcp, session })
    }

    fn with_mut<R>(
        &mut self,
        f: impl FnOnce(&mut rustls::Stream<'_, ClientConnection, TcpStream>) -> R,
    ) -> R {
        let mut stream = rustls::Stream::new(&mut self.session, &mut self.tcp);
        f(&mut stream)
    }
}

#[derive(Debug)]
struct NoVerify;

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[derive(Clone, Copy, Debug)]
enum ErrKind {
    Timeout,
    Connect,
    Tls,
    Read,
    Write,
    Parse,
    Other,
}

impl ErrKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::Connect => "connect",
            Self::Tls => "tls",
            Self::Read => "read",
            Self::Write => "write",
            Self::Parse => "parse",
            Self::Other => "other",
        }
    }
}

fn map_io(err: std::io::Error) -> ErrKind {
    let kind = err.kind();
    drop(err);
    match kind {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => ErrKind::Timeout,
        ErrorKind::ConnectionRefused
        | ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::NotConnected
        | ErrorKind::AddrNotAvailable => ErrKind::Connect,
        ErrorKind::UnexpectedEof => ErrKind::Read,
        _ => ErrKind::Other,
    }
}

struct Pacer {
    interval_ns: u64,
    next_ns: AtomicU64,
    origin: Instant,
}

impl Pacer {
    fn new(qps: u64) -> Self {
        Self {
            interval_ns: (1_000_000_000 / qps.max(1)).max(1),
            next_ns: AtomicU64::new(0),
            origin: Instant::now(),
        }
    }

    fn reserve(&self) -> Option<Duration> {
        let interval = self.interval_ns;
        let now = u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX);
        loop {
            let prev = self.next_ns.load(Ordering::Relaxed);
            let slot = prev.max(now);
            let next = slot.saturating_add(interval);
            if self
                .next_ns
                .compare_exchange_weak(prev, next, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return slot
                    .checked_sub(now)
                    .filter(|d| *d > 0)
                    .map(Duration::from_nanos);
            }
        }
    }
}

struct Partial {
    success: u64,
    errors: u64,
    bytes: u64,
    sum_ns: u128,
    min_ns: u64,
    max_ns: u64,
    hist: Histogram<u64>,
    status: [u64; 600],
    kinds: [u64; 7],
}

impl Partial {
    fn new() -> Self {
        Self {
            success: 0,
            errors: 0,
            bytes: 0,
            sum_ns: 0,
            min_ns: u64::MAX,
            max_ns: 0,
            hist: new_histogram(),
            status: [0; 600],
            kinds: [0; 7],
        }
    }

    fn aborted() -> Self {
        let mut stats = Self::new();
        stats.fail(ErrKind::Other);
        stats
    }

    fn record(&mut self, status: u16, nbytes: u64, elapsed: Duration) {
        self.success += 1;
        self.bytes += nbytes;
        let ns = u64::try_from(elapsed.as_nanos())
            .unwrap_or(u64::MAX)
            .clamp(1, super::HIST_MAX_NS);
        self.sum_ns += u128::from(ns);
        self.min_ns = self.min_ns.min(ns);
        self.max_ns = self.max_ns.max(ns);
        let _ = self.hist.record(ns);
        if (status as usize) < self.status.len() {
            self.status[status as usize] += 1;
        }
    }

    fn fail(&mut self, kind: ErrKind) {
        self.errors += 1;
        self.kinds[kind as usize] += 1;
    }

    fn fail_many(&mut self, kind: ErrKind, count: u64) {
        self.errors += count;
        self.kinds[kind as usize] += count;
    }
}

fn split_quota(requests: Option<u64>, connections: usize) -> Vec<Option<u64>> {
    match requests {
        None => vec![None; connections],
        Some(total) => {
            let connections = connections.max(1) as u64;
            let base = total / connections;
            let extra = total % connections;
            (0..connections)
                .map(|i| Some(base + u64::from(i < extra)))
                .filter(|n| *n != Some(0))
                .collect()
        }
    }
}

fn merge(prep: &Prepared, parts: Vec<Partial>, elapsed: Duration) -> LoadReport {
    let mut hist = new_histogram();
    let mut success = 0u64;
    let mut errors = 0u64;
    let mut bytes = 0u64;
    let mut sum_ns = 0u128;
    let mut min_ns = u64::MAX;
    let mut max_ns = 0u64;
    let mut status = [0u64; 600];
    let mut kinds = [0u64; 7];
    for part in parts {
        success += part.success;
        errors += part.errors;
        bytes += part.bytes;
        sum_ns += part.sum_ns;
        if part.success > 0 {
            min_ns = min_ns.min(part.min_ns);
            max_ns = max_ns.max(part.max_ns);
            let _ = hist.add(&part.hist);
        }
        for (i, count) in part.status.iter().enumerate() {
            status[i] += count;
        }
        for (i, count) in part.kinds.iter().enumerate() {
            kinds[i] += count;
        }
    }
    let mean_ns = if success == 0 {
        0.0
    } else {
        sum_ns as f64 / success as f64
    };
    let stdev_ns = if success == 0 { 0.0 } else { hist.stdev() };
    let quantile = |q| {
        if success == 0 {
            0
        } else {
            hist.value_at_quantile(q)
        }
    };
    if success == 0 {
        min_ns = 0;
    }
    let mut status_codes = BTreeMap::new();
    for (code, count) in status.iter().enumerate() {
        if *count > 0 {
            status_codes.insert(code as u16, *count);
        }
    }
    let mut error_kinds = BTreeMap::new();
    for (idx, count) in kinds.iter().enumerate() {
        if *count > 0 {
            let kind = match idx {
                0 => ErrKind::Timeout,
                1 => ErrKind::Connect,
                2 => ErrKind::Tls,
                3 => ErrKind::Read,
                4 => ErrKind::Write,
                5 => ErrKind::Parse,
                _ => ErrKind::Other,
            };
            error_kinds.insert(kind.as_str().to_string(), *count);
        }
    }
    LoadReport {
        url: prep.url.to_string(),
        method: prep.method.clone(),
        total: success + errors,
        success,
        errors,
        bytes,
        elapsed,
        min_ns,
        max_ns,
        mean_ns,
        stdev_ns,
        p50_ns: quantile(0.50),
        p75_ns: quantile(0.75),
        p90_ns: quantile(0.90),
        p95_ns: quantile(0.95),
        p99_ns: quantile(0.99),
        status_codes,
        error_kinds,
    }
}
