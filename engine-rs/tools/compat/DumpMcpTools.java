package aisurada.cli;

/**
 * Java MCP 서버가 내놓는 도구 정의(이름, 설명, 입력 스키마, 쓰기·삭제성 깃발)를 Rust 소스로 찍는다.
 * `gen-mcp-tools.sh`가 이 출력을 `ais-cli`의 `mcp_tools.rs`로 저장한다. `McpTools`가 패키지 안에서만 보이므로 같은 패키지 이름으로 컴파일한다.
 */
public class DumpMcpTools {
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

    public static void main(String[] args) {
        StringBuilder out = new StringBuilder();
        out.append("//! MCP 도구 정의 표. Java 엔진을 지운 뒤로는 이 파일이 원본이라 직접 고친다.\n");
        out.append("//!\n");
        out.append("//! 처음에는 `engine-rs/tools/compat/gen-mcp-tools.sh`가 Java `McpTools`(태그 `java-engine-final`)에서 뽑아 썼다. 이름과 설명,\n");
        out.append("//! 입력 스키마(한 줄 JSON 글), 쓰기·삭제성 깃발이 기준 Java 서버와 같아야 `mcp_compat`가 어긋남 0으로 끝난다.\n");
        out.append("//! 생성기는 이제 이 파일을 덮어쓰지 않고 `target/oracle-tables/`에 기준 표를 뽑아 견주는 데만 쓴다.\n\n");
        out.append("use crate::mcp::McpTool;\n\n");
        out.append("#[rustfmt::skip]\n");
        out.append("pub const TOOLS: &[McpTool] = &[\n");
        for (McpTools.Tool tool : McpTools.all()) {
            out.append("    McpTool {\n");
            out.append("        name: ").append(literal(tool.name())).append(",\n");
            out.append("        description: ").append(literal(tool.description())).append(",\n");
            out.append("        input_schema: ").append(literal(tool.inputSchema())).append(",\n");
            out.append("        write: ").append(tool.write()).append(",\n");
            out.append("        destructive: ").append(tool.destructive()).append(",\n");
            out.append("    },\n");
        }
        out.append("];\n");
        System.out.print(out);
    }
}
