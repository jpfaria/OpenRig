//! The catalog is process-wide and changed from several threads at once —
//! an install worker, a create worker and the dispatching thread can each
//! load or unload a package. No edit may lose another one's package.

use std::fs;
use std::path::{Path, PathBuf};

const THREADS: usize = 16;
const ROUNDS: usize = 20;

fn write_ir_plugin(parent: &Path, id: &str) {
    let dir = parent.join(id);
    fs::create_dir_all(&dir).expect("create plugin dir");
    fs::write(dir.join("imp.wav"), b"").expect("write placeholder wav");
    let manifest = format!(
        "manifest_version: 1\n\
         id: {id}\n\
         display_name: Concurrent Cab\n\
         type: cab\n\
         backend: ir\n\
         parameters:\n\
         - name: capture\n  \
           display_name: Capture\n  \
           values:\n  \
           - main\n\
         captures:\n\
         - values:\n    \
             capture: main\n  \
           file: imp.wav\n"
    );
    fs::write(dir.join("manifest.yaml"), manifest).expect("write manifest");
}

#[test]
fn concurrent_reloads_never_drop_another_package() {
    let base = std::env::temp_dir().join(format!(
        "openrig_registry_concurrent_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&base);
    let parents: Vec<(String, PathBuf)> = (0..THREADS)
        .map(|i| {
            let id = format!("concurrent_cab_{i}");
            let parent = base.join(&id);
            write_ir_plugin(&parent, &id);
            (id, parent)
        })
        .collect();

    let handles: Vec<_> = parents
        .iter()
        .cloned()
        .map(|(id, parent)| {
            std::thread::spawn(move || {
                for _ in 0..ROUNDS {
                    let _ = plugin_loader::registry::unload(&id);
                    plugin_loader::registry::load_one(&id, std::slice::from_ref(&parent))
                        .expect("load_one");
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("worker panicked");
    }

    let missing: Vec<&str> = parents
        .iter()
        .map(|(id, _)| id.as_str())
        .filter(|id| plugin_loader::registry::find(id).is_none())
        .collect();
    let _ = fs::remove_dir_all(&base);
    assert!(
        missing.is_empty(),
        "packages lost by a concurrent edit: {missing:?}"
    );
}
