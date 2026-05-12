use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub fn lock_or_recover<T>(m: &Arc<Mutex<T>>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| {
        tracing::warn!("mutex poisoned, recovering");
        e.into_inner()
    })
}

pub fn read_or_recover<T>(m: &Arc<RwLock<T>>) -> RwLockReadGuard<'_, T> {
    m.read().unwrap_or_else(|e| {
        tracing::warn!("rwlock read poisoned, recovering");
        e.into_inner()
    })
}

pub fn write_or_recover<T>(m: &Arc<RwLock<T>>) -> RwLockWriteGuard<'_, T> {
    m.write().unwrap_or_else(|e| {
        tracing::warn!("rwlock write poisoned, recovering");
        e.into_inner()
    })
}
