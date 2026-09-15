use super::*;
use crate::binary_resolver::{INSPECT_SCRIPT, standalone_path};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
use zed_extension_api::serde_json::{from_slice, json};

const NPM_SHELL_SHIM: &str = include_str!("fixtures/npm-vp.sh");
const NPM_CMD_SHIM: &str = include_str!("fixtures/npm-vp.cmd");

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "oxc-zed-vite-plus-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn put(&self, name: &str, content: &str) {
        let file = self.0.join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, content).unwrap();
    }

    fn install(&self, dir: &str, package: &str, bin: &str) {
        self.put(
            &format!("{dir}/node_modules/{package}/package.json"),
            &json!({"name": package}).to_string(),
        );
        self.put(
            &format!("{dir}/node_modules/{package}/bin/{bin}"),
            "#!/usr/bin/env node\nconsole.log('vp');",
        );
    }

    fn path(&self, path: &str) -> String {
        self.0.join(path).to_string_lossy().into_owned()
    }

    fn directories(&self, start: &str) -> Vec<Value> {
        probe("ancestors", &self.path(start), "oxlint", "").as_array().unwrap().clone()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn node() -> &'static str {
    static NODE: OnceLock<String> = OnceLock::new();
    NODE.get_or_init(|| {
        let output = Command::new("node")
            .args(["-p", "process.execPath"])
            .output()
            .expect("Node is required for resolver tests");
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().into()
    })
}

fn probe(mode: &str, root: &str, tool: &str, search_path: &str) -> Value {
    let output = Command::new(node())
        .args(["-e", INSPECT_SCRIPT, "--", mode, root, tool])
        .env("PATH", search_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    from_slice(&output.stdout).unwrap()
}

// Names and expected roots mirror the complete fixture table in RFC #1614.
#[test]
fn rfc_conformance_fixtures() {
    for name in [
        "root-declared-and-installed",
        "pnpm-subpackage-declared-root-hoisted",
        "npm-subpackage-direct-dep-unhoisted",
        "root-declared-no-local-no-global",
        "root-declared-no-local-global-on-path",
        "transitive-install",
        "global-vp-without-declaration",
        "parent-vite-plus-nested-repo",
        "plain-non-vite-plus",
        "yarn4-pnp",
    ] {
        let tree = Tree::new();
        tree.put("repo/package.json", r#"{"workspaces": ["packages/*"]}"#);
        let mut start = "repo";
        let mut root = "repo";
        let mut install = None;
        let mut global = false;
        let mut declared = true;
        match name {
            "root-declared-and-installed" => install = Some("repo"),
            "pnpm-subpackage-declared-root-hoisted" => {
                tree.put("repo/pnpm-workspace.yaml", "packages: ['packages/*']");
                start = "repo/packages/app";
                root = start;
                install = Some("repo");
            }
            "npm-subpackage-direct-dep-unhoisted" => {
                start = "repo/packages/app";
                root = start;
                install = Some(start);
            }
            "root-declared-no-local-no-global" => {}
            "yarn4-pnp" => tree.put(
                "repo/.pnp.cjs",
                "throw new Error('Project loaders must not run during detection');",
            ),
            "root-declared-no-local-global-on-path" => global = true,
            "transitive-install" => {
                declared = false;
                install = Some("repo");
            }
            "global-vp-without-declaration" => {
                declared = false;
                global = true;
            }
            "parent-vite-plus-nested-repo" => {
                tree.put("package.json", r#"{"dependencies":{"vite-plus":"*"}}"#);
                tree.install(".", "vite-plus", "vp");
                declared = false;
            }
            "plain-non-vite-plus" => declared = false,
            _ => unreachable!(),
        }
        if declared {
            let package = if root == "repo" {
                json!({"devDependencies": {"vite-plus": "*"}, "workspaces": ["packages/*"]})
            } else {
                // A subpackage is not a workspace boundary.
                json!({"dependencies": {"vite-plus": "*"}})
            };
            tree.put(&format!("{root}/package.json"), &package.to_string());
        }
        if let Some(dir) = install {
            tree.install(dir, "vite-plus", "vp");
        }
        if global {
            tree.put("global/vp", "#!/usr/bin/env node\n");
        }
        let project = detect_project(&tree.directories(start), &Options::default());
        if !declared {
            assert_eq!(project, None, "{name}");
            continue;
        }
        let project = project.unwrap();
        assert_eq!(project.root, tree.path(root), "{name}");
        if let Some(dir) = install {
            assert_eq!(
                project.vp_path,
                Some(tree.path(&format!("{dir}/node_modules/vite-plus/bin/vp"))),
                "{name}"
            );
        } else {
            assert_eq!(project.vp_path, None, "{name}");
            let executable = probe(
                "global",
                &tree.path(start),
                "vp",
                &if global { tree.path("global") } else { String::new() },
            );
            assert_eq!(!executable.is_null(), global, "{name}");
        }
    }
}

#[test]
fn boundaries_apply_to_both_identity_and_binary_lookup() {
    for (marker, content) in [
        ("pnpm-workspace.yaml", ""),
        ("lerna.json", "{}"),
        ("package.json", r#"{"workspaces":[]}"#),
    ] {
        let tree = Tree::new();
        tree.put("package.json", r#"{"dependencies":{"vite-plus":"*"}}"#);
        tree.install(".", "vite-plus", "vp");
        tree.put(&format!("repo/{marker}"), content);
        tree.put("repo/app/package.json", "{}");
        assert_eq!(
            detect_project(&tree.directories("repo/app"), &Options::default()),
            None,
            "{marker}"
        );
        tree.put("repo/app/package.json", r#"{"dependencies":{"vite-plus":"*"}}"#);
        let project = detect_project(&tree.directories("repo/app"), &Options::default()).unwrap();
        assert_eq!(project.root, tree.path("repo/app"));
        assert_eq!(project.vp_path, None, "{marker}");
    }
}

#[test]
fn detection_ignores_invalid_manifests_optional_and_peer_dependencies() {
    let tree = Tree::new();
    tree.put("repo/pnpm-workspace.yaml", "");
    for package in [
        "broken JSON",
        "null",
        "[]",
        r#"{"peerDependencies":{"vite-plus":"*"}}"#,
        r#"{"optionalDependencies":{"vite-plus":"*"}}"#,
        r#"{"dependencies":{"vite-plus":null}}"#,
    ] {
        tree.put("repo/package.json", package);
        tree.install("repo", "vite-plus", "vp");
        assert_eq!(
            detect_project(&tree.directories("repo"), &Options::default()),
            None,
            "{package}"
        );
    }
}

#[test]
fn install_validation_and_restart_recheck() {
    let tree = Tree::new();
    tree.put("repo/package.json", r#"{"workspaces":[],"dependencies":{"vite-plus":"*"}}"#);
    let detect = || detect_project(&tree.directories("repo"), &Options::default()).unwrap();
    assert_eq!(detect().vp_path, None);
    tree.install("repo", "vite-plus", "vp");
    assert!(detect().vp_path.is_some());
    tree.put("repo/node_modules/vite-plus/package.json", r#"{"name":"not-vite-plus"}"#);
    assert_eq!(detect().vp_path, None);
    tree.install("repo", "vite-plus", "vp");
    fs::remove_file(tree.0.join("repo/node_modules/vite-plus/bin/vp")).unwrap();
    assert_eq!(detect().vp_path, None);
}

#[test]
fn source_options_are_independent_and_explicit_paths_win() {
    let tree = Tree::new();
    tree.put("repo/package.json", r#"{"workspaces":[]}"#);
    tree.install("repo", "vite-plus", "vp");
    let dirs = tree.directories("repo");
    assert_eq!(detect_project(&dirs, &Options::default()), None);
    for source in [BinarySource::Auto, BinarySource::VitePlus, BinarySource::Oxc] {
        let options = Options { source, vp_path: Some("custom/vp".into()) };
        let project = detect_project(&dirs, &options);
        if source == BinarySource::Oxc {
            assert_eq!(project, None);
        } else {
            assert_eq!(project.unwrap().vp_path.as_deref(), Some("custom/vp"));
        }
    }
    assert!(
        detect_project(&dirs, &Options { source: BinarySource::VitePlus, vp_path: None }).is_some()
    );
    assert_eq!(Options::from_settings(None).unwrap(), Options::default());
    for value in
        [json!({"binarySource":"wrong"}), json!({"binarySource":false}), json!({"vpPath":""})]
    {
        assert!(Options::from_settings(Some(&value)).is_err());
    }
}

#[test]
fn explicit_paths_and_pnpm_shims_resolve_without_system_node() {
    let tree = Tree::new();
    tree.install("repo", "vite-plus", "vp");
    tree.put(
        "repo/node_modules/.bin/vp",
        &NPM_SHELL_SHIM.replace("{{target}}", "../vite-plus/bin/vp"),
    );
    let executable = probe("executable", &tree.path("repo"), "node_modules/.bin/vp", "");
    assert_eq!(executable["node"], true);
    assert_eq!(
        Path::new(executable["path"].as_str().unwrap()).canonicalize().unwrap(),
        tree.0.join("repo/node_modules/vite-plus/bin/vp").canonicalize().unwrap()
    );
    assert_eq!(probe("executable", &tree.path("repo"), "missing", ""), Value::Null);
    tree.put(
        "repo/vp.cmd",
        &NPM_CMD_SHIM.replace("{{target}}", "%dp0%/node_modules/vite-plus/bin/vp"),
    );
    assert_eq!(probe("executable", &tree.path("repo"), "vp.cmd", "")["node"], true);
}

#[test]
fn global_relative_path_entries_use_the_worktree_directory() {
    let tree = Tree::new();
    tree.put("repo/tools/vp", "#!/usr/bin/env node\n");
    tree.put("host/tools/vp", "#!/usr/bin/env node\n");
    let search_path = std::env::join_paths([Path::new("missing"), Path::new("tools")]).unwrap();
    let output = Command::new(node())
        .args(["-e", INSPECT_SCRIPT, "--", "global", &tree.path("repo"), "vp"])
        .current_dir(tree.path("host"))
        .env("PATH", search_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let executable: Value = from_slice(&output.stdout).unwrap();
    assert_eq!(
        Path::new(executable["path"].as_str().unwrap()).canonicalize().unwrap(),
        tree.0.join("repo/tools/vp").canonicalize().unwrap()
    );
}

#[test]
fn custom_wrappers_preserve_environment_and_arguments() {
    let tree = Tree::new();
    tree.put("repo/entry.js", "#!/usr/bin/env node\nconsole.log(JSON.stringify({args:process.argv.slice(2),marker:process.env.VP_WRAPPER_MARKER}));");
    let wrapper = if cfg!(windows) {
        tree.put(
            "repo/custom vp.cmd",
            "@ECHO off\nSET VP_WRAPPER_MARKER=configured\nnode \"%~dp0entry.js\" --custom %*\n",
        );
        "custom vp.cmd"
    } else {
        tree.put("repo/custom vp", &format!("#!/bin/sh\nexport VP_WRAPPER_MARKER=configured\nexec node \"{}\" --custom \"$@\"\n", tree.path("repo/entry.js")));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(tree.0.join("repo/custom vp"), fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        "custom vp"
    };
    let executable = probe("executable", &tree.path("repo"), wrapper, "");
    assert_eq!(executable["node"], false);
    let output = Command::new(node())
        .args([
            "-e",
            LAUNCH_SCRIPT,
            "--",
            &tree.path("repo"),
            executable["path"].as_str().unwrap(),
            "native",
            "lint",
        ])
        .env("PATH", "")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output: Value = from_slice(&output.stdout).unwrap();
    assert_eq!(output["args"], json!(["--custom", "lint", "--lsp"]));
    assert_eq!(output["marker"], "configured");
}

#[test]
fn modified_package_manager_shims_are_kept_intact() {
    let tree = Tree::new();
    tree.install("repo", "vite-plus", "vp");
    let shell = NPM_SHELL_SHIM.replace("{{target}}", "node_modules/vite-plus/bin/vp");
    let cmd = NPM_CMD_SHIM.replace("{{target}}", "%dp0%/node_modules/vite-plus/bin/vp");
    for modified in [
        shell.replace("#!/bin/sh\n", "#!/bin/sh\nexport VP_WRAPPER_MARKER=configured\n"),
        shell.replace("\"$@\"", "--custom \"$@\""),
        cmd.replace("@ECHO off\n", "@ECHO off\nSET VP_WRAPPER_MARKER=configured\n"),
        cmd.replace(" %*", " --custom %*"),
    ] {
        tree.put("repo/vp", &modified);
        assert_eq!(probe("executable", &tree.path("repo"), "vp", "")["node"], false);
    }
}

#[test]
fn launcher_preserves_arguments_cwd_and_stdio() {
    let tree = Tree::new();
    tree.put("repo/vp", "console.log(JSON.stringify({args:process.argv.slice(2),cwd:process.cwd(),env:process.env.TEST_SETTING})); process.stdin.pipe(process.stdout);");
    for tool in ["lint", "fmt"] {
        let output = Command::new(node())
            .args([
                "-e",
                LAUNCH_SCRIPT,
                "--",
                &tree.path("repo"),
                &tree.path("repo/vp"),
                "node",
                tool,
            ])
            .env("PATH", "")
            .env("TEST_SETTING", "kept")
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let output: Value = from_slice(&output.stdout).unwrap();
        assert_eq!(output["args"], json!([tool, "--lsp"]));
        assert_eq!(
            Path::new(output["cwd"].as_str().unwrap()),
            tree.0.join("repo").canonicalize().unwrap()
        );
        assert_eq!(output["env"], "kept");
    }
}

#[test]
fn launcher_reports_upgrade_hint_on_failure() {
    let tree = Tree::new();
    tree.put("vp", "process.exit(2)");
    let output = Command::new(node())
        .args(["-e", LAUNCH_SCRIPT, "--", &tree.path("."), &tree.path("vp"), "node", "lint"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Install or upgrade vite-plus"));
}

#[test]
#[cfg(unix)]
fn launcher_cleans_up_server_and_helper_descendants() {
    let tree = Tree::new();
    tree.put("launcher.cjs", LAUNCH_SCRIPT);
    tree.put("lifecycle.cjs", include_str!("fixtures/launcher-lifecycle.cjs"));
    let output = Command::new(node())
        .args([&tree.path("lifecycle.cjs"), &tree.path("launcher.cjs"), &tree.path(".")])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn standalone_subpackage_uses_hoisted_install_without_vite_plus_wrappers() {
    let tree = Tree::new();
    tree.put("repo/package.json", r#"{"workspaces":["app"]}"#);
    tree.put("repo/app/package.json", r#"{"devDependencies":{"oxlint":"*"}}"#);
    tree.install("repo", "oxlint", "oxlint");
    tree.install("repo/app", "vite-plus", "vp");
    let dirs = tree.directories("repo/app");
    assert_eq!(detect_project(&dirs, &Options::default()), None);
    assert_eq!(
        standalone_path(&dirs, "oxlint"),
        Some(tree.path("repo/node_modules/oxlint/bin/oxlint").as_str())
    );
    tree.put("repo/node_modules/oxlint/package.json", r#"{"name":"oxlint-alias"}"#);
    assert!(standalone_path(&tree.directories("repo/app"), "oxlint").is_some());
    assert_eq!(
        Options::from_settings(Some(&json!({"binarySource":"oxc", "vpPath":false}))).unwrap(),
        Options { source: BinarySource::Oxc, vp_path: None }
    );
}

#[test]
fn declaring_ancestor_controls_lookup_instead_of_a_descendant_install() {
    let tree = Tree::new();
    tree.put(
        "repo/package.json",
        r#"{"workspaces":["app"],"devDependencies":{"vite-plus":"*","oxlint":"*"}}"#,
    );
    tree.put("repo/app/package.json", "{}");
    tree.install("repo/app", "vite-plus", "vp");
    tree.install("repo", "vite-plus", "vp");
    tree.install("repo", "oxlint", "oxlint");
    let project = detect_project(&tree.directories("repo/app"), &Options::default()).unwrap();
    assert_eq!(project.root, tree.path("repo"));
    assert_eq!(project.vp_path, Some(tree.path("repo/node_modules/vite-plus/bin/vp")));
}

#[test]
fn explicit_js_and_absolute_shim_targets_use_zed_node() {
    let tree = Tree::new();
    tree.put("vp.js", "console.log('no shebang needed');");
    assert_eq!(probe("executable", &tree.path("."), "vp.js", "")["node"], true);
    tree.install("global", "vite-plus", "vp");
    let target = tree.path("global/node_modules/vite-plus/bin/vp");
    tree.put("vp.cmd", &NPM_CMD_SHIM.replace("{{target}}", &target));
    assert_eq!(probe("executable", &tree.path("."), "vp.cmd", "")["path"], target);
}

#[test]
#[cfg(unix)]
fn symlinked_global_pnpm_shim_keeps_its_recorded_entry() {
    let tree = Tree::new();
    tree.install("global", "vite-plus", "vp");
    tree.put(
        "global/vp",
        &NPM_SHELL_SHIM.replace("{{target}}", "node_modules/vite-plus/bin/vp").replace(
            "if [ -x",
            "if [ -z \"$NODE_PATH\" ]; then\n  export NODE_PATH=\"/global/node_modules\"\nelse\n  export NODE_PATH=\"/global/node_modules:$NODE_PATH\"\nfi\nif [ -x",
        ),
    );
    fs::create_dir_all(tree.0.join("bin")).unwrap();
    std::os::unix::fs::symlink(tree.0.join("global/vp"), tree.0.join("bin/vp")).unwrap();
    let executable = probe("global", &tree.path("."), "vp", &tree.path("bin"));
    assert_eq!(executable["node"], true);
    assert_eq!(
        Path::new(executable["path"].as_str().unwrap()).canonicalize().unwrap(),
        tree.0.join("global/node_modules/vite-plus/bin/vp").canonicalize().unwrap()
    );
}

#[test]
#[cfg(unix)]
fn native_vp_launch_uses_arguments_and_working_directory() {
    use std::os::unix::fs::PermissionsExt;
    let tree = Tree::new();
    tree.put("vp", "#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$1\" \"$2\"\n");
    fs::set_permissions(tree.0.join("vp"), fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(probe("executable", &tree.path("."), "vp", "")["node"], false);
    let output = Command::new(node())
        .args(["-e", LAUNCH_SCRIPT, "--", &tree.path("."), &tree.path("vp"), "native", "fmt"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(Path::new(lines[0]), tree.0.canonicalize().unwrap());
    assert_eq!(&lines[1..], ["fmt", "--lsp"]);
}
