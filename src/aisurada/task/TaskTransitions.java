package aisurada.task;

import aisurada.cli.AisException;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;

/**
 * 태스크 상태 전이 규칙. 화면, 러너, 명령이 모두 {@code transition-task}를 거치므로 조건이 한 곳에서만 검사됩니다.
 * 규칙은 {@code docs/developer-guide.md}의 「태스크」 절 표와 같아야 합니다.
 *
 * <p>누가 바꾸는지는 {@code human}(사람), {@code runner}(러너)입니다. 지금은 Done을 사람이 누르지만(결정 1) 러너도 같은 조건으로 부를 수 있게
 * 열어 두어, 나중에 자동 전환을 붙일 때 규칙을 다시 짜지 않게 했습니다.
 */
public final class TaskTransitions {

    public static final String HUMAN = "human";
    public static final String RUNNER = "runner";
    public static final Set<String> ACTORS = Set.of(HUMAN, RUNNER);

    /** 규칙 한 줄. */
    public record Rule(TaskStatus from, TaskStatus to, Set<String> actors) {}

    /** Ready For Develop 조건 하나. */
    public record Check(String item, boolean ok, String label) {}

    private static final Set<String> BOTH = Set.of(HUMAN, RUNNER);
    private static final Set<String> HUMAN_ONLY = Set.of(HUMAN);
    private static final Set<String> RUNNER_ONLY = Set.of(RUNNER);

    public static final List<Rule> RULES;

    static {
        List<Rule> r = new ArrayList<>();
        r.add(new Rule(TaskStatus.DRAFT, TaskStatus.READY, HUMAN_ONLY));
        r.add(new Rule(TaskStatus.READY, TaskStatus.DRAFT, HUMAN_ONLY));
        r.add(new Rule(TaskStatus.READY, TaskStatus.IN_PROGRESS, BOTH));
        r.add(new Rule(TaskStatus.IN_PROGRESS, TaskStatus.WAITING, RUNNER_ONLY));
        r.add(new Rule(TaskStatus.WAITING, TaskStatus.IN_PROGRESS, BOTH));
        r.add(new Rule(TaskStatus.IN_PROGRESS, TaskStatus.IN_REVIEW, RUNNER_ONLY));
        r.add(new Rule(TaskStatus.IN_REVIEW, TaskStatus.DONE, BOTH));
        for (TaskStatus s : List.of(TaskStatus.READY, TaskStatus.IN_PROGRESS, TaskStatus.WAITING, TaskStatus.IN_REVIEW)) {
            r.add(new Rule(s, TaskStatus.FAILED, RUNNER_ONLY));
        }
        for (TaskStatus s : List.of(TaskStatus.DRAFT, TaskStatus.READY, TaskStatus.IN_PROGRESS, TaskStatus.WAITING, TaskStatus.IN_REVIEW)) {
            r.add(new Rule(s, TaskStatus.CANCELLED, HUMAN_ONLY));
        }
        r.add(new Rule(TaskStatus.FAILED, TaskStatus.DRAFT, HUMAN_ONLY));
        r.add(new Rule(TaskStatus.CANCELLED, TaskStatus.DRAFT, HUMAN_ONLY));
        RULES = List.copyOf(r);
    }

    private TaskTransitions() {}

    /** 이 상태에서 갈 수 있는 규칙들(누가 하는지와 무관). */
    public static List<Rule> from(TaskStatus status) {
        List<Rule> out = new ArrayList<>();
        for (Rule r : RULES) {
            if (r.from() == status) out.add(r);
        }
        return out;
    }

    /** Ready For Develop으로 가기 위한 조건. 하나라도 비면 Draft에서 못 나간다. */
    public static List<Check> readiness(Task t) {
        List<Check> out = new ArrayList<>();
        out.add(new Check("title", t.title() != null && !t.title().isBlank(), "요약이 있어요"));
        out.add(new Check("criteria", t.criteria() != null && !t.criteria().isEmpty(), "완료 기준이 1개 이상이에요"));
        out.add(new Check("repo", t.repo() != null && !t.repo().isBlank(), "저장소를 골랐어요"));
        return out;
    }

    /**
     * 전이를 검사한다. 안 되면 까닭을 담은 {@link AisException}이다(규칙에 없으면 409, 조건이 모자라면 409, 누구인지 틀리면 400).
     *
     * @param unfinishedBlockers 이 태스크를 막고 있으면서 아직 Done이 아닌 태스크 키들. Ready For Develop → In Progress 조건이다
     */
    public static void require(Task t, TaskStatus to, String actor, List<String> unfinishedBlockers) {
        if (!ACTORS.contains(actor)) {
            throw new AisException(400, "actor 는 human 이나 runner 여야 한다: " + actor);
        }
        Rule rule = null;
        for (Rule r : RULES) {
            if (r.from() == t.status() && r.to() == to) {
                rule = r;
                break;
            }
        }
        if (rule == null) {
            List<String> allowed = new ArrayList<>();
            for (Rule r : from(t.status())) allowed.add(r.to().label());
            throw new AisException(409, t.status().label() + " 에서 " + to.label() + " 로는 바꿀 수 없다. 갈 수 있는 곳: " + allowed);
        }
        if (!rule.actors().contains(actor)) {
            throw new AisException(409, t.status().label() + " → " + to.label() + " 는 " + String.join("·", rule.actors()) + " 만 바꾼다");
        }
        if (to == TaskStatus.READY) {
            List<String> missing = new ArrayList<>();
            for (Check c : readiness(t)) {
                if (!c.ok()) missing.add(c.item());
            }
            if (!missing.isEmpty()) {
                throw new AisException(409, "Ready For Develop 으로 가려면 먼저 채워야 한다: " + missing);
            }
        }
        if (t.status() == TaskStatus.READY && to == TaskStatus.IN_PROGRESS && !unfinishedBlockers.isEmpty()) {
            throw new AisException(409, "다른 태스크에 막혀 있어 시작하지 못한다(그 태스크가 Done 이 되어야 한다): " + unfinishedBlockers);
        }
    }

    /** 화면에 보일 전이 목록의 한 줄. {@code ok}가 false면 {@code reason}이 까닭이다. */
    public record Option(TaskStatus to, boolean ok, String reason) {}

    /** 이 태스크를 {@code actor}가 바꿀 수 있는 곳들과, 막힌 곳의 까닭. */
    public static List<Option> options(Task t, String actor, List<String> unfinishedBlockers) {
        List<Option> out = new ArrayList<>();
        for (Rule r : from(t.status())) {
            if (!r.actors().contains(actor)) continue;
            try {
                require(t, r.to(), actor, unfinishedBlockers);
                out.add(new Option(r.to(), true, null));
            } catch (AisException e) {
                out.add(new Option(r.to(), false, e.getMessage()));
            }
        }
        return out;
    }
}
