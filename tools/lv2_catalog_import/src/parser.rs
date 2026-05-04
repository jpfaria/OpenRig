use crate::model::{Bundle, Plugin, Port, PortDirection, PortKind};
use anyhow::{Context, Result};
use oxttl::TurtleParser;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use walkdir::WalkDir;

const LV2_NS: &str = "http://lv2plug.in/ns/lv2core#";
const DOAP_NS: &str = "http://usefulinc.com/ns/doap#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const ATOM_NS: &str = "http://lv2plug.in/ns/ext/atom#";
const MOD_NS: &str = "http://moddevices.com/ns/mod#";
const MODGUI_NS: &str = "http://moddevices.com/ns/modgui#";
const MIDI_NS: &str = "http://lv2plug.in/ns/ext/midi#";
const UNITS_UNIT: &str = "http://lv2plug.in/ns/extensions/units#unit";

pub fn discover_bundles(root: &Path) -> Result<Vec<Bundle>> {
    let mut bundles = Vec::new();
    for entry in std::fs::read_dir(root).with_context(|| format!("read {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let bundle_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let plugins = parse_bundle(&path).unwrap_or_else(|err| {
            eprintln!("warn: failed to parse bundle {}: {err:#}", bundle_name);
            Vec::new()
        });
        if plugins.is_empty() {
            continue;
        }
        bundles.push(Bundle {
            bundle_dir: bundle_name,
            plugins,
        });
    }
    bundles.sort_by(|a, b| a.bundle_dir.cmp(&b.bundle_dir));
    Ok(bundles)
}

pub fn parse_bundle(bundle_path: &Path) -> Result<Vec<Plugin>> {
    let bundle_dir = bundle_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let mut store = TripleStore::default();
    for entry in WalkDir::new(bundle_path).max_depth(2).into_iter().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("ttl") {
            continue;
        }
        if let Err(err) = ingest_ttl(path, &mut store) {
            eprintln!("warn: ttl parse error in {}: {err:#}", path.display());
        }
    }

    let mut plugins = Vec::new();
    for (subject, _) in store.iter_subjects_of_type(&format!("{LV2_NS}Plugin")) {
        let plugin = build_plugin(&store, subject, &bundle_dir);
        plugins.push(plugin);
    }
    plugins.sort_by(|a, b| a.uri.cmp(&b.uri));
    Ok(plugins)
}

#[derive(Default)]
struct TripleStore {
    triples: HashMap<String, Vec<(String, Object)>>,
}

#[derive(Debug, Clone)]
enum Object {
    Iri(String),
    Blank(String),
    Literal { lex: String },
}

impl TripleStore {
    fn add(&mut self, subject: String, predicate: String, object: Object) {
        self.triples
            .entry(subject)
            .or_default()
            .push((predicate, object));
    }

    fn objects<'a>(
        &'a self,
        subject: &str,
        predicate: &'a str,
    ) -> impl Iterator<Item = &'a Object> + 'a {
        self.triples
            .get(subject)
            .into_iter()
            .flat_map(move |entries| {
                entries
                    .iter()
                    .filter(move |(p, _)| p == predicate)
                    .map(|(_, o)| o)
            })
    }

    fn iri_objects<'a>(
        &'a self,
        subject: &str,
        predicate: &'a str,
    ) -> impl Iterator<Item = &'a str> + 'a {
        self.objects(subject, predicate).filter_map(|o| match o {
            Object::Iri(s) => Some(s.as_str()),
            _ => None,
        })
    }

    fn first_iri<'a>(&'a self, subject: &str, predicate: &'a str) -> Option<&'a str> {
        self.iri_objects(subject, predicate).next()
    }

    fn first_literal<'a>(&'a self, subject: &str, predicate: &'a str) -> Option<&'a str> {
        self.objects(subject, predicate).find_map(|o| match o {
            Object::Literal { lex } => Some(lex.as_str()),
            _ => None,
        })
    }

    fn first_float(&self, subject: &str, predicate: &str) -> Option<f32> {
        self.first_literal(subject, predicate)
            .and_then(|s| s.trim().parse::<f32>().ok())
    }

    fn iter_subjects_of_type<'a>(
        &'a self,
        type_iri: &'a str,
    ) -> impl Iterator<Item = (&'a str, ())> + 'a {
        self.triples.iter().filter_map(move |(subject, entries)| {
            let has_type = entries
                .iter()
                .any(|(p, o)| p == RDF_TYPE && matches!(o, Object::Iri(s) if s == type_iri));
            if has_type {
                Some((subject.as_str(), ()))
            } else {
                None
            }
        })
    }
}

fn ingest_ttl(path: &Path, store: &mut TripleStore) -> Result<()> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let parser = TurtleParser::new()
        .with_base_iri(format!("file://{}", path.display()))
        .with_context(|| "set base iri")?
        .for_reader(reader);
    for triple in parser {
        let triple = match triple {
            Ok(t) => t,
            Err(err) => {
                eprintln!("warn: triple err in {}: {err}", path.display());
                continue;
            }
        };
        let subject = term_subject_string(&triple.subject);
        let predicate = triple.predicate.as_str().to_string();
        let object = term_to_object(&triple.object);
        store.add(subject, predicate, object);
    }
    Ok(())
}

fn term_subject_string(term: &oxrdf::Subject) -> String {
    match term {
        oxrdf::Subject::NamedNode(n) => n.as_str().to_string(),
        oxrdf::Subject::BlankNode(b) => format!("_:{}", b.as_str()),
    }
}

fn term_to_object(term: &oxrdf::Term) -> Object {
    match term {
        oxrdf::Term::NamedNode(n) => Object::Iri(n.as_str().to_string()),
        oxrdf::Term::BlankNode(b) => Object::Blank(format!("_:{}", b.as_str())),
        oxrdf::Term::Literal(l) => Object::Literal {
            lex: l.value().to_string(),
        },
    }
}

fn build_plugin(store: &TripleStore, subject: &str, bundle_dir: &str) -> Plugin {
    let plugin_iri = format!("{LV2_NS}Plugin");
    let plugin_classes: Vec<String> = store
        .iri_objects(subject, RDF_TYPE)
        .filter(|iri| iri.starts_with(LV2_NS) && iri.ends_with("Plugin") && *iri != plugin_iri)
        .map(|s| s.strip_prefix(LV2_NS).unwrap_or(s).to_string())
        .collect();

    let doap_name_pred = format!("{DOAP_NS}name");
    let mod_brand_pred = format!("{MOD_NS}brand");
    let mod_label_pred = format!("{MOD_NS}label");
    let binary_pred = format!("{LV2_NS}binary");
    let port_pred = format!("{LV2_NS}port");

    let doap_name = store
        .first_literal(subject, &doap_name_pred)
        .map(|s| s.to_string());
    let mod_brand = store
        .first_literal(subject, &mod_brand_pred)
        .map(|s| s.to_string());
    let mod_label = store
        .first_literal(subject, &mod_label_pred)
        .map(|s| s.to_string());
    let binary = store
        .first_iri(subject, &binary_pred)
        .map(|iri| iri.rsplit('/').next().unwrap_or(iri).to_string());

    // Walk modgui:gui blank node → modgui:thumbnail to find the icon PNG.
    let modgui_pred = format!("{MODGUI_NS}gui");
    let thumbnail_pred = format!("{MODGUI_NS}thumbnail");
    let thumbnail = store
        .objects(subject, &modgui_pred)
        .find_map(|gui| {
            let gui_subject = match gui {
                Object::Blank(s) => s.as_str(),
                Object::Iri(s) => s.as_str(),
                _ => return None,
            };
            store.first_iri(gui_subject, &thumbnail_pred).map(|raw| {
                raw.trim_start_matches("file://")
                    .rsplit_once('/')
                    .map(|(_, name)| name.to_string())
                    .unwrap_or_else(|| raw.to_string())
            })
        })
        .map(|name| {
            if name.starts_with("modgui/") {
                name
            } else {
                format!("modgui/{name}")
            }
        });

    let mut ports = Vec::new();
    let port_objs: Vec<Object> = store.objects(subject, &port_pred).cloned().collect();
    for port_obj in port_objs {
        let port_subject = match &port_obj {
            Object::Blank(s) => s.as_str(),
            Object::Iri(s) => s.as_str(),
            _ => continue,
        };
        if let Some(p) = build_port(store, port_subject) {
            ports.push(p);
        }
    }
    ports.sort_by_key(|p| p.index);

    Plugin {
        uri: subject.to_string(),
        bundle_dir: bundle_dir.to_string(),
        binary,
        doap_name,
        mod_brand,
        mod_label,
        plugin_classes,
        ports,
        thumbnail,
    }
}

fn build_port(store: &TripleStore, subject: &str) -> Option<Port> {
    let types: Vec<&str> = store.iri_objects(subject, RDF_TYPE).collect();

    let direction = if types.contains(&format!("{LV2_NS}InputPort").as_str()) {
        PortDirection::Input
    } else if types.contains(&format!("{LV2_NS}OutputPort").as_str()) {
        PortDirection::Output
    } else {
        PortDirection::Bidirectional
    };

    let kind = if types.contains(&format!("{LV2_NS}AudioPort").as_str()) {
        PortKind::Audio
    } else if types.contains(&format!("{LV2_NS}ControlPort").as_str()) {
        PortKind::Control
    } else if types.contains(&format!("{LV2_NS}CVPort").as_str()) {
        PortKind::Cv
    } else if types.contains(&format!("{ATOM_NS}AtomPort").as_str()) {
        PortKind::Atom
    } else if types.iter().any(|t| t.contains("EventPort")) {
        PortKind::Event
    } else {
        PortKind::Other
    };

    let index = store
        .first_literal(subject, &format!("{LV2_NS}index"))
        .and_then(|s| s.trim().parse::<usize>().ok())?;
    let symbol = store
        .first_literal(subject, &format!("{LV2_NS}symbol"))
        .unwrap_or("")
        .to_string();
    let name = store
        .first_literal(subject, &format!("{LV2_NS}name"))
        .map(|s| s.to_string());
    let default = store.first_float(subject, &format!("{LV2_NS}default"));
    let minimum = store.first_float(subject, &format!("{LV2_NS}minimum"));
    let maximum = store.first_float(subject, &format!("{LV2_NS}maximum"));

    let port_property_pred = format!("{LV2_NS}portProperty");
    let port_properties: Vec<&str> = store.iri_objects(subject, &port_property_pred).collect();
    let int_iri = format!("{LV2_NS}integer");
    let enum_iri = format!("{LV2_NS}enumeration");
    let toggle_iri = format!("{LV2_NS}toggled");
    let is_integer = port_properties.iter().any(|p| *p == int_iri);
    let is_enumeration = port_properties.iter().any(|p| *p == enum_iri);
    let is_toggle = port_properties.iter().any(|p| *p == toggle_iri);
    let is_logarithmic = port_properties.iter().any(|p| p.contains("logarithmic"));

    let supports_pred = format!("{ATOM_NS}supports");
    let midi_iri = format!("{MIDI_NS}MidiEvent");
    let supports_midi = store
        .iri_objects(subject, &supports_pred)
        .any(|s| s == midi_iri);

    let unit_uri = store.first_iri(subject, UNITS_UNIT).map(|s| s.to_string());

    Some(Port {
        index,
        symbol,
        name,
        kind,
        direction,
        default,
        minimum,
        maximum,
        is_integer,
        is_enumeration,
        is_toggle,
        is_logarithmic,
        supports_midi,
        unit_uri,
    })
}
