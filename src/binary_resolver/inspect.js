// Filesystem access for the Rust resolver. WASI can only read the extension's
// directory; Zed's worktree API cannot read ancestors of an opened subpackage.
const fs = require("node:fs");
const path = require("node:path");
const [mode, start, tool] = process.argv.slice(1);

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return null;
  }
}

function isFile(file) {
  try {
    return fs.statSync(file).isFile();
  } catch {
    return false;
  }
}

function packageEntry(dir, name, bin) {
  const packageDir = path.join(dir, "node_modules", name);
  const pkg = readJson(path.join(packageDir, "package.json"));
  const entry = path.join(packageDir, "bin", bin);
  // Standalone dependencies can be npm aliases. Only Vite+ identity requires
  // the package-name validation specified by the detection RFC.
  return (name !== "vite-plus" || pkg?.name === name) && isFile(entry) ? entry : null;
}

function ancestors(start, tool) {
  const directories = [];
  let dir = path.resolve(start);
  while (true) {
    const pkg = readJson(path.join(dir, "package.json"));
    const boundary =
      fs.existsSync(path.join(dir, "pnpm-workspace.yaml")) ||
      fs.existsSync(path.join(dir, "lerna.json")) ||
      (pkg !== null && Object.hasOwn(pkg, "workspaces"));
    directories.push({
      root: dir,
      package: pkg,
      vp: packageEntry(dir, "vite-plus", "vp"),
      standalone: packageEntry(dir, tool, tool),
    });
    const parent = path.dirname(dir);
    if (boundary || dir === parent) return directories;
    dir = parent;
  }
}

function readHeader(file) {
  // Read only the beginning: vp may be a large native executable.
  const fd = fs.openSync(file, "r");
  const buffer = Buffer.alloc(8192);
  try {
    const bytesRead = fs.readSync(fd, buffer, 0, buffer.length, 0);
    return buffer.subarray(0, bytesRead).toString();
  } finally {
    fs.closeSync(fd);
  }
}

function shimTarget(header) {
  // Match complete forwarding templates, not arbitrary quoted paths. Custom
  // scripts can set environment variables or add arguments that must survive.
  let script = header
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .join("\n");
  const shellPreamble = String.raw`#!/bin/sh
basedir=$(dirname "$(echo "$0" | sed -e 's,\\,/,g')")
case \`uname\` in
*CYGWIN*|*MINGW*|*MSYS*)
if command -v cygpath > /dev/null 2>&1; then
basedir=\`cygpath -w "$basedir"\`
fi
;;
esac
`.replaceAll("\\`", "`");
  if (script.startsWith(shellPreamble)) {
    script = script.slice(shellPreamble.length);
    // pnpm adds only module search paths to npm's forwarding template.
    script = script.replace(
      /^if \[ -z "\$NODE_PATH" \]; then\nexport NODE_PATH="([^"$`\n]*)"\nelse\nexport NODE_PATH="\1:\$NODE_PATH"\nfi\n/,
      "",
    );
    const match = new RegExp(
      [
        '^if \\[ -x "\\$basedir/node" \\]; then\\n',
        'exec "\\$basedir/node" +"(\\$basedir/[^"$`\\n]+)" "\\$@"\\n',
        'else\\nexec node +"\\1" "\\$@"\\nfi$',
      ].join(""),
    ).exec(script);
    return match?.[1];
  }

  const cmdPreamble = String.raw`@ECHO off
GOTO start
:find_dp0
SET dp0=%~dp0
EXIT /b
:start
SETLOCAL
CALL :find_dp0
`;
  if (script.startsWith(cmdPreamble)) {
    script = script.slice(cmdPreamble.length);
    const match = new RegExp(
      [
        '^IF EXIST "%dp0%\\\\node\\.exe" \\(\\nSET "_prog=%dp0%\\\\node\\.exe"\\n',
        '\\) ELSE \\(\\nSET "_prog=node"\\n(?:SET PATHEXT=%PATHEXT:;\\.JS;=;%\\n)?\\)\\n',
        "endLocal & goto #_undefined_# 2>NUL \\|\\| title %COMSPEC% & ",
        '(?:set PATHEXT=%PATHEXT:;\\.JS;=;% & )?"%_prog%" +"([^"\\n]+)" %\\*$',
      ].join(""),
    ).exec(script);
    return match?.[1];
  }
}

function executable(file) {
  if (!isFile(file)) return null;
  const real = fs.realpathSync(file);
  const header = readHeader(real);
  if (/^#![^\r\n]*\bnode\b/.test(header) || /\.[cm]?js$/i.test(real)) {
    return { path: real, node: true };
  }

  // Only unwrap a complete known shim, including symlinks to global shims.
  const shimPath = fs.statSync(real).size <= 8192 && shimTarget(header);
  if (shimPath) {
    const target = shimPath
      .replace(/^(?:\$basedir|%~dp0|%dp0%)[\\/]/, path.dirname(real) + path.sep)
      .replaceAll("\\", path.sep);
    if (!path.isAbsolute(target) || path.resolve(target) === real || !isFile(target)) {
      return { path: file, node: false };
    }
    const targetHeader = readHeader(target);
    if (/^#![^\r\n]*\bnode\b/.test(targetHeader)) return { path: path.resolve(target), node: true };
  }
  return { path: file, node: false };
}

function globalExecutable(start) {
  const pathKey = Object.keys(process.env).find((key) => key.toUpperCase() === "PATH");
  const names = process.platform === "win32" ? ["vp.cmd", "vp.exe", "vp"] : ["vp"];
  const directories = (process.env[pathKey] || "").split(path.delimiter).filter(Boolean);
  for (const dir of directories) {
    for (const name of names) {
      const result = executable(path.resolve(start, dir, name));
      if (result) return result;
    }
  }
  return null;
}

try {
  let result;
  if (mode === "ancestors") {
    result = ancestors(start, tool);
  } else if (mode === "global") {
    result = globalExecutable(start);
  } else {
    result = executable(path.resolve(start, tool));
  }
  process.stdout.write(JSON.stringify(result));
} catch (error) {
  process.stderr.write(String(error));
  process.exitCode = 1;
}
