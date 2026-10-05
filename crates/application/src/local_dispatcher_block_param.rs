//! Responsibility: handles the block param commands.
//! Block-parameter handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::command::{BlockCommand, Command};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::local_dispatcher_ir_reseed::reseed_ir_output_db;
use crate::local_dispatcher_tempo::release_sync_on_manual_edit;

impl LocalDispatcher {
    /// Block-parameter commands: set/select a single parameter on a block.
    pub(crate) fn handle_block_param(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Block(BlockCommand::SetBlockParameterNumber {
                chain,
                block,
                path,
                value,
            }) => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_number(b, &path, value)?;
                    reseed_ir_output_db(b, &path);
                    release_sync_on_manual_edit(b, &path);
                    Ok(())
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::Block(BlockCommand::SetBlockParameterBool {
                chain,
                block,
                path,
                value,
            }) => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_bool(b, &path, value)
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::Block(BlockCommand::SetBlockParameterText {
                chain,
                block,
                path,
                value,
            }) => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_text(b, &path, &value)?;
                    reseed_ir_output_db(b, &path);
                    Ok(())
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::Block(BlockCommand::SelectBlockParameterOption {
                chain,
                block,
                path,
                value,
                index: _,
            }) => {
                let bpm = self.metronome_snapshot().settings.bpm;
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_option(b, &path, &value)?;
                    reseed_ir_output_db(b, &path);
                    if block_core::tempo_sync::synced_value_path(&path).is_some() {
                        project::tempo_retime::retime_block(b, bpm);
                    }
                    Ok(())
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::Block(BlockCommand::PickBlockParameterFile {
                chain,
                block,
                path,
                file,
            }) => {
                self.with_block(&chain, &block, |b| {
                    let file_str = file.to_string_lossy();
                    project::block::param_writer::set_parameter_text(b, &path, file_str.as_ref())
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            other => unreachable!("handle_block_param received non-param command: {other:?}"),
        }
    }
}
