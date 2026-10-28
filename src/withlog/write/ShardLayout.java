package withlog.write;

import withlog.config.EngineConfig;

import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.regex.Pattern;

/**
 * 설명서 1.3의 샤드 판정. 새 노트와 옮기는 노트의 부모 폴더가 notes/<slug>인지
 * notes/<slug>/<YYYY-MM>인지를 정합니다. create_doc과 update_doc이 같은 판정을 씁니다.
 */
public final class ShardLayout {

    private ShardLayout() {}

    public static boolean isSharded(Path vaultRoot) {
        // 1.3, 33.1: 뜰 때 명령줄, JVM 속성, 환경변수 차례로 읽어 검증한 설정이 디스크 판정을 덮는다
        EngineConfig config = EngineConfig.active();
        if (config != null && config.shardByMonth() != null) {
            return config.shardByMonth();
        }

        // 디스크 판정 (1.3)
        Path notesDir = vaultRoot.resolve("notes");
        if (!Files.isDirectory(notesDir)) {
            return false;
        }

        Pattern ymPattern = Pattern.compile("^[0-9]{4}-(0[1-9]|1[0-2])$");
        try (DirectoryStream<Path> agendaDirs = Files.newDirectoryStream(notesDir)) {
            for (Path aDir : agendaDirs) {
                if (!Files.isDirectory(aDir)) continue;
                try (DirectoryStream<Path> ymDirs = Files.newDirectoryStream(aDir)) {
                    for (Path ymDir : ymDirs) {
                        if (!Files.isDirectory(ymDir)) continue;
                        if (ymPattern.matcher(ymDir.getFileName().toString()).matches()) {
                            if (containsDirectNote(ymDir)) {
                                return true;
                            }
                        }
                    }
                }
            }
        } catch (IOException ignored) {}
        return false;
    }

    private static boolean containsDirectNote(Path dir) {
        try (DirectoryStream<Path> entries = Files.newDirectoryStream(dir)) {
            for (Path entry : entries) {
                String name = entry.getFileName().toString();
                if (Files.isDirectory(entry)) {
                    if (Files.isRegularFile(entry.resolve("note.md")) ||
                            Files.isRegularFile(entry.resolve("note.json")) ||
                            Files.isRegularFile(entry.resolve("note.html"))) {
                        return true;
                    }
                } else {
                    if (name.endsWith(".md") || name.endsWith(".html") || name.endsWith(".json")) {
                        if (!"comments.json".equals(name) && !"ask.json".equals(name)) {
                            return true;
                        }
                    }
                }
            }
        } catch (IOException ignored) {}
        return false;
    }
}
