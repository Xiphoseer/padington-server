use std::marker::PhantomData;

#[derive(Debug)]
pub struct Counter<T>(u64, PhantomData<T>);

impl<T> Default for Counter<T> {
    fn default() -> Self {
        Self(0, PhantomData)
    }
}

impl<T: From<u64>> Counter<T> {
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
