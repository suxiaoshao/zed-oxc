use crate::binary_resolver;
use crate::vite_plus::{self, Options};
use log::debug;
use std::{collections::BTreeMap, env};
use zed_extension_api::serde_json::{Value, json};
use zed_extension_api::settings::LspSettings;
use zed_extension_api::{
    Command, EnvVars, LanguageServerId, LanguageServerInstallationStatus, Result, Worktree,
    node_binary_path, npm_install_package, npm_package_installed_version,
    npm_package_latest_version, set_language_server_installation_status,
};

pub const OXLINT_SERVER_ID: &str = "oxlint";
pub const OXFMT_SERVER_ID: &str = "oxfmt";

pub trait ZedLspSupport {
    fn package_name(&self) -> &'static str;
    fn sources(&self) -> &BTreeMap<u64, bool>;
    fn sources_mut(&mut self) -> &mut BTreeMap<u64, bool>;

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        // Restarts recheck settings and installs, including after startup failures.
        self.sources_mut().remove(&worktree.id());
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        let env =
            server_env(&settings, worktree.shell_env(), &worktree.root_path(), self.package_name());

        // Honor complete overrides before resolving Node or downloading a fallback.
        if let Some(command) = custom_command(&settings, env.clone())? {
            self.sources_mut().insert(worktree.id(), false);
            return Ok(command);
        }

        let options = Options::from_settings(configured_settings(&settings).as_ref())?;
        let node = node_binary_path()?;
        let directories = binary_resolver::inspect(
            &node,
            "ancestors",
            &worktree.root_path(),
            self.package_name(),
            &env,
        )?;
        let directories = directories.as_array().ok_or("Expected a list of project directories")?;

        if let Some(project) = vite_plus::detect_project(directories, &options) {
            let tool = if self.package_name() == OXLINT_SERVER_ID { "lint" } else { "fmt" };
            let command =
                project.language_server_command(node, &worktree.root_path(), tool, env)?;
            self.sources_mut().insert(worktree.id(), true);
            return Ok(command);
        }

        let path = binary_resolver::standalone_path(directories, self.package_name());
        let path = if let Some(path) = path {
            path.to_owned()
        } else {
            self.update_extension_language_server_if_outdated(language_server_id)?;
            env::current_dir()
                .map_err(|err| err.to_string())?
                .join("node_modules")
                .join(self.package_name())
                .join("bin")
                .join(self.package_name())
                .to_string_lossy()
                .into_owned()
        };
        debug!("Starting {} --lsp from {path}", self.package_name());
        self.sources_mut().insert(worktree.id(), false);
        Ok(Command { command: node, args: vec![path, "--lsp".into()], env })
    }

    fn known_source(&self, settings: &LspSettings, worktree_id: u64) -> Option<bool> {
        // Zed launches custom binaries without calling language_server_command,
        // so an override must win even when a previous Vite+ selection is cached.
        if settings.binary.as_ref().and_then(|binary| binary.path.as_ref()).is_some() {
            return Some(false);
        }
        self.sources().get(&worktree_id).copied()
    }

    fn uses_vite_plus(&self, settings: &LspSettings, worktree: &Worktree) -> Result<bool> {
        if let Some(source) = self.known_source(settings, worktree.id()) {
            // Configuration follows the running command until a server restart.
            return Ok(source);
        }
        // Zed may request configuration before requesting a command.
        let options = Options::from_settings(configured_settings(settings).as_ref())?;
        let directories = binary_resolver::inspect(
            &node_binary_path()?,
            "ancestors",
            &worktree.root_path(),
            self.package_name(),
            &worktree.shell_env(),
        )?;
        Ok(vite_plus::detect_project(
            directories.as_array().ok_or("Expected a list of project directories")?,
            &options,
        )
        .is_some())
    }

    fn language_server_initialization_options(
        &self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        let vite_plus = self.uses_vite_plus(&settings, worktree)?;
        Ok(initialization_options(&settings, self.package_name(), vite_plus))
    }

    fn language_server_workspace_configuration(
        &self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)?;
        let vite_plus = self.uses_vite_plus(&settings, worktree)?;
        Ok(workspace_configuration(&settings, self.package_name(), vite_plus))
    }

    fn update_extension_language_server_if_outdated(
        &self,
        language_server_id: &LanguageServerId,
    ) -> Result<()> {
        set_language_server_installation_status(
            language_server_id,
            &LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let package_name = self.package_name();
        let current_version = npm_package_installed_version(package_name)?;
        let latest_version = npm_package_latest_version(package_name)?;
        if current_version.as_deref() != Some(latest_version.as_str()) {
            set_language_server_installation_status(
                language_server_id,
                &LanguageServerInstallationStatus::Downloading,
            );
            npm_install_package(package_name, &latest_version)?;
        }
        set_language_server_installation_status(
            language_server_id,
            &LanguageServerInstallationStatus::None,
        );
        Ok(())
    }
}

fn custom_command(settings: &LspSettings, env: EnvVars) -> Result<Option<Command>> {
    let Some(binary) = &settings.binary else {
        return Ok(None);
    };
    match (&binary.path, &binary.arguments) {
        (Some(path), args) => {
            Ok(Some(Command { command: path.clone(), args: args.clone().unwrap_or_default(), env }))
        }
        (None, None) => Ok(None),
        _ => Err("When supplying binary.arguments, binary.path must be supplied.".into()),
    }
}

fn server_env(settings: &LspSettings, shell: EnvVars, root: &str, tool: &str) -> EnvVars {
    let mut env = BTreeMap::new();
    let overrides = settings.binary.as_ref().and_then(|binary| binary.env.as_ref());
    for (key, value) in shell.into_iter().chain(
        overrides.into_iter().flat_map(|env| env.iter().map(|(k, v)| (k.clone(), v.clone()))),
    ) {
        env.insert(if key.eq_ignore_ascii_case("PATH") { "PATH".into() } else { key }, value);
    }
    if tool == OXLINT_SERVER_ID
        && let Some(path) = env.get_mut("OXLINT_TSGOLINT_PATH")
    {
        // WASI paths use Unix syntax even when the host is Windows.
        let absolute = path.starts_with('/')
            || path.starts_with('\\')
            || path.as_bytes().get(1) == Some(&b':');
        if !absolute {
            *path = format!("{root}/{path}");
        }
    }
    env.into_iter().collect()
}

fn configured_settings(settings: &LspSettings) -> Option<Value> {
    let mut config =
        settings.initialization_options.as_ref().and_then(|v| v.get("settings")).cloned();
    // Zed's workspace settings override the matching initialization settings.
    // Both callbacks use this merge; Zed subsequently reapplies the user's
    // initialization_options before sending the initialize request.
    if let Some(workspace) = &settings.settings {
        match (&mut config, workspace) {
            (Some(Value::Object(target)), Value::Object(overrides)) => {
                target.extend(overrides.clone());
            }
            _ => config = Some(workspace.clone()),
        }
    }
    config
}

fn initialization_options(settings: &LspSettings, tool: &str, vite_plus: bool) -> Option<Value> {
    let mut options = settings.initialization_options.clone();
    if let Some(config) = workspace_configuration(settings, tool, vite_plus) {
        let options = options.get_or_insert_with(|| json!({}));
        if !options.is_object() {
            *options = json!({});
        }
        options["settings"] = config;
    }
    options
}

fn workspace_configuration(settings: &LspSettings, tool: &str, vite_plus: bool) -> Option<Value> {
    let mut config = configured_settings(settings);
    if let Some(Value::Object(settings)) = config.as_mut() {
        settings.remove("binarySource");
        settings.remove("vpPath");
    }
    if vite_plus {
        force_nested_config(config.get_or_insert_with(|| json!({})), tool);
    }
    config
}

fn force_nested_config(settings: &mut Value, tool: &str) {
    if !settings.is_object() {
        *settings = json!({});
    }
    let key =
        if tool == OXLINT_SERVER_ID { "disableNestedConfig" } else { "fmt.disableNestedConfig" };
    settings[key] = true.into();
}

#[cfg(test)]
mod tests {
    use super::*;
    use zed_extension_api::serde_json::from_value;

    #[test]
    fn custom_commands_override_invalid_source_settings_and_allow_env_only_settings() {
        let settings = from_value(json!({
            "binary": {"path":"/custom/oxlint", "arguments":["--lsp"]},
            "initialization_options": {"settings":{"binarySource":"invalid", "vpPath":"missing"}}
        }))
        .unwrap();
        let command =
            custom_command(&settings, vec![("TEST".into(), "value".into())]).unwrap().unwrap();
        assert_eq!(command.command, "/custom/oxlint");
        assert_eq!(command.args, ["--lsp"]);
        assert_eq!(command.env, [("TEST".into(), "value".into())]);
        let path_only = from_value(json!({"binary":{"path":"/custom/wrapper"}})).unwrap();
        let command = custom_command(&path_only, vec![]).unwrap().unwrap();
        assert_eq!(command.command, "/custom/wrapper");
        assert!(command.args.is_empty());
        assert!(custom_command(
            &from_value(json!({"binary":{"arguments":["--lsp"]}})).unwrap(), vec![],
        ).is_err());
        assert!(
            custom_command(
                &from_value(json!({"binary":{"env":{"TEST":"value"}}})).unwrap(),
                vec![]
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn vite_plus_overrides_nested_config_at_initialization_and_configuration() {
        for (tool, key) in [
            (OXLINT_SERVER_ID, "disableNestedConfig"),
            (OXFMT_SERVER_ID, "fmt.disableNestedConfig"),
        ] {
            let original = json!({"settings":{"binarySource":"vite-plus", "vpPath":"/custom/vp", key:false, "run":"onSave", "configPath":"custom.json"}});
            let settings = from_value(json!({"initialization_options":original})).unwrap();
            let options = initialization_options(&settings, tool, true).unwrap();
            assert_eq!(options["settings"][key], true);
            assert_eq!(options["settings"]["run"], "onSave");
            assert!(options["settings"].get("binarySource").is_none());
            assert!(options["settings"].get("vpPath").is_none());
            assert_eq!(original["settings"][key], false);
            let config = workspace_configuration(&from_value(json!({"initialization_options":original, "settings":{key:false, "run":"onType"}})).unwrap(), tool, true).unwrap();
            assert_eq!(config[key], true);
            assert_eq!(config["run"], "onType");
            assert_eq!(config["configPath"], "custom.json");
            assert!(config.get("binarySource").is_none());
            assert!(config.get("vpPath").is_none());
            for value in [None, Some(Value::Null), Some(json!({"settings":null}))] {
                assert_eq!(
                    initialization_options(
                        &LspSettings { initialization_options: value, ..Default::default() },
                        tool,
                        true
                    )
                    .unwrap()["settings"][key],
                    true
                );
            }
        }
    }

    #[test]
    fn standalone_configuration_preserves_user_values() {
        let original =
            json!({"settings":{"disableNestedConfig":false, "fmt.disableNestedConfig":false}});
        let settings = from_value(json!({"initialization_options":original})).unwrap();
        assert_eq!(
            initialization_options(&settings, OXLINT_SERVER_ID, false),
            Some(original.clone())
        );
        assert_eq!(
            workspace_configuration(&settings, OXLINT_SERVER_ID, false),
            Some(original["settings"].clone())
        );
        assert_eq!(initialization_options(&LspSettings::default(), OXLINT_SERVER_ID, false), None);
    }

    #[test]
    fn source_selection_reads_nested_settings_and_workspace_overrides() {
        use crate::vite_plus::BinarySource;
        for tool in [OXLINT_SERVER_ID, OXFMT_SERVER_ID] {
            let mut settings: LspSettings = from_value(json!({
                "initialization_options": {
                    "binarySource":"oxc", "vpPath":"ignored-top-level-path",
                    "settings":{"binarySource":"vite-plus", "vpPath":"local/vp", "run":"onSave"}
                }
            }))
            .unwrap();
            assert_eq!(
                Options::from_settings(configured_settings(&settings).as_ref()).unwrap(),
                Options { source: BinarySource::VitePlus, vp_path: Some("local/vp".into()) }
            );
            settings.settings = Some(json!({"vpPath":"other/vp", "run":"onType"}));
            assert_eq!(
                Options::from_settings(configured_settings(&settings).as_ref()).unwrap(),
                Options { source: BinarySource::VitePlus, vp_path: Some("other/vp".into()) }
            );
            let options = initialization_options(&settings, tool, true).unwrap();
            let config = workspace_configuration(&settings, tool, true).unwrap();
            assert_eq!(options["settings"], config);
            assert_eq!(config["run"], "onType");
            assert!(config.get("binarySource").is_none());
            assert!(config.get("vpPath").is_none());

            settings.settings = Some(json!({"binarySource":"oxc", "vpPath":false}));
            assert_eq!(
                Options::from_settings(configured_settings(&settings).as_ref()).unwrap(),
                Options { source: BinarySource::Oxc, vp_path: None }
            );
            let config = workspace_configuration(&settings, tool, false).unwrap();
            assert_eq!(config, json!({"run":"onSave"}));
            assert_eq!(settings.initialization_options.unwrap()["settings"]["vpPath"], "local/vp");
        }

        let mut settings: LspSettings = from_value(json!({
            "initialization_options":{"binarySource":"vite-plus", "vpPath":"ignored"}
        }))
        .unwrap();
        assert_eq!(
            Options::from_settings(configured_settings(&settings).as_ref()).unwrap(),
            Options::default()
        );
        settings.settings = Some(json!({"binarySource":"vite-plus", "vpPath":"workspace/vp"}));
        assert_eq!(
            Options::from_settings(configured_settings(&settings).as_ref()).unwrap(),
            Options { source: BinarySource::VitePlus, vp_path: Some("workspace/vp".into()) }
        );
        settings.settings = Some(json!({"binarySource":"invalid"}));
        assert!(Options::from_settings(configured_settings(&settings).as_ref()).is_err());
    }

    #[test]
    fn tool_sources_are_separate_for_each_worktree() {
        let mut lint = crate::oxlint::ZedOxlintLsp::default();
        let mut fmt = crate::oxfmt::ZedOxfmtLsp::default();
        lint.sources_mut().insert(1, true);
        lint.sources_mut().insert(2, false);
        fmt.sources_mut().insert(1, false);
        assert_eq!(lint.sources().get(&1), Some(&true));
        assert_eq!(lint.sources().get(&2), Some(&false));
        assert_eq!(fmt.sources().get(&1), Some(&false));
    }

    #[test]
    fn custom_binary_overrides_cached_vite_plus_configuration() {
        let mut lint = crate::oxlint::ZedOxlintLsp::default();
        let mut fmt = crate::oxfmt::ZedOxfmtLsp::default();
        for server in [&mut lint as &mut dyn ZedLspSupport, &mut fmt] {
            server.sources_mut().insert(1, true);
            assert_eq!(server.known_source(&LspSettings::default(), 1), Some(true));
            for binary in [
                json!({"path":"/custom/wrapper"}),
                json!({"path":"/custom/oxlint", "arguments":["--lsp"]}),
            ] {
                let original = json!({"settings": {
                    "binarySource":"invalid", "vpPath":false,
                    "disableNestedConfig":false, "fmt.disableNestedConfig":false,
                }});
                let settings =
                    from_value(json!({"binary":binary,"initialization_options":original})).unwrap();
                // Custom binaries bypass the command callback, including the
                // cache reset, on both the first start and subsequent restarts.
                for worktree_id in [1, 2] {
                    let source = server.known_source(&settings, worktree_id).unwrap();
                    assert!(!source);
                    let config =
                        workspace_configuration(&settings, server.package_name(), source).unwrap();
                    assert_eq!(config["disableNestedConfig"], false);
                    assert_eq!(config["fmt.disableNestedConfig"], false);
                }
            }
        }
    }

    #[test]
    fn environment_overrides_preserve_shell_and_resolve_tsgolint_paths() {
        let settings = from_value(json!({"binary":{"env":{"Path":"/custom/bin", "OXLINT_TSGOLINT_PATH":"tools/tsgolint"}}})).unwrap();
        let shell =
            vec![("PATH".into(), "/shell/bin".into()), ("SHELL_SETTING".into(), "kept".into())];
        let env: BTreeMap<_, _> =
            server_env(&settings, shell, "/repo", OXLINT_SERVER_ID).into_iter().collect();
        assert_eq!(env["PATH"], "/custom/bin");
        assert_eq!(env["SHELL_SETTING"], "kept");
        assert_eq!(env["OXLINT_TSGOLINT_PATH"], "/repo/tools/tsgolint");
        assert!(!env.contains_key("Path"));
        for path in ["/absolute/tsgolint", r"C:\tools\tsgolint.exe", r"\\server\share\tsgolint.exe"]
        {
            let settings =
                from_value(json!({"binary":{"env":{"OXLINT_TSGOLINT_PATH":path}}})).unwrap();
            assert_eq!(
                server_env(&settings, vec![], "/repo", OXLINT_SERVER_ID),
                [("OXLINT_TSGOLINT_PATH".into(), path.into())]
            );
        }
    }
}
