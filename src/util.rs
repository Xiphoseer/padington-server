//! # Misc utitlities
//!
//! This module contains some utilities that are used but not specific to `padington`.
use std::marker::PhantomData;

/// A counter that produces IDs of type T
#[derive(Debug)]
pub struct Counter<T>(u64, PhantomData<fn() -> T>);

impl<T> Default for Counter<T> {
    fn default() -> Self {
        Self(0, PhantomData)
    }
}

/*impl <T: From<u64>> Iterator for Counter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.0.checked_add(1) {
            let id = self.0;
            self.0 = next;
            Some(T::from(id))
        } else {
            None
        }
    }
}*/

#[allow(clippy::should_implement_trait)]
impl<T: From<u64>> Counter<T> {
    /// Get the next value from this counter
    pub fn next(&mut self) -> T {
        let id = self.0;
        self.0 = id + 1;
        T::from(id)
    }
}

pub(crate) enum LoopState<T> {
    Break(T),
    Continue,
}
