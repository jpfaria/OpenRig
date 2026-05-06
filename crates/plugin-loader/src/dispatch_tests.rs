use super::*;
use crate::manifest::ParameterValue as ManifestParameterValue;
use domain::value_objects::ParameterValue as DomainValue;

fn cap(values: &[(&str, f64)], file: &str) -> GridCapture {
    GridCapture {
        values: values
            .iter()
            .map(|(k, v)| ((*k).to_string(), ManifestParameterValue::Number(*v)))
            .collect(),
        file: file.into(),
    }
}

fn pset(pairs: &[(&str, f32)]) -> ParameterSet {
    let mut p = ParameterSet::default();
    for (k, v) in pairs {
        p.insert(*k, DomainValue::Float(*v));
    }
    p
}

#[test]
fn resolve_capture_picks_exact_numeric_match() {
    let parameters = vec![GridParameter {
        name: "gain".into(),
        display_name: None,
        values: vec![
            ManifestParameterValue::Number(10.0),
            ManifestParameterValue::Number(20.0),
        ],
    }];
    let captures = vec![
        cap(&[("gain", 10.0)], "g10.nam"),
        cap(&[("gain", 20.0)], "g20.nam"),
    ];
    let chosen = resolve_capture(&parameters, &captures, &pset(&[("gain", 20.0)])).unwrap();
    assert_eq!(chosen.file.to_str(), Some("g20.nam"));
}

#[test]
fn resolve_capture_snaps_to_nearest_numeric() {
    let parameters = vec![GridParameter {
        name: "gain".into(),
        display_name: None,
        values: vec![
            ManifestParameterValue::Number(10.0),
            ManifestParameterValue::Number(50.0),
            ManifestParameterValue::Number(90.0),
        ],
    }];
    let captures = vec![
        cap(&[("gain", 10.0)], "low.nam"),
        cap(&[("gain", 50.0)], "mid.nam"),
        cap(&[("gain", 90.0)], "high.nam"),
    ];
    let chosen = resolve_capture(&parameters, &captures, &pset(&[("gain", 47.0)])).unwrap();
    assert_eq!(chosen.file.to_str(), Some("mid.nam"));
}

#[test]
fn resolve_capture_returns_first_when_no_parameters() {
    let captures = vec![cap(&[], "only.wav"), cap(&[], "other.wav")];
    let chosen = resolve_capture(&[], &captures, &ParameterSet::default()).unwrap();
    assert_eq!(chosen.file.to_str(), Some("only.wav"));
}

#[test]
fn scan_lv2_ports_extracts_audio_and_control_indices() {
    use std::fs;
    let tmp = std::env::temp_dir().join(format!("openrig-ttl-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    let ttl = "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:plug>\n\
a lv2:Plugin ;\n\
lv2:port [\n\
    a lv2:InputPort, lv2:AudioPort ;\n\
    lv2:index 0 ;\n\
    lv2:symbol \"in_l\" ;\n\
] ,\n\
[\n\
    a lv2:OutputPort, lv2:AudioPort ;\n\
    lv2:index 1 ;\n\
    lv2:symbol \"out_l\" ;\n\
] ,\n\
[\n\
    a lv2:InputPort, lv2:ControlPort ;\n\
    lv2:index 2 ;\n\
    lv2:symbol \"gain\" ;\n\
    lv2:default 0.5 ;\n\
] .\n";
    fs::write(tmp.join("plug.ttl"), ttl).unwrap();
    let ports = scan_lv2_ports(&tmp, "urn:test:plug").unwrap();
    assert_eq!(ports.len(), 3);
    let audio_in: Vec<_> = ports
        .iter()
        .filter(|p| p.role == Lv2PortRole::AudioIn)
        .collect();
    assert_eq!(audio_in.len(), 1);
    assert_eq!(audio_in[0].index, 0);
    assert_eq!(audio_in[0].symbol, "in_l");
    let control: Vec<_> = ports
        .iter()
        .filter(|p| p.role == Lv2PortRole::ControlIn)
        .collect();
    assert_eq!(control.len(), 1);
    assert_eq!(control[0].symbol, "gain");
    assert_eq!(control[0].default_value, Some(0.5));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn scan_lv2_ports_picks_block_with_ports_when_uri_appears_in_multiple_ttls() {
    use std::fs;
    let tmp = std::env::temp_dir().join(format!("openrig-ttl-multi-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    // manifest.ttl: alphabetically first — declares URI but NO ports.
    // This mirrors the real LV2 bundle layout where manifest.ttl is
    // a tiny pointer and <plugin>_dsp.ttl carries the port info.
    fs::write(
        tmp.join("manifest.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:plug>\n\
a lv2:Plugin ;\n\
lv2:binary <plug.so> .\n",
    )
    .unwrap();
    fs::write(
        tmp.join("plug.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:plug>\n\
a lv2:Plugin ;\n\
lv2:port [\n\
    a lv2:InputPort, lv2:AudioPort ;\n\
    lv2:index 0 ;\n\
    lv2:symbol \"in\" ;\n\
] ,\n\
[\n\
    a lv2:InputPort, lv2:ControlPort ;\n\
    lv2:index 1 ;\n\
    lv2:symbol \"gain\" ;\n\
    lv2:default 0.7 ;\n\
] .\n",
    )
    .unwrap();

    let ports = scan_lv2_ports(&tmp, "urn:test:plug").expect("ports");
    assert_eq!(
        ports.len(),
        2,
        "expected scan to find ports declared in plug.ttl even though manifest.ttl mentions the URI first; got {ports:?}"
    );
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn scan_lv2_ports_handles_urls_with_periods_before_port_block() {
    // Real bundles (Dragonfly, TAP, ZAM) have URL declarations like
    // <http://lv2plug.in/ns/ext/state#interface> in the plugin
    // header BEFORE the lv2:port [...] blocks. The parser must not
    // mistake the period inside `lv2plug.in` for the turtle
    // statement terminator and stop early.
    use std::fs;
    let tmp = std::env::temp_dir().join(format!("openrig-ttl-url-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::write(
        tmp.join("plug.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:plug>\n\
a lv2:Plugin ;\n\
lv2:extensionData <http://lv2plug.in/ns/ext/state#interface> ;\n\
lv2:requiredFeature <http://lv2plug.in/ns/ext/urid#map> ;\n\
lv2:port [\n\
    a lv2:InputPort, lv2:AudioPort ;\n\
    lv2:index 0 ;\n\
    lv2:symbol \"in\" ;\n\
] ,\n\
[\n\
    a lv2:InputPort, lv2:ControlPort ;\n\
    lv2:index 1 ;\n\
    lv2:symbol \"gain\" ;\n\
    lv2:default 0.7 ;\n\
] .\n",
    )
    .unwrap();

    let ports = scan_lv2_ports(&tmp, "urn:test:plug").expect("ports");
    assert_eq!(
        ports.len(),
        2,
        "expected parser to skip past URL periods and find the lv2:port blocks; got {ports:?}"
    );
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn lv2_control_value_falls_back_through_chain() {
    let user = pset(&[("dry_level", 80.0)]);
    assert_eq!(lv2_control_value("dry_level", Some(50.0), &user), 80.0);
    assert_eq!(lv2_control_value("missing", Some(50.0), &user), 50.0);
    assert_eq!(lv2_control_value("missing", None, &user), 0.0);
}

#[test]
fn scan_lv2_ports_finds_ttls_in_shared_data_dir() {
    // Real-world bundles dedupe TTLs into `<package>/data/` and
    // keep `<package>/platform/<slot>/<binary>` per-platform.
    // scan_lv2_ports must read from `data/` when called with that
    // path, even if no `.ttl` lives next to the binary itself.
    use std::fs;
    let tmp = std::env::temp_dir().join(format!("openrig-lv2-data-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join("data")).unwrap();
    fs::write(
        tmp.join("data").join("manifest.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:plug>\n\
a lv2:Plugin ;\n\
lv2:binary <plug.so> .\n",
    )
    .unwrap();
    fs::write(
        tmp.join("data").join("plug.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:plug>\n\
a lv2:Plugin ;\n\
lv2:port [\n\
    a lv2:InputPort, lv2:AudioPort ;\n\
    lv2:index 0 ;\n\
    lv2:symbol \"in\" ;\n\
] ,\n\
[\n\
    a lv2:InputPort, lv2:ControlPort ;\n\
    lv2:index 1 ;\n\
    lv2:symbol \"gain\" ;\n\
    lv2:default 0.7 ;\n\
] .\n",
    )
    .unwrap();

    let ports = scan_lv2_ports(&tmp.join("data"), "urn:test:plug").expect("ports");
    assert_eq!(ports.len(), 2, "expected 2 ports from data/, got {ports:?}");
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn scan_lv2_ports_resolves_prefixed_plugin_names() {
    // Real-world TTLs (Fomp, Calf, Caps...) declare the plugin using a
    // turtle prefix:local form (`fomp:cs_phaser1`) instead of the
    // expanded `<http://drobilla.net/plugins/fomp/cs_phaser1>` form.
    // The manifest still carries the absolute URI, so the parser must
    // expand `@prefix` declarations and match either form.
    use std::fs;
    let tmp = std::env::temp_dir().join(format!("openrig-lv2-prefix-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::write(
        tmp.join("plug.ttl"),
        "@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
@prefix fomp: <http://drobilla.net/plugins/fomp/> .\n\
fomp:cs_phaser1\n\
    a lv2:Plugin ;\n\
    lv2:port [\n\
        a lv2:InputPort, lv2:AudioPort ;\n\
        lv2:index 0 ;\n\
        lv2:symbol \"in\" ;\n\
    ] ,\n\
    [\n\
        a lv2:InputPort, lv2:ControlPort ;\n\
        lv2:index 1 ;\n\
        lv2:symbol \"fb_gain\" ;\n\
        lv2:default 0.0 ;\n\
        lv2:minimum -1.0 ;\n\
        lv2:maximum 1.0 ;\n\
    ] .\n",
    )
    .unwrap();

    let ports = scan_lv2_ports(&tmp, "http://drobilla.net/plugins/fomp/cs_phaser1").expect("ports");
    assert_eq!(
        ports.len(),
        2,
        "expected parser to expand `fomp:` prefix and match the absolute URI; got {ports:?}"
    );
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn scan_lv2_ports_falls_back_to_only_plugin_when_uri_mismatches() {
    // Real bundles in OpenRig-plugins (MDA Leslie, MDA RoundPan, MDA
    // Stereo, etc.) ship a manifest.yaml whose `plugin_uri` does not
    // match the URI declared inside the bundle's `<plugin>.ttl`. For
    // MDA Leslie the manifest carries `http://drobilla.net/plugins/mda/Leslie`
    // but the TTL says `http://moddevices.com/plugins/mda/Leslie`.
    //
    // When this happens AND the bundle contains exactly one `a lv2:Plugin`
    // declaration, the parser must fall back to that single plugin so
    // the GUI still sees its control ports — otherwise dozens of LV2
    // packages surface zero parameters even though the binary works.
    use std::fs;
    let tmp = std::env::temp_dir().join(format!("openrig-lv2-uri-mismatch-{}", std::process::id()));
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
                 lv2:default 0.5 ;\n\
             ] .\n",
    )
    .unwrap();

    // Manifest carries the WRONG URI (drobilla.net) while the TTL
    // declares the plugin under moddevices.com. Same shape as MDA Leslie.
    let ports = scan_lv2_ports(&tmp, "http://drobilla.net/plugins/mda/Leslie")
        .expect("expected fallback to find the only plugin in the bundle");
    assert_eq!(
        ports.len(),
        2,
        "expected parser to fall back to the only plugin in the bundle when manifest URI doesn't match; got {ports:?}"
    );
    let _ = fs::remove_dir_all(&tmp);
}
