package withlog.write;

import withlog.cli.WlException;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

/**
 * 설명서 20.5 지우기 5번의 파일 옮기기. delete-agenda와 delete-project(21.5의 5번)가 같은 걸음으로 옮긴다.
 * 보관물은 휴지통 매니페스트에 안 들고 엔진이 다시 읽지도 않는다(2.6).
 */
public final class TrashArchive {

    private TrashArchive() {}

    /**
     * 5-1부터 5-5까지입니다. {@code source}가 없으면 경고 로그만 남기고 성공한다. 사람이 파일을 손으로
     * 지웠어도 그 항목을 지울 길이 있어야 하기 때문이다. {@code kindLabel}은 로그 줄의 「아젠다」나 「프로젝트」다.
     */
    public static void move(Path source, Path target, String kindLabel) {
        // 5-1, 5-2: 링크는 따라가서 본다. 끊어진 링크는 「없음」이라 그 자리에 남는다
        if (!Files.exists(source)) {
            System.err.println("Warning: 옮길 " + kindLabel + " 파일이 없어 옮기지 않았다: " + source);
            return;
        }
        // 5-3: 옮길 자리가 차 있으면 덮지 않는다
        if (Files.exists(target)) {
            throw new WlException(500, "옮길 자리에 이미 무엇이 있어 옮기지 않았다: " + target);
        }
        // 5-4: 휴지통 폴더를 위쪽까지 만든다. 실패한 조각이 위여도 만들려던 폴더 전체를 찍는다
        Path trashDir = target.getParent();
        try {
            Files.createDirectories(trashDir);
        } catch (IOException e) {
            throw new WlException(500, "폴더를 만들지 못했다: " + trashDir);
        }
        // 5-5: 이름 바꾸기 1번. 폴더는 폴더째, 심볼릭 링크는 링크 자신을 옮긴다
        try {
            Files.move(source, target, StandardCopyOption.ATOMIC_MOVE);
        } catch (IOException e) {
            throw new WlException(500, "옮기지 못했다: " + source + " -> " + target);
        }
    }
}
