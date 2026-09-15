const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");
const { test } = require("node:test");
const { setTimeout: delay } = require("node:timers/promises");

const [mode, directory] = process.argv.slice(2);

if (["lint", "fmt", "server", "helper"].includes(mode)) {
  const role = mode === "lint" || mode === "fmt" ? "vp" : mode;

  if (role !== "vp" && process.env.E2E_IGNORE_TERM === "1") {
    process.on("SIGTERM", () => {});
  }

  if (role !== "helper") {
    spawn(process.execPath, [__filename, role === "vp" ? "server" : "helper"], {
      stdio: "inherit",
    });
  }

  if (role === "vp") {
    process.stdin.once("data", (data) => process.exit(Number(data.toString())));
  }

  setInterval(() => {}, 1000);
  fs.writeFileSync(`${role}.pid`, String(process.pid));
} else {
  const launcher = fs.readFileSync(mode, "utf8");

  function alive(pid) {
    try {
      process.kill(pid, 0);
      return true;
    } catch (error) {
      if (error.code !== "ESRCH") throw error;
      return false;
    }
  }

  async function waitFor(check) {
    const deadline = Date.now() + 5000;
    while (!check()) {
      assert.ok(Date.now() < deadline, "Process lifecycle deadline exceeded");
      await delay(25);
    }
  }

  function start(root, tool, ignoreTerm, executable = __filename, loader = "node") {
    const child = spawn(process.execPath, ["-e", launcher, "--", root, executable, loader, tool], {
      env: { ...process.env, E2E_IGNORE_TERM: ignoreTerm ? "1" : "0" },
      stdio: ["pipe", "pipe", "pipe"],
    });
    const state = { child, closed: false, code: null, stdout: "", stderr: "" };
    child.stdout.on("data", (data) => (state.stdout += data));
    child.stderr.on("data", (data) => (state.stderr += data));
    child.on("close", (code) => {
      state.closed = true;
      state.code = code;
    });
    return state;
  }

  for (const tool of ["lint", "fmt"]) {
    for (const scenario of [
      { target: "launcher", signal: "SIGTERM" },
      { target: "launcher", signal: "SIGINT" },
      { target: "vp", signal: "SIGTERM" },
      { target: "vp", signal: "SIGKILL" },
      { target: "vp", code: 0 },
      { target: "vp", code: 23 },
      { target: "launcher", signal: "SIGTERM", ignoreTerm: true },
    ]) {
      const name = `${tool}-${scenario.target}-${scenario.signal ?? scenario.code}-${!!scenario.ignoreTerm}`;
      test(name, async () => {
        const root = path.join(directory, name);
        fs.mkdirSync(root);
        const state = start(root, tool, scenario.ignoreTerm);
        const sibling = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
          stdio: "ignore",
        });
        const pids = { launcher: state.child.pid };

        try {
          await waitFor(() => {
            for (const role of ["vp", "server", "helper"]) {
              const file = path.join(root, `${role}.pid`);
              if (!fs.existsSync(file)) return false;
              const pid = Number(fs.readFileSync(file, "utf8"));
              if (!Number.isInteger(pid) || pid <= 0) return false;
              pids[role] = pid;
            }
            return true;
          });

          if (scenario.signal) process.kill(pids[scenario.target], scenario.signal);
          else state.child.stdin.end(String(scenario.code));

          // 'close' also requires EOF on inherited stdout/stderr, unlike 'exit'.
          await waitFor(() => state.closed);
          await waitFor(() => Object.values(pids).every((pid) => !alive(pid)));
          assert.equal(state.code, scenario.code ?? 1, state.stderr);
          assert.equal(state.stdout, "");
          assert.ok(alive(sibling.pid), "Cleanup terminated an unrelated process");
        } finally {
          // Read any late PID files so a failed readiness check cannot leak workers.
          for (const role of ["vp", "server", "helper"]) {
            const file = path.join(root, `${role}.pid`);
            if (fs.existsSync(file)) pids[role] = Number(fs.readFileSync(file, "utf8"));
          }
          for (const pid of [...Object.values(pids), sibling.pid]) {
            if (Number.isInteger(pid) && pid > 0 && alive(pid)) process.kill(pid, "SIGKILL");
          }
          state.child.stdin.destroy();
        }
      });
    }
  }

  test("spawn failure has no process group to signal", async () => {
    const state = start(directory, "lint", false, path.join(directory, "missing-vp"), "native");
    try {
      await waitFor(() => state.closed);
      assert.equal(state.code, 1);
      assert.match(state.stderr, /Install or upgrade vite-plus/);
      assert.match(state.stderr, /ENOENT/);
    } finally {
      if (alive(state.child.pid)) state.child.kill("SIGKILL");
      state.child.stdin.destroy();
    }
  });
}
