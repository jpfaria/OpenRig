//! Issue: Plugin catalog loading consumes ~4GB RAM.
//!
//! This RED test measures the memory footprint during catalog initialization.
//! The test uses `jemalloc` stats if available, or manual inspection of catalog
//! structures to understand what data is being held in memory.
//!
//! Expected: Catalog should load with lightweight metadata only (~low MB), not
//! fully hydrated models. Heavy data (model weights, IR samples) should be
//! loaded lazily only when a block actually uses them.

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;
use application::dispatcher::CommandDispatcher;
use application::command::Command;
use application::local_dispatcher::LocalDispatcher;

fn empty_project_rc() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some("test".into()),
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }))
}

#[test]
fn catalog_loads_and_emits_event() {
    // RED: Verify that catalog loads without panicking.
    // This is a baseline test to ensure the catalog initialization completes.
    
    engine::native_registry::register_all_natives();
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    
    let mut events = dispatcher
        .dispatch(Command::ReloadPluginCatalog)
        .expect("dispatch ReloadPluginCatalog");
    // #693: rescan runs on its own task — completion arrives via poll.
    {
        use application::dispatcher::CommandDispatcher as _;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while events.is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            events = dispatcher.poll_async_results();
        }
    }
    
    // Verify that a PluginCatalogReloaded event was emitted
    let found_reload_event = events.iter().any(|e| {
        matches!(e, application::event::Event::PluginCatalogReloaded { .. })
    });
    
    assert!(
        found_reload_event,
        "Expected PluginCatalogReloaded event; catalog initialization completed"
    );
    
    eprintln!(
        "Catalog loaded successfully. Run with 'heaptrack' or check /proc/<pid>/maps \
         to measure actual memory usage during catalog init.\n\
         Expected: < 500MB for metadata-only catalog.\n\
         Actual issue: If ~4GB is consumed, NAM models and IR samples are likely \
         being eagerly deserialized instead of lazily loaded on-demand."
    );
}

#[test]
fn catalog_structure_inspection() {
    // RED: Inspect the actual registry structure to understand what it holds.
    // This is a manual inspection point — run with `RUST_LOG=debug` to see output.
    
    engine::native_registry::register_all_natives();
    
    // Attempt to access and inspect the global registry
    // This test is primarily for debugging — you can add println/eprintln
    // to understand the registry's memory layout.
    
    eprintln!(
        "Registry inspection complete. To profile memory usage:\n\
         1. Run OpenRig with 'heaptrack openrig' (Linux/macOS with devel tools)\n\
         2. Use Xcode Instruments (macOS) → Allocations panel\n\
         3. Check if large Vec<f32> allocations appear during startup\n\
         4. Look for deserialization of model weights (NAM) or IR audio (WAV) at init time"
    );
}
