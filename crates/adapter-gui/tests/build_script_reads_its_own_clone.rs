//! A build script must read the package directory cargo hands it when it
//! runs, never the one it was compiled in.
//!
//! Every clone of the repo shares one cargo target directory, and cargo gives
//! a workspace member the same build-script hash in every clone. A build
//! script that bakes `env!("CARGO_MANIFEST_DIR")` in at compile time keeps
//! reading the clone that compiled it first: the GUI shipped another clone's
//! translations (every key added on the branch showed raw, #398).

const BUILD_SCRIPTS: [(&str, &str); 2] = [
    ("crates/adapter-gui/build.rs", include_str!("../build.rs")),
    ("crates/nam/build.rs", include_str!("../../nam/build.rs")),
];

#[test]
fn build_scripts_read_the_manifest_dir_at_run_time() {
    for (path, source) in BUILD_SCRIPTS {
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !code.contains("env!(\"CARGO_MANIFEST_DIR\")"),
            "{path}: read CARGO_MANIFEST_DIR with std::env::var at run time; \
             env! freezes the clone that compiled the script"
        );
    }
}
