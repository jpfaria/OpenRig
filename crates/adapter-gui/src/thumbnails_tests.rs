    // Note: resolve_thumbnail_path, thumbnail_png, and read_cached all depend
    // on infra_filesystem::asset_paths() being initialized, which requires
    // global state setup. These are integration-level functions rather than
    // pure functions, so we skip them here.
    //
    // The cache logic and path construction are implicitly tested when the
    // full GUI runs.

    #[test]
    fn module_compiles_and_exports_thumbnail_png() {
        // Verify the public API exists and has the expected signature
        let _: fn(&str, &str) -> Option<Vec<u8>> = super::thumbnail_png;
    }
