// Zed's Command has no cwd field. Keep Vite+ anchored to the declaring package,
// even when the editor opens a subdirectory or uses a global vp installation.
const { spawn } = require("node:child_process");
const path = require("node:path");

const [root, executable, loader, tool] = process.argv.slice(1);
const hint =
  "Vite+ language server failed. Install or upgrade vite-plus, then restart the language server.";

const pathKey = Object.keys(process.env).find((key) => key.toUpperCase() === "PATH") || "PATH";
const searchPath = process.env[pathKey] || "";
delete process.env[pathKey];
process.env.PATH = path.dirname(process.execPath) + path.delimiter + searchPath;

const isWindows = process.platform === "win32";
const batch = loader !== "node" && isWindows && /\.(cmd|bat)$/i.test(executable);
let command = executable;
let args = [tool, "--lsp"];

if (loader === "node") {
  command = process.execPath;
  args.unshift(executable);
} else if (batch) {
  command = process.env.ComSpec || process.env.COMSPEC || "cmd.exe";
  args = ["/d", "/s", "/c", `""${executable}" ${tool} --lsp"`];
}

const child = spawn(command, args, {
  cwd: root,
  env: process.env,
  stdio: "inherit",
  // Vite+ spawns the real server. Give its descendants a group we can stop
  // without signaling Zed or another language server.
  detached: !isWindows,
  windowsVerbatimArguments: batch,
});

let groupId = child.pid;
let stopping = false;

function signalGroup(signal) {
  if (groupId === undefined) return false;

  try {
    process.kill(-groupId, signal);
    return true;
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
    groupId = undefined;
    return false;
  }
}

function stop(signal = "SIGTERM") {
  if (isWindows) {
    if (child.exitCode === null && child.signalCode === null) child.kill(signal);
    return;
  }

  if (stopping) return;
  stopping = true;
  if (!signalGroup(signal)) return;

  // Keep the launcher alive while descendants exit, even if vp already died.
  const deadline = Date.now() + 1000;
  const timer = setInterval(() => {
    if (!signalGroup(0)) {
      clearInterval(timer);
    } else if (Date.now() >= deadline) {
      signalGroup("SIGKILL");
      clearInterval(timer);
    }
  }, 50);
}

child.on("error", (error) => {
  console.error(`${hint}\n${error.message}`);
  process.exitCode = 1;
  stop();
});

child.on("exit", (code, signal) => {
  if (code) console.error(hint);
  process.exitCode = code ?? (signal ? 1 : 0);
  stop();
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => stop(signal));
}

process.on("exit", () => {
  if (isWindows) stop();
  else signalGroup("SIGKILL");
});
