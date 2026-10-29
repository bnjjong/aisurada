package withlog.write;

import withlog.text.Utf16;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * 설명서 2.4 템플릿 파일의 쓰기 바이트. {@code ---\n}, YAML, {@code ---\n}, 본문 차례로 잇는다.
 * 울타리는 포맷과 상관없이 늘 {@code ---}이고, 본문 뒤에는 아무것도 안 붙인다.
 */
public final class TemplateFileBytes {

    private TemplateFileBytes() {}

    /**
     * 문서 템플릿의 vault 상대 경로 {@code .withlog/templates/doc/<id>.<확장자>}를 돌려줍니다.
     * 확장자는 {@code format}이 markdown이면 md이고, 그 밖은 포맷 이름 그대로다.
     */
    public static String docRelPath(String id, String format) {
        return ".withlog/templates/doc/" + id + "." + ("markdown".equals(format) ? "md" : format);
    }

    /**
     * 리추얼 템플릿의 vault 상대 경로 {@code .withlog/templates/ritual/<rev>.md}를 돌려줍니다.
     * 파일 이름이 id가 아니라 rev라서 같은 rev 2개는 같은 파일이다.
     */
    public static String ritualRelPath(String rev) {
        return ".withlog/templates/ritual/" + rev + ".md";
    }

    /**
     * 문서 템플릿 바이트를 짓습니다. 키 차례는 id, name, type, format, desc, builtin이다(2.4).
     * format은 읽을 때 확장자가 정해서 안 보는 값인데도 적는다. 옛 구현이 적어서 빼면 모든 템플릿 파일이 1줄씩 달라진다.
     */
    public static byte[] renderDoc(String id, String name, String type, String format, String desc,
                                   boolean builtin, String body) {
        Map<String, Object> yaml = new LinkedHashMap<>();
        yaml.put("id", id);
        yaml.put("name", name);
        yaml.put("type", type);
        yaml.put("format", format);
        yaml.put("desc", desc);
        yaml.put("builtin", builtin);
        return render(yaml, body);
    }

    /**
     * 리추얼 템플릿 바이트를 짓습니다. 키 차례는 id, rev, name, icon, format, desc다(2.4).
     */
    public static byte[] renderRitual(String id, String rev, String name, String icon, String format,
                                      String desc, String body) {
        Map<String, Object> yaml = new LinkedHashMap<>();
        yaml.put("id", id);
        yaml.put("rev", rev);
        yaml.put("name", name);
        yaml.put("icon", icon);
        yaml.put("format", format);
        yaml.put("desc", desc);
        return render(yaml, body);
    }

    /** YAML은 1.9로 찍고, 본문의 짝 없는 서로게이트는 UTF-8로 바꿀 때 ?가 된다. */
    private static byte[] render(Map<String, Object> yaml, String body) {
        String content = "---\n" + FrontmatterWriter.dumpGeneralMap(yaml) + "---\n" + body;
        return Utf16.toUtf8BytesWithQuestionForUnpaired(content);
    }
}
