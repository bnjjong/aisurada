package withlog.vault;

import java.io.IOException;
import java.nio.file.*;

/** 28장: 폴더 링크는 내려가지 않고 파일 링크는 대상 크기로 센다. */
public record FileSize(int files, long bytes) {
    public static FileSize measure(Path root) {
        if (Files.isRegularFile(root)) return new FileSize(1, size(root));
        if (!Files.isDirectory(root, LinkOption.NOFOLLOW_LINKS)) return new FileSize(0, 0);
        int count = 0; long bytes = 0;
        // 루트 자체를 못 여는 IOException만 0으로 접는다. 안쪽 순회 오류는 전파한다.
        try (var paths = Files.walk(root)) {
            var iterator = paths.iterator();
            while (iterator.hasNext()) {
                Path path = iterator.next();
                if (Files.isRegularFile(path)) { count++; bytes += size(path); }
            }
        } catch (IOException e) { return new FileSize(0, 0); }
        return new FileSize(count, bytes);
    }
    private static long size(Path path) { try { return Files.size(path); } catch (IOException e) { return 0; } }
}
