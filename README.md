<p align="center">
  <br>
  <br>
  <a href="https://oxc.rs" target="_blank" rel="noopener noreferrer">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://oxc.rs/oxc-light.svg">
      <source media="(prefers-color-scheme: light)" srcset="https://oxc.rs/oxc-dark.svg">
      <img alt="Oxc logo" src="https://oxc.rs/oxc-dark.svg" height="60">
    </picture>
  </a>
  <br>
  <br>
  <br>
</p>

# Oxc extension for Zed

This extension adds support for [Oxc](https://github.com/oxc-project/oxc) in [Zed](https://zed.dev/).

The supported languages for Oxfmt and Oxlint can be seen within the [extension.toml](extension.toml) file.

## Installation

Requires Zed >= **v0.205.0** and oxlint >= **v1.35.0**.

This extension is available in the extensions view inside the Zed editor. Open `zed: extensions` and search for _Oxc_.

## Configuration

Configuration is managed in your `.zed/settings.json` file. Examples are available in the [examples](./examples) directory.

See https://github.com/oxc-project/oxc/tree/main/crates/oxc_language_server for the options that are supported by the language server.

### Vite+

The extension detects a direct `vite-plus` dependency in `dependencies` or
`devDependencies` and starts `vp lint --lsp` and `vp fmt --lsp`. It does not use
the deprecated `vite-plus/bin/oxlint` or `vite-plus/bin/oxfmt` wrappers.

Detection starts at the opened worktree and walks up to the nearest monorepo
root (`pnpm-workspace.yaml`, `package.json#workspaces`, or `lerna.json`), or the
filesystem root. Opening a subpackage can therefore find an ancestor's
declaration and a hoisted installation. Detection is per worktree; opening a
file in a different subdirectory does not select a different project.

The extension searches for a valid `node_modules/vite-plus/bin/vp` from the
declaring package through that boundary, then searches the worktree's `PATH`.
A global or transitive installation alone does not select Vite+. If Vite+ is
selected but no executable is available, Zed shows an install hint. Install the
project's dependencies and run **editor: restart language server** to retry.

Select the source for each tool with `initialization_options.settings.binarySource`:

| Value | Behavior |
| --- | --- |
| `auto` (default) | Detect a direct dependency, or use an explicit `vpPath`. |
| `vite-plus` | Use Vite+ without requiring a dependency declaration. |
| `oxc` | Use standalone Oxlint or Oxfmt and ignore `vpPath`. |

For example, use standalone Oxlint with Vite+ formatting:

```json
{
  "lsp": {
    "oxlint": {
      "initialization_options": {
        "settings": { "binarySource": "oxc" }
      }
    },
    "oxfmt": {
      "initialization_options": {
        "settings": { "binarySource": "vite-plus" }
      }
    }
  }
}
```

Set `initialization_options.settings.vpPath` for either tool to use a particular `vp`
executable, Node entry, or npm/pnpm shim. Relative paths are resolved from the
opened worktree. Custom wrappers keep their environment setup and arguments.
For example:

```json
{
  "lsp": {
    "oxfmt": {
      "initialization_options": {
        "settings": {
          "vpPath": "./node_modules/vite-plus/bin/vp"
        }
      }
    }
  }
}
```

An existing `binary.path` setting remains a complete command override and takes
priority over source selection, including after a restart. `binary.arguments` is
optional when `binary.path` is set; omitted arguments default to an empty list.
`binary.env` applies to discovery and server launch, including `PATH` overrides.
Discovery resolves relative `PATH` entries from the opened worktree.
Restart the affected language server after changing its source or executable.

You can also put these options in `lsp.<tool>.settings`. Its keys override matching
keys in `initialization_options.settings` for source selection and workspace
configuration. During initialization, Zed reapplies `initialization_options`, so
values there win when the same server option is set in both locations. Avoid
setting the same server option to different values in these two locations.

Vite+ servers run from the declaring package (or the nearest package in forced
mode). The extension sets `disableNestedConfig: true` for lint and
`fmt.disableNestedConfig: true` for formatting. When using Vite+, leave these
options unset or set them to `true`. Do not explicitly set either option to
`false`: Zed reapplies user initialization options after the extension's defaults.
Saved settings and standalone servers keep their configured values. Arbitrary
custom commands supplied through `binary.path` receive the settings you specify.

Node entries use Zed's Node runtime. Native `vp` executables run directly through
the launcher. The launcher also makes that Node runtime available to child tools.
If an older Vite+ installation fails to start, upgrade `vite-plus` and restart
the language server.

See the [Vite+ example](./examples/vite-plus) and the
[editor detection RFC](https://github.com/voidzero-dev/vite-plus/pull/1614).

# [Sponsored By](https://oxc.rs/sponsor)

<p align="center">
  <a href="https://oxc.rs/sponsor">
    <img src="https://raw.githubusercontent.com/oxc-project/sponsors/main/sponsors.svg" alt="Our sponsors" />
  </a>
</p>
