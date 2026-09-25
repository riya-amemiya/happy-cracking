mod engine;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use clap::{Subcommand, ValueEnum};
use hdrhistogram::Histogram;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::Read;
use std::time::Duration;
use url::Url;

const DEFAULT_REQUESTS: u64 = 200;
const DEFAULT_CONNECTIONS: usize = 50;
const MAX_CONNECTIONS: usize = 4096;
const MAX_REQUESTS: u64 = 50_000_000;
const MAX_DURATION: Duration = Duration::from_hours(24);
const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;
const MAX_HEADERS: usize = 64;
const MAX_HEADER_NAME: usize = 256;
const MAX_HEADER_VALUE: usize = 8 * 1024;
const MAX_URL_LEN: usize = 8 * 1024;
const MAX_QPS: u64 = 1_000_000;
const MAX_REDIRECTS: usize = 20;
pub(crate) const HIST_MAX_NS: u64 = 60_000_000_000;

#[derive(Subcommand)]
pub enum HttpLoadAction {
    #[command(about = "Send concurrent HTTP requests and print throughput and latency")]
    Run {
        #[arg(help = "Target URL (http or https)")]
        url: String,
        #[arg(
            short = 'n',
            long,
            help = "Number of requests to send. Defaults to 200 when --duration is omitted"
        )]
        requests: Option<u64>,
        #[arg(
            short = 'c',
            long,
            default_value_t = DEFAULT_CONNECTIONS,
            help = "Concurrent connections"
        )]
        connections: usize,
        #[arg(
            short = 'z',
            long,
            help = "Run for this long (examples: 500ms, 10s, 1m, 1h) instead of a fixed count"
        )]
        duration: Option<String>,
        #[arg(
            short = 'q',
            long,
            help = "Limit the total request rate (queries per second)"
        )]
        qps: Option<u64>,
        #[arg(short = 'm', long, default_value = "GET", help = "HTTP method")]
        method: String,
        #[arg(
            short = 'H',
            long = "header",
            help = "Request header, 'Name: value'. Repeatable"
        )]
        headers: Vec<String>,
        #[arg(short = 'd', long, help = "Request body")]
        data: Option<String>,
        #[arg(long, help = "Read the request body from a file")]
        data_file: Option<String>,
        #[arg(
            short = 't',
            long,
            default_value = "30s",
            help = "Per-request socket timeout"
        )]
        timeout: String,
        #[arg(long, help = "Accept invalid TLS certificates")]
        insecure: bool,
        #[arg(long, help = "Open a new TCP connection for every request")]
        disable_keepalive: bool,
        #[arg(
            short = 'r',
            long,
            default_value_t = 0,
            help = "Maximum redirects to follow for each request"
        )]
        redirects: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text, help = "Summary format")]
        output_format: OutputFormat,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone)]
pub struct LoadRequest {
    pub url: String,
    pub method: String,
    pub requests: Option<u64>,
    pub connections: usize,
    pub duration: Option<Duration>,
    pub qps: Option<u64>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub timeout: Duration,
    pub insecure: bool,
    pub disable_keepalive: bool,
    pub redirects: usize,
}

#[derive(Debug, Clone)]
pub struct LoadReport {
    pub url: String,
    pub method: String,
    pub total: u64,
    pub success: u64,
    pub errors: u64,
    pub bytes: u64,
    pub elapsed: Duration,
    pub min_ns: u64,
    pub max_ns: u64,
    pub mean_ns: f64,
    pub stdev_ns: f64,
    pub p50_ns: u64,
    pub p75_ns: u64,
    pub p90_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub status_codes: BTreeMap<u16, u64>,
    pub error_kinds: BTreeMap<String, u64>,
}

impl LoadReport {
    #[must_use]
    pub fn requests_per_sec(&self) -> f64 {
        let elapsed = self.elapsed.as_secs_f64();
        if elapsed > 0.0 {
            self.total as f64 / elapsed
        } else {
            0.0
        }
    }
}

pub fn run(action: HttpLoadAction) -> Result<()> {
    match action {
        HttpLoadAction::Run {
            url,
            requests,
            connections,
            duration,
            qps,
            method,
            headers,
            data,
            data_file,
            timeout,
            insecure,
            disable_keepalive,
            redirects,
            output_format,
        } => {
            let duration = match duration {
                Some(raw) => Some(parse_duration(&raw)?),
                None => None,
            };
            let timeout = parse_duration(&timeout)?;
            let requests = match (requests, duration) {
                (Some(n), _) => Some(n),
                (None, Some(_)) => None,
                (None, None) => Some(DEFAULT_REQUESTS),
            };
            let body = read_body(data.as_deref(), data_file.as_deref())?;
            let mut parsed_headers = Vec::with_capacity(headers.len());
            for line in &headers {
                parsed_headers.push(parse_header_line(line)?);
            }
            let req = LoadRequest {
                url,
                method,
                requests,
                connections,
                duration,
                qps,
                headers: parsed_headers,
                body,
                timeout,
                insecure,
                disable_keepalive,
                redirects,
            };
            let report = run_load(&req)?;
            match output_format {
                OutputFormat::Text => print!("{}", format_text(&report)),
                OutputFormat::Json => println!("{}", format_json(&report)?),
            }
        }
    }
    Ok(())
}

pub fn run_load(req: &LoadRequest) -> Result<LoadReport> {
    let prepared = prepare(req)?;
    engine::execute(&prepared)
}

#[must_use]
pub fn format_text(report: &LoadReport) -> String {
    let mut out = String::new();
    let elapsed_s = duration_secs(report.elapsed);
    let rps = report.requests_per_sec();
    let bps = if elapsed_s > 0.0 {
        report.bytes as f64 / elapsed_s
    } else {
        0.0
    };
    let _ = writeln!(out, "URL: {}", report.url);
    let _ = writeln!(out, "Method: {}", report.method);
    let _ = writeln!(out, "Requests: {}", report.total);
    let _ = writeln!(out, "Success: {}", report.success);
    let _ = writeln!(out, "Errors: {}", report.errors);
    let _ = writeln!(out, "Duration: {elapsed_s:.3}s");
    let _ = writeln!(out, "Requests/sec: {rps:.2}");
    let _ = writeln!(out, "Transfer: {} bytes", report.bytes);
    let _ = writeln!(out, "Transfer/sec: {bps:.2} bytes");
    out.push('\n');
    out.push_str("Latency:\n");
    let _ = writeln!(out, "  min    {}", fmt_ns(report.min_ns));
    let _ = writeln!(out, "  mean   {}", fmt_ns_f(report.mean_ns));
    let _ = writeln!(out, "  stdev  {}", fmt_ns_f(report.stdev_ns));
    let _ = writeln!(out, "  max    {}", fmt_ns(report.max_ns));
    let _ = writeln!(out, "  p50    {}", fmt_ns(report.p50_ns));
    let _ = writeln!(out, "  p75    {}", fmt_ns(report.p75_ns));
    let _ = writeln!(out, "  p90    {}", fmt_ns(report.p90_ns));
    let _ = writeln!(out, "  p95    {}", fmt_ns(report.p95_ns));
    let _ = writeln!(out, "  p99    {}", fmt_ns(report.p99_ns));
    if !report.status_codes.is_empty() {
        out.push('\n');
        out.push_str("Status codes:\n");
        for (code, count) in &report.status_codes {
            let _ = writeln!(out, "  {code}  {count}");
        }
    }
    if !report.error_kinds.is_empty() {
        out.push('\n');
        out.push_str("Error kinds:\n");
        for (kind, count) in &report.error_kinds {
            let _ = writeln!(out, "  {kind}  {count}");
        }
    }
    out
}

pub fn format_json(report: &LoadReport) -> Result<String> {
    let elapsed_s = duration_secs(report.elapsed);
    let rps = if elapsed_s > 0.0 {
        report.total as f64 / elapsed_s
    } else {
        0.0
    };
    let bps = if elapsed_s > 0.0 {
        report.bytes as f64 / elapsed_s
    } else {
        0.0
    };
    let body = JsonReport {
        url: &report.url,
        method: &report.method,
        total: report.total,
        success: report.success,
        errors: report.errors,
        duration_ms: elapsed_s * 1000.0,
        requests_per_sec: rps,
        bytes: report.bytes,
        bytes_per_sec: bps,
        latency_ms: JsonLatency {
            min: ns_to_ms(report.min_ns),
            mean: report.mean_ns / 1_000_000.0,
            stdev: report.stdev_ns / 1_000_000.0,
            max: ns_to_ms(report.max_ns),
            p50: ns_to_ms(report.p50_ns),
            p75: ns_to_ms(report.p75_ns),
            p90: ns_to_ms(report.p90_ns),
            p95: ns_to_ms(report.p95_ns),
            p99: ns_to_ms(report.p99_ns),
        },
        status_codes: &report.status_codes,
        error_kinds: &report.error_kinds,
    };
    Ok(serde_json::to_string_pretty(&body)?)
}

pub fn parse_duration(input: &str) -> Result<Duration> {
    let s = input.trim();
    if s.is_empty() {
        bail!("duration is empty");
    }
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    if num.is_empty() {
        bail!("duration '{input}' is missing a number");
    }
    let value: u64 = num
        .parse()
        .with_context(|| format!("invalid duration '{input}'"))?;
    let unit = unit.trim().to_ascii_lowercase();
    let dur = match unit.as_str() {
        "" | "s" => Duration::from_secs(value),
        "ms" => Duration::from_millis(value),
        "us" | "µs" => Duration::from_micros(value),
        "m" => Duration::from_secs(value.saturating_mul(60)),
        "h" => Duration::from_secs(value.saturating_mul(60 * 60)),
        _ => bail!("unsupported duration unit in '{input}'"),
    };
    if dur.is_zero() {
        bail!("duration must be greater than zero");
    }
    if dur > MAX_DURATION {
        bail!("duration exceeds 24h");
    }
    Ok(dur)
}

pub fn parse_header_line(line: &str) -> Result<(String, String)> {
    let (name, value) = line
        .split_once(':')
        .context("header must look like 'Name: value'")?;
    let name = name.trim();
    let value = value.trim();
    if name.is_empty()
        || name.len() > MAX_HEADER_NAME
        || value.len() > MAX_HEADER_VALUE
        || name
            .bytes()
            .any(|b| b == b'\r' || b == b'\n' || b == b' ' || b == b':')
        || value.bytes().any(|b| b == b'\r' || b == b'\n')
    {
        bail!("invalid header '{line}'");
    }
    if !name.bytes().all(|b| b.is_ascii() && !b.is_ascii_control()) {
        bail!("invalid header name '{name}'");
    }
    Ok((name.to_string(), value.to_string()))
}

pub(crate) struct Prepared {
    pub url: Url,
    pub method: String,
    pub request: Vec<u8>,
    pub requests: Option<u64>,
    pub connections: usize,
    pub duration: Option<Duration>,
    pub qps: Option<u64>,
    pub timeout: Duration,
    pub insecure: bool,
    pub keepalive: bool,
    pub redirects: usize,
    pub method_head: bool,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

fn prepare(req: &LoadRequest) -> Result<Prepared> {
    if req.url.len() > MAX_URL_LEN {
        bail!("URL exceeds {MAX_URL_LEN} bytes");
    }
    let url = Url::parse(req.url.trim()).context("invalid URL")?;
    match url.scheme() {
        "http" | "https" => {}
        other => bail!("unsupported URL scheme '{other}'"),
    }
    if url.host_str().is_none() {
        bail!("URL is missing a host");
    }
    let method = req.method.trim().to_ascii_uppercase();
    if method.is_empty()
        || !method
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        bail!("invalid HTTP method '{}'", req.method);
    }
    let requests = match req.requests {
        Some(0) => bail!("request count must be greater than zero"),
        Some(n) if n > MAX_REQUESTS => bail!("request count exceeds {MAX_REQUESTS}"),
        other => other,
    };
    if req.requests.is_none() && req.duration.is_none() {
        bail!("set a request count or a duration");
    }
    if req.connections == 0 || req.connections > MAX_CONNECTIONS {
        bail!("connections must be between 1 and {MAX_CONNECTIONS}");
    }
    if req.qps.is_some_and(|qps| qps == 0 || qps > MAX_QPS) {
        bail!("qps must be between 1 and {MAX_QPS}");
    }
    if req.redirects > MAX_REDIRECTS {
        bail!("redirects must be at most {MAX_REDIRECTS}");
    }
    if req.headers.len() > MAX_HEADERS {
        bail!("at most {MAX_HEADERS} headers are allowed");
    }
    if req.body.len() > MAX_BODY_BYTES {
        bail!("request body exceeds {MAX_BODY_BYTES} bytes");
    }
    if req.timeout.is_zero() || req.timeout > MAX_DURATION {
        bail!("timeout is out of range");
    }
    let connections = match requests {
        Some(n) => req
            .connections
            .min(usize::try_from(n).unwrap_or(req.connections)),
        None => req.connections,
    };
    let request = build_request(
        &method,
        &url,
        &req.headers,
        &req.body,
        !req.disable_keepalive,
    )?;
    Ok(Prepared {
        url,
        method: method.clone(),
        request,
        requests,
        connections,
        duration: req.duration,
        qps: req.qps,
        timeout: req.timeout,
        insecure: req.insecure,
        keepalive: !req.disable_keepalive,
        redirects: req.redirects,
        method_head: method == "HEAD",
        headers: req.headers.clone(),
        body: req.body.clone(),
    })
}

pub(crate) fn build_request(
    method: &str,
    url: &Url,
    headers: &[(String, String)],
    body: &[u8],
    keepalive: bool,
) -> Result<Vec<u8>> {
    let mut path = url.path().to_string();
    if path.is_empty() {
        path.push('/');
    }
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let host = host_header(url)?;
    let mut out = Vec::with_capacity(256 + body.len());
    out.extend_from_slice(method.as_bytes());
    out.push(b' ');
    out.extend_from_slice(path.as_bytes());
    out.extend_from_slice(b" HTTP/1.1\r\n");
    let mut has_host = false;
    let mut has_connection = false;
    let mut has_content_length = false;
    let mut has_auth = false;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("host") {
            has_host = true;
        } else if name.eq_ignore_ascii_case("connection") {
            has_connection = true;
        } else if name.eq_ignore_ascii_case("content-length") {
            has_content_length = true;
        } else if name.eq_ignore_ascii_case("authorization") {
            has_auth = true;
        }
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(value.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    if !has_host {
        out.extend_from_slice(b"Host: ");
        out.extend_from_slice(host.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    if !has_auth && let Some(auth) = basic_auth(url) {
        out.extend_from_slice(b"Authorization: Basic ");
        out.extend_from_slice(auth.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    if !has_connection {
        if keepalive {
            out.extend_from_slice(b"Connection: keep-alive\r\n");
        } else {
            out.extend_from_slice(b"Connection: close\r\n");
        }
    }
    if !body.is_empty() && !has_content_length {
        let len = body.len().to_string();
        out.extend_from_slice(b"Content-Length: ");
        out.extend_from_slice(len.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(body);
    Ok(out)
}

fn host_header(url: &Url) -> Result<String> {
    let host = url.host_str().context("URL is missing a host")?;
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    if let Some(port) = url.port() {
        Ok(format!("{host}:{port}"))
    } else {
        Ok(host)
    }
}

fn basic_auth(url: &Url) -> Option<String> {
    if url.username().is_empty() && url.password().is_none() {
        return None;
    }
    let raw = match url.password() {
        Some(password) => format!("{}:{password}", url.username()),
        None => format!("{}:", url.username()),
    };
    Some(base64::engine::general_purpose::STANDARD.encode(raw.as_bytes()))
}

fn read_body(data: Option<&str>, data_file: Option<&str>) -> Result<Vec<u8>> {
    match (data, data_file) {
        (Some(_), Some(_)) => bail!("pass only one of --data and --data-file"),
        (Some(text), None) => {
            if text.len() > MAX_BODY_BYTES {
                bail!("request body exceeds {MAX_BODY_BYTES} bytes");
            }
            Ok(text.as_bytes().to_vec())
        }
        (None, Some(path)) => {
            let file = File::open(path).with_context(|| format!("failed to read {path}"))?;
            let mut buf = Vec::new();
            file.take((MAX_BODY_BYTES as u64) + 1)
                .read_to_end(&mut buf)
                .with_context(|| format!("failed to read {path}"))?;
            if buf.len() > MAX_BODY_BYTES {
                bail!("request body exceeds {MAX_BODY_BYTES} bytes");
            }
            Ok(buf)
        }
        (None, None) => Ok(Vec::new()),
    }
}

pub(crate) fn new_histogram() -> Histogram<u64> {
    Histogram::<u64>::new_with_bounds(1, HIST_MAX_NS, 3).expect("histogram bounds")
}

fn duration_secs(d: Duration) -> f64 {
    d.as_secs_f64()
}

fn ns_to_ms(ns: u64) -> f64 {
    ns as f64 / 1_000_000.0
}

fn fmt_ns(ns: u64) -> String {
    fmt_ns_f(ns as f64)
}

fn fmt_ns_f(ns: f64) -> String {
    if ns >= 1_000_000_000.0 {
        format!("{:.3} s", ns / 1_000_000_000.0)
    } else if ns >= 1_000_000.0 {
        format!("{:.3} ms", ns / 1_000_000.0)
    } else if ns >= 1_000.0 {
        format!("{:.3} us", ns / 1_000.0)
    } else {
        format!("{ns:.0} ns")
    }
}

#[derive(Serialize)]
struct JsonReport<'a> {
    url: &'a str,
    method: &'a str,
    total: u64,
    success: u64,
    errors: u64,
    duration_ms: f64,
    requests_per_sec: f64,
    bytes: u64,
    bytes_per_sec: f64,
    latency_ms: JsonLatency,
    status_codes: &'a BTreeMap<u16, u64>,
    error_kinds: &'a BTreeMap<String, u64>,
}

#[derive(Serialize)]
struct JsonLatency {
    min: f64,
    mean: f64,
    stdev: f64,
    max: f64,
    p50: f64,
    p75: f64,
    p90: f64,
    p95: f64,
    p99: f64,
}
