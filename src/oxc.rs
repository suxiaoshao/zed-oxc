mod lsp;
mod oxfmt;
mod oxlint;

use crate::lsp::{OXFMT_SERVER_ID, OXLINT_SERVER_ID, ZedLspSupport};
use crate::oxfmt::ZedOxfmtLsp;
use crate::oxlint::ZedOxlintLsp;
use log::Level;
use serde_json::Value;
use simple_logger::init_with_level;
use std::sync::{Arc, RwLock};
use zed_extension_api::{
    Command, Extension, LanguageServerId, Result, Worktree, register_extension,
    serde_json::{self},
};

struct OxcExtension {
    oxfmt_lsp: Arc<RwLock<ZedOxfmtLsp>>,
    oxlint_lsp: Arc<RwLock<ZedOxlintLsp>>,
}

impl OxcExtension {
    fn is_oxfmt_language_server(&self, language_server_id: &LanguageServerId) -> bool {
        language_server_id.as_ref() == OXFMT_SERVER_ID
    }

    fn is_oxlint_language_server(&self, language_server_id: &LanguageServerId) -> bool {
        language_server_id.as_ref() == OXLINT_SERVER_ID
    }
}

impl Extension for OxcExtension {
    fn new() -> Self
    where
        Self: Sized,
    {
        init_with_level(Level::Debug).unwrap();

        Self {
            oxfmt_lsp: Arc::new(RwLock::new(ZedOxfmtLsp::new())),
            oxlint_lsp: Arc::new(RwLock::new(ZedOxlintLsp::new())),
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        if self.is_oxfmt_language_server(language_server_id) {
            return self
                .oxfmt_lsp
                .read()
                .unwrap()
                .language_server_command(language_server_id, worktree);
        }

        if self.is_oxlint_language_server(language_server_id) {
            return self
                .oxlint_lsp
                .read()
                .unwrap()
                .language_server_command(language_server_id, worktree);
        }

        Err(format!("Unsupported language server id: {language_server_id:?}"))
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        if self.is_oxfmt_language_server(language_server_id) {
            return self
                .oxfmt_lsp
                .read()
                .unwrap()
                .language_server_initialization_options(language_server_id, worktree);
        }

        if self.is_oxlint_language_server(language_server_id) {
            return self
                .oxlint_lsp
                .read()
                .unwrap()
                .language_server_initialization_options(language_server_id, worktree);
        }

        Err(format!("Unsupported language server id: {language_server_id:?}"))
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        if self.is_oxfmt_language_server(language_server_id) {
            return self
                .oxfmt_lsp
                .read()
                .unwrap()
                .language_server_workspace_configuration(language_server_id, worktree);
        }

        if self.is_oxlint_language_server(language_server_id) {
            return self
                .oxlint_lsp
                .read()
                .unwrap()
                .language_server_workspace_configuration(language_server_id, worktree);
        }

        Err(format!("Unsupported language server id: {language_server_id:?}"))
    }
}

register_extension!(OxcExtension);
