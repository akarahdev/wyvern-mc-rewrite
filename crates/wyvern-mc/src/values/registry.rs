use voxidian_protocol::registry::{RegEntry, Registry as PtcRegistry};

use super::Key;

pub struct Registry<T> {
    pub(crate) inner: PtcRegistry<T>,
}

impl<T: Clone> Clone for Registry<T> {
    fn clone(&self) -> Self {
        let mut reg = Registry::new();
        for (key, value) in self.entries() {
            reg.insert(key.clone(), value.clone());
        }
        reg
    }
}

impl<T> Registry<T> {
    pub fn new() -> Registry<T> {
        Registry {
            inner: PtcRegistry::new(),
        }
    }
    pub fn insert(&mut self, key: Key<T>, value: T) {
        self.inner.insert(key.into(), value);
    }

    pub fn get(&self, key: Key<T>) -> Option<&T> {
        self.inner.get(&key.into())
    }

    pub fn keys(&self) -> impl Iterator<Item = Key<T>> {
        self.inner.keys().map(|x| x.clone().into())
    }

    pub fn entries(&self) -> impl Iterator<Item = (Key<T>, &T)> {
        self.inner.entries().map(|x| (x.0.clone().into(), x.1))
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }

    pub(crate) fn get_entry(&self, key: Key<T>) -> Option<RegEntry<T>> {
        self.inner.get_entry(&key.into())
    }
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<PtcRegistry<T>> for Registry<T> {
    fn from(value: PtcRegistry<T>) -> Registry<T> {
        Registry { inner: value }
    }
}
