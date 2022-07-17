use core::ops::Deref;
use std::collections::{HashMap, HashSet};
use std::marker::Unsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

use common::errors::*;

/*
TODO: Use the following hueristics for GC'ing
- Assume cycles are not very frequent.
- When we insert an object, if we hit a power of 2 number of stored objects (or )
- When an ObjectWeak is dropped and the weak count is 1, increment a pool wide counter
    - When this counter reaches 25% of the total pool size, trigger a GC
- When an ObjectStrong is dropped fully, increment a counter in the ObjectPool
    - When the number of living pointers is <25% of the number of dead ones, kill the object.
- Other optimizations:
    - If we drop a ObjectStrong instance which was never downgraded, we can simply delete that one value from the pool.
    - Split up the set of roots into those added before and after the last GC.

*/

pub trait Object {
    /// Should mark the object as immutable. This must minimally ensure that
    /// future calls to referenced_objects() always returns the same set of
    /// values.
    fn freeze_object(&self);

    /// Should return the set of all directly referenced objects.
    fn referenced_objects(&self, out: &mut Vec<ObjectWeak<Self>>);
}

/// A collection of owned objects of some types.
///
/// Objects in a pool can be referenced via two reference types:
/// - ObjectPoolRoot: Strong pointer to an object. Implies that the referenced
///   object and all objects it references can't be deleted while at least one
///   ObjectPoolRoot struct referencing it is still alive.
/// - Object: Weak pointer to an object. An object only referenced by Object
///   structs will be garbage collected once all internal references to it from
///   strong pointers are removed.
///
/// NOTE: Cloning an ObjectPool references the same internal object storage.
pub struct ObjectPool<O: Object + ?Sized> {
    shared: Arc<ObjectPoolShared<O>>,
}

struct ObjectPoolShared<O: Object + ?Sized> {
    /// NOTE: This is always set to true when the state is locked and never
    /// returns to false.
    frozen: AtomicBool,
    state: Mutex<ObjectPoolState<O>>,
}

struct ObjectPoolState<O: Object + ?Sized> {
    /// Index of every object
    roots: HashSet<usize>,
    entries: Vec<ObjectPoolEntry<O>>,
}

struct ObjectPoolEntry<O: Object + ?Sized> {
    object: Arc<(AtomicUsize, O)>,
    marked: bool,
}

impl<O: Object + ?Sized> Clone for ObjectPool<O> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<O: Object + ?Sized> ObjectPool<O> {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(ObjectPoolShared {
                frozen: AtomicBool::new(false),
                state: Mutex::new(ObjectPoolState {
                    roots: HashSet::new(),
                    entries: vec![],
                }),
            }),
        }
    }

    pub fn insert<T: Unsize<O>>(&self, data: T) -> Result<ObjectStrong<O>> {
        let mut state = self.shared.state.lock().unwrap();

        if self.shared.frozen.load(Ordering::SeqCst) {
            return Err(err_msg("ObjectPool is frozen"));
        }

        let index = state.entries.len();

        let object: Arc<(AtomicUsize, O)> =
            Arc::<(AtomicUsize, T)>::new((AtomicUsize::new(index), data));

        state.entries.push(ObjectPoolEntry {
            object: object.clone(),
            marked: false,
        });

        state.roots.insert(index);

        Ok(ObjectStrong {
            pool: self.shared.clone(),
            object,
        })
    }

    pub fn freeze(&self) -> Result<()> {
        {
            let mut state = self.shared.state.lock().unwrap();
            self.shared.frozen.store(true, Ordering::SeqCst);

            for entry in &state.entries {
                entry.object.1.freeze_object();
            }
        }

        self.gc()
    }

    pub fn gc(&self) -> Result<()> {
        let mut state = self.shared.state.lock().unwrap();
        if state.roots.is_empty() {
            state.entries.clear();
            return Ok(());
        }

        // Un-mark
        for entry in &mut state.entries {
            entry.marked = false;
        }

        // Mark
        let mut pending_indices = state.roots.iter().cloned().collect::<Vec<usize>>();
        while let Some(index) = pending_indices.pop() {
            if state.entries[index].marked {
                continue;
            }

            state.entries[index].marked = true;

            let object = &state.entries[index].object.1;

            let mut referenced = vec![];
            object.referenced_objects(&mut referenced);

            for obj in referenced {
                // If the referenced object is in a different pool, we assume that there are no
                // cycles back to the current pool.
                //
                // TODO: Enforce some form of pool hierarchy to gurantee this is true.
                if !core::ptr::eq::<ObjectPoolShared<O>>(obj.pool.as_ptr(), self.shared.as_ref()) {
                    let other_pool = match obj.pool.upgrade() {
                        Some(v) => v,
                        None => {
                            return Err(err_msg(
                                "ObjectPool contains references to unknown remote pools",
                            ))
                        }
                    };

                    if !other_pool.frozen.load(Ordering::SeqCst) {
                        return Err(err_msg(
                            "ObjectPool contains reference to non-frozen remote pool",
                        ));
                    }

                    continue;
                }

                if let Some(ptr) = obj.object.upgrade() {
                    pending_indices.push(ptr.0.load(Ordering::SeqCst));
                } else {
                    return Err(err_msg("ObjectPool contains dangling pointers"));
                }
            }
        }

        // Sweep
        let mut index = 0;
        while index < state.entries.len() {
            if !state.entries[index].marked {
                state.entries.swap_remove(index);

                // Repair index references after the swap.
                if index < state.entries.len() {
                    let old_index = state.entries.len();
                    state.entries[index].object.0.store(index, Ordering::SeqCst);
                    if state.roots.remove(&old_index) {
                        state.roots.insert(index);
                    }
                }

                continue;
            }

            index += 1;
        }

        Ok(())
    }
}

// NOTE: It is iomportant that no ValueInstance type contains a ValuePoolRoot
// object to avoid cyclic roots.
pub struct ObjectStrong<O: Object + ?Sized> {
    pool: Arc<ObjectPoolShared<O>>,
    object: Arc<(AtomicUsize, O)>,
}

impl<O: Object + ?Sized> Clone for ObjectStrong<O> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            object: self.object.clone(),
        }
    }
}

impl<O: Object + ?Sized> Drop for ObjectStrong<O> {
    fn drop(&mut self) {
        if Arc::strong_count(&self.object) == 2 {
            let mut state = self.pool.state.lock().unwrap();

            // NOTE: Because we locked the state, we are guranteed that this isn't being
            // actively changed.
            let index = self.object.0.load(Ordering::SeqCst);

            if state.roots.remove(&index) {
                // TODO: Maybe trigger garbage collect.
            }
        }
    }
}

impl<O: Object + ?Sized> ObjectStrong<O> {
    /// NOTE: To avoid the return value of this being immediately garbage
    /// collected it should be used before the ValuePoolRoot instance is
    /// dropped.
    pub fn downgrade(&self) -> ObjectWeak<O> {
        ObjectWeak {
            pool: Arc::downgrade(&self.pool),
            object: Arc::downgrade(&self.object),
        }
    }
}

impl<O: Object + ?Sized> Deref for ObjectStrong<O> {
    type Target = O;

    fn deref(&self) -> &Self::Target {
        &self.object.1
    }
}

/// A shared reference to some data.
/// Cloning a Value will continue referencing the same underlying data.
pub struct ObjectWeak<O: ?Sized + Object> {
    pool: Weak<ObjectPoolShared<O>>,

    /// Reference to the object's data.
    ///
    /// This is technically redundant data given that we know the pool
    /// and id which could be used to loop up this pointer. But we prefer to
    /// store a direct pointer to the object for two reasons:
    /// 1. Avoid pointer indirections which reading the object.
    /// 2. Ensure that if stray ObjectWeak instances escape, we can be sure that
    /// the id hasn't been re-used by a newer object by checking the state of
    /// the weak pointer.
    object: Weak<(AtomicUsize, O)>,
}

impl<O: ?Sized + Object> Clone for ObjectWeak<O> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            object: self.object.clone(),
        }
    }
}

impl<O: ?Sized + Object> ObjectWeak<O> {
    pub fn upgrade(&self) -> Option<ObjectStrong<O>> {
        let pool = match self.pool.upgrade() {
            Some(v) => v,
            None => return None,
        };

        let object = match self.object.upgrade() {
            Some(v) => v,
            None => return None,
        };

        Some(ObjectStrong { pool, object })
    }
}
