import test from "node:test";
import assert from "node:assert/strict";
import {
  plainBody,
  searchTrim,
  findNotes,
  emptySearch,
} from "../src/ui/search.ts";
import {
  displayTitle,
  visibleNotes,
  agendaOrder,
  railAgendaOrder,
  tagCandidates,
  recommendations,
  linkCandidates,
  relativeFile,
  preferences,
  scheduleNotice,
  agendaRepoField,
  railGroupCollapsed,
} from "../src/ui/model.ts";
import { RecentNotes, recentNotes, recentKey } from "../src/ui/recent.ts";
import {
  lanes,
  timeAt,
  addDays,
  canPlaceAt,
  movedTime,
} from "../src/ui/CalendarAgenda.tsx";
import { reportIds } from "../src/ui/OtherPages.tsx";
import { readingHTML } from "../src/ui/ReadMarkdown.tsx";
import { restoreReason } from "../src/ui/History.tsx";
import {
  addLayer,
  topLayer,
  blocksEscape,
  captureEscape,
} from "../src/ui/layers.ts";
import { fixture, fail, deferred, spin } from "./autosave-fixture.mjs";
const note = (id, extra = {}) => ({
  id,
  title: id,
  body: "",
  format: "markdown",
  agendaId: "ag",
  agenda: "demo",
  date: "2026-11-01",
  time: "09:00",
  created: "2026-11-01T09:00:00+09:00",
  ...extra,
});
const memory = () => {
  const v = new Map();
  return { getItem: (k) => v.get(k) ?? null, setItem: (k, x) => v.set(k, x) };
};
test("검색 ID·제목·아젠다/태그·본문 순위와 작성시각 역순", () => {
  const ns = [
    note("id", { title: "다른 글" }),
    note("title", { title: "ID 제목" }),
    note("tags", { tags: ["ID"] }),
    note("body", { body: "여기에 id" }),
    note("z", { title: "id", created: "2026-11-02" }),
    note("blank", { title: "  ", body: "id" }),
  ];
  assert.deepEqual(
    findNotes(ns, [], " ID ").map((h) => [h.note.id, h.rank]),
    [
      ["id", 0],
      ["z", 1],
      ["title", 1],
      ["tags", 2],
      ["body", 3],
    ],
  );
  assert.equal(findNotes(ns, [], "#doc:id/sub")[0].note.id, "id");
  assert.equal(
    findNotes([note("a")], [{ id: "ag", title: "ID 의논" }], "id")[0].rank,
    2,
  );
});
test("검색 양끝 공백에는 정보 구분 문자도 포함", () => {
  assert.equal(searchTrim("\u001c\u001f\ufeff 찾기 \u001e"), "찾기");
  assert.equal(
    findNotes([note("a", { body: "찾기" })], [], "\u001d찾기\u001f").length,
    1,
  );
});
test("Markdown 12단계 평문은 주석·펜스·그림 제외와 표·강조를 처리", () => {
  assert.equal(
    plainBody(
      "<!--c--># 제목\n```js\n숨김\n```\n`코드` ![그림](a) [링크](url)\n> [!TIP] 인용\n- [/] 할 일\n| 이름 | 값 |\n| :--- | ---: |\n| **굵게** | ~~취소~~ |\n___\n*기울임* __강조__",
    ),
    "제목 코드 링크 인용 할 일 이름 값 굵게 취소 기울임 강조",
  );
});
test("HTML 정리와 JSON 하위 텍스트 합치기, 깨진 JSON", () => {
  assert.equal(
    plainBody(
      "<p>a &amp; b&nbsp;&lt;c&gt;</p><script>x()</script><style>hide</style>",
      "html",
    ),
    "a & b <c>",
  );
  assert.equal(
    plainBody(
      JSON.stringify({
        root: {
          children: [
            { children: [{ text: "첫" }, { text: "줄" }] },
            { text: "둘째" },
          ],
        },
      }),
      "json",
    ),
    "첫줄 둘째",
  );
  assert.equal(plainBody("broken", "json"), "");
});
test("본문 캐시 갱신과 UTF16 앞24/뒤76 대목", () => {
  const a = note("cache", { body: "a".repeat(30) + "needle" + "b".repeat(90) });
  assert.equal(
    findNotes([a], [], "needle")[0].snippet,
    "…" + "a".repeat(24) + "needle" + "b".repeat(76) + "…",
  );
  assert.equal(findNotes([{ ...a, body: "없음" }], [], "needle").length, 0);
  assert.equal(
    findNotes([{ ...a, body: "없음", attachText: "needle" }], [], "needle")[0]
      .rank,
    3,
  );
  assert.equal(
    findNotes(
      [{ ...a, body: "<script>needle</script>", format: "html" }],
      [],
      "needle",
    ).length,
    0,
  );
});
test("빈 검색 최근 열기 다음 생성, 중복·열린 노트 제외, 20개", () => {
  const ns = Array.from({ length: 25 }, (_, i) =>
    note("n" + i, { created: String(i).padStart(2, "0") }),
  );
  const hs = emptySearch(
    ns,
    [
      { id: "n2", at: 100 },
      { id: "n3", at: 90 },
    ],
    new Set(["n3", "n24"]),
  );
  assert.equal(hs.length, 20);
  assert.equal(hs[0].note.id, "n2");
  assert.equal(hs[0].group, "최근 연 노트");
  assert.equal(hs[1].note.id, "n23");
  assert.equal(new Set(hs.map((h) => h.note.id)).size, 20);
});
test("최근 노트 읽기 유효성·중복 병합·최신50개", () => {
  const s = memory();
  s.setItem(
    recentKey("/v"),
    JSON.stringify([
      { id: "a", at: 9 },
      { id: "a", at: 5 },
      { id: "bad", at: "x" },
      ...Array.from({ length: 60 }, (_, i) => ({ id: "n" + i, at: i })),
    ]),
  );
  assert.equal(recentNotes(s, "/v").length, 50);
  assert.equal(recentNotes(s, "/v")[0].id, "n59");
  assert.equal(recentNotes(s, "").length, 0);
});
test("최근 열기 10초 문턱·화면 참조 수·다른 창 값 병합", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout", "Date"], now: 100 });
  const s = memory(),
    r = new RecentNotes(s),
    one = r.enter("/v", "a"),
    two = r.enter("/v", "a");
  t.mock.timers.tick(9999);
  assert.equal(r.list("/v").length, 0);
  one();
  assert(r.mounted().has("a"));
  t.mock.timers.tick(1);
  assert.equal(r.list("/v")[0].at, 10100);
  s.setItem(recentKey("/v"), JSON.stringify([{ id: "other", at: 10101 }]));
  t.mock.timers.tick(1);
  two();
  assert.deepEqual(
    new Set(r.list("/v").map((n) => n.id)),
    new Set(["a", "other"]),
  );
  assert.equal(r.mounted().size, 0);
});
test("샘플 현재 화면은 제외하되 최근 저장 안 함", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const s = memory(),
    r = new RecentNotes(s),
    leave = r.enter("", "sample");
  t.mock.timers.tick(10000);
  assert(r.mounted().has("sample"));
  leave();
  assert.equal(s.getItem(recentKey("")), null);
});
test("날짜 제목은 같은 날짜만 한번 붙이고 다른 날짜 보존", () => {
  for (const title of [
    "20261101-제목",
    "2026-11-01 제목",
    "2026.11.01_제목",
    "2026/11/01:제목",
    "제목",
  ])
    assert.equal(displayTitle({ title, date: "2026-11-01" }), "20261101-제목");
  assert.equal(
    displayTitle({ title: "20261101", date: "2026-11-01" }),
    "20261101",
  );
  assert.equal(
    displayTitle({ title: "20261031-제목", date: "2026-11-01" }),
    "20261101-20261031-제목",
  );
  assert.equal(displayTitle({ title: "원문" }, false), "원문");
});
test("기록 숨김은 메모리를 유지하고 아젠다 차례·태그 후보에는 기록 참여", () => {
  const ns = [
    note("a", { tags: ["공통", "z"] }),
    note("r", {
      tier: "record",
      agendaId: "b",
      created: "2026-12-01",
      tags: ["공통", "a"],
    }),
  ];
  assert.deepEqual(
    visibleNotes(ns).map((n) => n.id),
    ["a"],
  );
  assert.equal(ns.length, 2);
  assert.deepEqual(
    agendaOrder(
      [
        { id: "ag", slug: "ag" },
        { id: "b", slug: "b" },
        { id: "empty", slug: "empty" },
      ],
      ns,
    ).map((a) => a.id),
    ["empty", "b", "ag"],
  );
  assert.deepEqual(tagCandidates(ns, []), ["공통", "a", "z"]);
});
test("추천 앞뒤와 공유태그, 연결 후보의 종류·검색·태그 제외", () => {
  const a = note("a", {
      created: "2026-11-01T09",
      tags: ["tag"],
      links: [{ to: "linked", kind: "direct" }],
    }),
    ns = [
      a,
      note("b", { created: "2026-11-01T10" }),
      note("c", { title: "Search", agendaId: "other", tags: ["tag"] }),
      note("linked", { created: "2026-11-01T11", tags: ["tag"] }),
    ];
  const rs = recommendations(a, ns);
  assert.equal(rs[0].note.id, "b");
  assert.equal(rs.find((r) => r.note.id === "c").kind, "attach");
  assert(!rs.some((r) => r.note.id === "linked"));
  assert.deepEqual(
    linkCandidates(a, ns, "direct", "SEARCH", "tag").map((n) => n.id),
    ["c"],
  );
  assert(
    linkCandidates(a, ns, "attach", "", "").some((n) => n.id === "linked"),
  );
});
test("외부 md 상대 링크 한 번 디코드·점 경로, URL과 다른 파일 제외", () => {
  assert.equal(
    relativeFile("/a/b/start.md", "../문서%20하나.MARKDOWN?q=x#h"),
    "/a/문서 하나.MARKDOWN",
  );
  for (const s of ["https://a/x.md", "/x.md", "#x", "x.png", "%ZZ.md"])
    assert.equal(relativeFile("/a/start.md", s), null);
});
test("C193, 목록 5번: 일 보기 화살표는 15분 그대로, 끌기는 끈 거리를 15분 단위로 반올림해 더하고 시작은 23:00에서 소요 시간을 뺀 때까지 가둔다", () => {
  assert.equal(movedTime("14:37", 0, 15, false), "14:52");
  assert.equal(movedTime("14:37", 0, -15, false), "14:22");
  assert.equal(movedTime("14:37", 0, 30, true), "15:07");
  assert.equal(movedTime("14:37", 0, 7, true), "14:37");
  assert.equal(movedTime("14:37", 0, 8, true), "14:52");
  assert.equal(movedTime("22:30", 30, 15, false), "22:30"); // already at the held end: same time, nothing is written
  assert.equal(movedTime("22:00", 30, 15, false), "22:15");
  assert.equal(movedTime("22:40", 60, 15, false), null);
  assert.equal(movedTime("21:30", 60, 60, true), "22:00");
  assert.equal(movedTime("21:30", 60, 90, true), "22:00");
  assert.equal(movedTime("23:30", 0, 15, false), null);
  assert.equal(movedTime("23:30", 0, 40 / 0.8, true), null);
  assert.equal(movedTime("00:05", 0, -15, false), "00:00");
  assert.equal(movedTime("23:00", 0, 15, false), "23:00");
  assert.equal(movedTime("09:00", 90, 8 * 60, true), "17:00");
  assert.equal(movedTime("09:00", 90, 13 * 60, true), "21:30");
});
test("C28: 아젠다 폼은 연 때 저장소가 있던 아젠다에서 비운 칸만 repo= 로 싣는다", () => {
  assert.deepEqual(agendaRepoField("git@x:a.git", ""), { repo: "" });
  assert.deepEqual(agendaRepoField("", ""), {});
  assert.deepEqual(agendaRepoField("", "  "), {});
  assert.deepEqual(agendaRepoField("", "git@x:b.git"), { repo: "git@x:b.git" });
  assert.deepEqual(agendaRepoField("git@x:a.git", "git@x:b.git"), { repo: "git@x:b.git" });
});
test("C178, C197, C199, C200: 읽기 화면은 편집기와 같은 짝을 그리고 스스로는 짝을 짓지 않는다", () => {
  const html = (md, external = false) => readingHTML(md, external).trim();
  assert.equal(html("**(주의)**를 확인"), "<p><strong>(주의)</strong>를 확인</p>");
  assert.equal(html("**주의.** 값은 **`timeout` 을 넘지 않는다.**"), "<p><strong>주의.</strong> 값은 <strong><code>timeout</code> 을 넘지 않는다.</strong></p>");
  assert.equal(html("「**(주의)**」"), "<p>「<strong>(주의)</strong>」</p>");
  assert.equal(html("x**(a)**y"), "<p>x**(a)**y</p>");
  assert.equal(html("*기울임* _밑줄_ ~~취소~~ ==강조=="), "<p><em>기울임</em> <em>밑줄</em> <del>취소</del> <mark>강조</mark></p>");
  assert.equal(html("가_나_다 2 * 3 * 4 `**코드**`"), "<p>가_나_다 2 * 3 * 4 <code>**코드**</code></p>");
  assert.match(html("| **a** | b **c** \\| d |\n|---|---|\n| 1 | 2 |"), /<th><strong>a<\/strong><\/th>\n<th>b <strong>c<\/strong> \| d<\/th>/);
  assert.equal(html("긴 문단의 **굵게\n이어짐** 끝"), "<p>긴 문단의 <strong>굵게 이어짐</strong> 끝</p>");
  assert.equal(html("setext\n===", true), "<h1>setext</h1>");
});
test("C198: 읽기 화면도 블록 안에서 줄을 넘는 서식을 편집기처럼 줄마다 그린다", () => {
  const html = (md, external = false) => readingHTML(md, external).trim().replace(/\n/g, "⏎");
  assert.equal(html("> **굵게\n> 이어짐** 끝"), "<blockquote>⏎<p><strong>굵게</strong>⏎<strong>이어짐</strong> 끝</p>⏎</blockquote>");
  assert.equal(html("- **항목\n  이어짐** 끝"), "<ul>⏎<li><strong>항목</strong>⏎<strong>이어짐</strong> 끝</li>⏎</ul>");
  assert.equal(html("**굵게  \n이어짐** 끝"), "<p><strong>굵게</strong><br><strong>이어짐</strong> 끝</p>");
  assert.equal(html("긴 **문단\n이어짐** 끝", true), "<p>긴 <strong>문단</strong>⏎<strong>이어짐</strong> 끝</p>");
  assert.equal(html("> **a\n>\n> b**"), "<blockquote>⏎<p>**a</p>⏎<p>b**</p>⏎</blockquote>");
});
test("목록 27번(13.4): 되돌리기를 막은 까닭은 첫 commit-vault의 답으로 표의 차례대로 가른다", () => {
  assert.equal(restoreReason(null), "커밋이 실패했어요. 잠시 뒤 다시 해 주세요.");
  assert.equal(restoreReason({ outcome: "failed" }), "커밋이 실패했어요. 잠시 뒤 다시 해 주세요.");
  assert.equal(restoreReason({ outcome: "disabled", blocked: "off" }), "자동 커밋이 꺼져 있거나 vault가 git 저장소가 아니에요.");
  assert.equal(restoreReason({ outcome: "skipped", blocked: "index-lock" }), "vault의 git이 잠겨 있어요(.git/index.lock).");
  assert.equal(restoreReason({ outcome: "skipped", blocked: "in-progress" }), "vault에서 git 병합이나 리베이스가 진행 중이에요.");
  assert.equal(restoreReason({ outcome: "skipped", blocked: "not-main" }), "vault의 git이 main 브랜치에 있지 않아요.");
  assert.equal(restoreReason({ outcome: "skipped" }), "vault에서 다른 git 작업이 돌고 있어요. 잠시 뒤 다시 해 주세요.");
  assert.equal(restoreReason({ outcome: "skipped", blocked: "off" }), "vault에서 다른 git 작업이 돌고 있어요. 잠시 뒤 다시 해 주세요.");
});
test("목록 29번(12.1): 프로젝트가 0개면 접어 둔 「미분류」도 아젠다를 숨기지 않는다", () => {
  assert.equal(railGroupCollapsed(["(미분류)"], 0, "(미분류)"), false);
  assert.equal(railGroupCollapsed(["(미분류)"], 0, ""), false);
  assert.equal(railGroupCollapsed(["(미분류)"], 1, "(미분류)"), true);
  assert.equal(railGroupCollapsed(["", "x"], 2, "__none__"), true);
  assert.equal(railGroupCollapsed(["p1"], 2, "p1"), true);
  assert.equal(railGroupCollapsed(["p1"], 2, "p2"), false);
  assert.equal(railGroupCollapsed([], 2, "(미분류)"), false);
});
test("캘린더 15분 스냅·경계·한 시간 겹침 레인", () => {
  assert.equal(timeAt(67), "01:00");
  assert.equal(timeAt(68), "01:15");
  assert.equal(timeAt(-20), "00:00");
  assert.equal(timeAt(1500), "23:45");
  assert.equal(addDays("2026-12-31", 1), "2027-01-01");
  const l = lanes([
    note("a", { time: "09:00" }),
    note("b", { time: "09:30" }),
    note("c", { time: "10:00" }),
  ]);
  assert.equal(l.count, 2);
  assert.equal(l.map.get("a"), 0);
  assert.equal(l.map.get("b"), 1);
  assert.equal(l.map.get("c"), 0);
});
test("리포트 막대 평일·기존 리포트·월요일 주간 기록", () => {
  assert.deepEqual(reportIds("2026-11-01", [note("a")], []), []);
  assert.deepEqual(
    reportIds("2026-11-02", [note("a", { date: "2026-11-03" })], []),
    ["rv-weekly-2026-11-02"],
  );
  assert.deepEqual(
    reportIds("2026-11-01", [], [{ id: "rv-evening-2026-11-01" }]),
    ["rv-evening-2026-11-01"],
  );
});
test("겹 화면 Esc 최상위와 같은 이벤트의 하위 중복 닫기 방지", () => {
  const a = Symbol(),
    b = Symbol(),
    x = addLayer(a),
    y = addLayer(b),
    e = { key: "Escape" };
  assert.equal(topLayer(), b);
  captureEscape(e);
  y();
  assert.equal(topLayer(), a);
  x();
  assert.equal(topLayer(), undefined);
  assert.equal(blocksEscape(e), true);
  assert.equal(blocksEscape({ key: "Escape" }), false);
});
test("시각 변경 created 즉시 갱신, 실패 때 두 값 동시 복구", async (t) => {
  const d = deferred(),
    f = fixture(t, {
      handler: (a) => (a[0] === "update-doc" ? d.promise : undefined),
    });
  const before = f.ws.notes[0].created,
    work = f.data.updateNote({ id: "a" }, { time: "10:15" });
  assert.match(f.ws.notes[0].created, /T10:15/);
  await spin();
  assert(f.calls.at(-1).includes("time=10:15"));
  assert(!f.calls.at(-1).some((v) => v.startsWith("created=")));
  d.resolve(fail(3));
  await work;
  assert.equal(f.ws.notes[0].time, "09:00");
  assert.equal(f.ws.notes[0].created, before);
});
test("샘플 새 엔티티·템플릿 선택용 id 및 삭제, 호스트 안 부름", async (t) => {
  const f = fixture(t, { workspace: { sample: true } }),
    a = await f.data.saveEntity("agenda", { title: "추가" }),
    tpl = await f.data.saveTemplate({
      name: "추가",
      type: "plan",
      body: "# [제목]",
    });
  assert(f.ws.agendas.some((n) => n.id === a.value.id));
  assert(f.ws.templates.some((n) => n.id === tpl.value.id));
  await f.data.deleteTemplate(tpl.value.id);
  assert(!f.ws.templates.some((n) => n.id === tpl.value.id));
  assert.equal(f.calls.length, 0);
});
test("설정 기본 토글과 깨진 저장소 처리", () => {
  const p = preferences(memory());
  assert.deepEqual(
    [p.titleDate, p.showRecords, p.diagrams],
    [true, false, false],
  );
  assert.equal(preferences({ getItem: () => "{broken" }).titleDate, true);
});

test("리포트 배너는 문서화한 작업별 상태를 읽는다", () => {
  assert.equal(
    scheduleNotice({
      kinds: [
        {
          kind: "morning",
          lastRun: {
            lastFailureDisplay: "이유",
            failureStreak: 2,
            nudge: "다시 확인",
          },
        },
      ],
    }),
    "morning: 이유 다시 확인",
  );
  assert.match(scheduleNotice({ staleWrapper: true }), /다시 설치/);
  assert.equal(scheduleNotice(null), "");
  assert.equal(scheduleNotice({ kinds: [] }), "");
});

test("빈 칸 이동은 긴 카드와의 겹침도 막고 다른 레인은 허용한다", () => {
  const ns = [
      note("a", { time: "09:00", mins: 120 }),
      note("b", { time: "11:00" }),
      note("c", { time: "09:30" }),
    ],
    l = lanes(ns);
  assert.equal(canPlaceAt(ns[1], ns, l, "10:30"), false);
  assert.equal(canPlaceAt(ns[1], ns, l, "11:00"), true);
  assert.equal(canPlaceAt(ns[2], ns, l, "10:30"), true);
});

test("주 보기의 레일 순서는 프로젝트 입력 차례와 미분류 마지막을 지킨다", () => {
  const agendas = [
    { id: "a", slug: "a", project: "second" },
    { id: "b", slug: "b", project: "first" },
    { id: "c", slug: "c" },
  ];
  assert.deepEqual(
    railAgendaOrder(agendas, [], [{ slug: "first" }, { slug: "second" }]).map(
      (a) => a.id,
    ),
    ["b", "a", "c"],
  );
});
