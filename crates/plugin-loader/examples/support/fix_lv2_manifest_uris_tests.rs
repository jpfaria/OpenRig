use super::*;

#[test]
fn rewrite_preserves_other_lines_and_indentation() {
    let original = "manifest_version: 1\n\
        id: lv2_x\n\
        plugin_uri: http://wrong.example/Plugin\n\
        backend: lv2\n";
    let new_text = rewrite_plugin_uri(original, "http://right.example/Plugin");
    assert_eq!(
        new_text,
        "manifest_version: 1\n\
         id: lv2_x\n\
         plugin_uri: http://right.example/Plugin\n\
         backend: lv2\n"
    );
}

#[test]
fn read_plugin_uri_strips_quotes() {
    assert_eq!(
        read_plugin_uri_line("plugin_uri: \"http://x.example/y\"\n"),
        Some("http://x.example/y".to_string())
    );
}
