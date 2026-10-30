/* Runs in an actual WKWebView. Only fixtures from this run's /config are touched. */
const { invoke, Channel, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const cfg = await (await fetch("/config")).json();
const assert = (v, msg = "assertion failed") => {
  if (!v) throw Error(msg);
};
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(fn, ms = 7000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await fn()) return;
    await wait(50);
  }
  throw Error("timed out");
}
async function refused(name, args, options) {
  try {
    await invoke(name, args, options);
  } catch {
    return;
  }
  throw Error(`${name} unexpectedly succeeded`);
}
async function json(path, body) {
  return (
    await fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    })
  ).json();
}
async function image(path) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(Error("image refused"));
    img.src = convertFileSrc(path);
  });
}
async function frame(path, expect) {
  return new Promise((resolve, reject) => {
    const f = document.createElement("iframe");
    f.sandbox = "allow-scripts";
    f.src =
      "wlframe://localhost" + path.split("/").map(encodeURIComponent).join("/");
    let timer = setTimeout(
      () => {
        removeEventListener("message", listener);
        f.remove();
        expect ? reject(Error("frame height missing")) : resolve();
      },
      expect ? 6000 : 1000,
    );
    function listener(e) {
      if (
        e.source !== f.contentWindow ||
        typeof e.data?.wlframeHeight !== "number"
      )
        return;
      clearTimeout(timer);
      removeEventListener("message", listener);
      f.remove();
      expect && e.data.wlframeHeight === 321
        ? resolve()
        : reject(Error("unexpected frame height " + e.data.wlframeHeight));
    }
    addEventListener("message", listener);
    document.body.append(f);
  });
}
const tests = [];
function test(id, fn) {
  tests.push([id, fn]);
}
let self = [],
  windows = [],
  navigations = [];
await listen("wl://self-write", (e) => self.push(e.payload));
await listen("wl://note-windows", (e) => windows.push(e.payload));
await listen("wl://navigate", (e) => navigations.push(e.payload));
if (location.pathname.startsWith("/note-window/")) {
  const id = location.pathname.split("/").pop();
  await until(
    async () => Boolean((await (await fetch("/note-go")).json()).go),
    120000,
  );
  if (id === cfg.id) {
    try {
      await refused("allow_workspace", { path: cfg.vault });
      await invoke("remember_note_window", {
        geometry: { width: 910, height: 830, x: 100, y: 100 },
      });
      await invoke("announce_self_write", {
        root: cfg.vault,
        relPath: "notes/note-self.md",
      });
      await invoke("show_in_main", {
        path: "/doc/" + cfg.id,
        rail: "rail-fixture",
        sub: "sn-fixture",
      });
      await json("/note-result", { ok: true });
      await invoke("destroy_current_window");
    } catch (e) {
      await json("/note-result", { ok: false, error: String(e) });
    }
  }
} else {
  test("before-open", async () => {
    assert((await invoke("vault_root")) === null);
    try {
      await invoke("sidecar_call", { argv: ["list-docs"] });
    } catch (e) {
      assert(String(e) === "vault가 아직 열리지 않았어요");
      return;
    }
    throw Error("sidecar accepted unopened vault");
  });
  test("is-workspace", async () => {
    assert(await invoke("is_workspace", { root: cfg.vault }));
    assert(!(await invoke("is_workspace", { root: cfg.root })));
  });
  test("create-dir", async () => {
    assert(
      (
        await invoke("create_workspace_dir", {
          parent: cfg.root,
          name: "created",
        })
      ).endsWith("/created"),
    );
    await refused("create_workspace_dir", {
      parent: cfg.root,
      name: "../escape",
    });
  });
  test("allow", async () => {
    await invoke("allow_workspace", { path: cfg.vault });
    assert((await invoke("vault_root")) === cfg.vault);
  });
  test("self-write", async () => {
    self = [];
    await invoke("announce_self_write", {
      root: cfg.vault,
      relPath: "notes/self.md",
    });
    await until(() => self.length === 1);
    assert(self[0].from === "main");
    await refused("announce_self_write", { root: cfg.vault, relPath: "" });
    await refused("announce_self_write", { root: cfg.vault, relPath: "../x" });
    await invoke("announce_self_write", {
      root: cfg.root,
      relPath: "notes/self.md",
    });
    await wait(100);
    assert(self.length === 1);
  });
  test("attachment", async () => {
    const data = Uint8Array.from(atob(cfg.png), (c) => c.charCodeAt(0));
    const path = await invoke("write_attachment_bytes", data, {
      headers: {
        "x-wl-note-dir": encodeURIComponent(cfg.noteDir),
        "x-wl-name": "part2-check.png",
      },
    });
    const img = await image(cfg.vault + "/" + path);
    assert(img.width === 1);
    for (const name of ["x.txt", "../x.png"])
      await refused("write_attachment_bytes", data, {
        headers: {
          "x-wl-note-dir": encodeURIComponent(cfg.noteDir),
          "x-wl-name": encodeURIComponent(name),
        },
      });
    await refused("write_attachment_bytes", data, {
      headers: {
        "x-wl-note-dir": encodeURIComponent(cfg.root),
        "x-wl-name": "x.png",
      },
    });
    try {
      await invoke("write_attachment_bytes", new Uint8Array(5000001), {
        headers: {
          "x-wl-note-dir": encodeURIComponent(cfg.noteDir),
          "x-wl-name": "x.png",
        },
      });
    } catch {
      return;
    }
    throw Error("oversized accepted");
  });
  test("open-refusals", async () => {
    for (const path of cfg.refused)
      await refused("open_in_default_app", { path });
  });
  test("md-target", async () => {
    const n = await invoke("resolve_md_target", {
      path: cfg.note,
      knownRoots: [cfg.vault + "///"],
    });
    assert(n.kind === "note" && n.ws === cfg.vault && n.chain[0] === cfg.id);
    const e = await invoke("resolve_md_target", {
      path: cfg.external,
      knownRoots: [],
    });
    assert(e.kind === "external" && e.path === cfg.external);
    for (const path of [cfg.text, cfg.missing])
      await refused("resolve_md_target", { path, knownRoots: [] });
  });
  test("external-md", async () => {
    assert(
      (await invoke("read_external_md", { path: cfg.external })) ===
        cfg.externalText,
    );
    for (const path of [cfg.huge, cfg.invalid])
      await refused("read_external_md", { path });
  });
  test("external-images", async () => {
    try {
      await image(cfg.otherImage);
      throw Error("outside image already allowed");
    } catch (e) {
      assert(String(e).includes("image refused"));
    }
    assert(
      (await invoke("allow_external_images", {
        mdPath: cfg.external,
        srcs: [
          "pic.png?x",
          "pic.png#y",
          "https://x/p.png",
          "../other/pic.png",
          "/pic.png",
          "missing.png",
        ],
      })) === 1,
    );
    await image(cfg.localImage);
    try {
      await image(cfg.otherImage);
      throw Error("other image allowed");
    } catch (e) {
      assert(String(e).includes("image refused"));
    }
  });
  test("wlframe", async () => {
    await frame(cfg.frame, true);
    await frame(cfg.outsideFrame, false);
  });
  test("list", async () => {
    const r = await invoke("sidecar_call", { argv: ["list-docs"] });
    assert(r.status === 0 && r.stdout.includes(cfg.id));
  });
  test("get", async () => {
    const r = await invoke("sidecar_call", { argv: ["get-doc", cfg.id] });
    assert(r.status === 0 && r.stdout.includes("native fixture"));
  });
  test("missing", async () =>
    assert(
      (
        await invoke("sidecar_call", {
          argv: ["get-doc", "ndmissing-00000001"],
        })
      ).status !== 0,
    ));
  test("engine-part4", async () => {
    const r = await invoke("sidecar_call", { argv: ["get-ask", cfg.id] });
    assert(r.status === 3 && r.stderr.includes("WL-409"));
    for (const name of [
      "ask_claude",
      "ask_runtime",
      "list_claude_agents",
      "alive_pids",
      "held_agent_sessions",
      "release_agent_session",
      "write_session_image",
      "pty_start_session",
      "start_dictation",
    ]) {
      try {
        await invoke(name, {});
      } catch (e) {
        assert(String(e).includes("not found"));
        continue;
      }
      throw Error(name + " unexpectedly registered");
    }
  });
  let output = "",
    key = "part2-check";
  let channel;
  test("pty", async () => {
    channel = new Channel();
    channel.onmessage = (b) => {
      output += new TextDecoder().decode(
        Uint8Array.from(atob(b), (c) => c.charCodeAt(0)),
      );
    };
    const r = await invoke("pty_start", {
      key,
      cols: 80,
      rows: 24,
      onData: channel,
    });
    assert(!r.fellBack);
    await invoke("pty_write", { key, data: "echo part2-$((6*7))\n" });
    await until(() => output.includes("part2-42"));
    assert(await invoke("pty_alive", { key }));
    await invoke("pty_resize", { key, cols: 100, rows: 30 });
    await invoke("pty_detach", { key });
    await invoke("pty_start", { key, cols: 80, rows: 24, onData: channel });
  });
  test("pty-stats", async () => {
    const s = await invoke("pty_stats", { key });
    assert(
      s.bytesOut > 0 && s.chunksOut > 0 && /^[0-9a-f]{16}$/.test(s.hashOut),
    );
    const e = await invoke("pty_stats", { key: "missing" });
    assert(
      e.bytesOut === 0 && e.chunksOut === 0 && e.hashOut === "cbf29ce484222325",
    );
    await invoke("pty_write", { key, data: "exit\n" });
    await until(async () => !(await invoke("pty_alive", { key })));
  });
  test("secret-table", async () => {
    assert(!(await invoke("secret_table_status")).ready);
    await refused("mask_scrollback", { lines: ["fictional text"] });
  });
  test("shapes", async () => {
    const s = await invoke("scan_text_shapes", {
      text: "-----BEGIN RSA PRIVATE KEY----- redis://u:pw@host https://user:password@host",
    });
    assert(
      s.find((s) => s.kind === "pem-private-key")?.count === 1 &&
        s.find((s) => s.kind === "url-credential")?.count === 2,
    );
    assert(
      (await invoke("scan_text_shapes", { text: "harmless fixture" }))
        .length === 0,
    );
  });
  test("note-windows", async () =>
    assert((await invoke("note_windows")).length === 0));
  test("note-restore", async () => {
    assert((await invoke("saved_note_windows")).includes(cfg.restoreId));
    const ids = await invoke("restore_note_windows", {
      docs: [
        { docId: cfg.restoreId, title: "Restored fixture" },
        { docId: cfg.restoreId },
        { docId: "not-saved" },
      ],
    });
    assert(ids.length === 1 && ids[0] === cfg.restoreId);
    assert((await invoke("note_windows")).includes(cfg.restoreId));
    await until(() => windows.some((ids) => ids.includes(cfg.restoreId)));
  });
  test("note-refusals", async () => {
    await refused("destroy_current_window");
    for (const path of [
      "https://outside",
      "//outside",
      "/note-window",
      "/note-window/x",
    ])
      await refused("show_in_main", { path });
  });
  test("note-open", async () => {
    await invoke("open_note_window", {
      docId: cfg.id,
      title: "Fixture note",
      geometry: { width: 900, height: 820, x: 120, y: 120 },
    });
    await invoke("open_note_window", {
      docId: cfg.id,
      title: null,
      geometry: null,
    });
    assert(
      (await invoke("note_windows")).filter((id) => id === cfg.id).length === 1,
    );
    await json("/note-go", { go: true });
    await until(async () =>
      Boolean((await (await fetch("/note-results")).json()).done),
    );
    const result = await (await fetch("/note-results")).json();
    assert(result.ok, result.error);
    await until(async () => !(await invoke("note_windows")).includes(cfg.id));
    assert(!(await invoke("saved_note_windows")).includes(cfg.id));
    await until(
      () =>
        navigations.some(
          (n) =>
            n.path === "/doc/" + cfg.id &&
            n.rail === "rail-fixture" &&
            n.sub === "sn-fixture",
        ) && self.some((s) => s.from === "note-" + cfg.id),
    );
  });
  test("note-close", async () => {
    await invoke("close_note_windows");
    await until(async () => (await invoke("note_windows")).length === 0);
  });
  const results = [];
  for (const [id, fn] of tests) {
    try {
      await fn();
      results.push({ id, ok: true });
    } catch (e) {
      results.push({ id, ok: false, error: String(e) });
    }
    document.querySelector("pre").textContent = JSON.stringify(
      results,
      null,
      2,
    );
    await json("/progress", results.at(-1));
  }
  await wait(1000);
  await json("/result", results);
}
