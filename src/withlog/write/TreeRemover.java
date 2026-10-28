package withlog.write;

import withlog.cli.WlException;
import withlog.vault.NoteIndex;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Stream;

/**
 * 설명서 7.4의 지우기.
 * 하위 노트 폴더처럼 휴지통을 안 거치는 자리를 통째로 지웁니다. 심볼릭 링크는 따라가지 않아서 vault 밖 파일이 딸려 사라지지 않습니다.
 */
public final class TreeRemover {

    private TreeRemover() {}

    /**
     * [target]을 지웁니다. 아무것도 없으면(끊긴 링크 포함) 그대로 성공합니다.
     * 폴더면 안의 항목을 모두 모아 경로의 바이트 차례를 거꾸로 하여 안쪽부터 지우고,
     * 도중에 실패하면 되돌리지 않고 `WL-500 지우지 못했다: <받은 자리의 절대 경로>`로 섭니다.
     * 항목을 모으다 안쪽 폴더 목록을 못 읽으면 이 문구로 안 바꾸고 예상 밖 실패로 둡니다(7.6의 11번).
     */
    public static void remove(Path target) {
        if (!Files.exists(target)) {
            return;
        }
        List<Path> entries = new ArrayList<>();
        if (Files.isDirectory(target, LinkOption.NOFOLLOW_LINKS)) {
            try (Stream<Path> walk = Files.walk(target)) {
                walk.forEach(entries::add);
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        } else {
            entries.add(target);
        }
        entries.sort((a, b) -> NoteIndex.compareUtf8Bytes(b.toString(), a.toString()));
        for (Path p : entries) {
            try {
                Files.delete(p);
            } catch (IOException e) {
                throw new WlException(500, "지우지 못했다: " + target);
            }
        }
    }
}
