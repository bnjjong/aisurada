package withlog.write;

import withlog.text.Utf16;

/** 24.5/24.8: 갈래별 키 순서를 고정하고 본문은 끝 줄바꿈까지 받은 그대로 둔다. */
public final class TemplateFile {
    private TemplateFile() {}

    public static String docPath(String id) { return ".withlog/templates/doc/" + id + ".md"; }
    public static String ritualPath(String rev) { return ".withlog/templates/ritual/" + rev + ".md"; }

    public static byte[] document(String id, String name, String type, String desc, boolean builtin, String body) {
        StringBuilder file = new StringBuilder("---\n");
        string(file, "id", id); string(file, "name", name); string(file, "type", type);
        string(file, "format", "markdown"); string(file, "desc", desc);
        file.append("builtin: ").append(builtin).append("\n---\n").append(body);
        return Utf16.toUtf8BytesWithQuestionForUnpaired(file.toString());
    }

    public static byte[] ritual(String id, String rev, String name, String icon, String format, String desc, String body) {
        StringBuilder file = new StringBuilder("---\n");
        string(file, "id", id); string(file, "rev", rev); string(file, "name", name);
        string(file, "icon", icon); string(file, "format", format); string(file, "desc", desc);
        file.append("---\n").append(body);
        return Utf16.toUtf8BytesWithQuestionForUnpaired(file.toString());
    }

    private static void string(StringBuilder file, String key, String value) {
        // 24장에 명시된 빈 설명과 앞뒤 ASCII 공백의 큰따옴표 예시를 따른다.
        String scalar = value.isEmpty() || value.indexOf('\n') < 0 && (value.startsWith(" ") || value.endsWith(" "))
                ? FrontmatterWriter.dumpDoubleQuoted(value) : FrontmatterWriter.dumpString(value, 1);
        file.append(key).append(": ").append(scalar).append('\n');
    }
}
