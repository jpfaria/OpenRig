use anyhow::{bail, Result};
use pedal-setup::setup::Setup;
use std::collections::HashSet;

pub fn validate_setup(setup: &Setup) -> Result<()> {
    if setup.tracks.is_empty() {
        bail!("setup invalido: nenhuma track definida");
    }

    let inputs: HashSet<_> = setup.inputs.iter().map(|v| v.id.clone()).collect();
    let outputs: HashSet<_> = setup.outputs.iter().map(|v| v.id.clone()).collect();

    for track in &setup.tracks {
        if !inputs.contains(&track.input_id) {
            bail!("track referencia input inexistente");
        }
        if track.output_ids.is_empty() {
            bail!("track sem outputs");
        }
        for output_id in &track.output_ids {
            if !outputs.contains(output_id) {
                bail!("track referencia output inexistente");
            }
        }
    }

    Ok(())
}
