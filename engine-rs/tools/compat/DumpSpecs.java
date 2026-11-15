import aisurada.args.ArgsParser;
import aisurada.cli.AisException;
import aisurada.cmd.CommandRegistry;
import aisurada.cmd.CommandSpec;

import java.util.ArrayList;
import java.util.List;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Java 엔진의 명령 명세(이름, 도구 이름, 쓰기·4부 여부, 사용법, 쓸 수 있는 옵션, 자리 인자 수)를 Rust 소스로 찍는다.
 * `gen-spec-table.sh`가 이 출력을 `ais-cli`의 `spec_table.rs`로 저장한다. 옵션과 자리 인자 수는 Java 파서가 내는 오류 글에서 읽는다.
 */
public class DumpSpecs {
    static String literal(String s) {
        StringBuilder sb = new StringBuilder("\"");
        for (char c : s.toCharArray()) {
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> sb.append(c);
            }
        }
        return sb.append('"').toString();
    }

    public static void main(String[] a) {
        Pattern allowedPattern = Pattern.compile("쓸 수 있는 것은 \\[(.*)\\] 뿐이다\\.$");
        Pattern maxPattern = Pattern.compile("받는 자리 인자는 (\\d+) 개인데");
        StringBuilder out = new StringBuilder();
        out.append("//! 명령 명세 표. Java 엔진을 지운 뒤로는 이 파일이 원본이라 직접 고친다.\n");
        out.append("//!\n");
        out.append("//! 처음에는 `engine-rs/tools/compat/gen-spec-table.sh`가 Java 엔진의 `CommandRegistry`와 `ArgsParser`(태그 `java-engine-final`)에서 뽑아 썼다.\n");
        out.append("//! 생성기는 이제 이 파일을 덮어쓰지 않고 `target/oracle-tables/`에 기준 표를 뽑아 견주는 데만 쓴다.\n\n");
        out.append("use crate::registry::Spec;\n\n");
        out.append("#[rustfmt::skip]\n");
        out.append("pub const SPECS: &[Spec] = &[\n");
        for (CommandSpec s : CommandRegistry.SPECS) {
            String name = s.name();
            List<String> allowed = new ArrayList<>();
            try {
                ArgsParser.parse(name, List.of("zzzzzz=1"));
            } catch (AisException e) {
                Matcher m = allowedPattern.matcher(e.getMessage());
                if (m.find() && !m.group(1).isEmpty()) {
                    for (String o : m.group(1).split(", ")) allowed.add(o);
                }
            }
            int max = -1;
            List<String> many = new ArrayList<>();
            for (int i = 0; i < 70; i++) many.add("p" + i);
            try {
                ArgsParser.parse(name, many);
            } catch (AisException e) {
                Matcher m = maxPattern.matcher(e.getMessage());
                if (m.find()) max = Integer.parseInt(m.group(1));
            }
            out.append("    Spec {\n");
            out.append("        name: ").append(literal(name)).append(",\n");
            out.append("        tool: ").append(s.toolName() == null ? "None" : "Some(" + literal(s.toolName()) + ")").append(",\n");
            out.append("        write: ").append(s.isWrite()).append(",\n");
            out.append("        part4: ").append(s.isPart4()).append(",\n");
            out.append("        usage: ").append(literal(s.usage())).append(",\n");
            out.append("        allowed: &[");
            for (int i = 0; i < allowed.size(); i++) out.append(i > 0 ? ", " : "").append(literal(allowed.get(i)));
            out.append("],\n");
            out.append("        max: ").append(max < 0 ? "None" : "Some(" + max + ")").append(",\n");
            out.append("    },\n");
        }
        out.append("];\n");
        System.out.print(out);
    }
}
