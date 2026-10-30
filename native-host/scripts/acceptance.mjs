#!/usr/bin/env node
// Isolated chapter 18 equivalent: 24 actual WKWebView checks + 2 disk checks.
// No production vault, launchd registration, shell configuration or Keychain is used.
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
  existsSync,
  realpathSync,
  rmSync,
  chmodSync,
} from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
const host = resolve(fileURLToPath(new URL("..", import.meta.url))),
  repo = resolve(host, "..");
const root = realpathSync(mkdtempSync(join(tmpdir(), "wl-p2-"))),
  home = join(root, "home"),
  vault = join(root, "vault"),
  zdot = join(root, "zdot");
const identifier = `co.kcd.withlog.part2.check${process.pid}`;
const data = join(home, "Library/Application Support", identifier);
const mkdir = (p) => mkdirSync(p, { recursive: true }),
  write = (p, s) => {
    mkdir(resolve(p, ".."));
    writeFileSync(p, s);
  };
mkdir(home);
mkdir(zdot);
mkdir(data);
const id = "ndfixture-00000001",
  restoreId = "ndfixture-00000002",
  noteDir = join(vault, "notes/demo/root"),
  note = join(noteDir, "note.md");
write(join(vault, ".withlog/workspace.json"), "{}");
write(
  join(vault, ".withlog/agendas/demo.md"),
  "---\nid: agfixture-00000001\nslug: demo\ntitle: Demo\n---\n",
);
for (const [folder, nid] of [
  ["root", id],
  ["restore", restoreId],
])
  write(
    join(vault, "notes/demo", folder, "note.md"),
    `---\nid: ${nid}\nagenda: demo\ntitle: Native fixture\ntype: free\ncreated: '2026-06-16T10:00:00+09:00'\n---\nnative fixture\n`,
  );
write(
  join(data, "note-windows.dev.json"),
  JSON.stringify({
    [vault]: [
      { id: restoreId, geometry: { width: 900, height: 820, x: 100, y: 100 } },
    ],
  }),
);
const png =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aPZkAAAAASUVORK5CYII=";
const external = join(root, "external/README.md"),
  externalText = "\uFEFF# External fixture\r\n",
  localImage = join(root, "external/pic.png"),
  otherImage = join(root, "other/pic.png");
write(external, externalText);
for (const p of [localImage, otherImage]) write(p, Buffer.from(png, "base64"));
const huge = join(root, "external/huge.md"),
  invalid = join(root, "external/invalid.md"),
  text = join(root, "external/file.txt"),
  missing = join(root, "missing.md");
write(huge, Buffer.alloc(2097153, 65));
write(invalid, Buffer.from([255]));
write(text, "text");
const frame = join(noteDir, "attach/frame.html"),
  outsideFrame = join(vault, "notes/demo/outside.html");
const html =
  '<!doctype html><body style="margin:0"><div style="height:321px">isolated fixture</div></body>';
write(frame, html);
write(outsideFrame, html);
const refused = [join(root, "outside.pdf"), join(vault, "missing.pdf")];
write(refused[0], "fixture");
for (const name of ["run.command", "bin", "tool.py"]) {
  const p = join(noteDir, "attach", name);
  write(p, "exit 0\n");
  chmodSync(p, 0o700);
  refused.push(p);
}
const app = join(noteDir, "attach/Some.app");
mkdir(app);
refused.push(app);
const java = process.env.JAVA_HOME
  ? join(process.env.JAVA_HOME, "bin/java")
  : "/usr/bin/java";
const quote = (s) => "'" + s.replaceAll("'", "'\\''") + "'";
const wrapper = join(root, "java");
write(
  wrapper,
  `#!/bin/sh\nexec ${quote(java)} ${quote("-Duser.home=" + home)} "$@"\n`,
);
chmodSync(wrapper, 0o700);
const fake = join(root, "bin/claude");
write(
  fake,
  `#!/bin/sh\nprintf called >> ${quote(join(root, "claude-calls"))}\nexit 97\n`,
);
chmodSync(fake, 0o700);
const env = {
  ...process.env,
  HOME: home,
  ZDOTDIR: zdot,
  HISTFILE: join(root, "history"),
  CARGO_HOME: process.env.CARGO_HOME || join(homedir(), ".cargo"),
  RUSTUP_HOME: process.env.RUSTUP_HOME || join(homedir(), ".rustup"),
  WITHLOG_VAULT: vault,
  WITHLOG_CLI_JAR: join(repo, "dist/wl-cli.jar"),
  WITHLOG_SIDECAR_JAVA: wrapper,
  WITHLOG_SIDECAR_SOCKET: join(root, "engine.sock"),
  WITHLOG_VAULT_AUTOCOMMIT_ENABLED: "false",
  PATH: join(root, "bin") + ":" + process.env.PATH,
};
for (const k of [
  "WITHLOG_SIDECAR_JAR",
  "CLAUDE_CODE_SESSION_ID",
  "WITHLOG_ASK_RUNTIME",
  "WITHLOG_ASK_USE_AGENT",
])
  delete env[k];
if (!existsSync(join(repo, "dist/wl-cli.jar")))
  throw Error("Run bash build.sh first");
const config = {
  root,
  vault,
  id,
  restoreId,
  noteDir,
  note,
  external,
  externalText,
  localImage,
  otherImage,
  huge,
  invalid,
  text,
  missing,
  frame,
  outsideFrame,
  refused,
  png,
};
let noteGo = false,
  noteResult = {},
  finish;
const complete = new Promise((r) => (finish = r));
const server = createServer(async (req, res) => {
  const url = new URL(req.url, "http://localhost");
  const send = (value, status = 200) => {
    res.writeHead(status, { "Content-Type": "application/json" });
    res.end(JSON.stringify(value));
  };
  if (req.method === "POST") {
    let body = "";
    for await (const chunk of req) body += chunk;
    const data = JSON.parse(body || "{}");
    if (url.pathname === "/progress") {
      console.log(
        `${data.ok ? "PASS" : "FAIL"} ${data.id}${data.error ? " " + data.error : ""}`,
      );
    }
    if (url.pathname === "/result") finish(data);
    if (url.pathname === "/note-go") noteGo = data.go;
    if (url.pathname === "/note-result") noteResult = { ...data, done: true };
    send({ ok: true });
    return;
  }
  if (url.pathname === "/config") return send(config);
  if (url.pathname === "/note-go") return send({ go: noteGo });
  if (url.pathname === "/note-results") return send(noteResult);
  if (url.pathname === "/checks.js") {
    res.writeHead(200, { "Content-Type": "text/javascript" });
    res.end(readFileSync(join(host, "scripts/acceptance/checks.js")));
    return;
  }
  res.writeHead(200, { "Content-Type": "text/html" });
  res.end(
    '<!doctype html><html><head><meta charset="utf-8"><title>withlog isolated native checks</title></head><body><h1>withlog native checks</h1><pre>Running…</pre><script type="module" src="/checks.js"></script></body></html>',
  );
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const port = server.address().port;
const override = join(root, "tauri.json");
write(
  override,
  JSON.stringify({
    identifier,
    build: { devUrl: `http://127.0.0.1:${port}`, beforeDevCommand: null },
    app: { withGlobalTauri: true },
    bundle: { resources: null },
  }),
);
const children = [];
let log = "";
const start = (cmd, args, options = {}) => {
  const c = spawn(cmd, args, {
    cwd: host,
    env,
    stdio: ["ignore", "pipe", "pipe"],
    detached: true,
    ...options,
  });
  children.push(c);
  c.stdout.on("data", (b) => {
    log += b;
  });
  c.stderr.on("data", (b) => {
    log += b;
  });
  return c;
};
let result = [];
let failure;
try {
  const engine = start(wrapper, [
    "-jar",
    join(repo, "dist/wl-sidecar.jar"),
    "serve",
  ]);
  for (let i = 0; i < 600 && !existsSync(env.WITHLOG_SIDECAR_SOCKET); i++) {
    if (engine.exitCode !== null) throw Error("engine exited");
    await new Promise((r) => setTimeout(r, 25));
  }
  if (!existsSync(env.WITHLOG_SIDECAR_SOCKET)) throw Error("engine not ready");
  const native = start(join(host, "node_modules/.bin/tauri"), [
    "dev",
    "--config",
    override,
    "--no-watch",
    "--",
    "--no-default-features",
  ]);
  native.on("exit", (code) => {
    if (code && !result.length)
      finish([{ id: "native-process", ok: false, error: `exited ${code}` }]);
  });
  const timer = setTimeout(
    () =>
      finish([
        {
          id: "harness-timeout",
          ok: false,
          error: "No page completion within 5 minutes",
        },
      ]),
    300000,
  );
  result = await complete;
  clearTimeout(timer);
  if (
    !readFileSync(join(noteDir, "attach/part2-check.png")).equals(
      Buffer.from(png, "base64"),
    )
  )
    throw Error("Attachment bytes changed");
  const layouts = JSON.parse(
    readFileSync(join(data, "window-layouts.dev.json"), "utf8"),
  );
  const records = Object.values(layouts).flatMap(Object.values);
  result.push({
    id: "disk-window-layouts",
    ok:
      records.length > 0 &&
      records.every(
        (r) =>
          typeof r.monitor === "string" &&
          ["dx", "dy", "width", "height"].every(
            (k) => typeof r[k] === "number",
          ) &&
          (r.desktop === undefined || Number.isInteger(r.desktop)),
      ),
  });
  const main = JSON.parse(
    readFileSync(join(data, "main-window.dev.json"), "utf8"),
  );
  result.push({
    id: "disk-main-window",
    ok: ["width", "height", "x", "y"].every((k) => typeof main[k] === "number"),
  });
  if (existsSync(join(root, "claude-calls")))
    throw Error("Part 4 process unexpectedly invoked");
  for (const r of result.slice(24))
    console.log(`${r.ok ? "PASS" : "FAIL"} ${r.id}`);
  if (result.length !== 26 || result.some((r) => !r.ok))
    throw Error("Acceptance checks failed");
  console.log("26 / 26 native checks passed; Claude calls: 0");
} catch (e) {
  failure = e;
  console.error(String(e));
} finally {
  for (const c of children.reverse()) {
    try {
      process.kill(-c.pid, "SIGTERM");
    } catch {}
  }
  await new Promise((r) => setTimeout(r, 700));
  for (const c of children) {
    try {
      process.kill(-c.pid, "SIGKILL");
    } catch {}
  }
  server.close();
  const report = { results: result, ok: !failure, root, identifier };
  if (process.env.WITHLOG_CHECK_REPORT)
    write(process.env.WITHLOG_CHECK_REPORT, JSON.stringify(report, null, 2));
  if (failure) {
    write(join(root, "host.log"), log);
    console.error(`Fixture diagnostics retained: ${root}`);
  } else rmSync(root, { recursive: true, force: true });
}
if (failure) process.exitCode = 1;
