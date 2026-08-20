//! The snapshot, with no engine and no framework — which is the point.
//!
//! ```sh
//! cargo run -p dynamic-config-web-core --example core_snapshot
//! ```
//!
//! `Sections` never names a configuration type, only `Arc<T>`: a section
//! is a closure answering "the current value", whatever owns it. Here
//! that owner is a `Mutex` this example flips by hand, standing in for
//! the engine — which is exactly how the crate's own tests prove the
//! atomicity story without loading anything.

use std::sync::{Arc, Mutex, OnceLock};

use dynamic_config_web_core::Sections;

#[derive(Debug)]
struct Server {
    port: u16,
    generation: u32,
}

#[derive(Debug)]
struct Features {
    cache: bool,
    generation: u32,
}

/// The stand-in engine: two "configurations" that reload together.
fn state() -> &'static Mutex<(Arc<Server>, Arc<Features>)> {
    static STATE: OnceLock<Mutex<(Arc<Server>, Arc<Features>)>> = OnceLock::new();

    STATE.get_or_init(|| {
        Mutex::new((
            Arc::new(Server {
                port: 8080,
                generation: 1,
            }),
            Arc::new(Features {
                cache: false,
                generation: 1,
            }),
        ))
    })
}

fn reload(generation: u32) {
    let mut guard = state().lock().expect("not poisoned");

    *guard = (
        Arc::new(Server {
            port: 8080 + u16::try_from(generation).unwrap_or(0),
            generation,
        }),
        Arc::new(Features {
            cache: generation.is_multiple_of(2),
            generation,
        }),
    );
}

fn main() {
    let sections = Sections::new()
        .section(|| Some(state().lock().expect("not poisoned").0.clone()))
        .section(|| Some(state().lock().expect("not poisoned").1.clone()));

    println!("sections: {:?}\n", sections.names());

    // One `take()` is one reading of every section. Everything read
    // *through the snapshot* afterwards is from that instant — a reload
    // in between cannot tear it.
    let snapshot = sections.take();

    let server = snapshot.require::<Server>().expect("in scope");

    reload(2); // ← a deployment lands mid-request

    let features = snapshot.require::<Features>().expect("in scope");

    println!("through one snapshot, across a reload:");
    println!(
        "  server   = port {}, generation {}",
        server.port, server.generation
    );
    println!(
        "  features = cache {}, generation {} (same instant)\n",
        features.cache, features.generation
    );
    assert_eq!(server.generation, features.generation);

    // The next request's snapshot sees the new state — this is
    // per-request pinning, not staleness.
    let next = sections.take();

    println!("the next snapshot:");
    println!(
        "  features = {:?}\n",
        next.require::<Features>().expect("in scope")
    );

    // And the error half: a type nobody declared answers by name.
    let missing = snapshot.require::<String>().unwrap_err();

    println!("asking for an undeclared section:\n  {missing}");
}
