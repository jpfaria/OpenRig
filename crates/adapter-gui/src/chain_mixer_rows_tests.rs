//! #1007: the compact view's chain faders, shaped for the strips.

use application::chain_fader_view::ChainFaderView;
use project::looper::LooperConfig;

use super::*;
use crate::mixer_fader_law::position_from_db;
use crate::MixerStripRow;

fn global(id: &str, is_input: bool) -> MixerStripRow {
    MixerStripRow {
        id: id.into(),
        name: format!("{id} name").into(),
        detail: "IN 1".into(),
        is_input,
        position: 0.9,
        gain_label: "+6.0 dB".into(),
        muted: true,
        soloed: true,
        solo_silenced: false,
    }
}

fn fader(strip: &str, gain_db: f32, muted: bool) -> ChainFaderView {
    ChainFaderView {
        strip: strip.into(),
        gain_db,
        muted,
    }
}

#[test]
fn the_chain_row_sits_beside_its_global_strip_in_the_same_order() {
    let rows = chain_side_rows(
        &[global("in:0@d", true), global("in:1@d", true)],
        &[fader("in:1@d", -12.0, false), fader("in:0@d", -3.0, true)],
    );
    let ids: Vec<_> = rows.iter().map(|r| r.id.to_string()).collect();
    assert_eq!(ids, vec!["in:0@d", "in:1@d"]);
    assert_eq!(rows[0].gain_label.as_str(), "-3.0 dB");
    assert!(rows[0].muted);
    assert_eq!(rows[1].position, position_from_db(-12.0));
    assert!(!rows[1].muted);
}

#[test]
fn the_chain_row_carries_the_strip_label_but_never_the_global_state() {
    let rows = chain_side_rows(&[global("in:0@d", true)], &[fader("in:0@d", 0.0, false)]);
    assert_eq!(rows[0].name.as_str(), "in:0@d name");
    assert_eq!(rows[0].detail.as_str(), "IN 1");
    assert!(rows[0].is_input);
    assert!(!rows[0].soloed);
    assert!(!rows[0].solo_silenced);
    assert!(!rows[0].muted);
}

#[test]
fn a_strip_without_a_chain_fader_reads_unity() {
    let rows = chain_side_rows(&[global("out:0,1@d", false)], &[]);
    assert_eq!(rows[0].gain_label.as_str(), "0.0 dB");
    assert_eq!(rows[0].position, position_from_db(0.0));
}

#[test]
fn the_di_row_shows_the_chain_di_gain() {
    let row = di_row(-12.0);
    assert_eq!(row.id.as_str(), DI_FADER_ID);
    assert_eq!(row.gain_label.as_str(), "-12.0 dB");
    assert_eq!(row.position, position_from_db(-12.0));
}

#[test]
fn the_master_row_shows_the_chain_volume_in_db_and_percent() {
    let row = master_row(50.0);
    assert_eq!(row.id.as_str(), MASTER_FADER_ID);
    assert_eq!(row.gain_label.as_str(), "-6.0 dB");
    assert_eq!(row.detail.as_str(), "50%");
}

#[test]
fn every_tab_gets_its_rows_from_the_chain() {
    let mut chain = project::chain::Chain {
        disabled_endpoints: Default::default(),
        id: domain::ids::ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 50.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![LooperConfig::new(3)],
        mix: Default::default(),
    };
    chain.mix.di_gain_db = -6.0;
    let rows = chain_mixer_rows(
        &[global("in:0@d", true)],
        &[global("out:0,1@d", false)],
        &[fader("out:0,1@d", -3.0, false)],
        &chain,
    );
    assert_eq!(rows.inputs[0].gain_label.as_str(), "0.0 dB");
    assert_eq!(rows.outputs[0].gain_label.as_str(), "-3.0 dB");
    assert_eq!(rows.di[0].gain_label.as_str(), "-6.0 dB");
    assert_eq!(rows.loopers[0].id.as_str(), "looper:3");
    assert_eq!(rows.master[0].detail.as_str(), "50%");
}

#[test]
fn one_looper_row_per_looper_addressed_by_its_uid() {
    let mut a = LooperConfig::new(7);
    a.mix = 1.0;
    let mut b = LooperConfig::new(9);
    b.mix = 0.5;
    let rows = looper_rows(&[a, b]);
    let ids: Vec<_> = rows.iter().map(|r| r.id.to_string()).collect();
    assert_eq!(ids, vec!["looper:7", "looper:9"]);
    assert_eq!(rows[0].gain_label.as_str(), "0.0 dB");
    assert_eq!(rows[1].gain_label.as_str(), "-6.0 dB");
}
