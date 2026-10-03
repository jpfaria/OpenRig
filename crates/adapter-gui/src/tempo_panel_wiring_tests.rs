use super::*;

fn ids() -> Vec<ChainId> {
    vec![ChainId("rig:guitar".into()), ChainId("rig:bass".into())]
}

#[test]
fn storing_names_the_chain_at_the_clicked_row() {
    let cmd = preset_bpm_command(&ids(), 1, Some(97.0));
    assert!(matches!(
        cmd,
        Some(Command::Selection(SelectionCommand::SetRigPresetBpm { chain, bpm: Some(b) }))
            if chain.0 == "rig:bass" && b == 97.0
    ));
}

#[test]
fn clearing_sends_no_tempo() {
    let cmd = preset_bpm_command(&ids(), 0, None);
    assert!(matches!(
        cmd,
        Some(Command::Selection(SelectionCommand::SetRigPresetBpm { chain, bpm: None }))
            if chain.0 == "rig:guitar"
    ));
}

#[test]
fn a_row_that_is_gone_sends_nothing() {
    assert!(preset_bpm_command(&ids(), 2, Some(97.0)).is_none());
    assert!(preset_bpm_command(&ids(), -1, Some(97.0)).is_none());
}
