package withlog.write;

import withlog.text.Utf16;

/**
 * 설명서 21장 프로젝트 파일의 쓰기 규칙. 바이트는 아젠다(20.5)와 같은 꼴이고 키가 4개뿐이다(2.2).
 * {@code .withlog/agendas/} 안에 두지 않는다. 아젠다 읽기가 그 폴더의 .md를 모두 아젠다로 읽어서
 * 프로젝트가 노트 0건짜리 가짜 아젠다로 그려지기 때문이다.
 */
public final class ProjectFile {

    private ProjectFile() {}

    /**
     * 프로젝트 파일의 vault 상대 경로 {@code .withlog/projects/<slug>.md}를 돌려줍니다.
     */
    public static String relPath(String slug) {
        return ".withlog/projects/" + slug + ".md";
    }

    /**
     * 지운 프로젝트 파일을 옮길 vault 상대 경로 {@code .withlog/trash/projects/<slug>-<id>.md}를 돌려줍니다.
     */
    public static String archivedRelPath(String slug, String id) {
        return ".withlog/trash/projects/" + slug + "-" + id + ".md";
    }

    /**
     * 21.5의 쓰는 바이트를 짓습니다. 키는 id, slug, title, hue 4개를 늘 이 차례로 적고, 줄은 1.9로 찍는다.
     * 조건부 키가 없어서 모든 프로젝트 파일의 프론트매터 줄 수가 같다.
     * 닫는 {@code ---\n} 바로 뒤에 {@code summary}를 받은 그대로 붙이고 끝에 줄바꿈을 더하지 않는다.
     */
    public static byte[] render(String id, String slug, String title, String hue, String summary) {
        String frontmatter = FrontmatterWriter.buildAgendaFrontmatter(id, slug, title, hue);
        String content = "---\n" + frontmatter + "---\n" + (summary == null ? "" : summary);
        return Utf16.toUtf8BytesWithQuestionForUnpaired(content);
    }
}
