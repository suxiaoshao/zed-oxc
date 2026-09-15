use crate::binary_resolver::{declares_package, inspect};
use log::debug;
use zed_extension_api::serde_json::Value;
use zed_extension_api::{Command, EnvVars, Result};

pub const LAUNCH_SCRIPT: &str = include_str!("launch_vp.js");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BinarySource {
    #[default]
    Auto,
    VitePlus,
    Oxc,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub source: BinarySource,
    pub vp_path: Option<String>,
}

impl Options {
    pub fn from_settings(options: Option<&Value>) -> Result<Self> {
        let source = match options.and_then(|v| v.get("binarySource")) {
            None | Some(Value::Null) => BinarySource::Auto,
            Some(Value::String(value)) if value == "auto" => BinarySource::Auto,
            Some(Value::String(value)) if value == "vite-plus" => BinarySource::VitePlus,
            Some(Value::String(value)) if value == "oxc" => BinarySource::Oxc,
            _ => return Err("binarySource must be auto, vite-plus, or oxc.".into()),
        };
        if source == BinarySource::Oxc {
            return Ok(Self { source, vp_path: None });
        }
        let vp_path = match options.and_then(|v| v.get("vpPath")) {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if !value.is_empty() => Some(value.clone()),
            _ => return Err("vpPath must be a non-empty executable path.".into()),
        };
        Ok(Self { source, vp_path })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Project {
    pub root: String,
    pub vp_path: Option<String>,
}

impl Project {
    pub fn language_server_command(
        &self,
        node: String,
        worktree_root: &str,
        tool: &str,
        env: EnvVars,
    ) -> Result<Command> {
        let executable = if let Some(path) = &self.vp_path {
            // Explicit relative paths are relative to the opened worktree.
            inspect(&node, "executable", worktree_root, path, &env)?
        } else {
            inspect(&node, "global", worktree_root, "vp", &env)?
        };
        let path = executable["path"].as_str().ok_or_else(|| {
            format!(
                "Vite+ selected for {} but vp was not found. Install dependencies (for example, pnpm install), or set initialization_options.settings.vpPath, then restart the language server.",
                self.root
            )
        })?;
        let loader = if executable["node"] == true { "node" } else { "native" };
        debug!("Starting vp {tool} --lsp from {path} in {}", self.root);
        Ok(Command {
            command: node,
            args: ["-e", LAUNCH_SCRIPT, "--", &self.root, path, loader, tool]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            env,
        })
    }
}

/// Port of RFC #1614's identity and local resolution phases. The filesystem
/// bridge supplies ancestors from the worktree through the monorepo root.
pub fn detect_project(directories: &[Value], options: &Options) -> Option<Project> {
    if options.source == BinarySource::Oxc {
        return None;
    }
    let forced = options.source == BinarySource::VitePlus || options.vp_path.is_some();
    let index = if forced {
        directories.iter().position(|dir| dir["package"].is_object()).unwrap_or(0)
    } else {
        directories.iter().position(|dir| declares_package(&dir["package"], "vite-plus"))?
    };
    let root = directories.get(index)?["root"].as_str()?.to_owned();
    let vp_path = options.vp_path.clone().or_else(|| {
        directories[index..].iter().find_map(|dir| dir["vp"].as_str().map(str::to_owned))
    });
    Some(Project { root, vp_path })
}

#[cfg(test)]
mod tests;
