//! The style, as ONE snapshot (`docs/rfc-global-state.md`, phase 3).
//!
//! Every configured colour, radius, font and size, and the registry of flattened config keys,
//! is a field of one [`Style`], published whole as an `Arc` and swapped on change. Until
//! 2026-10-08 each was a `static RwLock` of its own — about 170 of them beside the registry's
//! lock — so a reload wrote them one by one and a frame drawn meanwhile could see half of it,
//! and every read took a lock.
//!
//! What a slot is now: a typed handle, [`StyleCell`], on one field of the snapshot. It keeps
//! the `RwLock` API its callers were written against — `.read()` and `.write()`, each
//! returning a `Result` whose `Ok` is a guard — so they did not change:
//!
//! - **`.read()`** derefs into the current snapshot. Taking it costs no lock: each thread
//!   keeps the last snapshot it saw and checks a generation counter.
//! - **`.write()`** gives a copy of the field to change; when the guard drops, a new snapshot
//!   with that field changed is published. Guards never wait on each other, so writes may
//!   nest as freely as before.
//! - **[`batch`]** runs a reload as one change: inside it, on its thread, writes go to one
//!   pending snapshot and reads see it; the snapshot is published once when the batch ends.
//!   A reload is atomic now. What it publishes is the newest snapshot with the fields the
//!   batch wrote carried onto it, so a write another thread published meanwhile is kept
//!   (field by field: a field both wrote ends with the batch's value).
//!
//! The per-thread test overlays (`color::style_write`, the registry's) are unchanged: they
//! key on a cell's address, which a `StyleCell` has as a `static` as the lock did.

use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

/// The whole style: the colour slots, the layout slots and the registry.
#[derive(Clone, Default)]
pub struct Style {
    pub(crate) color: crate::color::Slots,
    pub(crate) layout: crate::layout::Slots,
    pub(crate) registry: crate::layout::StyleRegistry,
    /// Fields no production code touches, so a test below sees only its own writes.
    #[cfg(test)]
    probe: [u32; 5],
}

static GLOBAL: RwLock<Option<Arc<Style>>> = RwLock::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// The snapshot this thread read last, and the generation it was.
    static CACHE: RefCell<(u64, Option<Arc<Style>>)> = const { RefCell::new((0, None)) };
    /// A batch in progress on this thread: its pending snapshot and its depth.
    static PENDING: Cell<*mut Style> = const { Cell::new(std::ptr::null_mut()) };
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    /// The fields that batch has written (see [`touch`]).
    static TOUCHED: RefCell<Vec<Touched>> = const { RefCell::new(Vec::new()) };
}

/// A field a batch wrote: its getter's address and type, which name it, and how to copy it
/// from the batch's snapshot into another.
type Touched = ((usize, TypeId), Box<dyn Fn(&Style, &mut Style)>);

/// Record that the batch on this thread writes the field `get` reads. Two getters at one
/// address and of one type are one field (same code, same offset, same extent).
fn touch<T: Clone + 'static>(get: fn(&Style) -> &T, get_mut: fn(&mut Style) -> &mut T) {
    let key = (get as usize, TypeId::of::<T>());
    TOUCHED.with(|t| {
        let mut t = t.borrow_mut();
        if !t.iter().any(|(k, _)| *k == key) {
            t.push((key, Box::new(move |from, to| *get_mut(to) = get(from).clone())));
        }
    });
}

/// The current snapshot.
pub fn current() -> Arc<Style> {
    let generation = GENERATION.load(Ordering::Acquire);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.0 == generation {
            if let Some(s) = &c.1 {
                return Arc::clone(s);
            }
        }
        let s = {
            let mut g = GLOBAL.write().unwrap_or_else(|e| e.into_inner());
            Arc::clone(g.get_or_insert_with(|| Arc::new(Style::default())))
        };
        *c = (generation, Some(Arc::clone(&s)));
        s
    })
}

/// Publish a batch's snapshot, `pending`, begun from `base`. When nothing was published since,
/// that is `pending` itself; otherwise the fields the batch wrote are carried onto the newest
/// snapshot, so another thread's publish meanwhile is kept, not reverted.
fn publish_batch(pending: Style, base: &Arc<Style>, touched: &[Touched]) {
    let mut g = GLOBAL.write().unwrap_or_else(|e| e.into_inner());
    let style = match g.as_ref() {
        Some(newest) if !Arc::ptr_eq(newest, base) => {
            let mut style = (**newest).clone();
            for (_, copy) in touched {
                copy(&pending, &mut style);
            }
            style
        }
        _ => pending,
    };
    *g = Some(Arc::new(style));
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

/// Apply `change` to a copy of the current snapshot and publish it.
fn change(change: impl FnOnce(&mut Style)) {
    let mut g = GLOBAL.write().unwrap_or_else(|e| e.into_inner());
    let mut style = g.as_deref().cloned().unwrap_or_default();
    change(&mut style);
    *g = Some(Arc::new(style));
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

fn pending() -> Option<*mut Style> {
    let p = PENDING.with(Cell::get);
    (!p.is_null()).then_some(p)
}

/// Run `f` as one change of style (see the module docs): its writes are published together
/// when it returns, and its reads on this thread see them as they go. Batches nest; the
/// outermost publishes.
pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    if DEPTH.with(Cell::get) > 0 {
        DEPTH.with(|d| d.set(d.get() + 1));
        let r = f();
        DEPTH.with(|d| d.set(d.get() - 1));
        return r;
    }
    let base = current();
    let boxed = Box::into_raw(Box::new((*base).clone()));
    PENDING.with(|p| p.set(boxed));
    DEPTH.with(|d| d.set(1));
    // Publish and clear even if `f` unwinds: a half-applied reload is still the newest.
    // `base` is held to the end, so no later snapshot can share its address.
    struct End(*mut Style, Arc<Style>);
    impl Drop for End {
        fn drop(&mut self) {
            PENDING.with(|p| p.set(std::ptr::null_mut()));
            DEPTH.with(|d| d.set(0));
            let touched = TOUCHED.with(|t| std::mem::take(&mut *t.borrow_mut()));
            // SAFETY: the box made above, released exactly once, here.
            let style = unsafe { Box::from_raw(self.0) };
            publish_batch(*style, &self.1, &touched);
        }
    }
    let _end = End(boxed, base);
    f()
}

/// A handle on one field of the [`Style`]: what a style slot's `static` is.
pub struct StyleCell<T: 'static> {
    get: fn(&Style) -> &T,
    get_mut: fn(&mut Style) -> &mut T,
}

/// A style slot cannot fail to read or write; the `Result` is the `RwLock` API's shape.
#[derive(Debug)]
pub struct StyleError;

impl<T: Clone + 'static> StyleCell<T> {
    pub const fn new(get: fn(&Style) -> &T, get_mut: fn(&mut Style) -> &mut T) -> StyleCell<T> {
        StyleCell { get, get_mut }
    }

    /// The field in the current snapshot (or this thread's pending one, in a batch).
    pub fn read(&self) -> Result<StyleRef<T>, StyleError> {
        match pending() {
            // SAFETY: the pending snapshot lives until its batch ends, and a guard is a
            // temporary of code running inside the batch; read and write guards on one
            // slot never overlap there, as they could not under the lock this replaced.
            Some(p) => Ok(StyleRef::Pending((self.get)(unsafe { &*p }) as *const T)),
            None => Ok(StyleRef::Snapshot(current(), self.get)),
        }
    }

    /// The field to change: published when the guard drops (in a batch, changed in place).
    pub fn write(&self) -> Result<StyleMut<T>, StyleError> {
        match pending() {
            Some(p) => {
                touch(self.get, self.get_mut);
                // SAFETY: as in `read`.
                Ok(StyleMut::Pending((self.get_mut)(unsafe { &mut *p }) as *mut T))
            }
            None => Ok(StyleMut::Copy(Some((self.get)(&current()).clone()), self.get_mut)),
        }
    }

    /// The field's value now.
    pub fn get(&self) -> T {
        self.read().map(|v| v.clone()).unwrap_or_else(|_| unreachable!())
    }
}

/// A read of one style field.
pub enum StyleRef<T: 'static> {
    Snapshot(Arc<Style>, fn(&Style) -> &T),
    Pending(*const T),
}

impl<T> std::ops::Deref for StyleRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            StyleRef::Snapshot(s, get) => get(s),
            // SAFETY: see `StyleCell::read`.
            StyleRef::Pending(p) => unsafe { &**p },
        }
    }
}

/// A write of one style field.
pub enum StyleMut<T: 'static> {
    /// Outside a batch: a copy, published into a new snapshot on drop.
    Copy(Option<T>, fn(&mut Style) -> &mut T),
    /// In a batch: the pending snapshot's field.
    Pending(*mut T),
}

impl<T> std::ops::Deref for StyleMut<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            StyleMut::Copy(v, _) => v.as_ref().expect("taken only on drop"),
            // SAFETY: see `StyleCell::read`.
            StyleMut::Pending(p) => unsafe { &**p },
        }
    }
}

impl<T> std::ops::DerefMut for StyleMut<T> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            StyleMut::Copy(v, _) => v.as_mut().expect("taken only on drop"),
            // SAFETY: see `StyleCell::read`.
            StyleMut::Pending(p) => unsafe { &mut **p },
        }
    }
}

impl<T: 'static> Drop for StyleMut<T> {
    fn drop(&mut self) {
        if let StyleMut::Copy(v, get_mut) = self {
            if let Some(value) = v.take() {
                let get_mut = *get_mut;
                change(|s| *get_mut(s) = value);
            }
        }
    }
}

/// Declare a module's style slots: the fields of its part of the [`Style`] and their defaults.
/// Each has a `static` [`StyleCell`] handle of the same name beside the module's getters.
macro_rules! style_slots {
    ($( $name:ident : $ty:ty = $default:expr ; )*) => {
        /// This module's style slots (see `crate::style`).
        #[derive(Clone)]
        #[allow(non_snake_case)]
        pub struct Slots { $( pub(crate) $name: $ty, )* }
        impl Default for Slots {
            fn default() -> Self { Slots { $( $name: $default, )* } }
        }
    };
}
pub(crate) use style_slots;

#[cfg(test)]
mod tests {
    use super::*;

    // Handles on `Style::probe`, which nothing but these tests writes: what each test reads back
    // is its own, whatever the tests running beside it publish.
    static A: StyleCell<u32> = StyleCell::new(|s| &s.probe[0], |s| &mut s.probe[0]);
    static B: StyleCell<u32> = StyleCell::new(|s| &s.probe[1], |s| &mut s.probe[1]);
    static C: StyleCell<u32> = StyleCell::new(|s| &s.probe[2], |s| &mut s.probe[2]);
    static X: StyleCell<u32> = StyleCell::new(|s| &s.probe[3], |s| &mut s.probe[3]);
    static Y: StyleCell<u32> = StyleCell::new(|s| &s.probe[4], |s| &mut s.probe[4]);

    /// A write publishes; a batch publishes once, and its own reads see its writes before.
    #[test]
    fn writes_publish_and_a_batch_publishes_once() {
        let before = GENERATION.load(Ordering::Acquire);
        *A.write().unwrap() = 1;
        assert_eq!(A.get(), 1);
        assert!(GENERATION.load(Ordering::Acquire) > before);

        batch(|| {
            *B.write().unwrap() = 2;
            *C.write().unwrap() = 3;
            assert_eq!((B.get(), C.get()), (2, 3), "a batch reads its own writes");
            // Another thread still sees the snapshot from before the batch.
            let seen = std::thread::spawn(|| (B.get(), C.get())).join().unwrap();
            assert_eq!(seen, (0, 0), "nothing published yet");
        });
        assert_eq!((B.get(), C.get()), (2, 3));
    }

    /// A batch publishes the fields it wrote, not the whole snapshot it began from: a write
    /// another thread published while it ran survives its end.
    #[test]
    fn a_batch_keeps_what_another_thread_published_meanwhile() {
        let (began, wait_began) = std::sync::mpsc::channel();
        let (published, wait_published) = std::sync::mpsc::channel::<()>();
        let reload = std::thread::spawn(move || {
            batch(|| {
                *X.write().unwrap() = 1;
                began.send(()).unwrap();
                wait_published.recv().unwrap();
            })
        });
        wait_began.recv().unwrap();
        *Y.write().unwrap() = 7;
        published.send(()).unwrap();
        reload.join().unwrap();
        assert_eq!((X.get(), Y.get()), (1, 7));
    }
}
