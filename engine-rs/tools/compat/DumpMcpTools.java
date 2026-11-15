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
        out.append("//! MCP 도구 정의 표. 손으로 고치지 않는다.\n");
        out.append("//!\n");
        out.append("//! `engine-rs/tools/compat/gen-mcp-tools.sh`가 Java `McpTools`에서 뽑아 쓴다. 이름과 설명, 입력 스키마(한 줄 JSON 글),\n");
        out.append("//! 쓰기·삭제성 깃발이 Java 서버와 같다.\n\n");
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
