#!/usr/bin/env python3
"""지운 Java 시험(태그 java-engine-final)을 꺼내, 엔진 호출을 진짜 Rust `ais-cli`에 겨누게 고친 사본을 만든다. 이슈 #81.

사용: prepare.py <출력 폴더>

고치는 것은 둘이다.
1. 점검이 어긋나도 멈추지 않고 `Failures.record`로 기록하고 계속 간다(`throw new AssertionError(` 바꾸기). 한 번에 어긋남을 모두 본다.
2. SKIPS: 어긋남이 JVM 특성이라 받아들이기로 한 입력만 시험 사본에서 뺀다. 뒤쪽 점검이 막히지 않게 하려는 것이다.
   무엇을 왜 받아들였는지는 docs/rust-engine-java-test-rerun.md에 있다.
"""
import os
import re
import subprocess
import sys

TAG = "java-engine-final"
TESTS = {
    "ChapterTestSupport": "test/aisurada/ChapterTestSupport.java",
    "ApprovalsTest": "test/aisurada/ApprovalsTest.java",
    "ResourcesTest": "test/aisurada/ResourcesTest.java",
    "TasksTest": "test/aisurada/TasksTest.java",
    "ProjectReposTest": "test/aisurada/ProjectReposTest.java",
    "AgentSessionTest": "test/aisurada/AgentSessionTest.java",
    "SessionCommandsTest": "test/aisurada/SessionCommandsTest.java",
    "CommentCommandsTest": "test/aisurada/CommentCommandsTest.java",
    "AgendaFactsTest": "test/aisurada/AgendaFactsTest.java",
    "Chapter24Test": "test/aisurada/Chapter24Test.java",
    "Chapter31Test": "test/aisurada/Chapter31Test.java",
}

# (시험, 바꿀 글, 새 글). 받아들인 JVM 특성만 뺀다.
SKIPS = [
    # 유니코드 숫자(`Character.digit`)를 판 번호·최소 노트 수·날짜 자리에서 받는 것
    ("Chapter31Test", 'List.of("+2", "002", "۲", "２")', 'List.of("+2", "002")'),
    ("Chapter31Test", 'check(stderr(error(v, 409, "history", "missing", "commit=" + digits)).contains(digits), "unicode commit passes syntax");', ""),
    ("Chapter31Test", 'ok(v, "history", ID, "۲")', 'ok(v, "history", ID, "2")'),
    # 보충 평면 글자(𐐨)의 대소문자 접기
    ("Chapter31Test", 'List.of("I", "k", "σ", "𐐨")', 'List.of("I", "k", "σ")'),
    ("AgendaFactsTest", 'List.of("1", "+1", "００１", "١", "2147483647")', 'List.of("1", "+1", "2147483647")'),
    ("AgendaFactsTest", 'List.of("0", "-0", "０", "-０００")', 'List.of("0", "-0")'),
    ("AgendaFactsTest", 'List.of("-007", "-７", "-2147483648")', 'List.of("-007", "-2147483648")'),
    ("AgendaFactsTest", '"２０２６-０６-１６", ', ""),
    # 확장 연도(음수, +10000 이상) 입력
    ("AgendaFactsTest", 'List.of("0000-02-29", "-0001-01-07", "+10000-01-07", "+00010000-01-07", "+999999999-01-07")', 'List.of("0000-02-29")'),
    # 이슈 #80: 도구 표면의 기록 층 코멘트 읽기는 일부러 막았다
    ("CommentCommandsTest", 'eq(ok(v,"list_comments","{\\"parent\\":\\""+ROOT+"\\"}").get("state"),"loaded","no tier gate");', "// (#80)"),
]


def show(path):
    return subprocess.run(["git", "show", f"{TAG}:{path}"], check=True, capture_output=True, text=True).stdout


def tolerant(text):
    return text.replace("throw new AssertionError(", "aisurada.Failures.record(")


def write(root, rel, text):
    path = os.path.join(root, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)


def main(out):
    for name, path in TESTS.items():
        text = tolerant(show(path))
        for test, old, new in SKIPS:
            if test == name:
                if old not in text:
                    sys.exit(f"{name}: 뺄 글을 못 찾았다(시험 원문이 바뀌었나): {old[:60]}")
                text = text.replace(old, new, 1)
        write(out, f"src/aisurada/{name}.java", text)

    mcp = show("test/aisurada/cli/McpServerTest.java")
    # 서버 프로세스 왕복(3부): Java serve 대신 Rust ais-cli mcp를 띄운다
    real = mcp.replace('List.of(java, "-Duser.home=" + home, "-jar", cliJar, "mcp", "--write")', 'List.of(RUST, "mcp", "--write")')
    real = real.replace('List.of(java, "-Duser.home=" + home, "-jar", cliJar, "mcp")', 'List.of(RUST, "mcp")')
    real = real.replace('new java.util.ArrayList<>(List.of(java, "-Duser.home=" + home, "-jar", cliJar))', 'new java.util.ArrayList<>(List.of(RUST))')
    real = real.replace("public final class McpServerTest {", 'public final class McpServerRust {\n    private static final String RUST = System.getProperty("shim.cli");', 1)
    real = real.replace('ProcessBuilder serverBuilder = new ProcessBuilder(java, "-Duser.home=" + home, "-jar", engineJar, "serve");', 'ProcessBuilder serverBuilder = new ProcessBuilder("sleep", "600");', 1)
    real = real.replace('check(Files.exists(socket), "엔진이 떴다: " + Files.readString(home.resolve("server.err")));', "", 1)
    real = real.replace('"AISURADA_SIDECAR_SOCKET", socket.toString());', '"AISURADA_LOCK_DIR", temp.resolve("locks").toString(), "AISURADA_APPROVALS_DIR", temp.resolve("approvals").toString());', 1)
    real = real.replace("        schemasMatchEngine();\n        exposureRules();\n        protocolWithFakeCaller();\n        sessionFromMeta();\n", "", 1)
    real = real.replace("McpServerTest", "McpServerRust")
    write(out, "src/aisurada/cli/McpServerRust.java", tolerant(real))

    # 규격(2부): 가짜 호출기 대신 진짜 Rust mcp 프로세스에 줄을 보낸다(shim의 McpServer). 가짜 호출기에 기대는 점검은 뺀다
    proto = mcp.replace("McpServerTest", "McpProtocolRust")
    proto = proto.replace("        schemasMatchEngine();\n        exposureRules();\n        protocolWithFakeCaller();\n        sessionFromMeta();\n        realEngineRoundTrip();\n", "        protocolWithFakeCaller();\n", 1)
    proto = re.sub(r"^(\s*)eq\(fake\.calls\.get\(\d\).*$", r"\1// (가짜 호출기 점검 생략)", proto, flags=re.M)
    write(out, "src/aisurada/cli/McpProtocolRust.java", tolerant(proto))


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(sys.argv[1])
