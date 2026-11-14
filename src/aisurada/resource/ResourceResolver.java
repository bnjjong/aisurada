package aisurada.resource;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * 「프로젝트가 쓸 수 있는 자원」을 계산하는 <b>유일한 곳</b>입니다. <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <p>규칙: 쓸 수 있는 자원 = 프로젝트 자원 + 참조한 글로벌 자원. 참조하지 않은 글로벌 자원은 여기에 나오지 않고, 화면의 선택 목록, AI가 보는
 * 목록, 실행에 내려 주는 입력은 모두 이 결과만 써야 합니다. 그래서 규칙이 한 곳에서만 지켜집니다.
 *
 * <p>여기서 보는 것은 <b>승인한 판</b>뿐입니다(#53). 자원은 {@link ResourceStore#scanServed}, 참조 목록은
 * {@link ResourceRefs#readApproved}로 읽어서, 사람이 승인하지 않은 변경은 실행 입력에 들어오지 않습니다.
 */
public final class ResourceResolver {

    /** 쓸 수 있는 자원 하나와 그 출처({@code own} = 프로젝트 자원, {@code ref} = 참조한 글로벌 자원). */
    public record Item(Resource resource, String source) {}

    /** 참조는 했는데 글로벌 자원이 없는(지웠거나 휴지통에 있는) 항목. */
    public record Broken(String id, String hint) {}

    /**
     * 쓸 수 있는 에이전트가 필요로 하지만 쓸 수 없는 스킬. {@code skill}은 그 ID로 찾은 글로벌 스킬이고, 없으면 null이다.
     * 글로벌 스킬이면 참조해서 채울 수 있다.
     */
    public record Missing(Resource agent, String skillId, Resource skill) {
        public boolean canReference() {
            return skill != null && skill.isGlobal();
        }
    }

    public record Usable(String project, List<Item> skills, List<Item> agents, List<Broken> broken, List<Missing> missingSkills) {}

    private ResourceResolver() {}

    public static Usable resolve(Path vault, String project) {
        ResourceStore.requireProject(vault, project);
        ResourceStore.Served scan = ResourceStore.scanServed(vault);
        Map<String, Resource> globals = new HashMap<>();
        Map<String, Resource> all = new HashMap<>();
        for (Resource r : scan.items()) {
            all.put(r.id(), r);
            if (r.isGlobal()) globals.put(r.id(), r);
        }

        List<Item> skills = new ArrayList<>();
        List<Item> agents = new ArrayList<>();
        for (Resource r : scan.items()) {
            if (project.equals(r.project())) add(r, "own", skills, agents);
        }
        List<Broken> broken = new ArrayList<>();
        for (ResourceRefs.Entry e : ResourceRefs.readApproved(vault, project)) {
            Resource g = globals.get(e.id());
            if (g == null) {
                broken.add(new Broken(e.id(), e.hint()));
            } else {
                add(g, "ref", skills, agents);
            }
        }

        Set<String> usableSkillIds = new HashSet<>();
        for (Item s : skills) usableSkillIds.add(s.resource().id());
        List<Missing> missing = new ArrayList<>();
        for (Item a : agents) {
            for (String sid : a.resource().skills()) {
                if (!usableSkillIds.contains(sid)) {
                    Resource found = all.get(sid);
                    missing.add(new Missing(a.resource(), sid, found != null && found.kind() == ResourceKind.SKILL ? found : null));
                }
            }
        }
        return new Usable(project, skills, agents, broken, missing);
    }

    private static void add(Resource r, String source, List<Item> skills, List<Item> agents) {
        (r.kind() == ResourceKind.SKILL ? skills : agents).add(new Item(r, source));
    }
}
