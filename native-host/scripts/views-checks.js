// Full production React UI + real Tauri/Java, using only the isolated fixture.
const cfg = window.__WL_FIXTURE__,
  invoke = (...args) => window.__TAURI_INTERNALS__.invoke(...args);
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
const assert = (yes, why) => {
  if (!yes) throw Error(why);
};
const post = (path, data) =>
  fetch(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(data),
  });
async function until(fn, label, timeout = 20000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (await fn()) return;
    await pause(40);
  }
  throw Error("timeout: " + label);
}
const engine = async (argv) => {
  const r = await invoke("sidecar_call", { argv });
  assert(r.status === 0, `${argv[0]} status ${r.status}: ${r.stderr}`);
  return JSON.parse(r.stdout);
};
const go = (path) => {
  history.pushState({}, "", path);
  dispatchEvent(new PopStateEvent("popstate"));
};
const $ = (q, root = document) => root?.querySelector(q) || null,
  $$ = (q, root = document) => (root ? [...root.querySelectorAll(q)] : []);
const button = (text, root = document) =>
  $$("button", root).find((b) => b.textContent.trim() === text);
const click = (text, root = document) => {
  const b = button(text, root);
  assert(b, "button " + text + " absent");
  b.click();
};
const fill = (node, text) => {
  assert(node, "missing input");
  const prototype =
    node instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : node instanceof HTMLSelectElement
        ? HTMLSelectElement.prototype
        : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, "value").set.call(node, text);
  node.dispatchEvent(
    new Event(node instanceof HTMLSelectElement ? "change" : "input", {
      bubbles: true,
    }),
  );
};
const key = (node, k, options = {}) =>
  node.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: k,
      bubbles: true,
      cancelable: true,
      ...options,
    }),
  );
const esc = () => key(document.activeElement, "Escape");
const content = () => $("#screen-content .lex-content");
async function open(id = cfg.id) {
  const title = (await engine(["get-doc", id])).title,
    path = "/doc/" + id;
  go(path);
  await until(() => {
    if (location.pathname !== path) {
      go(path);
      return false;
    }
    return content()?.getAttribute("aria-label") === title + " 본문 편집";
  }, "open " + id);
  await pause(100);
}
function append(text, root = content()) {
  root.focus({ preventScroll: true });
  const r = document.createRange();
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let last, node;
  while ((node = walker.nextNode())) if (node.textContent) last = node;
  assert(last, "append text target missing");
  r.setStart(last, last.textContent.length);
  r.collapse(true);
  getSelection().removeAllRanges();
  getSelection().addRange(r);
  assert(document.execCommand("insertText", false, text), "insertText");
}
const clean = async () =>
  until(
    () => !$$(".vb-savestate").some((x) => /중|못|충돌/.test(x.textContent)),
    "clean",
  );
const results = [];
async function test(id, work) {
  try {
    await work();
    results.push({ id, ok: true });
  } catch (error) {
    results.push({ id, ok: false, error: String(error) });
  }
  await post("/progress", results.at(-1));
  if (!results.at(-1).ok) throw Error(id);
}
if (location.pathname.startsWith("/note-window/")) {
  try {
    await until(
      () =>
        $(".note-window .lex-content")?.textContent.includes("새 창 이전 편집"),
      "moved note buffer",
    );
    assert(!$(".tab-bar"), "note window has tabs");
    await post("/note-result", { ok: true });
  } catch (error) {
    await post("/note-result", { ok: false, error: String(error) });
  }
} else {
  let created, projectId, agendaId, newChild;
  try {
    await until(() => $(".calendar-controls"), "boot");
    await test("rail-seven-order-settings-three-local-switches", async () => {
      assert(
        $$(".workspace-rail .nav-item")
          .map((b) => b.textContent.trim())
          .join("|") ===
          "캘린더|즐겨찾기|연결|데일리 리포트|템플릿|휴지통|연동 가이드",
        "rail order",
      );
      const before = (await fetch("/calls").then((r) => r.json())).length;
      $('button.iconbtn[title="설정"]').click();
      await until(() => $(".settings-modal"), "settings");
      assert(
        $$(".set-switch").length === 3 && !$(".set-ask-timeout"),
        "toggle count",
      );
      $(".set-switch input").click();
      assert(
        JSON.parse(localStorage.getItem("withlog.prefs.v1")).state.titleDate ===
          false,
        "immediate preference",
      );
      esc();
      await until(() => !$(".settings-modal"), "settings closed");
      assert(
        (await fetch("/calls").then((r) => r.json())).length === before,
        "settings invoked engine",
      );
    });
    await open();
    await test("note-meta-toc-and-quiet-open", async () => {
      assert($(".doc-meta-history"), "history button");
      await until(() => $(".note-toc button"), "initial headings");
      assert($$(".note-toc button").length === 2, "first heading excluded");
      assert($(".doc-meta-mins-input"), "mins input");
      const calls = await fetch("/calls").then((r) => r.json());
      assert(
        !calls.some((c) => c.args.argv[0] === "update-doc"),
        "open performed write",
      );
    });
    await test("favorite-evaluation-and-minutes-use-current-tokens", async () => {
      $(".doc-meta .fav-toggle").click();
      await until(
        async () => (await engine(["get-doc", cfg.id])).favorite === true,
        "favorite",
      );
      $(".doc-eval-badge").click();
      await until(() => $(".doc-eval-range"), "evaluation popover");
      fill($(".doc-eval-range"), "83");
      fill($(".doc-eval-note"), " 평가 메모 ");
      click("저장", $(".doc-eval-pop"));
      await until(
        async () =>
          (await engine(["get-doc", cfg.id])).evaluation?.score === 83,
        "evaluation",
      );
      const input = $(".doc-meta-mins-input");
      input.focus();
      fill(input, "35");
      input.blur();
      await until(
        async () => (await engine(["get-doc", cfg.id])).mins === 35,
        "minutes",
      );
    });
    await test("C112-untouched-minutes-follow-memory-and-blur-writes-nothing", async () => {
      const current = await engine(["get-doc", cfg.id]);
      await engine(["update-doc", cfg.id, "mins=60", "f1=" + current.precondition]);
      dispatchEvent(new Event("focus"));
      await until(() => $(".doc-meta-mins-input")?.value === "60", "minutes follow outside change");
      const before = (await fetch("/calls").then((r) => r.json())).length;
      $(".doc-meta-mins-input").focus();
      $(".doc-meta-mins-input").blur();
      await pause(600);
      const calls = (await fetch("/calls").then((r) => r.json())).slice(before);
      assert(!calls.some((c) => c.args.argv[0] === "update-doc"), "blur without typing wrote");
      assert((await engine(["get-doc", cfg.id])).mins === 60, "outside minutes overwritten");
      const input = $(".doc-meta-mins-input");
      input.focus();
      fill(input, "45");
      input.blur();
      await until(async () => (await engine(["get-doc", cfg.id])).mins === 45, "typed minutes");
      const after = await engine(["get-doc", cfg.id]);
      await engine(["update-doc", cfg.id, "mins=70", "f1=" + after.precondition]);
      dispatchEvent(new Event("focus"));
      await until(() => $(".doc-meta-mins-input")?.value === "70", "follows memory again after typing");
      // Later checks expect the minutes the previous check set.
      $(".doc-meta-mins-input").focus();
      fill($(".doc-meta-mins-input"), "35");
      $(".doc-meta-mins-input").blur();
      await until(async () => (await engine(["get-doc", cfg.id])).mins === 35, "minutes restored");
    });
    await test("C110-evaluation-escape-keeps-the-preview-open", async () => {
      go("/calendar?mode=day&date=" + cfg.today);
      await until(() => $('.tl-block[data-calendar-note="' + cfg.id + '"]'), "day");
      key($('.tl-block[data-calendar-note="' + cfg.id + '"]'), "Enter");
      await until(() => $(".note-preview .doc-eval-badge"), "preview");
      $(".note-preview .doc-eval-badge").click();
      await until(() => $(".note-preview .doc-eval-pop"), "evaluation popover in preview");
      fill($(".doc-eval-note"), "쓰던 메모");
      key($(".doc-eval-note"), "Escape");
      await until(() => !$(".doc-eval-pop"), "popover closed");
      await pause(100);
      assert($(".note-preview"), "Esc closed the preview with the popover");
      esc();
      await until(() => !$(".note-preview"), "preview closes on its own Esc");
      await open();
    });
    await test("tag-enter-deduplicate-blur-and-backspace", async () => {
      const input = $('[aria-label="태그 추가"]');
      fill(input, " 공유태그 ");
      key(input, "Enter");
      await until(
        async () =>
          (await engine(["get-doc", cfg.id])).tags?.includes("공유태그"),
        "tag",
      );
      fill(input, "공유태그");
      key(input, ",");
      assert($$(".tag-chip").length === 1, "duplicate tag");
      fill(input, "보류");
      input.focus();
      input.blur();
      assert(
        $$(".tag-chip").length === 1 && input.value === "보류",
        "blur created tag",
      );
      fill(input, "");
      key(input, "Backspace");
      await until(
        async () => (await engine(["get-doc", cfg.id])).tags?.length === 0,
        "remove tag",
      );
    });
    await test("comment-markdown-create-edit-delete", async () => {
      await until(() => $('[aria-label="새 코멘트"]'), "thread");
      fill($('[aria-label="새 코멘트"]'), "**화면 댓글**");
      click("추가", $(".note-comment-thread"));
      await until(
        () => $(".comment-body strong")?.textContent === "화면 댓글",
        "comment render",
      );
      click("고치기", $(".comment-row"));
      await until(() => $('[aria-label="코멘트 고치기"]'), "comment edit form");
      fill($('[aria-label="코멘트 고치기"]'), "고친 댓글");
      click("저장", $(".comment-row"));
      await until(
        () => $(".comment-body")?.textContent.includes("고친 댓글"),
        "comment update",
      );
      click("지우기", $(".comment-row"));
      await until(
        () => $('[aria-label="코멘트를 삭제할까요?"]'),
        "delete modal",
      );
      click("삭제", $('[aria-label="코멘트를 삭제할까요?"]'));
      await until(() => !$(".comment-row"), "comment delete");
      await until(
        async () => !(await engine(["list-comments", cfg.id])).comments?.length,
        "disk comment deleted",
      );
    });
    await test("links-candidate-add-and-remove", async () => {
      click("연결 추가", $(".note-links"));
      await until(() => $(".link-candidate"), "link candidates");
      const b = $$(".link-candidate").find((b) =>
        b.textContent.includes("옛 노트"),
      );
      assert(b, "legacy target");
      b.click();
      await until(
        async () => (await engine(["get-doc", cfg.id])).links?.length === 1,
        "link disk",
      );
      $('.note-links [aria-label="연결 끊기"]').click();
      await until(
        async () => (await engine(["get-doc", cfg.id])).links?.length === 0,
        "unlink disk",
      );
    });
    await test("subnote-breadcrumb-history-and-delete-removes-parent-card", async () => {
      await engine(["commit-vault"]);
      await until(() => $(".subnote-row"), "sub list");
      $(".subnote-row").click();
      await until(() => $(".subnote-page .lex-content h1"), "sub editor");
      assert(
        $('.subnote-fullscreen [aria-label="하위 페이지 닫기"]'),
        "required sub markers",
      );
      $(".subnote-page .doc-meta-history").click();
      await until(
        () => $(".nhist-item") && !$(".nhist-restore").disabled,
        "sub history ready",
      );
      esc();
      await until(() => !$(".nhist"), "history escape");
      assert($(".subnote-page"), "history Esc also closed sub");
      click("삭제", $(".subnote-page .doc-meta"));
      await until(
        () => $('[aria-label="하위 페이지를 삭제할까요?"]'),
        "sub delete confirm",
      );
      click("삭제", $('[aria-label="하위 페이지를 삭제할까요?"]'));
      await until(
        () =>
          !$(".subnote-page") &&
          !content().querySelector(".lex-reference-card"),
        "sub deletion",
      );
      await until(
        async () =>
          !(await engine(["get-doc", cfg.id])).body.includes(
            "#sub:" + cfg.childId,
          ),
        "parent card disk",
      );
    });
    await test("create-subnote-from-panel", async () => {
      click("추가", $(".subnotes"));
      await until(
        () =>
          $(".subnote-page .lex-content h1")?.textContent === "새 후속 노트",
        "new child",
      );
      const list = await engine(["list-sub-notes", cfg.id, "bodies=true"]);
      newChild = list.subNotes.find((n) => n.title === "새 후속 노트").id;
      assert(newChild, "new child id");
      $('[aria-label="하위 페이지 닫기"]').click();
      await until(() => !$(".subnote-page"), "close child");
    });
    await test("history-query-keeps-selection-and-real-restore", async () => {
      $(".doc-meta-history").click();
      await until(
        () => $$(".nhist-item").length >= 2 && !$(".nhist-restore").disabled,
        "history list",
      );
      const previous = $$(".nhist-item").at(-1);
      previous.click();
      await until(
        () => $(".nhist-diff")?.textContent.includes("이전 판만의 문장"),
        "older diff",
      );
      const selected = $('.nhist-item[aria-selected="true"]').textContent;
      fill($(".nhist-search"), "current impossible query");
      await until(
        () => $(".nhist-search-note")?.textContent === "판 0개에 있어요",
        "history search",
      );
      assert(
        $(".nhist-diff").textContent.includes("이전 판만의 문장"),
        "search changed selection",
      );
      fill($(".nhist-search"), "");
      await until(() => $$(".nhist-item").length >= 2, "query cleared");
      assert(
        $('.nhist-item[aria-selected="true"]').textContent === selected,
        "selection lost",
      );
      $(".nhist-modes .nhist-mode:last-child").click();
      await until(
        () => $(".nhist-diff")?.textContent.includes("이전 판만의 문장"),
        "previous mode",
      );
      $(".nhist-restore").click();
      await until(
        () => $(".nhist-notice")?.textContent.includes("되돌렸어요"),
        "restore notice",
        30000,
      );
      const n = await engine(["get-doc", cfg.id]);
      assert(
        n.body.includes("이전 판만의 문장") &&
          !n.body.includes("현재 판의 문장"),
        "restore disk",
      );
      assert(
        n.favorite === true && n.mins === 35 && n.evaluation?.score === 83,
        "restore changed metadata",
      );
      $(".nhist-close").click();
      await until(() => !$(".nhist"), "close history");
      await clean();
    });
    await test("calendar-time-keyboard-and-view-navigation", async () => {
      go("/calendar?mode=day&date=" + cfg.today);
      await until(
        () => $('.tl-block[data-calendar-note="' + cfg.id + '"]'),
        "day",
      );
      const b = $('.tl-block[data-calendar-note="' + cfg.id + '"]');
      key(b, "ArrowDown");
      await until(
        async () =>
          (await engine(["get-doc", cfg.id])).created.includes("T09:45"),
        "time disk",
      );
      click("주", $(".calendar-controls"));
      await until(() => $(".week-agendas"), "week");
      click("날짜 열", $(".week-layout-controls"));
      await until(
        () => $$(".week-columns>section").length === 7,
        "week columns",
      );
      click("월", $(".calendar-controls"));
      await until(() => $$(".month-cell").length === 42, "month");
      assert(
        new URL(location.href).searchParams.get("mode") === "month",
        "month URL",
      );
    });
    await test("record-setting-and-search-body-do-not-use-engine", async () => {
      go("/calendar?mode=day&date=" + cfg.today);
      await until(() => $(".tl-block"), "day");
      assert(
        !$('.tl-block[data-calendar-note="ndfixture-00000003"]'),
        "record visible by default",
      );
      $('button.iconbtn[title="설정"]').click();
      await until(() => $(".set-switch"), "settings");
      $$(".set-switch input")[1].click();
      esc();
      await until(
        () => $('.tl-block[data-calendar-note="ndfixture-00000003"]'),
        "record enabled",
      );
      key(window, "k", { metaKey: true, code: "KeyK" });
      await until(() => $(".cmdk-input"), "search");
      const before = await fetch("/calls").then((r) => r.json());
      fill($(".cmdk-input"), "기록 전용 검색어");
      await until(() => $(".cmdk-item .cmdk-snippet"), "body search");
      assert($(".cmdk-item").textContent.includes("숨긴 기록"), "wrong search");
      const calls = await fetch("/calls").then((r) => r.json());
      assert(
        !calls
          .slice(before.length)
          .some((c) => c.args.argv[0] === "search-docs"),
        "search used engine",
      );
      esc();
      await until(() => !$(".cmdk-overlay"), "search closed");
    });
    await test("entity-project-and-agenda-form-create", async () => {
      $('[aria-label="프로젝트 추가"]').click();
      await until(() => $('[aria-label="프로젝트 이름"]'), "project form");
      fill($('[aria-label="프로젝트 이름"]'), "검증 프로젝트");
      click("만들기", $(".ndf"));
      await until(() => !$(".ndf"), "project form saved");
      projectId = (await engine(["list-projects"])).find(
        (p) => p.title === "검증 프로젝트",
      ).id;
      $('[aria-label="아젠다 추가"]').click();
      await until(() => $('[aria-label="아젠다 이름"]'), "agenda form");
      fill($('[aria-label="아젠다 이름"]'), "새 검증 아젠다");
      const project = (await engine(["list-projects"])).find(
        (p) => p.id === projectId,
      );
      fill($$(".ndf select")[1], project.slug);
      click("만들기", $(".ndf"));
      await until(
        () => $(".detail-head h2")?.textContent === "새 검증 아젠다",
        "agenda created",
      );
      agendaId = (await engine(["list-agendas"])).find(
        (a) => a.title === "새 검증 아젠다",
      ).id;
    });
    await test("new-note-template-title-replacement-and-auto-pane", async () => {
      click("문서 추가", $(".detail-head"));
      await until(() => $('[aria-label="새 문서 제목"]'), "new note form");
      fill($('[aria-label="새 문서 제목"]'), "양식으로 만든 노트");
      const selects = $$(".ndf select");
      fill(selects[2], "tplfixture-00000001");
      await until(
        () => $(".ndf .lex-content")?.textContent.includes("템플릿 본문"),
        "template body",
      );
      click("문서 만들기", $(".ndf"));
      await until(
        () =>
          $(".agenda-note .lex-content h1")?.textContent ===
          "양식으로 만든 노트",
        "created pane",
      );
      created = (await engine(["list-docs", "bodies=true"])).find(
        (n) => n.title === "양식으로 만든 노트",
      );
      assert(
        created?.body.includes("템플릿 본문") &&
          !created.body.includes("[제목]"),
        "template replacement",
      );
      assert(location.search.includes(created.id), "new pane URL");
    });
    await test("agenda-facts-save-conflict-diff-overwrite", async () => {
      go("/agenda/" + agendaId);
      await until(() => $(".ag-facts"), "facts panel");
      await until(() => button("적기", $(".ag-facts")), "facts ready");
      click("적기", $(".ag-facts"));
      await until(() => $(".ag-facts-area"), "facts editor");
      fill($(".ag-facts-area"), "처음 사실");
      click("저장", $(".ag-facts"));
      await until(
        () => $(".facts-text")?.textContent === "처음 사실",
        "facts saved",
      );
      click("고치기", $(".ag-facts"));
      await until(() => $(".ag-facts-area"), "facts editor again");
      fill($(".ag-facts-area"), "내가 바꾼 사실");
      await engine(["update-agenda-facts", agendaId, "facts=외부 사실"]);
      click("저장", $(".ag-facts"));
      await until(() => $(".ag-facts-diff-btn"), "facts conflict");
      assert($(".ag-facts-area").value === "내가 바꾼 사실", "draft lost");
      $(".ag-facts-diff-btn").click();
      await until(() => $('[aria-label="사실층 차이"]'), "facts diff");
      assert(
        $('[aria-label="사실층 차이"]').textContent.includes("외부 사실"),
        "diff missing disk",
      );
      click("그래도 저장", $('[aria-label="사실층 차이"]'));
      await until(
        () => $(".facts-text")?.textContent === "내가 바꾼 사실",
        "facts overwrite",
      );
    });
    await test("favorites-category-and-graph-accessible-node", async () => {
      go("/favorites");
      await until(() => $(".fav-row"), "favorite row");
      click("카테고리 없음", $(".fav-row"));
      await until(() => $('[aria-label="카테고리 이름"]'), "category editor");
      fill($('[aria-label="카테고리 이름"]'), "검증 모음");
      key($('[aria-label="카테고리 이름"]'), "Enter");
      await until(
        async () =>
          (await engine(["get-doc", cfg.id])).favoriteCategory === "검증 모음",
        "favorite category",
      );
      go("/connections");
      await until(() => $(".cg-node"), "graph");
      key($(".cg-node"), "Enter");
      await until(() => $(".cx-detail-open"), "graph detail");
      $(".cx-detail-open").click();
      await until(() => $(".note-preview"), "graph preview");
      esc();
      await until(() => !$(".note-preview"), "close graph preview");
      click("목록", $(".list-tools"));
      assert(
        localStorage.getItem("withlog.connMode.v3") === "list",
        "graph persistence",
      );
    });
    await test("template-2000ms-save-unmount-and-delete-dirty", async () => {
      go("/templates?open=tplfixture-00000001");
      await until(
        () => $(".tpl-name-input")?.value === "화면 검증 템플릿",
        "template",
      );
      fill($(".tpl-name-input"), "이름 고친 템플릿");
      await until(
        async () =>
          (await engine(["list-templates"])).some(
            (t) => t.name === "이름 고친 템플릿",
          ),
        "template debounce",
      );
      fill($(".tpl-name-input"), "나가기 저장");
      go("/guide");
      await until(
        async () =>
          (await engine(["list-templates"])).some(
            (t) => t.name === "나가기 저장",
          ),
        "unmount saved",
      );
      go("/templates?open=tplfixture-00000001");
      await until(
        () => $(".tpl-name-input")?.value === "나가기 저장",
        "template return",
      );
      fill($(".tpl-name-input"), "삭제할 미저장 이름");
      click("삭제", $(".template-editor header"));
      await until(
        async () =>
          !(await engine(["list-templates"])).some(
            (t) => t.id === "tplfixture-00000001",
          ),
        "template deleted",
      );
      await pause(2100);
      assert(
        !(await engine(["list-templates"])).some(
          (t) => t.id === "tplfixture-00000001",
        ),
        "dirty template resurrected",
      );
    });
    await test("review-memo-body-save-and-ai-reset", async () => {
      go("/review");
      await until(() => $(".report-row textarea"), "review");
      const box = $(".report-row textarea"),
        id = box.getAttribute("aria-label").replace(/ 메모$/, "");
      fill(box, "회고 메모 저장");
      await until(
        async () =>
          (await engine(["list-reports", "bodies=true"])).some(
            (r) => r.id === id && r.memo === "회고 메모 저장",
          ),
        "memo disk",
      );
      click("편집", $(".report-row"));
      await until(() => $(".report-row .lex-content"), "report editor");
      await pause(1500);
      append("고친 리포트", $(".report-row .lex-content"));
      await pause(100);
      click("저장", $(".report-row"));
      await until(
        async () =>
          (await engine(["list-reports", "bodies=true"])).some(
            (r) => r.id === id && String(r.body || "").includes("고친 리포트"),
          ),
        "report body",
      );
      click("AI 원본", $(".report-row"));
      await until(
        async () =>
          (await engine(["list-reports", "bodies=true"])).find(
            (r) => r.id === id,
          )?.body == null,
        "report reset",
      );
    });
    await test("calendar-report-modal-note-preview-and-return", async () => {
      await engine([
        "save-report",
        "kind=morning",
        "date=" + cfg.today,
        "body=# 달력 리포트\n\n[노트 보기](#doc:" + cfg.id + ")",
        "memo=",
      ]);
      go("/calendar?mode=day&date=" + cfg.today);
      dispatchEvent(new Event("focus"));
      await until(() => button("모닝", $(".calendar-page")), "calendar report");
      click("모닝", $(".calendar-page"));
      await until(
        () => $(".report-dialog .read-markdown a"),
        "report dialog links",
      );
      $(".report-dialog .read-markdown a").click();
      await until(() => $(".note-preview"), "report note preview");
      click("회고로", $(".note-preview"));
      await until(() => !$(".note-preview"), "return from preview");
      assert($(".report-dialog"), "report closed with preview");
      esc();
      await until(() => !$(".report-dialog"), "report close");
    });
    await test("readonly-html-sanitized-no-editor-copy-history", async () => {
      go("/doc/ndfixture-00000004");
      await until(
        () => $(".read-markdown h1")?.textContent === "HTML 읽기",
        "HTML read",
      );
      assert(
        !$(".lex-content") &&
          !$(".doc-meta-history") &&
          !$(".doc-meta-body-copy"),
        "HTML editing controls",
      );
      assert(
        !$(".read-markdown script") && !window.__UNSAFE__,
        "unsanitized HTML",
      );
    });
    await test("external-md-raw-html-local-image-remote-chip-mermaid-relative-link", async () => {
      go("/file?path=" + encodeURIComponent(cfg.external));
      await until(() => $(".ext-md b"), "external HTML");
      assert($(".ext-md b").textContent === "굵게", "raw HTML not retained");
      assert(
        !$(".ext-md script") && !window.__UNSAFE__,
        "external script survived",
      );
      assert($(".ext-md-img-alt")?.textContent === "원격 그림", "remote chip");
      assert(!$('.ext-md img[src^="https:"]'), "remote image retained");
      await until(
        () => $(".ext-md img")?.naturalWidth === 1,
        "local image allowed",
      );
      await until(
        () => $(".ext-md-mermaid svg text"),
        "external Mermaid",
        30000,
      );
      const a = $$(".ext-md a").find((a) => a.textContent === "다음 파일");
      a.click();
      await until(
        () => $(".ext-md h1")?.textContent === "다음 파일",
        "relative MD route",
      );
      assert(
        new URL(location.href).searchParams
          .get("path")
          .endsWith("/external/next.MARKDOWN"),
        "relative path",
      );
      assert(!$(".lex-content"), "external editor");
    });
    await test("trash-note-restore-purge-and-empty", async () => {
      await open(created.id);
      click("삭제", $(".doc-meta"));
      await until(
        () => $('[aria-label="휴지통으로 옮길까요?"]'),
        "trash confirm",
      );
      click("휴지통으로", $('[aria-label="휴지통으로 옮길까요?"]'));
      go("/trash");
      await until(
        () => $(".trash-row-title")?.textContent.includes("양식으로 만든 노트"),
        "trash list",
      );
      click("복구", $(".trash-row"));
      await until(() => !$(".trash-row"), "restore removed trash");
      await until(
        async () => (await engine(["get-doc", created.id])).id === created.id,
        "restored note",
      );
      await open(created.id);
      click("삭제", $(".doc-meta"));
      await until(
        () => $('[aria-label="휴지통으로 옮길까요?"]'),
        "delete again",
      );
      click("휴지통으로", $('[aria-label="휴지통으로 옮길까요?"]'));
      go("/trash");
      await until(() => $(".trash-purge"), "purge action");
      $(".trash-purge").click();
      await until(
        () => $('[aria-label="완전히 삭제할까요?"]'),
        "purge confirm",
      );
      click("삭제", $('[aria-label="완전히 삭제할까요?"]'));
      await until(() => !$(".trash-row"), "purged");
      await open(cfg.legacyId);
      click("삭제", $(".doc-meta"));
      await until(
        () => $('[aria-label="휴지통으로 옮길까요?"]'),
        "legacy trash confirm",
      );
      click("휴지통으로", $('[aria-label="휴지통으로 옮길까요?"]'));
      go("/trash");
      await until(() => $(".trash-empty-btn"), "empty trash action");
      $(".trash-empty-btn").click();
      await until(
        () => $('[aria-label="휴지통을 비울까요?"]'),
        "empty confirm",
      );
      click("삭제", $('[aria-label="휴지통을 비울까요?"]'));
      await until(
        async () => !(await engine(["list-trash"])).length,
        "empty trash disk",
      );
    });
    await test("entity-form-edit-project-delete-and-agenda-delete", async () => {
      go("/calendar");
      await until(
        () => $('[aria-label="새 검증 아젠다 아젠다 편집"]'),
        "agenda edit control",
      );
      $('[aria-label="새 검증 아젠다 아젠다 편집"]').click();
      await until(() => $('[aria-label="아젠다 이름"]'), "edit form");
      fill($('[aria-label="아젠다 이름"]'), "고친 검증 아젠다");
      click("저장", $(".ndf"));
      await until(
        async () =>
          (await engine(["list-agendas"])).some(
            (a) => a.id === agendaId && a.title === "고친 검증 아젠다",
          ),
        "entity edit disk",
      );
      $('[aria-label="검증 프로젝트 프로젝트 편집"]').click();
      await until(() => $('[aria-label="프로젝트 이름"]'), "project edit form");
      click("삭제", $(".ndf"));
      await until(
        () => $('[aria-label="프로젝트를 삭제할까요?"]'),
        "project confirm",
      );
      click("삭제", $('[aria-label="프로젝트를 삭제할까요?"]'));
      await until(
        async () =>
          !(await engine(["list-projects"])).some((p) => p.id === projectId),
        "project deleted",
      );
      assert(
        !(await engine(["list-agendas"])).find((a) => a.id === agendaId)
          .project,
        "project did not unclassify",
      );
      $('[aria-label="고친 검증 아젠다 아젠다 편집"]').click();
      await until(() => $('[aria-label="아젠다 이름"]'), "agenda reopen");
      click("삭제", $(".ndf"));
      await until(
        () => $('[aria-label="아젠다를 삭제할까요?"]'),
        "agenda confirm",
      );
      click("삭제", $('[aria-label="아젠다를 삭제할까요?"]'));
      await until(
        async () =>
          !(await engine(["list-agendas"])).some((a) => a.id === agendaId),
        "agenda deleted",
      );
    });
    await test("move-window-flushes-body-once-before-opening", async () => {
      await open();
      append("새 창 이전 편집");
      click("새 창으로", $(".doc-meta"));
      click("새 창으로", $(".doc-meta"));
      let result;
      await until(async () => {
        result = await fetch("/note-result").then((r) => r.json());
        return result;
      }, "moved note window");
      assert(result.ok, result.error);
      assert(
        (await engine(["get-doc", cfg.id])).body.includes("새 창 이전 편집"),
        "moved buffer missing on disk",
      );
      await until(
        () =>
          location.pathname !== "/doc/" + cfg.id &&
          !$('.tab-item[data-tab-id="' + cfg.id + '"]'),
        "main root pane removed after window event",
      );
      assert((await invoke("note_windows")).length === 1, "duplicate window");
      await invoke("close_note_windows");
      await until(
        async () => !(await invoke("note_windows")).length,
        "moved note closed",
      );
    });
    await test("guide-three-steps-and-required-markers", async () => {
      go("/guide");
      await until(() => $$(".guide-step").length === 3, "guide");
      assert($$(".guide-step button").length === 3, "guide copy");
      const step2 = $$(".guide-step")[1].textContent;
      assert(!step2.includes("withlog-mcp.js") && !step2.includes("mcpServers"), "C119 guide still registers the removed MCP server");
      assert(step2.includes('install --cli "$APP/wl-cli.jar" --sidecar "$APP/wl-sidecar.jar"') && step2.includes("export WITHLOG_VAULT="), "C119 guide lacks the CLI mode setup");
      assert(!$(".set-ask-timeout"), "AI control visible");
    });
    // Last on purpose: a selection left in the template editor disturbs later navigation.
    await test("C57-template-editor-selection-offers-no-comment", async () => {
      const made = await engine(["save-template", "name=C57 템플릿", "type=free", "desc=", "body=고를 글이 있는 본문"]);
      dispatchEvent(new Event("focus"));
      go("/templates?open=" + encodeURIComponent(made.id));
      await until(() => $(".template-editor .lex-content")?.textContent.includes("고를 글"), "template body");
      const root = $(".template-editor .lex-content"),
        walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT),
        node = walker.nextNode();
      root.focus();
      const range = document.createRange();
      range.setStart(node, 0);
      range.setEnd(node, 2);
      getSelection().removeAllRanges();
      getSelection().addRange(range);
      document.dispatchEvent(new Event("selectionchange"));
      await until(() => $(".sel-bar"), "selection bar");
      assert(!$(".comment-add-btn"), "comment button in an editor that takes no comments");
    });
  } catch (error) {
    if (!results.some((r) => !r.ok))
      results.push({ id: "views-driver", ok: false, error: String(error) });
    console.error(error);
  }
  await post("/result", results);
}
