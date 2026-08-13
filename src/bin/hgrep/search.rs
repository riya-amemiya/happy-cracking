use memchr::{memchr, memchr_iter, memrchr};
use rayon::prelude::*;

use crate::cli::Cli;
use crate::matcher::Matcher;

fn line_bounds(buf: &[u8], start: usize, end: usize) -> (usize, usize) {
    (
        memrchr(b'\n', &buf[..start]).map_or(0, |i| i + 1),
        memchr(b'\n', &buf[end..]).map_or(buf.len(), |i| end + i),
    )
}

pub(crate) struct Job<'a> {
    pub(crate) matcher: &'a Matcher,
    pub(crate) cli: &'a Cli,
    pub(crate) show_name: bool,
    pub(crate) emit_lines: bool,
}

fn write_prefix(out: &mut Vec<u8>, job: &Job, name: &[u8], line_no: u64) {
    if job.show_name {
        out.extend_from_slice(name);
        out.push(b':');
    }
    if job.cli.line_number {
        out.extend_from_slice(line_no.to_string().as_bytes());
        out.push(b':');
    }
}

fn search_forward(buf: &[u8], job: &Job, name: &[u8], base_line: u64, out: &mut Vec<u8>) -> u64 {
    let limit = job.cli.max_count.unwrap_or(u64::MAX);
    if limit == 0 {
        return 0;
    }
    let trailing = buf.last().is_none_or(|&b| b == b'\n');
    let mut count = 0u64;
    let mut pos = 0usize;
    let mut line_no = base_line + 1;
    let mut counted = 0usize;

    while pos <= buf.len() {
        let Some((s, e)) = job.matcher.find_at(buf, pos) else {
            break;
        };
        if s == buf.len() && trailing {
            break;
        }
        let (ls, le) = line_bounds(buf, s, e);
        count += 1;

        if job.emit_lines {
            if job.cli.line_number {
                line_no += memchr_iter(b'\n', &buf[counted..ls]).count() as u64;
                counted = ls;
            }
            if job.cli.only_matching {
                let mut p = ls;
                while let Some((ms, me)) = job.matcher.find_at(&buf[..le], p) {
                    write_prefix(out, job, name, line_no);
                    out.extend_from_slice(&buf[ms..me]);
                    out.push(b'\n');
                    p = if me > ms { me } else { ms + 1 };
                    if p > le {
                        break;
                    }
                }
            } else {
                write_prefix(out, job, name, line_no);
                out.extend_from_slice(&buf[ls..le]);
                out.push(b'\n');
            }
        }

        if count >= limit || le >= buf.len() {
            break;
        }
        pos = le + 1;
    }
    count
}

fn search_inverted(buf: &[u8], job: &Job, name: &[u8], base_line: u64, out: &mut Vec<u8>) -> u64 {
    let limit = job.cli.max_count.unwrap_or(u64::MAX);
    if limit == 0 {
        return 0;
    }
    let mut count = 0u64;
    let mut line_no = base_line;
    let mut start = 0usize;

    while start <= buf.len() {
        let end = memchr(b'\n', &buf[start..]).map_or(buf.len(), |i| start + i);
        if start == buf.len() && buf.last().is_none_or(|&b| b == b'\n') {
            break;
        }
        line_no += 1;
        if !job.matcher.is_match(&buf[start..end]) {
            count += 1;
            if job.emit_lines {
                write_prefix(out, job, name, line_no);
                out.extend_from_slice(&buf[start..end]);
                out.push(b'\n');
            }
            if count >= limit {
                break;
            }
        }
        if end >= buf.len() {
            break;
        }
        start = end + 1;
    }
    count
}

fn is_binary(buf: &[u8]) -> bool {
    memchr(b'\0', &buf[..buf.len().min(8192)]).is_some()
}

const PARALLEL_THRESHOLD: usize = 1 << 20;

fn split_chunks(buf: &[u8], parts: usize) -> Vec<(usize, usize)> {
    let target = buf.len() / parts + 1;
    let mut chunks = Vec::with_capacity(parts + 1);
    let mut start = 0usize;
    while start < buf.len() {
        let want = (start + target).min(buf.len());
        let end = match memchr(b'\n', &buf[want..]) {
            Some(i) if want + i + 1 < buf.len() => want + i + 1,
            _ => buf.len(),
        };
        chunks.push((start, end));
        start = end;
    }
    chunks
}

fn search_slice(buf: &[u8], job: &Job, name: &[u8], base_line: u64, out: &mut Vec<u8>) -> u64 {
    if job.cli.invert {
        search_inverted(buf, job, name, base_line, out)
    } else {
        search_forward(buf, job, name, base_line, out)
    }
}

fn search_split(buf: &[u8], job: &Job, name: &[u8], out: &mut Vec<u8>) -> u64 {
    let chunks = split_chunks(buf, rayon::current_num_threads() * 4);
    let bases = if job.cli.line_number {
        chunks
            .par_iter()
            .map(|&(s, e)| memchr_iter(b'\n', &buf[s..e]).count() as u64)
            .collect::<Vec<_>>()
            .iter()
            .scan(0u64, |acc, n| {
                let base = *acc;
                *acc += n;
                Some(base)
            })
            .collect()
    } else {
        vec![0u64; chunks.len()]
    };

    let parts: Vec<(u64, Vec<u8>)> = chunks
        .par_iter()
        .zip(bases)
        .map(|(&(s, e), base)| {
            let mut body = Vec::new();
            let count = search_slice(&buf[s..e], job, name, base, &mut body);
            (count, body)
        })
        .collect();

    parts.iter().fold(0, |total, (count, body)| {
        out.extend_from_slice(body);
        total + count
    })
}

pub(crate) fn search_buf(buf: &[u8], job: &Job, name: &[u8], out: &mut Vec<u8>) -> u64 {
    let before = out.len();
    let count = if buf.len() >= PARALLEL_THRESHOLD && job.cli.max_count.is_none() {
        search_split(buf, job, name, out)
    } else {
        search_slice(buf, job, name, 0, out)
    };
    if count > 0 && !job.cli.text && is_binary(buf) {
        out.truncate(before);
        if job.emit_lines {
            out.extend_from_slice(b"Binary file ");
            out.extend_from_slice(name);
            out.extend_from_slice(b" matches\n");
        }
    }
    count
}

pub(crate) fn selected(cli: &Cli, count: u64) -> bool {
    if cli.files_without_match && !cli.files_with_matches {
        count == 0
    } else {
        count > 0
    }
}

pub(crate) fn report(job: &Job, name: &[u8], count: u64, body: Vec<u8>, out: &mut Vec<u8>) {
    let cli = job.cli;
    if cli.quiet {
        return;
    }
    if cli.files_with_matches {
        if count > 0 {
            out.extend_from_slice(name);
            out.push(b'\n');
        }
    } else if cli.files_without_match {
        if count == 0 {
            out.extend_from_slice(name);
            out.push(b'\n');
        }
    } else if cli.count {
        if job.show_name {
            out.extend_from_slice(name);
            out.push(b':');
        }
        out.extend_from_slice(count.to_string().as_bytes());
        out.push(b'\n');
    } else {
        out.extend_from_slice(&body);
    }
}
