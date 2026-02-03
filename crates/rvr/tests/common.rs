use std::sync::{Condvar, Mutex, OnceLock};

pub const MAX_TEST_THREADS: usize = 5;

pub struct Semaphore {
    max: usize,
    state: Mutex<usize>,
    cv: Condvar,
}

impl Semaphore {
    pub fn new(max: usize) -> Self {
        Self {
            max,
            state: Mutex::new(0),
            cv: Condvar::new(),
        }
    }

    pub fn acquire(&self) -> SemaphoreGuard<'_> {
        let mut state = self.state.lock().expect("semaphore lock poisoned");
        while *state >= self.max {
            state = self.cv.wait(state).expect("semaphore wait poisoned");
        }
        *state += 1;
        SemaphoreGuard { sem: self }
    }
}

pub struct SemaphoreGuard<'a> {
    sem: &'a Semaphore,
}

impl Drop for SemaphoreGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.sem.state.lock().expect("semaphore lock poisoned");
        *state = state.saturating_sub(1);
        self.sem.cv.notify_one();
    }
}

pub fn concurrency_guard() -> SemaphoreGuard<'static> {
    static SEM: OnceLock<Semaphore> = OnceLock::new();
    SEM.get_or_init(|| Semaphore::new(MAX_TEST_THREADS))
        .acquire()
}
