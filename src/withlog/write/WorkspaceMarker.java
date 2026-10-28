package withlog.write;

import withlog.cli.WlException;

import java.nio.file.Files;
import java.nio.file.Path;

/**
 * 워크스페이스 표식 검사기.
 * 설명서 1.1 및 7.1에 따라 대상 폴더의 .withlog/workspace.json 존재 여부를 검사합니다.
 */
public final class WorkspaceMarker {

    private WorkspaceMarker() {}

    /**
     * 지정한 vaultRoot에 워크스페이스 표식이 존재하는지 검사합니다.
     * 표식이 없거나 끊긴 링크인 경우 WL-400 예외를 던집니다.
     *
     * @param vaultRoot vault 루트 디렉터리 경로
     * @throws WlException 표식이 없을 때 WL-400
     */
    public static void check(Path vaultRoot) {
        Path markerPath = vaultRoot.resolve(".withlog/workspace.json");
        if (!Files.exists(markerPath)) {
            throw new WlException(400, "이 폴더는 아직 워크스페이스가 아니다: " +
                    vaultRoot.toAbsolutePath().normalize() +
                    ". .withlog/workspace.json 이 없다. 경로가 맞는지 보고, 새로 만들 자리면 init-workspace 를 먼저 부른다.");
        }
    }
}
