use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

pub(crate) struct WorkQueue<T> {
    state: Mutex<(VecDeque<T>, bool)>,
    ready: Condvar,
}

impl<T> WorkQueue<T> {
    pub(crate) fn new() -> WorkQueue<T> {
        WorkQueue {
            state: Mutex::new((VecDeque::new(), false)),
            ready: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, (VecDeque<T>, bool)> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn push(&self, item: T) {
        self.lock().0.push_back(item);
        self.ready.notify_one();
    }

    pub(crate) fn close(&self) {
        self.lock().1 = true;
        self.ready.notify_all();
    }

    pub(crate) fn pop_newest(&self) -> Option<T> {
        let mut state = self.lock();
        loop {
            if let Some(item) = state.0.pop_back() {
                return Some(item);
            }
            if state.1 {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    pub(crate) fn pop(&self) -> Option<T> {
        let mut state = self.lock();
        loop {
            if let Some(item) = state.0.pop_front() {
                return Some(item);
            }
            if state.1 {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkQueue;

    #[test]
    fn workers_drain_every_item_once() {
        let queue = WorkQueue::new();
        let total = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    while let Some(n) = queue.pop() {
                        total.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
                    }
                });
            }
            for n in 1..=100 {
                queue.push(n);
            }
            queue.close();
        });
        assert_eq!(total.into_inner(), 5050);
    }
}
