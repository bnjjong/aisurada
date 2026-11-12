package aisurada;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

import static aisurada.ChapterTestSupport.*;

/**
 * 프로젝트의 저장소 목록({@code repos:})과 아젠다의 {@code projectRepo}의 시험입니다. 외부 설명서에 없는 확장이라, 확장이 없을 때
 * 파일과 응답이 설명서 그대로인지, 줄 읽개가 읽을 수 있는 한 줄로 적히는지, 틀린 값을 거절하고 손으로 고친 파일은 너그럽게 읽는지를 잰다.
 */
public final class ProjectReposTest {

    private static final String REPOS = "[{\"name\":\"app\",\"path\":\"/Users/me/app\",\"github\":\"me/app\",\"githubId\":123},"
            + "{\"name\":\"docs\",\"github\":\"me/docs\",\"githubId\":456},{\"name\":\"local\",\"path\":\"/Users/me/local dir\"}]";

    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("project-repos")) {
            Path v = t.vault();

            // 1. 저장소가 있으면 hue 바로 뒤에 한 줄로 적힌다. 응답과 목록에도 실린다
            var created = ok(v, "create-project", "title=앱", "repos=" + REPOS);
            String slug = (String) created.get("slug");
            String file = Files.readString(project(v, slug));
            check(file.contains("hue: " + created.get("hue") + "\nrepos: [{\"name\":\"app\",\"path\":\"/Users/me/app\",\"github\":\"me/app\",\"githubId\":123},"), "hue 다음에 한 줄 JSON: " + file);
            eq(file.lines().filter(l -> l.startsWith("repos:")).count(), 1L, "repos 줄은 하나");
            var repos = repoList(created);
            eq(repos.size(), 3, "응답의 저장소 수");
            eq(repos.get(0).get("name"), "app", "첫 이름");
            eq(repos.get(0).get("githubId"), 123L, "githubId는 숫자");
            check(!repos.get(1).containsKey("path"), "없는 값은 키째 뺀다");
            check(!repos.get(2).containsKey("github") && !repos.get(2).containsKey("githubId"), "로컬만인 항목");
            var listed = list(v, "list-projects");
            eq(repoList(listed.get(0)), repos, "list-projects도 같은 목록");

            // 2. 저장소가 없으면 설명서 21.5의 4줄 그대로이고 응답에도 repos 키가 없다
            var plain = ok(v, "create-project", "title=저장소 없음");
            String plainFile = Files.readString(project(v, (String) plain.get("slug")));
            eq(plainFile.lines().filter(l -> l.contains(": ") && !l.startsWith("---")).count(), 4L, "프론트매터는 4줄: " + plainFile);
            check(!plainFile.contains("repos"), "repos 줄이 없다");
            check(!plain.containsKey("repos"), "응답에 repos 키가 없다");

            // 3. update-project: 안 줌은 두고, 빈 값과 빈 배열은 지우고, 배열은 통째로 바꾼다
            String id = (String) created.get("id");
            ok(v, "update-project", id, "title=앱 새 이름");
            eq(repoList(list(v, "list-projects").stream().filter(p -> id.equals(p.get("id"))).findFirst().get()).size(), 3, "repos를 안 주면 그대로");
            var replaced = ok(v, "update-project", id, "repos=[{\"name\":\"only\",\"github\":\"me/only\"}]");
            eq(repoList(replaced).size(), 1, "통째로 바뀐다");
            eq(Files.readString(project(v, slug)).lines().filter(l -> l.startsWith("repos:")).count(), 1L, "바뀐 뒤에도 repos 줄은 하나");
            check(!ok(v, "update-project", id, "repos=[]").containsKey("repos"), "빈 배열은 지운다");
            check(!Files.readString(project(v, slug)).contains("repos"), "지우면 줄도 없다");
            ok(v, "update-project", id, "repos=" + REPOS);
            check(!ok(v, "update-project", id, "repos=").containsKey("repos"), "빈 값은 지운다");

            // 4. 틀린 값은 AIS-400으로 거절하고 파일을 안 바꾼다
            ok(v, "update-project", id, "repos=" + REPOS);
            byte[] before = Files.readAllBytes(project(v, slug));
            String[][] bad = {
                    {"abc", "JSON 배열"}, {"{\"name\":\"a\"}", "JSON 배열"}, {"[1]", "객체"},
                    {"[{\"name\":\"a\",\"path\":\"/x\",\"extra\":1}]", "모르는 키"},
                    {"[{\"path\":\"/x\"}]", "name 이 필요"}, {"[{\"name\":\"a/b\",\"path\":\"/x\"}]", "안전한 이름"},
                    {"[{\"name\":\"a\",\"path\":\"/x\"},{\"name\":\"a\",\"path\":\"/y\"}]", "겹친다"},
                    {"[{\"name\":\"a\"}]", "path 나 github"}, {"[{\"name\":\"a\",\"path\":\"relative/dir\"}]", "절대 경로"},
                    {"[{\"name\":\"a\",\"github\":\"noslash\"}]", "owner/name"}, {"[{\"name\":\"a\",\"github\":\"a/b/c\"}]", "owner/name"},
                    {"[{\"name\":\"a\",\"path\":\"/x\",\"githubId\":5}]", "github 와 함께"},
                    {"[{\"name\":\"a\",\"github\":\"me/a\",\"githubId\":0}]", "양의 정수"},
                    {"[{\"name\":\"a\",\"github\":\"me/a\",\"githubId\":\"7\"}]", "양의 정수"},
                    {"[{\"name\":\"a\",\"github\":\"me/a\",\"githubId\":1.5}]", "양의 정수"},
                    {"[{\"name\":5,\"path\":\"/x\"}]", "글이어야"},
            };
            for (String[] c : bad) {
                var r = error(v, 400, "update-project", id, "repos=" + c[0]);
                check(stderr(r).contains(c[1]), "거절 문구에 '" + c[1] + "': " + c[0] + " → " + stderr(r));
            }
            StringBuilder tooMany = new StringBuilder("[");
            for (int i = 0; i < 21; i++) tooMany.append(i > 0 ? "," : "").append("{\"name\":\"r").append(i).append("\",\"path\":\"/p").append(i).append("\"}");
            check(stderr(error(v, 400, "update-project", id, "repos=" + tooMany + "]")).contains("20개까지"), "21개는 거절");
            check(java.util.Arrays.equals(before, Files.readAllBytes(project(v, slug))), "거절하면 파일이 그대로다");
            check(stderr(error(v, 400, "create-project", "title=나쁜 저장소", "repos=abc")).contains("JSON 배열"), "create-project도 거절");
            eq(list(v, "list-projects").size(), 2, "거절한 create-project는 파일을 안 만든다");

            // 5. 손으로 고친 파일은 너그럽게 읽는다: 들여쓴 YAML 목록은 줄 읽개가 못 보니 없는 것이고, 깨진 항목은 건너뛴다
            Path hand = project(v, slug);
            String head = Files.readString(hand).split("repos:")[0];
            Files.writeString(hand, head + "repos:\n  - name: app\n    path: /x\n---\n본문");
            check(!findProject(v, id).containsKey("repos"), "들여쓴 YAML 목록은 줄 읽개에 안 보인다(없는 것)");
            Files.writeString(hand, head + "repos: [not json\n---\n본문");
            check(!findProject(v, id).containsKey("repos"), "깨진 JSON은 없는 것");
            Files.writeString(hand, head + "repos: [{\"name\":\"ok\",\"path\":\"/ok\"},{\"name\":\"bad name/\",\"path\":\"/x\"},{\"name\":\"ok\",\"path\":\"/dup\"},5]\n---\n본문");
            var tolerant = repoList(findProject(v, id));
            eq(tolerant.size(), 1, "좋은 항목만 남고 겹친 이름은 첫 것만");
            eq(tolerant.get(0).get("path"), "/ok", "첫 항목이 이긴다");
            eq(findProject(v, id).get("summary"), "본문", "다른 칸은 그대로 읽힌다");
            ok(v, "update-project", id, "repos=" + REPOS);

            // 6. 아젠다의 projectRepo: 프로젝트의 repos 가운데 하나를 이름으로 가리킨다
            var agenda = ok(v, "create-agenda", "title=저장소 쓰는 아젠다", "project=" + slug, "projectRepo=app");
            eq(agenda.get("projectRepo"), "app", "응답의 projectRepo");
            String agendaFile = Files.readString(v.resolve(".aisurada/agendas/" + agenda.get("slug") + ".md"));
            check(agendaFile.contains("project: " + slug + "\nprojectRepo: app\n"), "project 바로 뒤에 projectRepo: " + agendaFile);
            eq(agendaList(v, (String) agenda.get("id")).get("projectRepo"), "app", "list-agendas도 싣는다");
            var plainAgenda = ok(v, "create-agenda", "title=가리킴 없음", "project=" + slug);
            check(!plainAgenda.containsKey("projectRepo"), "가리키지 않으면 키가 없다");
            check(!Files.readString(v.resolve(".aisurada/agendas/" + plainAgenda.get("slug") + ".md")).contains("projectRepo"), "파일에도 줄이 없다");

            check(stderr(error(v, 400, "create-agenda", "title=x", "projectRepo=app")).contains("project 와 함께"), "project 없이는 거절");
            check(stderr(error(v, 400, "create-agenda", "title=x", "project=없는-프로젝트", "projectRepo=app")).contains("프로젝트를 찾지 못했다"), "없는 프로젝트");
            var unknown = error(v, 400, "create-agenda", "title=x", "project=" + slug, "projectRepo=nope");
            check(stderr(unknown).contains("저장소 nope 이 없다") && stderr(unknown).contains("app"), "없는 이름이면 있는 이름을 알려 준다: " + stderr(unknown));

            // 7. update-agenda: 안 줌은 두고, 빈 값은 지우고, 프로젝트가 바뀌면 낡은 가리킴을 지운다
            String agendaId = (String) agenda.get("id");
            eq(ok(v, "update-agenda", agendaId, "title=이름만 바꿈").get("projectRepo"), "app", "안 주면 그대로");
            eq(ok(v, "update-agenda", agendaId, "projectRepo=docs").get("projectRepo"), "docs", "다른 이름으로 바꾼다");
            check(stderr(error(v, 400, "update-agenda", agendaId, "projectRepo=nope")).contains("저장소 nope 이 없다"), "바꿀 때도 없는 이름은 거절");
            check(!ok(v, "update-agenda", agendaId, "projectRepo=").containsKey("projectRepo"), "빈 값은 지운다");
            ok(v, "update-agenda", agendaId, "projectRepo=app");
            var other = ok(v, "create-project", "title=다른 프로젝트", "repos=[{\"name\":\"app\",\"path\":\"/Users/me/other\"}]");
            var moved = ok(v, "update-agenda", agendaId, "project=" + other.get("slug"));
            check(!moved.containsKey("projectRepo"), "프로젝트를 옮기면 이름이 같아도 낡은 가리킴은 지운다");
            var movedWith = ok(v, "update-agenda", agendaId, "project=" + slug, "projectRepo=app");
            eq(movedWith.get("projectRepo"), "app", "옮기면서 새 이름을 함께 주면 선다");
            var unclassified = ok(v, "update-agenda", agendaId, "project=");
            check(!unclassified.containsKey("project") && !unclassified.containsKey("projectRepo"), "미분류로 보내면 둘 다 없다");
            check(!Files.readString(v.resolve(".aisurada/agendas/" + agenda.get("slug") + ".md")).contains("projectRepo"), "파일에서도 사라진다");

            // 8. 프로젝트가 저장소를 지워도 아젠다 파일은 건드리지 않는다. 읽는 쪽이 낡은 이름을 그대로 보여 주고, 판단은 쓰는 쪽(화면)이 한다
            ok(v, "update-agenda", agendaId, "project=" + slug, "projectRepo=docs");
            ok(v, "update-project", id, "repos=[{\"name\":\"app\",\"path\":\"/Users/me/app\"}]");
            eq(agendaList(v, agendaId).get("projectRepo"), "docs", "프로젝트에서 지워진 이름도 아젠다 파일에는 남는다");

            // 9. 손으로 쓴 projectRepo: 프로젝트가 없으면 없는 것, 안전하지 않은 이름도 없는 것
            Path agendaPath = v.resolve(".aisurada/agendas/" + agenda.get("slug") + ".md");
            String agendaHead = Files.readString(agendaPath).split("project:")[0];
            Files.writeString(agendaPath, agendaHead + "projectRepo: app\n---\n");
            check(!agendaList(v, agendaId).containsKey("projectRepo"), "project 없는 projectRepo는 무시");
            Files.writeString(agendaPath, agendaHead + "project: " + slug + "\nprojectRepo: a/b\n---\n");
            check(!agendaList(v, agendaId).containsKey("projectRepo"), "안전하지 않은 이름은 무시");

            t.done();
        }
    }

    private static Path project(Path v, String slug) {
        return v.resolve(".aisurada/projects/" + slug + ".md");
    }

    private static Map<String, Object> findProject(Path v, String id) {
        return list(v, "list-projects").stream().filter(p -> id.equals(p.get("id"))).findFirst().orElseThrow();
    }

    private static Map<String, Object> agendaList(Path v, String id) {
        return list(v, "list-agendas").stream().filter(a -> id.equals(a.get("id"))).findFirst().orElseThrow();
    }

    @SuppressWarnings("unchecked")
    private static List<Map<String, Object>> repoList(Map<String, Object> project) {
        Object repos = project.get("repos");
        return repos == null ? List.of() : (List<Map<String, Object>>) repos;
    }
}
