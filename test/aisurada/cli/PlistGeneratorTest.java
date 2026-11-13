package aisurada.cli;

import java.nio.file.Files;
import java.nio.file.Path;

/**
 * {@link PlistGenerator#keepsExistingRegistration}의 시험. 빌드 폴더나 개발 실행의 앱이 켜는
 * {@code AISURADA_SIDECAR_KEEP_REGISTRATION} 스위치가, 쓸 수 있는 기존 등록일 때만 그대로 쓰게 하는지 잰다.
 * 진짜 HOME이나 launchd를 건드리지 않고 임시 폴더의 plist 파일만 읽는다.
 */
public final class PlistGeneratorTest {

    private static int checks;

    private static void check(boolean ok, String what) {
        checks++;
        if (!ok) {
            throw new AssertionError(what);
        }
    }

    private static Path registration(Path dir, String name, Path jar) throws Exception {
        Path plist = dir.resolve(name);
        Files.writeString(plist, PlistGenerator.generatePlist(
                "co.kcd.aisurada.sidecar.test", "/opt/java/bin/java", jar.toString(), "/v", "/s.sock", "/l.log"));
        return plist;
    }

    public static void main(String[] args) throws Exception {
        Path dir = Files.createTempDirectory("ais-plist-test");
        try {
            String on = "1";
            Path jar = Files.writeString(dir.resolve("ais-sidecar.jar"), "jar");
            Path plist = registration(dir, "live.plist", jar);

            check(PlistGenerator.keepsExistingRegistration(on, plist), "스위치가 켜졌고 plist의 jar가 있으면 그대로 쓴다");
            check(PlistGenerator.keepsExistingRegistration(" 1 ", plist), "값 앞뒤 공백은 무시한다");

            for (String off : new String[] {null, "", " ", "0", "true", "yes", "11"}) {
                check(!PlistGenerator.keepsExistingRegistration(off, plist), "스위치 값 '" + off + "'은 켜지 않은 것이다");
            }

            check(!PlistGenerator.keepsExistingRegistration(on, dir.resolve("none.plist")), "plist가 없으면 처음 등록해야 한다");

            Path gone = registration(dir, "gone.plist", dir.resolve("deleted.jar"));
            check(!PlistGenerator.keepsExistingRegistration(on, gone), "plist의 jar가 사라졌으면 다시 쓴다");

            Path folder = Files.createDirectory(dir.resolve("a-folder.jar"));
            check(!PlistGenerator.keepsExistingRegistration(on, registration(dir, "folder.plist", folder)), "jar 자리가 폴더이면 쓸 수 없다");

            Path noJar = Files.writeString(dir.resolve("nojar.plist"), "<plist><dict><string>hello</string></dict></plist>\n");
            check(!PlistGenerator.keepsExistingRegistration(on, noJar), "-jar가 없는 plist는 등록으로 보지 않는다");

            Path broken = Files.writeString(dir.resolve("broken.plist"), "\u0000\u0001 not a plist");
            check(!PlistGenerator.keepsExistingRegistration(on, broken), "읽을 수 없는 plist는 등록으로 보지 않는다");

            Path odd = Files.writeString(dir.resolve("a & b <c>.jar"), "jar");
            check(PlistGenerator.keepsExistingRegistration(on, registration(dir, "odd.plist", odd)),
                    "경로에 &, <, >가 있어도 XML 이스케이프를 되돌려 jar를 찾는다");

            check(PlistGenerator.KEEP_REGISTRATION_ENV.equals("AISURADA_SIDECAR_KEEP_REGISTRATION"), "스위치 이름");
            System.out.println("PlistGeneratorTest OK (" + checks + " checks)");
        } finally {
            try (var walk = Files.walk(dir)) {
                walk.sorted(java.util.Comparator.reverseOrder()).forEach(p -> p.toFile().delete());
            }
        }
    }
}
