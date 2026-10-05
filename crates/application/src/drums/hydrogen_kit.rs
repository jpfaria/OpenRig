//! Responsibility: parses a Hydrogen `drumkit.xml` into a kit description.
//!
//! Only what playback needs is read: each instrument's name, General MIDI
//! note, level, pan, mute group and velocity layers. Layers sharing one
//! velocity range are alternate takes, played round-robin.

use std::path::{Path, PathBuf};

use roxmltree::{Document, Node};

#[derive(Clone, Debug, PartialEq)]
pub struct KitLayerFiles {
    pub min_velocity: f32,
    pub max_velocity: f32,
    pub gain: f32,
    pub files: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KitInstrument {
    pub name: String,
    pub midi_note: Option<u8>,
    pub gain: f32,
    /// -1 hard left, 0 center, +1 hard right.
    pub pan: f32,
    pub choke_group: Option<u8>,
    pub layers: Vec<KitLayerFiles>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KitDescription {
    pub name: String,
    pub instruments: Vec<KitInstrument>,
}

/// Parses `xml`; sample paths are resolved against the kit folder `dir`.
pub fn parse_hydrogen_kit(xml: &str, dir: &Path) -> Result<KitDescription, String> {
    let doc = Document::parse(xml).map_err(|e| format!("invalid drumkit.xml: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "drumkit_info" {
        return Err("drumkit.xml has no <drumkit_info> root".into());
    }
    let name = text(root, "name").unwrap_or_default().to_string();
    let instruments = child(root, "instrumentList")
        .ok_or("drumkit.xml has no <instrumentList>")?
        .children()
        .filter(|n| n.has_tag_name_local("instrument"))
        .map(|n| parse_instrument(n, dir))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(KitDescription { name, instruments })
}

fn parse_instrument(node: Node, dir: &Path) -> Result<KitInstrument, String> {
    let name = text(node, "name").unwrap_or_default().to_string();
    let pan = match number(node, "pan") {
        Some(pan) => pan,
        None => number(node, "pan_R").unwrap_or(1.0) - number(node, "pan_L").unwrap_or(1.0),
    };
    let mut layers: Vec<KitLayerFiles> = Vec::new();
    for component in node
        .children()
        .filter(|n| n.has_tag_name_local("instrumentComponent"))
    {
        let component_gain = number(component, "gain").unwrap_or(1.0);
        for layer in component
            .children()
            .filter(|n| n.has_tag_name_local("layer"))
        {
            let file = text(layer, "filename")
                .ok_or_else(|| format!("instrument '{name}' has a layer without a file"))?;
            let min_velocity = number(layer, "min").unwrap_or(0.0);
            let max_velocity = number(layer, "max").unwrap_or(1.0);
            let gain = number(layer, "gain").unwrap_or(1.0) * component_gain;
            match layers
                .iter_mut()
                .find(|l| l.min_velocity == min_velocity && l.max_velocity == max_velocity)
            {
                Some(existing) => existing.files.push(dir.join(file)),
                None => layers.push(KitLayerFiles {
                    min_velocity,
                    max_velocity,
                    gain,
                    files: vec![dir.join(file)],
                }),
            }
        }
    }
    Ok(KitInstrument {
        midi_note: number(node, "midiOutNote")
            .filter(|n| (0.0..128.0).contains(n))
            .map(|n| n as u8),
        gain: number(node, "volume").unwrap_or(1.0) * number(node, "gain").unwrap_or(1.0),
        pan: pan.clamp(-1.0, 1.0),
        choke_group: number(node, "muteGroup")
            .filter(|g| (0.0..256.0).contains(g))
            .map(|g| g as u8),
        layers,
        name,
    })
}

trait LocalName {
    fn has_tag_name_local(&self, name: &str) -> bool;
}

impl LocalName for Node<'_, '_> {
    fn has_tag_name_local(&self, name: &str) -> bool {
        self.is_element() && self.tag_name().name() == name
    }
}

fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|n| n.has_tag_name_local(name))
}

fn text<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name).and_then(|n| n.text()).map(str::trim)
}

fn number(node: Node, name: &str) -> Option<f32> {
    text(node, name).and_then(|t| t.parse().ok())
}
