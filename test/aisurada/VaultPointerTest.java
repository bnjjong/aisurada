package aisurada;

import aisurada.vault.VaultPointer;

import java.nio.file.Files;
import java.nio.file.Path;

/**
 * 앱이 연 vault를 적어 둔 파일({@link VaultPointer})의 시험입니다. 읽기 규칙, 환경변수와의 우선순위, 쓰기와 읽기의 왕복,
 * 망가진 파일을 없는 것으로 보는지를 잰다. 진짜 HOME은 건드리지 않고 임시 폴더를 홈으로 쓴다.
 */
public final class VaultPointerTest {
    private static int checks;

    private static void check(boolean ok, String what) { checks++; if (!ok) throw new AssertionError(what); }
    private static void eq(Object actual, Object expected, String what) {
        check(java.util.Objects.equals(actual, expected), what + ": 기대 " + expected + ", 실제 " + actual);
    }

    public static void main(String[] args) throws Exception {
        Path home = Files.createTempDirectory("aisurada-pointer-");
        String h = home.toString();
        Path file = VaultPointer.file(h);
        check(file.endsWith("Library/Application Support/aisurada/current-vault.json"), "파일 자리: " + file);

        eq(VaultPointer.read(h), null, "파일이 없으면 null");
        eq(VaultPointer.read(null), null, "홈을 모르면 null");
        eq(VaultPointer.resolve(null, h), null, "환경변수도 파일도 없으면 null");

        // 왕복: 쓴 것을 그대로 읽는다. 공백과 한글이 든 경로도 같다
        VaultPointer.write(h, "/Users/me/df ai/보관함");
        eq(VaultPointer.read(h), "/Users/me/df ai/보관함", "쓴 값을 읽는다");
        check(Files.readString(file).contains("\"updatedAt\":"), "갱신 시각도 적는다");
        eq(Files.list(file.getParent()).count(), 1L, "임시 파일을 남기지 않는다");
        VaultPointer.write(h, "/Users/me/other");
        eq(VaultPointer.read(h), "/Users/me/other", "다시 쓰면 바뀐다");

        // 우선순위: 환경변수가 비지 않았으면 그것, 비었거나 공백이면 파일
        eq(VaultPointer.resolve("/env/vault", h), "/env/vault", "환경변수가 이긴다");
        eq(VaultPointer.resolve("   ", h), "/Users/me/other", "공백뿐인 환경변수는 없는 것");
        eq(VaultPointer.resolve("", h), "/Users/me/other", "빈 환경변수는 없는 것");
        eq(VaultPointer.resolve(null, h), "/Users/me/other", "환경변수가 없으면 앱이 적은 값");

        // 호스트(Rust)가 쓰는 모양도 읽는다: 키 순서, 숫자 갱신 시각
        Files.writeString(file, "{\"vault\":\"/from/host\",\"updatedAt\":1794438764661}\n");
        eq(VaultPointer.read(h), "/from/host", "호스트가 쓴 모양");

        // 망가진 파일은 없는 것으로 본다. 예외로 호출을 죽이지 않는다
        for (String bad : new String[] {"", "not json", "[]", "{}", "{\"vault\":5}", "{\"vault\":\"relative/path\"}", "{\"vault\":\"\"}", "{\"vault\":\"  \"}"}) {
            Files.writeString(file, bad);
            eq(VaultPointer.read(h), null, "망가진 파일은 null: " + bad);
        }
        Files.delete(file);
        Files.createDirectory(file);
        eq(VaultPointer.read(h), null, "파일 자리가 폴더여도 null");

        System.out.println("vault-pointer: " + checks + " checks passed");
    }
}
