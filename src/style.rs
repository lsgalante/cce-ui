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
//! - **`.write()`** gives a copy of the field to change; when the guard drops, the change is
//!   published as a new snapshot. A guard that changed nothing publishes nothing. One that did
//!   lands its field on the NEWEST snapshot, not the one it copied from: whole when no other
//!   write changed that field meanwhile, and otherwise through the cell's merge — a plain value
//!   is a store (the last guard to drop wins), the registry merges key by key
//!   ([`StyleCell::merging`]), so a write published while a guard was held is not reverted.
//!   Guards never wait on each other, so writes may nest, even on one field.
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
    probe: [u32; 6],
    #[cfg(test)]
    probe_registry: crate::layout::StyleRegistry,
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
    /// `merge(base, mine, newest)`: land a guard's change of the field, from `base` to `mine`,
    /// on `newest`, the field as another write left it meanwhile.
    merge: fn(&T, T, &mut T),
}

impl<T> Clone for StyleCell<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for StyleCell<T> {}

/// The default merge: a write is a store, so the guard's value replaces the other's.
fn overwrite<T>(_base: &T, mine: T, newest: &mut T) {
    *newest = mine;
}

/// A style slot cannot fail to read or write; the `Result` is the `RwLock` API's shape.
#[derive(Debug)]
pub struct StyleError;

impl<T: Clone + 'static> StyleCell<T> {
    pub const fn new(get: fn(&Style) -> &T, get_mut: fn(&mut Style) -> &mut T) -> StyleCell<T> {
        StyleCell { get, get_mut, merge: overwrite::<T> }
    }

    /// A cell whose writes are not plain stores — a collection that writers change piece by
    /// piece — with the `merge(base, mine, newest)` that lands a guard's change, from `base`
    /// to `mine`, on `newest`: the field as another write left it while the guard was held.
    pub const fn merging(
        get: fn(&Style) -> &T,
        get_mut: fn(&mut Style) -> &mut T,
        merge: fn(&T, T, &mut T),
    ) -> StyleCell<T> {
        StyleCell { get, get_mut, merge }
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

    /// The field's value now.
    pub fn get(&self) -> T {
        self.read().map(|v| v.clone()).unwrap_or_else(|_| unreachable!())
    }
}

impl<T: Clone + PartialEq + 'static> StyleCell<T> {
    /// The field to change: published when the guard drops (in a batch, changed in place).
    pub fn write(&self) -> Result<StyleMut<T>, StyleError> {
        match pending() {
            Some(p) => {
                touch(self.get, self.get_mut);
                // SAFETY: as in `read`.
                Ok(StyleMut::Pending((self.get_mut)(unsafe { &mut *p }) as *mut T))
            }
            None => {
                let base = current();
                let value = (self.get)(&base).clone();
                Ok(StyleMut::Copy { cell: *self, base, value: Some(value), land: land::<T> })
            }
        }
    }
}

/// Publish what a guard on `cell` changed: its copy, `value`, of the field it took from
/// `base`. Nothing, when the copy is unchanged; otherwise the newest snapshot with the field
/// set to `value` when no other write changed it since `base`, or with `value`'s change merged
/// onto it when one did.
fn land<T: Clone + PartialEq>(cell: StyleCell<T>, base: &Style, value: T) {
    let was = (cell.get)(base);
    if value == *was {
        return;
    }
    change(|style| {
        let field = (cell.get_mut)(style);
        if *field == *was {
            *field = value;
        } else {
            (cell.merge)(was, value, field);
        }
    });
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
    /// Outside a batch: a copy of the field in `base`, the snapshot it was taken from;
    /// published on drop by `land` (see [`StyleCell::write`]).
    Copy { cell: StyleCell<T>, base: Arc<Style>, value: Option<T>, land: fn(StyleCell<T>, &Style, T) },
    /// In a batch: the pending snapshot's field.
    Pending(*mut T),
}

impl<T> std::ops::Deref for StyleMut<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self {
            StyleMut::Copy { value, .. } => value.as_ref().expect("taken only on drop"),
            // SAFETY: see `StyleCell::read`.
            StyleMut::Pending(p) => unsafe { &**p },
        }
    }
}

impl<T> std::ops::DerefMut for StyleMut<T> {
    fn deref_mut(&mut self) -> &mut T {
        match self {
            StyleMut::Copy { value, .. } => value.as_mut().expect("taken only on drop"),
            // SAFETY: see `StyleCell::read`.
            StyleMut::Pending(p) => unsafe { &mut **p },
        }
    }
}

impl<T: 'static> Drop for StyleMut<T> {
    fn drop(&mut self) {
        if let StyleMut::Copy { cell, base, value, land } = self {
            if let Some(value) = value.take() {
                land(*cell, base, value);
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
    use crate::layout::StyleRegistry;
    use std::sync::mpsc::channel;

    // Handles on `Style::probe`, which nothing but these tests writes: what each test reads back
    // is its own, whatever the tests running beside it publish.
    static A: StyleCell<u32> = StyleCell::new(|s| &s.probe[0], |s| &mut s.probe[0]);
    static B: StyleCell<u32> = StyleCell::new(|s| &s.probe[1], |s| &mut s.probe[1]);
    static C: StyleCell<u32> = StyleCell::new(|s| &s.probe[2], |s| &mut s.probe[2]);
    static X: StyleCell<u32> = StyleCell::new(|s| &s.probe[3], |s| &mut s.probe[3]);
    static Y: StyleCell<u32> = StyleCell::new(|s| &s.probe[4], |s| &mut s.probe[4]);
    static Z: StyleCell<u32> = StyleCell::new(|s| &s.probe[5], |s| &mut s.probe[5]);
    static R: StyleCell<StyleRegistry> =
        StyleCell::merging(|s| &s.probe_registry, |s| &mut s.probe_registry, StyleRegistry::merge);

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

    /// A write lands its change on the newest snapshot, not the one it copied from: a write to
    /// the same field published while its guard was held survives the guard's drop (merged key
    /// by key, for a merging cell), and a guard that changed nothing publishes nothing.
    #[test]
    fn a_write_keeps_what_another_thread_published_to_its_field_meanwhile() {
        let (held, wait_held) = channel();
        let (published, wait_published) = channel::<()>();
        let writer = std::thread::spawn(move || {
            let mut r = R.write().unwrap();
            r.load_float("held", 1.0);
            let unchanged = Z.write().unwrap();
            held.send(()).unwrap();
            wait_published.recv().unwrap();
            drop(unchanged);
            drop(r);
        });
        wait_held.recv().unwrap();
        R.write().unwrap().load_float("meanwhile", 2.0);
        *Z.write().unwrap() = 7;
        published.send(()).unwrap();
        writer.join().unwrap();
        let r = R.read().unwrap();
        assert_eq!((r.floats.get("held"), r.floats.get("meanwhile")), (Some(&1.0), Some(&2.0)));
        assert_eq!(Z.get(), 7, "the guard that changed nothing did not put back its 0");
    }

    /// The case that showed it: under `cfg(test)` a registry setter writes this thread's
    /// overlay, so its guard changes nothing — and must not re-publish the registry it copied
    /// over a load another thread made meanwhile.
    #[test]
    fn a_registry_setter_does_not_revert_a_load_meanwhile() {
        let (held, wait_held) = channel();
        let (published, wait_published) = channel::<()>();
        let setter = std::thread::spawn(move || {
            let mut r = crate::layout::get_style_registry().write().unwrap();
            r.set_float("style-test-set", 1.0);
            held.send(()).unwrap();
            wait_published.recv().unwrap();
        });
        wait_held.recv().unwrap();
        crate::layout::get_style_registry().write().unwrap().load_float("style-test-loaded", 2.0);
        published.send(()).unwrap();
        setter.join().unwrap();
        let r = crate::layout::get_style_registry().read().unwrap();
        assert_eq!(r.floats.get("style-test-loaded"), Some(&2.0));
    }

    /// Writes to one field nest on one thread: the outer guard's drop merges onto what the
    /// inner one published, removals included, rather than putting back its copy.
    #[test]
    fn writes_to_one_field_nest() {
        R.write().unwrap().load_float("nest-moved", 0.5);
        let mut outer = R.write().unwrap();
        // `load_len` moves the key from the floats to the lens: a removal and an insertion.
        outer.load_len("nest-moved", crate::units::Len::px(3.0));
        R.write().unwrap().load_float("nest-inner", 2.0);
        drop(outer);
        let r = R.read().unwrap();
        assert_eq!(
            (r.floats.get("nest-moved"), r.lens.get("nest-moved"), r.floats.get("nest-inner")),
            (None, Some(&crate::units::Len::px(3.0)), Some(&2.0))
        );
    }
}
