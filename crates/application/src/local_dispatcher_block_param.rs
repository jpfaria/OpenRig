//! Block-parameter handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::local_dispatcher_ir_reseed::reseed_ir_output_db;

impl LocalDispatcher {
    /// Block-parameter commands: set/select a single parameter on a block.
    pub(crate) fn handle_block_param(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SetBlockParameterNumber {
                chain,
                block,
                path,
                value,
            } => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_number(b, &path, value)?;
                    reseed_ir_output_db(b, &path);
                    Ok(())
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::SetBlockParameterBool {
                chain,
                block,
                path,
                value,
            } => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_bool(b, &path, value)
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::SetBlockParameterText {
                chain,
                block,
                path,
                value,
            } => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_text(b, &path, &value)?;
                    reseed_ir_output_db(b, &path);
                    Ok(())
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::SelectBlockParameterOption {
                chain,
                block,
                path,
                value,
                index: _,
            } => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_option(b, &path, &value)?;
                    reseed_ir_output_db(b, &path);
                    Ok(())
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::PickBlockParameterFile {
                chain,
                block,
                path,
                file,
            } => {
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
