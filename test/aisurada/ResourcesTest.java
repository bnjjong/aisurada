package aisurada;

import aisurada.engine.EngineResult;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

import static aisurada.ChapterTestSupport.*;

/**
 * 스킬과 에이전트 정의(자원)의 시험입니다. 외부 설명서에 없는 확장이라, 새 영역({@code .aisurada/resources/})이 기존 명령과 공존하는지,
 * 글로벌·프로젝트 두 범위와 ID 참조 규칙(참조하지 않은 것은 쓰지 않는다)이 지켜지는지, 고치기 보호와 이력, 손으로 망가뜨린 파일을 잰다.
 */
public final class ResourcesTest {

    private static final String SID_MARK = "\u0000ais-session-id";
    private static final String AGENT_MARK = "\u0000ais-session-agent";

    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("resources")) {
            Path v = t.vault();
            ok(v, "create-project", "title=앱", "repos=[{\"name\":\"web\",\"path\":\"/Users/me/web\"}]");
            ok(v, "create-project", "title=문서");

            // 1. 만들기: 파일 모양, ID 머리, 도구별 칸 보존, 같은 값으로 다시 저장하면 바뀐 게 없다
            var skill = ok(v, "save-resource", "kind=skill", "scope=global", "name=번역 검수", "description=번역을 검수한다",
                    "body=# 지침\n용어집을 따른다.",
                    "tools={\"claude\":{\"allowed-tools\":[\"Read\"]},\"codex\":{\"model\":\"x\",\"n\":3}}");
            String sid = (String) skill.get("id");
            check(sid.matches("sk[0-9a-z]+-[0-9a-f]{8}"), "스킬 id 는 sk 머리: " + sid);
            eq(skill.get("scope"), "global", "글로벌 범위");
            eq(skill.get("path"), ".aisurada/resources/global/skills/번역-검수/SKILL.md", "스킬은 폴더 하나와 SKILL.md");
            String skillFile = Files.readString(v.resolve((String) skill.get("path")));
            check(skillFile.startsWith("---\nid: " + sid + "\nname: 번역 검수\ndescription: 번역을 검수한다\ntools:\n  claude:\n    allowed-tools:\n      - Read\n  codex:\n    model: x\n    n: 3\n"), "머리말 모양: " + skillFile);
            check(skillFile.contains("editedAt: '2026-10-21T14:05:07+09:00'\n---\n# 지침\n용어집을 따른다.\n"), "시각과 본문: " + skillFile);
            var got = ok(v, "get-resource", sid);
            eq(got.get("body"), "# 지침\n용어집을 따른다.", "본문은 그대로 돌아온다");
            eq(((Map<?, ?>) ((Map<?, ?>) got.get("tools")).get("codex")).get("n"), 3L, "도구 칸의 숫자도 보존");
            String tok = (String) got.get("precondition");
            check(tok.startsWith("f1:"), "f1 토큰");
            byte[] before = Files.readAllBytes(v.resolve((String) skill.get("path")));
            var same = ok(v, "save-resource", "id=" + sid, "f1=" + tok, "name=번역 검수", "body=# 지침\n용어집을 따른다.");
            eq(same.get("changed"), false, "같은 값이면 바뀐 게 없다");
            check(java.util.Arrays.equals(before, Files.readAllBytes(v.resolve((String) skill.get("path")))), "파일 바이트가 그대로");

            // 2. 에이전트: 권한 기본값, 스킬 연결, 파일 모양
            var agent = ok(v, "save-resource", "kind=agent", "scope=global", "name=코드 리뷰어", "description=코드를 리뷰한다",
                    "permission=edit", "skills=[\"" + sid + "\"]", "body=리뷰한다");
            String aid = (String) agent.get("id");
            check(aid.matches("ap[0-9a-z]+-[0-9a-f]{8}"), "에이전트 id 는 ap 머리: " + aid);
            check(Files.readString(v.resolve((String) agent.get("path"))).contains("permission: edit\nskills:\n  - " + sid + "\n"), "에이전트 머리말");
            eq(ok(v, "save-resource", "kind=agent", "scope=global", "name=기본", "description=권한을 안 줌").get("permission"), "read", "권한 기본값은 read");

            // 3. 틀린 값은 AIS-400/404로 거절한다
            String[][] bad = {
                    {"kind=skill|scope=global|name=|description=d", "name 이 필요"},
                    {"kind=skill|scope=global|name=a/b|description=d", "/ 와"},
                    {"kind=skill|scope=global|name=ok|description=", "description 이 필요"},
                    {"kind=skill|scope=global|name=ok2", "description 이 필요"},
                    {"kind=skill|name=ok|description=d", "scope=global 이나 project"},
                    {"kind=skill|scope=global|project=앱|name=ok|description=d", "함께 줄 수 없다"},
                    {"kind=skill|scope=local|name=ok|description=d", "global 이어야"},
                    {"kind=tool|scope=global|name=ok|description=d", "skill 이나 agent"},
                    {"kind=skill|scope=global|name=ok|description=d|permission=read", "에이전트에만"},
                    {"kind=skill|scope=global|name=ok|description=d|skills=[]", "에이전트에만"},
                    {"kind=agent|scope=global|name=ok|description=d|permission=admin", "permission 은"},
                    {"kind=agent|scope=global|name=ok|description=d|skills=abc", "JSON"},
                    {"kind=agent|scope=global|name=ok|description=d|skills=[\"" + aid + "\"]", "스킬 ID"},
                    {"kind=agent|scope=global|name=ok|description=d|skills=[\"skmwzzzzzz-00000000\"]", "쓸 수 있는 스킬이 아니다"},
                    {"kind=agent|scope=global|name=ok|description=d|skills=[\"" + sid + "\",\"" + sid + "\"]", "겹친다"},
                    {"kind=agent|scope=global|name=ok|description=d|repo=web", "글로벌 에이전트에는 repo"},
                    {"kind=skill|scope=global|name=ok|description=d|tools=[1]", "JSON 객체"},
                    {"kind=skill|scope=global|name=ok|description=d|tools={\"Claude\":{}}", "소문자 도구 이름"},
            };
            for (String[] c : bad) {
                var r = error(v, 400, concat("save-resource", c[0].split("\\|")));
                check(stderr(r).contains(c[1]), "거절 문구에 '" + c[1] + "': " + c[0] + " → " + stderr(r));
            }
            error(v, 404, "save-resource", "kind=skill", "project=없는프로젝트", "name=ok", "description=d");

            // 4. 같은 이름: 한 범위 안에서는 하나뿐이고(slug 가 같아도), 범위가 다르면 둘 다 둔다(ID 로 구분)
            check(stderr(error(v, 400, "save-resource", "kind=skill", "scope=global", "name=번역-검수", "description=d")).contains("같은 이름"), "slug 가 같은 이름은 거절");
            var pskill = ok(v, "save-resource", "kind=skill", "project=앱", "name=번역 검수", "description=프로젝트의 것");
            String psid = (String) pskill.get("id");
            check(!psid.equals(sid), "같은 이름이어도 ID 가 다르다");
            eq(pskill.get("path"), ".aisurada/resources/projects/앱/skills/번역-검수/SKILL.md", "프로젝트 자원의 위치");
            ok(v, "save-resource", "kind=agent", "scope=global", "name=번역 검수", "description=스킬과 이름이 같은 에이전트");

            // 5. 규칙 4: 참조하지 않은 글로벌 자원은 「쓸 수 있는 자원」에 안 나온다
            var u0 = ok(v, "usable-resources", "앱");
            eq(names(u0, "skills"), List.of("번역 검수(own)"), "참조 전에는 프로젝트 자원만");
            eq(list0(u0, "agents").size(), 0, "참조 전에는 에이전트가 없다");
            eq(list0(ok(v, "usable-resources", "문서"), "skills").size(), 0, "다른 프로젝트도 참조하기 전에는 없다");
            eq(list0(ok(v, "list-resources", "scope=global"), "resources").size(), 4, "관리 목록(list-resources)에는 글로벌 전부");

            // 6. 참조: 에이전트를 with-skills 로 켜면 필요한 글로벌 스킬도 함께 켠다
            var r1 = ok(v, "set-resource-ref", "앱", aid, "with-skills=true");
            eq(strs(r1, "added"), List.of(aid, sid), "에이전트와 그 스킬이 함께 켜진다");
            String refs = Files.readString(v.resolve(".aisurada/resources/projects/앱/refs.md"));
            check(refs.contains(aid + "  # 코드 리뷰어\n") && refs.contains(sid + "  # 번역 검수\n"), "refs.md 는 ID 와 이름 힌트: " + refs);
            var u1 = ok(v, "usable-resources", "앱");
            eq(names(u1, "skills"), List.of("번역 검수(own)", "번역 검수(ref)"), "같은 이름의 두 스킬이 출처로 구분된다");
            eq(names(u1, "agents"), List.of("코드 리뷰어(ref)"), "참조한 에이전트");
            eq(list0(ok(v, "usable-resources", "문서"), "skills").size(), 0, "참조는 프로젝트마다다");
            eq(strs(ok(v, "set-resource-ref", "앱", aid), "added").size(), 0, "다시 켜도 그대로");

            // 7. 참조 규칙의 거절
            check(stderr(error(v, 400, "set-resource-ref", "앱", psid)).contains("글로벌 자원만"), "프로젝트 자원은 참조하지 않는다");
            error(v, 404, "set-resource-ref", "앱", "skmwzzzzzz-00000000");
            error(v, 404, "set-resource-ref", "없는프로젝트", sid);
            error(v, 400, "set-resource-ref", "앱", "이름만");

            // 8. 이름을 바꿔도 ID 참조는 안 끊긴다. 파일은 옮겨지고 힌트는 다음 참조 때 새 이름이 된다
            tok = (String) ok(v, "get-resource", sid).get("precondition");
            var renamed = ok(v, "save-resource", "id=" + sid, "f1=" + tok, "name=번역 점검");
            eq(renamed.get("changed"), true, "이름이 바뀌었다");
            eq(renamed.get("path"), ".aisurada/resources/global/skills/번역-점검/SKILL.md", "폴더가 옮겨졌다");
            check(!Files.exists(v.resolve(".aisurada/resources/global/skills/번역-검수")), "옛 폴더는 없다");
            eq(names(ok(v, "usable-resources", "앱"), "skills"), List.of("번역 검수(own)", "번역 점검(ref)"), "참조는 새 이름으로 이어진다");
            ok(v, "set-resource-ref", "앱", sid);
            check(Files.readString(v.resolve(".aisurada/resources/projects/앱/refs.md")).contains(sid + "  # 번역 점검\n"), "힌트가 새 이름으로 바뀐다");

            // 9. 고치기 보호: f1 이 없거나 어긋나면 AIS-412(종료 코드 5), 종류·범위는 못 바꾼다
            tok = (String) ok(v, "get-resource", sid).get("precondition");
            EngineResult noToken = run(v, "save-resource", "id=" + sid, "description=바꾼 설명");
            eq(noToken.status(), 5, "f1 이 없으면 종료 코드 5");
            eq(noToken.errorCode(), 412, "f1 이 없으면 AIS-412");
            EngineResult stale = run(v, "save-resource", "id=" + sid, "f1=f1:" + "0".repeat(64), "description=바꾼 설명");
            eq(stale.errorCode(), 412, "어긋난 f1 은 AIS-412");
            check(stderr(error(v, 400, "save-resource", "id=" + sid, "f1=" + tok, "kind=agent")).contains("종류는 바꿀 수 없다"), "종류 변경 거절");
            check(stderr(error(v, 400, "save-resource", "id=" + sid, "f1=" + tok, "project=앱")).contains("범위는 바꿀 수 없다"), "범위 변경 거절");
            check(stderr(error(v, 400, "save-resource", "id=" + sid, "f1=" + tok, "permission=read")).contains("에이전트에만"), "스킬에 에이전트 칸 거절");
            error(v, 404, "save-resource", "id=skmwzzzzzz-00000000", "f1=" + tok, "name=x");

            // 10. 기록: 사람이 고치면 시각만, AI 세션이 고치면 세션 ID(와 Claude 가 아닌 CLI 이름)가 남는다
            var byAi = ok(v, "save-resource", "id=" + sid, "f1=" + tok, "description=AI 가 고친 설명", SID_MARK, "codex-thread-9", AGENT_MARK, "codex");
            eq(byAi.get("editedSession"), "codex-thread-9", "세션 ID 기록");
            eq(byAi.get("editedAgent"), "codex", "CLI 이름 기록");
            var byClaude = ok(v, "save-resource", "id=" + sid, "f1=" + byAi.get("precondition"), "description=클로드가 고친 설명", SID_MARK, "claude-1");
            eq(byClaude.get("editedSession"), "claude-1", "클로드 세션");
            check(!byClaude.containsKey("editedAgent"), "클로드는 CLI 이름을 안 적는다");
            var byHuman = ok(v, "save-resource", "id=" + sid, "f1=" + byClaude.get("precondition"), "description=사람이 고친 설명");
            check(!byHuman.containsKey("editedSession") && !byHuman.containsKey("editedAgent"), "사람이 고치면 세션 기록이 없다");

            // 11. 이력: 고치기 전의 판이 남고, 옛 판으로 되돌릴 수 있다. 30판까지만 둔다
            var vers = ok(v, "list-resource-versions", sid);
            check(list0(vers, "versions").size() >= 4, "이름 변경과 세 번의 고치기가 이력에 남는다: " + list0(vers, "versions").size());
            String oldest = (String) list0(vers, "versions").get(list0(vers, "versions").size() - 1).get("version");
            var old = ok(v, "get-resource", sid, "version=" + oldest);
            eq(old.get("description"), "번역을 검수한다", "가장 옛 판은 처음 설명");
            eq(old.get("precondition"), ok(v, "get-resource", sid).get("precondition"), "옛 판을 읽어도 토큰은 지금 파일의 것");
            var reverted = ok(v, "save-resource", "id=" + sid, "f1=" + old.get("precondition"), "description=" + old.get("description"));
            eq(reverted.get("description"), "번역을 검수한다", "옛 설명으로 되돌렸다");
            error(v, 404, "get-resource", sid, "version=zzzzzz");
            String cur = (String) reverted.get("precondition");
            for (int i = 0; i < 33; i++) {
                cur = (String) ok(v, "save-resource", "id=" + sid, "f1=" + cur, "body=판 " + i).get("precondition");
            }
            eq(list0(ok(v, "list-resource-versions", sid), "versions").size(), 30, "이력은 30판까지");

            // 12. 에이전트가 쓸 스킬: 글로벌 에이전트는 글로벌 스킬만, 프로젝트 에이전트는 자기 프로젝트의 스킬도, 저장소 이름도 검사한다
            check(stderr(error(v, 400, "save-resource", "kind=agent", "scope=global", "name=프로젝트 스킬에 기댐", "description=d", "skills=[\"" + psid + "\"]")).contains("글로벌 에이전트는 글로벌 스킬만"), "글로벌 에이전트는 프로젝트 스킬을 못 쓴다");
            var pagent = ok(v, "save-resource", "kind=agent", "project=앱", "name=웹 담당", "description=웹을 맡는다", "permission=publish",
                    "skills=[\"" + psid + "\"]", "repo=web");
            eq(pagent.get("repo"), "web", "프로젝트 에이전트의 저장소");
            check(stderr(error(v, 400, "save-resource", "kind=agent", "project=앱", "name=없는 저장소", "description=d", "repo=nope")).contains("있는 이름: [web]"), "저장소 이름 검사");
            eq(ok(v, "save-resource", "id=" + pagent.get("id"), "f1=" + pagent.get("precondition"), "repo=").containsKey("repo"), false, "빈 repo 는 지운다");

            // 13. 빠진 스킬: 쓸 수 있는 에이전트가 필요로 하지만 참조하지 않은 글로벌 스킬을 알려 준다
            var other = ok(v, "save-resource", "kind=skill", "scope=global", "name=보안 점검", "description=보안을 본다");
            var needy = ok(v, "save-resource", "kind=agent", "project=앱", "name=보안 담당", "description=보안을 맡는다", "skills=[\"" + other.get("id") + "\"]");
            var u2 = ok(v, "usable-resources", "앱");
            List<Map<String, Object>> missing = list0(u2, "missingSkills");
            eq(missing.size(), 1, "빠진 스킬 하나");
            eq(missing.get(0).get("skillId"), other.get("id"), "빠진 스킬 ID");
            eq(missing.get(0).get("canReference"), true, "글로벌 스킬이라 참조해서 채울 수 있다");
            check(names(u2, "skills").stream().noneMatch(s -> s.startsWith("보안 점검")), "빠진 스킬은 스스로 쓸 수 있게 되지 않는다(규칙 4)");

            // 14. 지우기와 되살리기: 지우면 깨진 참조와 빠진 스킬로 보이고, 되살리면 돌아온다. 스킬 폴더의 다른 파일도 같이 간다
            Files.writeString(v.resolve(".aisurada/resources/global/skills/번역-점검/notes.txt"), "부속 파일");
            var del = ok(v, "delete-resource", sid);
            eq(del.get("trashed"), true, "휴지통으로");
            eq(strs(del, "referencedBy"), List.of("앱"), "누가 참조했는지 알려 준다");
            check(!Files.exists(v.resolve(".aisurada/resources/global/skills/번역-점검")), "제자리에서 사라진다");
            eq(list0(ok(v, "list-resources", "scope=global", "kind=skill"), "resources").stream().filter(r -> sid.equals(r.get("id"))).count(), 0L, "목록에서 빠진다");
            eq(list0(ok(v, "list-resources", "trash=true"), "trash").get(0).get("id"), sid, "휴지통 목록");
            var u3 = ok(v, "usable-resources", "앱");
            eq(list0(u3, "broken").get(0).get("id"), sid, "깨진 참조");
            check(list0(u3, "missingSkills").stream().anyMatch(m -> sid.equals(m.get("skillId")) && Boolean.FALSE.equals(m.get("canReference"))), "지운 스킬은 빠진 스킬(참조 불가)");
            var back = ok(v, "restore-resource", sid);
            eq(back.get("restored"), true, "되살렸다");
            eq(Files.readString(v.resolve(".aisurada/resources/global/skills/번역-점검/notes.txt")), "부속 파일", "부속 파일도 돌아온다");
            eq(list0(ok(v, "usable-resources", "앱"), "broken").size(), 0, "되살리면 참조가 이어진다");
            error(v, 404, "restore-resource", sid);
            ok(v, "delete-resource", sid);
            ok(v, "save-resource", "kind=skill", "scope=global", "name=번역 점검", "description=같은 이름을 새로 만듦");
            check(stderr(error(v, 409, "restore-resource", sid)).contains("같은 이름이 이미 있어"), "제자리가 차 있으면 AIS-409");

            // 15. 손으로 망가뜨린 파일: 목록은 서고 그 파일만 건너뛰며, 모르는 권한은 가장 약한 것으로 읽고, 모르는 도구 칸은 보존한다
            Files.createDirectories(v.resolve(".aisurada/resources/global/skills/망가짐"));
            Files.writeString(v.resolve(".aisurada/resources/global/skills/망가짐/SKILL.md"), "머리말이 없다\n");
            Files.createDirectories(v.resolve(".aisurada/resources/global/skills/복사본"));
            Files.writeString(v.resolve(".aisurada/resources/global/skills/복사본/SKILL.md"), Files.readString(v.resolve((String) other.get("path"))));
            Files.writeString(v.resolve(".aisurada/resources/global/agents/손으로.md"),
                    "---\nid: apmwhand000-0000abcd\nname: 손으로\ndescription: 손으로 쓴 에이전트\npermission: root\ntools:\n  gemini:\n    flag: true\n  claude:\n    a: 1\n---\n본문\n");
            var broken = ok(v, "list-resources", "scope=global");
            List<Object> skipped = (List<Object>) broken.get("skipped");
            check(skipped != null && skipped.size() == 2, "망가진 것과 id 가 겹친 것 두 개를 건너뛴다: " + skipped);
            check(skipped.stream().anyMatch(s -> String.valueOf(s).contains("망가짐/SKILL.md")) && skipped.stream().anyMatch(s -> String.valueOf(s).contains("id 가 다른 자원과 겹친다")), "건너뛴 이유가 경로와 함께 나온다");
            var hand = list0(broken, "resources").stream().filter(r -> "손으로".equals(r.get("name"))).findFirst().get();
            eq(hand.get("permission"), "read", "모르는 권한은 read 로 읽는다");
            var handGet = ok(v, "get-resource", (String) hand.get("id"));
            ok(v, "save-resource", "id=" + hand.get("id"), "f1=" + handGet.get("precondition"), "description=손으로 쓴 에이전트를 고침");
            String handFile = Files.readString(v.resolve((String) hand.get("path")));
            check(handFile.contains("gemini:\n    flag: true\n") && handFile.contains("claude:\n    a: 1\n"), "모르는 도구 칸을 지우지 않는다: " + handFile);
            check(handFile.contains("permission: read\n"), "고치면 모르는 권한이 read 로 굳는다");
            eq(ok(v, "save-resource", "id=" + hand.get("id"), "f1=" + ok(v, "get-resource", (String) hand.get("id")).get("precondition"), "tools={}").containsKey("tools"), false, "tools={} 는 도구 칸을 비운다");

            // 16. 목록 거르기와 차례: 글로벌이 먼저, 그다음 프로젝트 slug, 종류, 이름
            var all = list0(ok(v, "list-resources"), "resources");
            eq(all.get(0).get("scope"), "global", "글로벌이 먼저");
            eq(list0(ok(v, "list-resources", "project=앱"), "resources").stream().map(r -> r.get("project")).distinct().toList(), List.of("앱"), "project 로 거른다");
            eq(list0(ok(v, "list-resources", "scope=global", "kind=agent"), "resources").stream().map(r -> r.get("kind")).distinct().toList(), List.of("agent"), "kind 로 거른다");
            check(stderr(error(v, 400, "list-resources", "foo=1")).contains("모르는 옵션"), "모르는 옵션은 거절");
            error(v, 404, "list-resources", "project=없는프로젝트");

            // 17. 기존 명령과 공존한다: 새 영역이 있어도 노트·프로젝트·아젠다·휴지통 명령이 그대로 돌고, 자원 휴지통은 건드리지 않는다
            ok(v, "create-doc", "agenda=demo", "type=free", "title=자원이 있는 vault의 노트", "body=본문");
            check(success(v, "list-docs") != null, "list-docs");
            check(list(v, "list-projects").size() == 2, "list-projects");
            check(!list(v, "list-agendas").isEmpty(), "list-agendas");
            success(v, "list-trash");
            check(success(v, "check-notes") != null, "check-notes");
            var trashed = ok(v, "delete-resource", (String) other.get("id"));
            success(v, "empty-trash");
            eq(ok(v, "restore-resource", (String) other.get("id")).get("restored"), true, "empty-trash 는 자원 휴지통을 안 비운다");
            ok(v, "delete-project", (String) list(v, "list-projects").stream().filter(p -> "문서".equals(p.get("slug"))).findFirst().get().get("id"));
            check(!trashed.isEmpty(), "delete-project 후에도 엔진이 돈다");
            System.out.println("ResourcesTest OK (" + checks + " checks)");
        }
    }

    // ------------------------------------------------------------------ 도우미

    private static String[] concat(String first, String[] rest) {
        String[] out = new String[rest.length + 1];
        out[0] = first;
        System.arraycopy(rest, 0, out, 1, rest.length);
        return out;
    }

    @SuppressWarnings("unchecked")
    private static List<Map<String, Object>> list0(Map<String, Object> m, String key) {
        return (List<Map<String, Object>>) m.get(key);
    }

    @SuppressWarnings("unchecked")
    private static List<String> strs(Map<String, Object> m, String key) {
        return (List<String>) m.get(key);
    }

    /** 쓸 수 있는 자원 목록을 「이름(출처)」 글로 줄인다. */
    private static List<String> names(Map<String, Object> usable, String key) {
        return list0(usable, key).stream().map(r -> r.get("name") + "(" + r.get("source") + ")").toList();
    }
}
