//! Per-thread io_uring batching of openat + read + close for small files.

use std::cell::RefCell;
use std::ffi::{CString, OsString};
use std::fs::File;
use std::mem::size_of;
use std::os::raw::c_void;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::FromRawFd;
use std::ptr;
use std::sync::atomic::{AtomicU32, Ordering, compiler_fence};

const QD: u32 = 256;
const BATCH: usize = 64;
const CHUNK: usize = 16 * 1024;
const IORING_OFF_SQ_RING: i64 = 0;
const IORING_OFF_CQ_RING: i64 = 0x8000000;
const IORING_OFF_SQES: i64 = 0x10000000;
const IORING_OP_OPENAT: u8 = 18;
const IORING_OP_CLOSE: u8 = 19;
const IORING_OP_READ: u8 = 22;
const IORING_ENTER_GETEVENTS: u32 = 1;
const IORING_FEAT_SINGLE_MMAP: u32 = 1;
const IORING_SETUP_COOP_TASKRUN: u32 = 1 << 8;
const IORING_SETUP_SINGLE_ISSUER: u32 = 1 << 12;
const IORING_SETUP_DEFER_TASKRUN: u32 = 1 << 13;
const OPEN_FLAGS: u32 =
    (libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NOATIME) as u32;
const OPEN_FLAGS_NO_NOATIME: u32 = (libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW) as u32;

#[repr(C)]
#[derive(Clone, Copy)]
struct SqOff {
    head: u32,
    tail: u32,
    mask: u32,
    entries: u32,
    flags: u32,
    dropped: u32,
    array: u32,
    resv1: u32,
    user_addr: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CqOff {
    head: u32,
    tail: u32,
    mask: u32,
    entries: u32,
    overflow: u32,
    cqes: u32,
    flags: u32,
    resv1: u32,
    user_addr: u64,
}

#[repr(C)]
struct Params {
    sq_entries: u32,
    cq_entries: u32,
    flags: u32,
    sq_thread_cpu: u32,
    sq_thread_idle: u32,
    features: u32,
    wq_fd: u32,
    resv: [u32; 3],
    sq_off: SqOff,
    cq_off: CqOff,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Sqe {
    opcode: u8,
    flags: u8,
    ioprio: u16,
    fd: i32,
    off: u64,
    addr: u64,
    len: u32,
    op_flags: u32,
    user_data: u64,
    buf_index: u16,
    personality: u16,
    file_index: u32,
    addr3: u64,
    pad2: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Cqe {
    user_data: u64,
    res: i32,
    flags: u32,
}

struct Ring {
    fd: i32,
    sq_map: *mut u8,
    sq_map_len: usize,
    cq_map: *mut u8,
    cq_map_len: usize,
    sqes: *mut Sqe,
    sqes_len: usize,
    sq_mask: u32,
    cq_mask: u32,
    sq_head: *mut u32,
    sq_tail: *mut u32,
    sq_array: *mut u32,
    cq_head: *mut u32,
    cq_tail: *mut u32,
    cqes: *mut Cqe,
    sq_tail_local: u32,
}

impl Drop for Ring {
    fn drop(&mut self) {
        unsafe {
            if !self.sqes.is_null() {
                libc::munmap(self.sqes.cast(), self.sqes_len);
            }
            if !self.sq_map.is_null() {
                libc::munmap(self.sq_map.cast(), self.sq_map_len);
            }
            if !self.cq_map.is_null() && self.cq_map != self.sq_map {
                libc::munmap(self.cq_map.cast(), self.cq_map_len);
            }
            libc::close(self.fd);
        }
    }
}

thread_local! {
    static RING: RefCell<Option<Option<Ring>>> = const { RefCell::new(None) };
    static BUFS: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
    static OPEN_FD: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    static READ_RES: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
    static NAMES: RefCell<Vec<Option<CString>>> = const { RefCell::new(Vec::new()) };
}

pub enum ReadOut<'a> {
    Bytes(&'a [u8]),
    File(File),
    Miss,
}

fn load_acq(p: *mut u32) -> u32 {
    unsafe { AtomicU32::from_ptr(p).load(Ordering::Acquire) }
}

fn store_rel(p: *mut u32, v: u32) {
    unsafe { AtomicU32::from_ptr(p).store(v, Ordering::Release) }
}

fn setup_with(flags: u32) -> Option<Ring> {
    const {
        assert!(size_of::<Sqe>() == 64);
        assert!(size_of::<Cqe>() == 16);
        assert!(size_of::<Params>() == 120);
    }
    let mut params: Params = unsafe { std::mem::zeroed() };
    params.flags = flags;
    let fd = unsafe { libc::syscall(libc::SYS_io_uring_setup, QD as libc::c_long, &mut params) };
    if fd < 0 {
        return None;
    }
    let fd = fd as i32;
    let sq_entries = params.sq_entries as usize;
    let cq_entries = params.cq_entries as usize;
    let sq_map_len = params.sq_off.array as usize + sq_entries * size_of::<u32>();
    let cq_map_len = params.cq_off.cqes as usize + cq_entries * size_of::<Cqe>();
    let single = params.features & IORING_FEAT_SINGLE_MMAP != 0;
    let sq_len = if single {
        sq_map_len.max(cq_map_len)
    } else {
        sq_map_len
    };
    let sq_map = unsafe {
        libc::mmap(
            ptr::null_mut(),
            sq_len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_POPULATE,
            fd,
            IORING_OFF_SQ_RING,
        )
    };
    if sq_map == libc::MAP_FAILED {
        unsafe { libc::close(fd) };
        return None;
    }
    let (cq_map, cq_len) = if single {
        (sq_map, sq_len)
    } else {
        let p = unsafe {
            libc::mmap(
                ptr::null_mut(),
                cq_map_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED | libc::MAP_POPULATE,
                fd,
                IORING_OFF_CQ_RING,
            )
        };
        if p == libc::MAP_FAILED {
            unsafe {
                libc::munmap(sq_map, sq_len);
                libc::close(fd);
            }
            return None;
        }
        (p, cq_map_len)
    };
    let sqes_len = sq_entries * size_of::<Sqe>();
    let sqes = unsafe {
        libc::mmap(
            ptr::null_mut(),
            sqes_len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_POPULATE,
            fd,
            IORING_OFF_SQES,
        )
    };
    if sqes == libc::MAP_FAILED {
        unsafe {
            if cq_map != sq_map {
                libc::munmap(cq_map, cq_len);
            }
            libc::munmap(sq_map, sq_len);
            libc::close(fd);
        }
        return None;
    }
    let sq_ptr = sq_map.cast::<u8>();
    let cq_ptr = cq_map.cast::<u8>();
    let array = unsafe { sq_ptr.add(params.sq_off.array as usize).cast::<u32>() };
    for i in 0..sq_entries {
        unsafe { *array.add(i) = i as u32 };
    }
    let sq_tail = unsafe { sq_ptr.add(params.sq_off.tail as usize).cast::<u32>() };
    Some(Ring {
        fd,
        sq_map: sq_ptr,
        sq_map_len: sq_len,
        cq_map: cq_ptr,
        cq_map_len: cq_len,
        sqes: sqes.cast(),
        sqes_len,
        sq_mask: unsafe { *sq_ptr.add(params.sq_off.mask as usize).cast::<u32>() },
        cq_mask: unsafe { *cq_ptr.add(params.cq_off.mask as usize).cast::<u32>() },
        sq_head: unsafe { sq_ptr.add(params.sq_off.head as usize).cast() },
        sq_tail,
        sq_array: array,
        cq_head: unsafe { cq_ptr.add(params.cq_off.head as usize).cast() },
        cq_tail: unsafe { cq_ptr.add(params.cq_off.tail as usize).cast() },
        cqes: unsafe { cq_ptr.add(params.cq_off.cqes as usize).cast() },
        sq_tail_local: load_acq(sq_tail),
    })
}

fn setup() -> Option<Ring> {
    setup_with(IORING_SETUP_COOP_TASKRUN | IORING_SETUP_SINGLE_ISSUER | IORING_SETUP_DEFER_TASKRUN)
        .or_else(|| setup_with(0))
}

fn with_ring<T>(f: impl FnOnce(&mut Ring) -> T) -> Option<T> {
    RING.with(|slot| {
        let mut g = slot.borrow_mut();
        if g.is_none() {
            *g = Some(setup());
        }
        g.as_mut().and_then(|inner| inner.as_mut().map(f))
    })
}

fn push(ring: &mut Ring, sqe: Sqe) {
    let tail = ring.sq_tail_local;
    let idx = tail & ring.sq_mask;
    unsafe {
        *ring.sqes.add(idx as usize) = sqe;
        *ring.sq_array.add(idx as usize) = idx;
    }
    ring.sq_tail_local = tail.wrapping_add(1);
}

fn submit_wait(ring: &mut Ring, wait: u32) -> bool {
    compiler_fence(Ordering::Release);
    store_rel(ring.sq_tail, ring.sq_tail_local);
    let pending = ring.sq_tail_local.wrapping_sub(load_acq(ring.sq_head));
    if pending == 0 && wait == 0 {
        return true;
    }
    loop {
        let rc = unsafe {
            libc::syscall(
                libc::SYS_io_uring_enter,
                ring.fd as libc::c_long,
                pending as libc::c_long,
                wait as libc::c_long,
                IORING_ENTER_GETEVENTS as libc::c_long,
                ptr::null_mut::<c_void>(),
                0 as libc::c_long,
            )
        };
        if rc >= 0 {
            return true;
        }
        if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        return false;
    }
}

fn harvest(ring: &mut Ring, wait: u32, out: &mut [i32]) -> bool {
    let mut seen = 0u32;
    while seen < wait {
        let tail = load_acq(ring.cq_tail);
        let mut head = load_acq(ring.cq_head);
        if head == tail {
            if !submit_wait(ring, wait - seen) {
                return false;
            }
            continue;
        }
        while head != tail && seen < wait {
            let cqe = unsafe { *ring.cqes.add((head & ring.cq_mask) as usize) };
            let i = cqe.user_data as usize;
            if i < out.len() {
                out[i] = cqe.res;
            }
            head = head.wrapping_add(1);
            seen += 1;
        }
        store_rel(ring.cq_head, head);
    }
    true
}

fn harvest_count(ring: &mut Ring, wait: u32) -> bool {
    let mut seen = 0u32;
    while seen < wait {
        let tail = load_acq(ring.cq_tail);
        let head = load_acq(ring.cq_head);
        if head == tail {
            if !submit_wait(ring, wait - seen) {
                return false;
            }
            continue;
        }
        let avail = tail.wrapping_sub(head);
        let take = avail.min(wait - seen);
        store_rel(ring.cq_head, head.wrapping_add(take));
        seen += take;
    }
    true
}

fn open_sqe(dirfd: i32, name: &CString, flags: u32, user: u64) -> Sqe {
    Sqe {
        opcode: IORING_OP_OPENAT,
        flags: 0,
        ioprio: 0,
        fd: dirfd,
        off: 0,
        addr: name.as_ptr() as u64,
        len: 0,
        op_flags: flags,
        user_data: user,
        buf_index: 0,
        personality: 0,
        file_index: 0,
        addr3: 0,
        pad2: 0,
    }
}

fn read_sqe(fd: i32, ptr: *mut u8, len: u32, user: u64) -> Sqe {
    Sqe {
        opcode: IORING_OP_READ,
        flags: 0,
        ioprio: 0,
        fd,
        off: 0,
        addr: ptr as u64,
        len,
        op_flags: 0,
        user_data: user,
        buf_index: 0,
        personality: 0,
        file_index: 0,
        addr3: 0,
        pad2: 0,
    }
}

fn close_sqe(fd: i32, user: u64) -> Sqe {
    Sqe {
        opcode: IORING_OP_CLOSE,
        flags: 0,
        ioprio: 0,
        fd,
        off: 0,
        addr: 0,
        len: 0,
        op_flags: 0,
        user_data: user,
        buf_index: 0,
        personality: 0,
        file_index: 0,
        addr3: 0,
        pad2: 0,
    }
}

fn close_leaked(fds: &[i32]) {
    for &fd in fds {
        if fd >= 0 {
            unsafe { libc::close(fd) };
        }
    }
}

/// Read each name relative to `dirfd`. Returns how many names were handled.
/// `0` with a non-empty list means the ring could not be used.
pub fn process(dirfd: i32, names: &[OsString], mut f: impl FnMut(usize, ReadOut<'_>)) -> usize {
    if names.is_empty() {
        return 0;
    }
    if with_ring(|_| ()).is_none() {
        return 0;
    }
    let mut done = 0usize;
    while done < names.len() {
        let n = (names.len() - done).min(BATCH);
        if !batch(dirfd, &names[done..done + n], done, &mut f) {
            return done;
        }
        done += n;
    }
    done
}

fn batch(
    dirfd: i32,
    names: &[OsString],
    base: usize,
    f: &mut impl FnMut(usize, ReadOut<'_>),
) -> bool {
    let n = names.len();
    let prepared = with_ring(|ring| {
        NAMES.with(|names_cell| {
            OPEN_FD.with(|fds_cell| {
                READ_RES.with(|res_cell| {
                    BUFS.with(|bufs_cell| {
                        let cnames = &mut *names_cell.borrow_mut();
                        let fds = &mut *fds_cell.borrow_mut();
                        let reads = &mut *res_cell.borrow_mut();
                        let bufs = &mut *bufs_cell.borrow_mut();
                        cnames.clear();
                        for name in names {
                            cnames.push(CString::new(name.as_bytes()).ok());
                        }
                        fds.clear();
                        fds.resize(n, i32::MIN);
                        reads.clear();
                        reads.resize(n, i32::MIN);
                        if bufs.len() < n {
                            bufs.resize_with(n, Vec::new);
                        }
                        let mut opens = 0u32;
                        for (i, cname) in cnames.iter().enumerate() {
                            let Some(cname) = cname else { continue };
                            push(ring, open_sqe(dirfd, cname, OPEN_FLAGS, i as u64));
                            opens += 1;
                        }
                        if opens > 0 && (!submit_wait(ring, opens) || !harvest(ring, opens, fds)) {
                            close_leaked(fds);
                            return false;
                        }
                        let mut eperm = 0u32;
                        for (i, fd) in fds.iter_mut().enumerate() {
                            if *fd == -libc::EPERM {
                                let Some(cname) = cnames[i].as_ref() else {
                                    continue;
                                };
                                push(
                                    ring,
                                    open_sqe(dirfd, cname, OPEN_FLAGS_NO_NOATIME, i as u64),
                                );
                                eperm += 1;
                            }
                        }
                        if eperm > 0 && (!submit_wait(ring, eperm) || !harvest(ring, eperm, fds)) {
                            close_leaked(fds);
                            return false;
                        }
                        let mut nread = 0u32;
                        for (i, fd) in fds.iter().copied().enumerate() {
                            if fd < 0 {
                                continue;
                            }
                            let buf = &mut bufs[i];
                            buf.clear();
                            if buf.capacity() < CHUNK {
                                buf.reserve(CHUNK);
                            }
                            let spare = buf.spare_capacity_mut();
                            let len = spare.len().min(CHUNK) as u32;
                            let ptr = spare.as_mut_ptr().cast::<u8>();
                            push(ring, read_sqe(fd, ptr, len, i as u64));
                            nread += 1;
                        }
                        if nread > 0 && (!submit_wait(ring, nread) || !harvest(ring, nread, reads))
                        {
                            close_leaked(fds);
                            return false;
                        }
                        let mut nclose = 0u32;
                        for (i, fd) in fds.iter().copied().enumerate() {
                            if fd < 0 {
                                continue;
                            }
                            let res = reads[i];
                            if res >= 0 && (res as usize) < CHUNK {
                                unsafe { bufs[i].set_len(res as usize) };
                                push(ring, close_sqe(fd, i as u64));
                                nclose += 1;
                            } else if res < 0 {
                                push(ring, close_sqe(fd, i as u64));
                                nclose += 1;
                            }
                        }
                        if nclose > 0
                            && (!submit_wait(ring, nclose) || !harvest_count(ring, nclose))
                        {
                            for (i, fd) in fds.iter().copied().enumerate() {
                                if fd >= 0 && reads[i] >= 0 && (reads[i] as usize) >= CHUNK {
                                    unsafe { libc::close(fd) };
                                }
                            }
                            return false;
                        }
                        true
                    })
                })
            })
        })
    });
    if prepared != Some(true) {
        return false;
    }
    BUFS.with(|bufs_cell| {
        OPEN_FD.with(|fds_cell| {
            READ_RES.with(|res_cell| {
                let bufs = bufs_cell.borrow();
                let fds = fds_cell.borrow();
                let reads = res_cell.borrow();
                for i in 0..n {
                    let fd = fds[i];
                    if fd < 0 {
                        f(base + i, ReadOut::Miss);
                        continue;
                    }
                    let res = reads[i];
                    if res >= 0 && (res as usize) < CHUNK {
                        f(base + i, ReadOut::Bytes(&bufs[i]));
                    } else if res >= 0 {
                        f(base + i, ReadOut::File(unsafe { File::from_raw_fd(fd) }));
                    } else {
                        f(base + i, ReadOut::Miss);
                    }
                }
            })
        })
    });
    true
}
