//! Small hand-written work-stealing-ish thread pool with ordered results.
//!
//! A pool of \`workers\` threads pulls \`(index, item)\` jobs from a shared
//! queue; each worker owns a \`Vec<Slot<S>>\` so results are collected in
//! input order and traversal order stays deterministic regardless of thread
//! scheduling. Panic safety: a panicked job records a \`None\` slot instead of
//! poisoning the pool.

use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex};

pub fn worker_count_for_configured(configured: Option<u32>) -> usize {
    if let Some(n) = configured {
        return (n as usize).clamp(1, 256);
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 64)
}

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct ThreadPool {
    shared: Arc<Shared>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

struct Shared {
    queue: Mutex<QueueState>,
    cvar: Condvar,
}

struct QueueState {
    jobs: VecDeque<Job>,
    shutdown: bool,
    live_workers: usize,
}

impl ThreadPool {
    pub fn new(workers: usize) -> Self {
        let workers = workers.clamp(1, 256);
        let shared = Arc::new(Shared {
            queue: Mutex::new(QueueState {
                jobs: VecDeque::new(),
                shutdown: false,
                live_workers: workers,
            }),
            cvar: Condvar::new(),
        });
        let mut handles = Vec::with_capacity(workers);
        for i in 0..workers {
            let shared = Arc::clone(&shared);
            let handle = std::thread::Builder::new()
                .name(format!("cw-scan-{}", i))
                .spawn(move || worker_loop(shared))
                .expect("spawn scan worker");
            handles.push(handle);
        }
        ThreadPool {
            shared,
            workers: handles,
        }
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn spawn(&self, job: impl FnOnce() + Send + 'static) {
        let mut st = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        if st.shutdown {
            return;
        }
        st.jobs.push_back(Box::new(job));
        drop(st);
        self.shared.cvar.notify_one();
    }

    /// Run \`f(i, item)\` for every item on the pool and collect results in
    /// input order. Jobs are queued as one task per item so long tails do not
    /// block short items; a panicking job yields \`None\` for that slot.
    pub fn map_ordered<T, S, F>(
        &self,
        items: impl IntoIterator<Item = T>,
        f: F,
    ) -> Vec<Option<S>>
    where
        T: Send + 'static,
        S: Send + 'static,
        F: Fn(usize, T) -> S + Send + Sync + 'static,
    {
        let items: Vec<T> = items.into_iter().collect();
        let n = items.len();
        if n == 0 {
            return Vec::new();
        }
        let slots: Arc<Vec<Mutex<Option<S>>>> =
            Arc::new((0..n).map(|_| Mutex::new(None)).collect());
        let done = Arc::new((Mutex::new(0usize), Condvar::new()));
        let f = Arc::new(f);
        for (idx, item) in items.into_iter().enumerate() {
            let slots = Arc::clone(&slots);
            let done = Arc::clone(&done);
            let f = Arc::clone(&f);
            self.spawn(move || {
                let produced = catch_unwind(AssertUnwindSafe(|| f(idx, item)))
                    .ok();
                if let Some(value) = produced {
                    *slots[idx].lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
                }
                let (lock, cvar) = &*done;
                let mut count = lock.lock().unwrap_or_else(|e| e.into_inner());
                *count += 1;
                if *count == n {
                    cvar.notify_all();
                }
            });
        }
        let (lock, cvar) = &*done;
        let mut count = lock.lock().unwrap_or_else(|e| e.into_inner());
        while *count < n {
            count = cvar
                .wait(count)
                .unwrap_or_else(|e| e.into_inner());
        }
        drop(count);
        Arc::try_unwrap(slots)
            .map(|v| {
                v.into_iter()
                    .map(|m| {
                        let out = m.into_inner().unwrap_or_else(|e| e.into_inner());
                        out
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Convenience wrapper for non-panicking workloads.
    pub fn map_ordered_total<T, S, F>(
        &self,
        items: impl IntoIterator<Item = T>,
        f: F,
    ) -> Vec<S>
    where
        T: Send + 'static,
        S: Send + 'static,
        F: Fn(usize, T) -> S + Send + Sync + 'static,
    {
        self.map_ordered(items, f).into_iter().flatten().collect()
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        {
            let mut st = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            st.shutdown = true;
        }
        self.shared.cvar.notify_all();
        for h in self.workers.drain(..) {
            let _ = h.join();
        }
    }
}

fn worker_loop(shared: Arc<Shared>) {
    loop {
        let job = {
            let mut st = shared
                .queue
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(job) = st.jobs.pop_front() {
                    break Some(job);
                }
                if st.shutdown {
                    st.live_workers = st.live_workers.saturating_sub(1);
                    break None;
                }
                st = shared.cvar.wait(st).unwrap_or_else(|e| e.into_inner());
            }
        };
        match job {
            Some(job) => job(),
            None => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn ordered_results_keep_input_order() {
        let pool = ThreadPool::new(4);
        let results = pool.map_ordered_total(0..100usize, move |_, i| i * 2);
        assert_eq!(results, (0..100).map(|i| i * 2).collect::<Vec<_>>());
        assert_eq!(pool.worker_count(), 4);
    }

    #[test]
    fn every_job_runs_exactly_once() {
        let pool = ThreadPool::new(8);
        let counter = Arc::new(AtomicUsize::new(0));
        for _ in 0..100 {
            let c = Arc::clone(&counter);
            pool.spawn(move || {
                c.fetch_add(1, Ordering::SeqCst);
            });
        }
        drop(pool);
        assert_eq!(counter.load(Ordering::SeqCst), 100);
    }

    #[test]
    fn panicking_job_does_not_kill_pool() {
        let pool = ThreadPool::new(2);
        let results = pool.map_ordered(0..10usize, move |_, i| {
            if i == 3 {
                panic!("boom");
            }
            i
        });
        let good: Vec<usize> = results.into_iter().flatten().collect();
        assert_eq!(good, vec![0, 1, 2, 4, 5, 6, 7, 8, 9]);
        // Pool still usable afterwards.
        let again = pool.map_ordered_total(0..5usize, move |_, i| i);
        assert_eq!(again, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn empty_input_is_trivial() {
        let pool = ThreadPool::new(1);
        let out: Vec<usize> = pool.map_ordered_total(Vec::<usize>::new(), move |_, i| i);
        assert!(out.is_empty());
    }
}
