
use super::*;
use std::fs;

#[test]
fn returns_absolute_uri_from_prefixed_ttl() {
    let tmp = std::env::temp_dir().join(format!("openrig-infer-uri-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::write(
        tmp.join("plug.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
             @prefix mda: <http://moddevices.com/plugins/mda/> .\n\
             mda:Leslie\n\
                 a lv2:Plugin ;\n\
                 lv2:port [\n\
                     a lv2:InputPort, lv2:AudioPort ;\n\
                     lv2:index 0 ;\n\
                     lv2:symbol \"in\" ;\n\
                 ] ,\n\
                 [\n\
                     a lv2:InputPort, lv2:ControlPort ;\n\
                     lv2:index 1 ;\n\
                     lv2:symbol \"speed\" ;\n\
                 ] .\n",
    )
    .unwrap();

    let inferred = infer_plugin_uri(&tmp).expect("expected to infer the absolute URI from TTL");
    assert_eq!(inferred, "http://moddevices.com/plugins/mda/Leslie");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn returns_absolute_uri_when_already_absolute() {
    let tmp = std::env::temp_dir().join(format!("openrig-infer-uri-abs-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::write(
        tmp.join("plug.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
             <urn:test:plug>\n\
                 a lv2:Plugin ;\n\
                 lv2:port [\n\
                     a lv2:InputPort, lv2:ControlPort ;\n\
                     lv2:index 0 ;\n\
                     lv2:symbol \"gain\" ;\n\
                 ] .\n",
    )
    .unwrap();

    let inferred = infer_plugin_uri(&tmp).expect("expected to infer the absolute URI");
    assert_eq!(inferred, "urn:test:plug");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn returns_none_when_multiple_plugins_in_bundle() {
    let tmp = std::env::temp_dir().join(format!("openrig-infer-uri-multi-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::write(
        tmp.join("plug.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
             <urn:a> a lv2:Plugin ; lv2:port [ a lv2:ControlPort ; lv2:symbol \"x\" ] .\n\
             <urn:b> a lv2:Plugin ; lv2:port [ a lv2:ControlPort ; lv2:symbol \"y\" ] .\n",
    )
    .unwrap();

    assert!(
        infer_plugin_uri(&tmp).is_none(),
        "expected None when bundle has multiple plugin declarations"
    );
    let _ = fs::remove_dir_all(&tmp);
}
