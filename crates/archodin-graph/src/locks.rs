//! Lock helpers shared by the Archodin crates (SD-10).
//!
//! A lock poisoned by a thread that panicked while holding it is recovered
//! explicitly and loudly: the poisoned state is taken over and the event is
//! logged to stderr, so one panic can never cascade into a panic in every
//! later lock user, and the recovery is never silent. The guarded data may
//! be half-updated -- callers that need stronger guarantees must validate
//! it themselves -- but taking the guard over keeps the process alive
//! instead of crashing the job that happens to lock next.
//!
//! The helpers live here, in one place, instead of one verbatim copy per
//! module: `archodin-store` and `archodin-console` both depend on
//! `archodin-graph`, so they use these too.

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Locks `m`, recovering from poisoning with a log line instead of panicking.
pub fn mutex<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    match m.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!(
                "[archodin-graph] mutex poisoned by a panicking thread; recovering the lock"
            );
            poisoned.into_inner()
        }
    }
}

/// Takes the read lock of `m`, recovering from poisoning with a log line.
pub fn read<T>(m: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    match m.read() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!(
                "[archodin-graph] RwLock poisoned by a panicking thread; recovering the read lock"
            );
            poisoned.into_inner()
        }
    }
}

/// Takes the write lock of `m`, recovering from poisoning with a log line.
pub fn write<T>(m: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    match m.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!(
                "[archodin-graph] RwLock poisoned by a panicking thread; recovering the write lock"
            );
            poisoned.into_inner()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poison_mutex(m: &Mutex<usize>) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut guard = m.lock().unwrap();
            *guard += 1;
            panic!("poison the mutex for the test");
        }));
    }

    #[test]
    fn a_poisoned_mutex_is_recovered_with_its_value_intact() {
        let m = Mutex::new(41usize);
        poison_mutex(&m);
        assert!(m.is_poisoned());
        assert_eq!(*mutex(&m), 42);
    }

    fn poison_rwlock(m: &RwLock<Vec<String>>) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut guard = m.write().unwrap();
            guard.push("before the panic".to_string());
            panic!("poison the RwLock for the test");
        }));
    }

    #[test]
    fn a_poisoned_rwlock_is_recovered_for_reading_and_writing() {
        let m = RwLock::new(Vec::<String>::new());
        poison_rwlock(&m);
        assert!(m.is_poisoned());
        assert_eq!(read(&m).as_slice(), ["before the panic"]);
        write(&m).push("after the recovery".to_string());
        assert_eq!(read(&m).len(), 2);
    }
}
