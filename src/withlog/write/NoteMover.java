package withlog.write;

import withlog.cli.WlException;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

/**
 * 설명서 7.4의 옮기기와 정식 이름 고르기.
 * 이미 있는 노트 폴더를 정식 이름(1.5)으로 폴더째 옮겨서 attach/, sub/, 코멘트 파일이 함께 가게 합니다.
 */
public final class NoteMover {

    private NoteMover() {}

    /**
     * 정식 이름 고르기(7.4)로 폴더 노트가 쓸 자리를 정하고, 다르면 옮깁니다.
     * 지금 자리와 글자로 같으면 안 옮기고, 지금 자리가 없으면 정식 이름에 새로 쓰게 둡니다.
     * 그 밖에는 정식 이름, <이름>-2 … <이름>-99 가운데 빈 첫 자리로 옮깁니다.
     *
     * @return 옮긴 뒤(또는 쓸) 폴더의 vault 상대 경로
     */
    public static String placeFolder(Path vaultRoot, String currentRel, String canonicalRel) {
        if (canonicalRel.equals(currentRel)) {
            return currentRel;
        }
        Path current = vaultRoot.resolve(currentRel);
        if (!Files.exists(current)) {
            return canonicalRel;
        }
        String chosen = null;
        for (int n = 1; n <= 99; n++) {
            String candidate = n == 1 ? canonicalRel : canonicalRel + "-" + n;
            if (!Files.exists(vaultRoot.resolve(candidate))) {
                chosen = candidate;
                break;
            }
        }
        if (chosen == null) {
            throw new WlException(500, "옮길 자리를 못 찾았다(같은 이름이 99 개까지 차 있다): " + canonicalRel);
        }
        move(current, vaultRoot.resolve(chosen));
        return chosen;
    }

    /** 파일이나 폴더 1개를 이름 바꾸기 1번으로 옮깁니다(7.4의 옮기기 1~3번). */
    public static void move(Path source, Path target) {
        Path src = source.toAbsolutePath().normalize();
        Path dst = target.toAbsolutePath().normalize();
        if (Files.exists(dst)) {
            throw new WlException(500, "옮길 자리에 이미 무엇이 있어 옮기지 않았다: " + dst);
        }
        Path parent = dst.getParent();
        if (parent != null && !Files.isDirectory(parent)) {
            try {
                Files.createDirectories(parent);
            } catch (IOException e) {
                throw new WlException(500, "폴더를 만들지 못했다: " + parent);
            }
        }
        try {
            Files.move(src, dst, StandardCopyOption.ATOMIC_MOVE);
        } catch (IOException e) {
            throw new WlException(500, "옮기지 못했다: " + src + " -> " + dst);
        }
    }
}
