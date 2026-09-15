mod binary_resolver;
mod lsp;
mod oxfmt;
mod oxlint;
mod vite_plus;

use crate::lsp::{OXFMT_SERVER_ID, OXLINT_SERVER_ID, ZedLspSupport};
use crate::oxfmt::ZedOxfmtLsp;
use crate::oxlint::ZedOxlintLsp;
use log::Level;
use simple_logger::init_with_level;
use zed_extension_api::{
    Command, Extension, LanguageServerId, Result, Worktree, register_extension, serde_json::Value,
};

struct OxcExtension {
    oxfmt_lsp: ZedOxfmtLsp,
    oxlint_lsp: ZedOxlintLsp,
}

impl OxcExtension {
    fn server(&mut self, id: &LanguageServerId) -> Result<&mut dyn ZedLspSupport> {
        match id.as_ref() {
            OXFMT_SERVER_ID => Ok(&mut self.oxfmt_lsp),
            OXLINT_SERVER_ID => Ok(&mut self.oxlint_lsp),
            _ => Err(format!("Unsupported language server id: {id:?}")),
        }
    }
}

impl Extension for OxcExtension {
    fn new() -> Self {
        init_with_level(Level::Debug).unwrap();
        Self { oxfmt_lsp: ZedOxfmtLsp::default(), oxlint_lsp: ZedOxlintLsp::default() }
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        self.server(id)?.language_server_command(id, worktree)
    }

    fn language_server_initialization_options(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        self.server(id)?.language_server_initialization_options(id, worktree)
    }

    fn language_server_workspace_configuration(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        self.server(id)?.language_server_workspace_configuration(id, worktree)
    }
}

register_extension!(OxcExtension);
