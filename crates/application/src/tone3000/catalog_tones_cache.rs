//! Responsibility: keeps one value derived from the plugin catalog per catalog generation.

use std::sync::Mutex;

/// The last value computed and the catalog generation it was computed at.
pub struct GenerationCache<T> {
    cached: Mutex<Option<(u64, T)>>,
}

impl<T> Default for GenerationCache<T> {
    fn default() -> Self {
        Self {
            cached: Mutex::new(None),
        }
    }
}

impl<T: Clone> GenerationCache<T> {
    /// The value for `generation`: the cached one when it was computed at
    /// that generation, else `compute`'s, which replaces it.
    pub fn get(&self, generation: u64, compute: impl FnOnce() -> T) -> T {
        let mut cached = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, value)) = cached.as_ref() {
            if *at == generation {
                return value.clone();
            }
        }
        let value = compute();
        *cached = Some((generation, value.clone()));
        value
    }
}

#[cfg(test)]
#[path = "catalog_tones_cache_tests.rs"]
mod tests;
