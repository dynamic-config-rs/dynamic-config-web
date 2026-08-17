//! One reading of each configuration section, taken when a request begins.
//!
//! `Config::current()` is an atomic load, and its own documentation says to
//! call it once per request: a reload landing between two calls lets one
//! request observe two configurations. With one section that is easy to
//! honour. With two it is not, because "the same generation" is a property
//! of a pair of reads that no single call site can see.
//!
//! This crate is the pair. A [`Sections`] list is read once into a
//! [`Snapshot`], the framework adapter puts that snapshot where the request
//! can reach it, and every handler read comes back out of it.
//!
//! ```
//! use std::sync::Arc;
//! use dynamic_config_web_core::Sections;
//!
//! #[derive(Debug, PartialEq)]
//! struct Server { port: u16 }
//! #[derive(Debug, PartialEq)]
//! struct Features { cache: bool }
//!
//! # let server = Arc::new(Server { port: 8080 });
//! # let features = Arc::new(Features { cache: true });
//! let sections = Sections::new()
//!     .section({ let it = Arc::clone(&server); move || Some(Arc::clone(&it)) })
//!     .section({ let it = Arc::clone(&features); move || Some(Arc::clone(&it)) });
//!
//! let snapshot = sections.take();
//!
//! assert_eq!(snapshot.get::<Server>().unwrap().port, 8080);
//! assert!(snapshot.get::<Features>().unwrap().cache);
//! ```
//!
//! In a service the closures are `|| ServerConfig::try_current()`, which the
//! [`sections!`] macro writes for you.
//!
//! # How one reading stays one reading
//!
//! Each configuration has its own atomic cell and the engine keeps no
//! epoch across them, so reading N sections is N independent loads — and a
//! reload landing between two of them would put two generations in one
//! snapshot, which is the bug this crate exists to prevent.
//!
//! [`Sections::take`] therefore reads the install counters, reads the
//! sections, and reads the counters again; if anything moved it starts
//! over. A section registered through [`Sections::section`] supplies no
//! counter, and a list containing one reads without the check —
//! [`Sections::is_consistent`] says which kind you have, and the
//! [`sections!`] macro always produces the checked kind.
//!
//! # Why closures rather than a trait
//!
//! `#[dynamic_config]` writes `try_current()` as an inherent method, so a
//! generic function cannot call it. A closure can, and the same shape covers
//! a `Dynamic<T>` instance — `move || handle.current()` — which a trait
//! implemented on the type could not reach.

#![forbid(unsafe_code)]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// What one request may read: one `Arc` per section, taken together.
///
/// Built by [`Sections::take`] when the request begins, and read by an
/// adapter's extractor. Two reads of the same section answer the same
/// `Arc`, however far apart in the handler they are and whatever lands in
/// between.
#[derive(Clone, Default)]
pub struct Snapshot {
    sections: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    /// Every type that was *registered*, whether or not it had loaded.
    ///
    /// Keyed by `TypeId` rather than by name because that is the identity
    /// that cannot collide: `type_name` is a diagnostic string the language
    /// makes no uniqueness promise about. The name rides along because a
    /// `TypeId` cannot be turned back into one, and an error that could
    /// only say "some type" would not be worth printing.
    registered: Vec<(TypeId, &'static str)>,
}

impl Snapshot {
    /// The section of type `T` this request began with.
    ///
    /// `None` when `T` was not among the sections, or was among them and
    /// had not loaded yet. [`require`](Self::require) tells those apart.
    #[must_use]
    pub fn get<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.sections
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|section| section.downcast::<T>().ok())
    }

    /// The section of type `T`, or why it is not here.
    ///
    /// # Errors
    ///
    /// [`NotInScope::NotListed`] when `T` was never registered — a wiring
    /// mistake, fixed where the layer is built. [`NotInScope::NotLoaded`]
    /// when it was registered and nothing has installed a value yet — a
    /// startup-order mistake, fixed by loading before serving. The two have
    /// different fixes, which is why they are different variants.
    pub fn require<T: Any + Send + Sync>(&self) -> Result<Arc<T>, NotInScope> {
        match self.get::<T>() {
            Some(section) => Ok(section),
            None => {
                let id = TypeId::of::<T>();
                let name = std::any::type_name::<T>();

                if self.registered.iter().any(|(known, _)| *known == id) {
                    Err(NotInScope::NotLoaded(name))
                } else {
                    Err(NotInScope::NotListed(name))
                }
            }
        }
    }

    /// How many sections this request may read.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sections.len()
    }

    /// Whether it carries nothing at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// The type names registered, whether or not each had loaded.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.registered.iter().map(|(_, name)| *name).collect()
    }

    /// This snapshot with `inner`'s sections laid over it.
    ///
    /// What an adapter uses when layers nest and each carries its own
    /// list. The inner one wins where both registered a type, because it
    /// is the more specific of the two — and nothing is lost, which is the
    /// point: a handler under both layers can read either's sections.
    #[must_use]
    pub fn merged_with(mut self, inner: Self) -> Self {
        for (id, name) in inner.registered {
            if !self.registered.iter().any(|(known, _)| *known == id) {
                self.registered.push((id, name));
            }
        }

        self.sections.extend(inner.sections);
        self
    }
}

impl fmt::Debug for Snapshot {
    /// The section names, never their contents.
    ///
    /// A configuration section holds credentials, and `{:?}` on a request
    /// is the kind of thing that reaches a log line.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Snapshot")
            .field("sections", &self.names())
            .finish()
    }
}

/// Why a section is not in this request's snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotInScope {
    /// The type was never registered on the layer.
    NotListed(&'static str),
    /// It was registered, and nothing had installed a value when the
    /// request began.
    NotLoaded(&'static str),
}

impl NotInScope {
    /// The type name this is about.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::NotListed(name) | Self::NotLoaded(name) => name,
        }
    }
}

impl fmt::Display for NotInScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotListed(name) => write!(
                formatter,
                "`{name}` is not one of this request's sections; add it where the \
                 layer is built, with `sections![.., {name}]`"
            ),
            Self::NotLoaded(name) => write!(
                formatter,
                "`{name}` is one of this request's sections but nothing had loaded \
                 it when the request began; call `init()` before serving"
            ),
        }
    }
}

impl std::error::Error for NotInScope {}

/// The sections a request reads, and how to read each one.
///
/// Built once, at startup, and handed to the framework adapter. Each entry
/// is a closure the adapter calls when a request begins — never during it,
/// which is what makes the reads agree.
#[derive(Default)]
pub struct Sections {
    readers: Vec<Registered>,
}

struct Registered {
    id: TypeId,
    name: &'static str,
    read: Reader,
    /// This section's install counter, when the caller could supply one.
    ///
    /// Each configuration has its own atomic cell, and the engine has no
    /// epoch across them — so reading N sections is N independent loads,
    /// and a reload landing between two of them would put two generations
    /// in one snapshot. Comparing this counter before and after the read
    /// is what detects that; see [`Sections::take`].
    generation: Option<Generation>,
}

type Reader = Box<dyn Fn() -> Option<Arc<dyn Any + Send + Sync>> + Send + Sync>;
type Generation = Box<dyn Fn() -> u64 + Send + Sync>;

/// How many times `take` re-reads before giving up on a quiet moment.
///
/// A reload is rare and a read is microseconds, so one retry is almost
/// always enough. The bound exists so that a pathological reload loop
/// cannot stall a request: after it, the last read is served, which is no
/// worse than having no check at all.
const ATTEMPTS: usize = 8;

impl Sections {
    /// An empty list.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one section, read by `read`.
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use dynamic_config_web_core::Sections;
    /// # struct Database;
    /// # fn try_current() -> Option<Arc<Database>> { None }
    /// let sections = Sections::new().section(try_current);
    /// ```
    ///
    /// In a service `read` is `|| Database::try_current()`, or
    /// `move || handle.current()` for a `Dynamic<T>`.
    ///
    /// Registering the same type twice keeps the last reader, so a
    /// composed list can override one entry without rebuilding it.
    #[must_use]
    pub fn section<T, F>(self, read: F) -> Self
    where
        T: Any + Send + Sync,
        F: Fn() -> Option<Arc<T>> + Send + Sync + 'static,
    {
        self.push::<T>(read, None)
    }

    /// Adds one section, with the install counter that says when it moved.
    ///
    /// What [`sections!`] uses. The counter lets [`take`](Self::take) tell
    /// a snapshot that straddled a reload from one that did not; a section
    /// registered through [`section`](Self::section) has none, and a list
    /// containing one cannot make that check.
    #[must_use]
    pub fn section_with_generation<T, F, G>(self, read: F, generation: G) -> Self
    where
        T: Any + Send + Sync,
        F: Fn() -> Option<Arc<T>> + Send + Sync + 'static,
        G: Fn() -> u64 + Send + Sync + 'static,
    {
        self.push::<T>(read, Some(Box::new(generation)))
    }

    fn push<T>(
        mut self,
        read: impl Fn() -> Option<Arc<T>> + Send + Sync + 'static,
        generation: Option<Generation>,
    ) -> Self
    where
        T: Any + Send + Sync,
    {
        let id = TypeId::of::<T>();
        self.readers.retain(|existing| existing.id != id);
        self.readers.push(Registered {
            id,
            name: std::any::type_name::<T>(),
            read: Box::new(move || read().map(|section| section as Arc<dyn Any + Send + Sync>)),
            generation,
        });

        self
    }

    /// Whether every section can say when it last moved.
    ///
    /// `false` when any was registered through [`section`](Self::section),
    /// which takes a reader and nothing else — [`take`](Self::take) then
    /// reads once without checking.
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.readers
            .iter()
            .all(|section| section.generation.is_some())
    }

    /// Reads every section once.
    ///
    /// Called by the adapter when a request begins. A section that has not
    /// loaded is left out rather than failing the request: the handler that
    /// asks for it gets [`NotInScope::NotLoaded`], and a handler that does
    /// not ask is unaffected.
    #[must_use]
    pub fn take(&self) -> Snapshot {
        // One section cannot straddle anything, and a list that cannot
        // report its generations has nothing to compare.
        if self.readers.len() < 2 || !self.is_consistent() {
            return self.read_once();
        }

        // Each configuration has its own atomic cell and the engine has no
        // epoch across them, so reading N sections is N independent loads.
        // Read the counters, read the sections, read the counters again: if
        // nothing moved, nothing could have landed in between, and the
        // snapshot is one generation throughout.
        for _ in 0..ATTEMPTS {
            let before = self.generations();
            let snapshot = self.read_once();

            if self.generations() == before {
                return snapshot;
            }
        }

        // Reloading faster than a read completes, eight times running. The
        // last read is served: no worse than not checking, which is what
        // every caller had before.
        self.read_once()
    }

    /// One pass over the readers, with no consistency check.
    fn read_once(&self) -> Snapshot {
        let mut sections = HashMap::with_capacity(self.readers.len());
        let mut registered = Vec::with_capacity(self.readers.len());

        for section in &self.readers {
            registered.push((section.id, section.name));

            if let Some(value) = (section.read)() {
                sections.insert(section.id, value);
            }
        }

        Snapshot {
            sections,
            registered,
        }
    }

    /// Every section's install counter, in registration order.
    fn generations(&self) -> Vec<u64> {
        self.readers
            .iter()
            .map(|section| section.generation.as_ref().map_or(0, |read| read()))
            .collect()
    }

    /// How many sections are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.readers.len()
    }

    /// Whether nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.readers.is_empty()
    }

    /// The registered type names, in order.
    #[must_use]
    pub fn names(&self) -> Vec<&'static str> {
        self.readers.iter().map(|section| section.name).collect()
    }
}

impl fmt::Debug for Sections {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sections")
            .field("sections", &self.names())
            .finish()
    }
}

/// The sections a request reads, by type.
///
/// ```ignore
/// let app = Router::new()
///     .route("/", get(handler))
///     .layer(SnapshotLayer::new(sections![ServerConfig, FeaturesConfig]));
/// ```
///
/// Each name expands to `|| Type::try_current()`, which is what
/// `#[dynamic_config]` generates. For a `Dynamic<T>` instance, call
/// [`Sections::section`] with a closure instead.
#[macro_export]
macro_rules! sections {
    () => {
        $crate::Sections::new()
    };
    ($($section:ty),+ $(,)?) => {{
        $crate::Sections::new()
            $(.section_with_generation(
                || <$section>::try_current(),
                || <$section>::generation(),
            ))+
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Server {
        port: u16,
    }

    #[derive(Debug, PartialEq)]
    struct Features {
        cache: bool,
    }

    #[derive(Debug)]
    struct NeverLoaded;

    fn server(port: u16) -> impl Fn() -> Option<Arc<Server>> + Send + Sync {
        move || Some(Arc::new(Server { port }))
    }

    #[test]
    fn a_snapshot_answers_every_section_it_took() {
        let snapshot = Sections::new()
            .section(server(8080))
            .section(|| Some(Arc::new(Features { cache: true })))
            .take();

        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot.get::<Server>().unwrap().port, 8080);
        assert!(snapshot.get::<Features>().unwrap().cache);
    }

    #[test]
    fn two_reads_of_one_snapshot_are_the_same_arc() {
        // The property the whole crate exists for: whatever happens between
        // two reads, they answer the same value.
        let snapshot = Sections::new().section(server(8080)).take();

        let first = snapshot.get::<Server>().unwrap();
        let second = snapshot.get::<Server>().unwrap();

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn a_snapshot_does_not_move_when_the_source_does() {
        use std::sync::atomic::{AtomicU16, Ordering};

        static PORT: AtomicU16 = AtomicU16::new(8080);

        let sections = Sections::new().section(|| {
            Some(Arc::new(Server {
                port: PORT.load(Ordering::Relaxed),
            }))
        });

        let taken = sections.take();

        // A reload lands between the two reads.
        PORT.store(9090, Ordering::Relaxed);

        assert_eq!(taken.get::<Server>().unwrap().port, 8080);
        // And the next request sees it, because a scope is not a cache.
        assert_eq!(sections.take().get::<Server>().unwrap().port, 9090);
    }

    #[test]
    fn a_section_that_never_loaded_is_named_as_such() {
        let snapshot = Sections::new()
            .section(server(1))
            .section(|| None::<Arc<NeverLoaded>>)
            .take();

        assert_eq!(snapshot.len(), 1, "the unloaded one is not in the map");
        assert_eq!(snapshot.names().len(), 2, "but it is still registered");

        match snapshot.require::<NeverLoaded>() {
            Err(NotInScope::NotLoaded(name)) => assert!(name.ends_with("NeverLoaded")),
            other => panic!("expected NotLoaded, got {other:?}"),
        }
    }

    #[test]
    fn two_types_with_the_same_name_are_told_apart() {
        // `type_name` carries no uniqueness promise, so the registered set
        // is keyed by `TypeId`. Two `Server`s in different modules are two
        // sections, and asking for the unregistered one says so.
        mod other {
            #[derive(Debug)]
            pub struct Server;
        }

        let snapshot = Sections::new()
            .section(server(8080))
            .section(|| Some(Arc::new(other::Server)))
            .take();

        // Both are present, and each answers as itself.
        assert_eq!(snapshot.get::<Server>().unwrap().port, 8080);
        assert!(snapshot.get::<other::Server>().is_some());
        assert_eq!(snapshot.len(), 2, "one name, two sections");
    }

    #[test]
    fn a_section_nobody_registered_is_a_different_error() {
        let snapshot = Sections::new().section(server(1)).take();

        match snapshot.require::<Features>() {
            Err(NotInScope::NotListed(name)) => assert!(name.ends_with("Features")),
            other => panic!("expected NotListed, got {other:?}"),
        }
    }

    #[test]
    fn the_two_errors_say_what_to_do_about_them() {
        let listed = NotInScope::NotLoaded("Server").to_string();
        let missing = NotInScope::NotListed("Server").to_string();

        assert!(listed.contains("init()"), "{listed}");
        assert!(missing.contains("sections!"), "{missing}");
    }

    #[test]
    fn registering_a_type_twice_keeps_the_last_reader() {
        let snapshot = Sections::new().section(server(1)).section(server(2)).take();

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot.get::<Server>().unwrap().port, 2);
    }

    #[test]
    fn debug_prints_the_names_and_not_the_values() {
        let snapshot = Sections::new().section(server(5432)).take();
        let rendered = format!("{snapshot:?}");

        assert!(rendered.contains("Server"), "{rendered}");
        assert!(
            !rendered.contains("5432"),
            "a value reached Debug: {rendered}"
        );
    }

    #[test]
    fn take_refuses_a_snapshot_that_straddled_a_reload() {
        // The race the whole crate turns on. Each section has its own
        // atomic cell, so reading two of them is two loads — and a reload
        // landing between them would put two generations in one snapshot.
        //
        // The first reader here *is* that reload: it moves both sections
        // while the read is in progress, which is the worst possible
        // timing, and it does so only on the first few attempts.
        use std::sync::atomic::{AtomicU64, Ordering};

        static A: AtomicU64 = AtomicU64::new(1);
        static B: AtomicU64 = AtomicU64::new(1);
        static DISTURB: AtomicU64 = AtomicU64::new(3);

        let sections = Sections::new()
            .section_with_generation(
                || {
                    // Reading `A` is where the reload lands.
                    if DISTURB.load(Ordering::SeqCst) > 0 {
                        DISTURB.fetch_sub(1, Ordering::SeqCst);
                        A.fetch_add(1, Ordering::SeqCst);
                        B.fetch_add(1, Ordering::SeqCst);
                    }

                    Some(Arc::new(Server {
                        port: A.load(Ordering::SeqCst) as u16,
                    }))
                },
                || A.load(Ordering::SeqCst),
            )
            .section_with_generation(
                || {
                    Some(Arc::new(Features {
                        cache: B.load(Ordering::SeqCst) % 2 == 0,
                    }))
                },
                || B.load(Ordering::SeqCst),
            );

        let snapshot = sections.take();

        // Both sections came from the same generation: `A` and `B` move
        // together, so `port` and `cache` must agree about which one.
        let port = u64::from(snapshot.get::<Server>().unwrap().port);
        let cache = snapshot.get::<Features>().unwrap().cache;

        assert_eq!(
            cache,
            port % 2 == 0,
            "the snapshot mixed generations: port={port}, cache={cache}"
        );
        assert_eq!(DISTURB.load(Ordering::SeqCst), 0, "it should have retried");
    }

    #[test]
    fn a_list_that_cannot_report_generations_still_reads() {
        // `section()` takes a reader and nothing else, so there is no
        // counter to compare. That list reads once, as it always did.
        let sections = Sections::new().section(server(8080));

        assert!(!sections.is_consistent());
        assert_eq!(sections.take().get::<Server>().unwrap().port, 8080);
    }

    #[test]
    fn a_snapshot_crosses_threads() {
        let snapshot = Sections::new().section(server(8080)).take();

        let moved = std::thread::spawn(move || snapshot.get::<Server>().unwrap().port);

        assert_eq!(moved.join().unwrap(), 8080);
    }
}
