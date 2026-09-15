use zed_extension_api::serde_json::{Value, from_slice};
use zed_extension_api::{EnvVars, Result, process};

pub const INSPECT_SCRIPT: &str = include_str!("inspect.js");

pub fn standalone_path<'a>(directories: &'a [Value], package: &str) -> Option<&'a str> {
    let index = directories.iter().position(|dir| declares_package(&dir["package"], package))?;
    directories[index..].iter().find_map(|dir| dir["standalone"].as_str())
}

pub fn declares_package(package: &Value, name: &str) -> bool {
    ["dependencies", "devDependencies"]
        .iter()
        .any(|key| package[*key][name].as_str().is_some_and(|version| !version.is_empty()))
}

pub fn inspect(node: &str, mode: &str, path: &str, tool: &str, env: &EnvVars) -> Result<Value> {
    let output = process::Command::new(node)
        .args(["-e", INSPECT_SCRIPT, "--", mode, path, tool])
        .envs(env.clone())
        .output()?;
    if output.status != Some(0) {
        return Err(format!(
            "Could not inspect the {tool} installation: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    from_slice(&output.stdout).map_err(|err| format!("Invalid installation information: {err}"))
}
