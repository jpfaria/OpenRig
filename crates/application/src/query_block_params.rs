//! Responsibility: reports the parameters one block exposes.

/// #572: list of materialised `BlockParameterDescriptor` for one placed
/// block (schema + `current_value` per parameter), serialised under a
/// `params` envelope. Mirrors what `list_chain_presets` does for the
/// preset bank — looks up the chain in the project, finds the block,
/// then delegates to `AudioBlock::parameter_descriptors()` (the same
/// helper the GUI uses) so the schema + current-value walk lives in
/// `project::block`, never re-derived per transport. Unknown chain /
/// block / schema mismatch → `Err`.
pub fn get_block_params(
    project: &project::project::Project,
    chain: &domain::ids::ChainId,
    block: &domain::ids::BlockId,
) -> Result<String, String> {
    let chain_ref = project
        .chains
        .iter()
        .find(|c| c.id == *chain)
        .ok_or_else(|| format!("chain not found: {}", chain.0))?;
    let block_ref = chain_ref
        .blocks
        .iter()
        .find(|b| b.id == *block)
        .ok_or_else(|| format!("block not found in chain {}: {}", chain.0, block.0))?;
    let descriptors = block_ref.parameter_descriptors()?;
    let payload = serde_json::to_string(&descriptors)
        .map_err(|e| format!("failed to serialize descriptors: {e}"))?;
    Ok(format!("{{\"params\": {payload}}}"))
}
