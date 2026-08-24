//! Thread-local stdout batching so walk/search workers don't lock per path.

use std::cell::RefCell;
use std::io::{self, Write};
use std::sync::Mutex;

const FLUSH_AT: usize = 128 * 1024;

thread_local! {
    static BUF: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn flush_vec(sink: &Mutex<io::BufWriter<io::Stdout>>, buf: &mut Vec<u8>) {
    if buf.is_empty() {
        return;
    }
    if let Ok(mut w) = sink.lock() {
        let _ = w.write_all(buf);
    }
    buf.clear();
}

pub fn push(sink: &Mutex<io::BufWriter<io::Stdout>>, bytes: &[u8], term: Option<u8>) {
    BUF.with(|slot| {
        let mut buf = slot.borrow_mut();
        if buf.capacity() < FLUSH_AT {
            buf.reserve(FLUSH_AT);
        }
        buf.extend_from_slice(bytes);
        if let Some(b) = term {
            buf.push(b);
        }
        if buf.len() >= FLUSH_AT {
            flush_vec(sink, &mut buf);
        }
    });
}

pub fn finish(sink: &Mutex<io::BufWriter<io::Stdout>>) {
    BUF.with(|slot| {
        flush_vec(sink, &mut slot.borrow_mut());
    });
    let _ = rayon::broadcast(|_| {
        BUF.with(|slot| {
            flush_vec(sink, &mut slot.borrow_mut());
        });
    });
    if let Ok(mut w) = sink.lock() {
        let _ = w.flush();
    }
}
