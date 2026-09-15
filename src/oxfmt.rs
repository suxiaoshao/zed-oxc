use crate::lsp::{ZedLspSupport, apply_env_overrides};
use log::debug;
use zed_extension_api::serde_json::Value;
use zed_extension_api::settings::LspSettings;
use zed_extension_api::{Command, LanguageServerId, Result, Worktree};

pub struct ZedOxfmtLsp {}

impl ZedOxfmtLsp {
    pub fn new() -> Self {
        ZedOxfmtLsp {}
    }
}

impl ZedLspSupport for ZedOxfmtLsp {
    fn get_package_name(&self) -> String {
        "oxfmt".to_string()
    }

    fn language_server_command(
        &self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        debug!("Oxfmt language_server_command LspSettings: {settings:?}");

        if let Some(binary) = settings.binary {
            if (binary.path.is_some() && binary.arguments.is_none())
                || (binary.path.is_none() && binary.arguments.is_some())
            {
                return Err(
                    "When supplying binary.arguments, binary.path must be supplied (or vice-versa).".to_string(),
                );
            }

            let has_env = binary.env.is_some();
            let env = binary.env.unwrap_or_default().into_iter().collect();
            if let (Some(command), Some(args)) = (binary.path, binary.arguments) {
                return Ok(Command { command, args, env });
            }

            let mut command =
                self.get_default_language_server_command(language_server_id, worktree)?;
            if has_env {
                apply_env_overrides(&mut command.env, env);
            }
            return Ok(command);
        }

        self.get_default_language_server_command(language_server_id, worktree)
    }

    fn language_server_initialization_options(
        &self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        debug!("Oxfmt language_server_initialization_options LspSettings: {settings:?}");

        Ok(settings.initialization_options)
    }

    fn language_server_workspace_configuration(
        &self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        debug!("Oxfmt language_server_workspace_configuration LspSettings: {settings:?}");

        Ok(settings.initialization_options.as_ref().and_then(|v| v.get("settings").cloned()))
    }
}
