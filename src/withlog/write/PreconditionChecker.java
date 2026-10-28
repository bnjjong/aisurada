package withlog.write;

import withlog.cli.WlException;
import withlog.vault.NoteScanner;
import withlog.vault.StrictUtf8;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * 설명서 7.2 선결 조건 토큰 판정기.
 * 쓰기 명령(update_doc, set_favorite, evaluate_doc, link_docs, update_sub_note)에서
 * 대상 본문 파일의 존재, 크기, 엄격한 UTF-8, 선결 조건 토큰(MISSING, MALFORMED, STALE)을 순서대로 검증합니다.
 */
public final class PreconditionChecker {

    private PreconditionChecker() {}

    /**
     * 선결 조건 토큰 및 본문 파일 읽기 가능 여부를 검증하고, 읽은 파일 바이트를 반환합니다.
     *
     * @param vaultRoot vault 루트 경로
     * @param note 대상 노트 레코드
     * @param providedToken 호출자가 제공한 f1 토큰 (생략/공백이면 null)
     * @param writeSnippet 실패 문구에 들어갈 동작 조각 (예: "메타를 저장하지", "고치지")
     * @return 파일 전체 바이트
     * @throws WlException WL-500(읽기 실패), WL-412(토큰 누락, 형식 불일치, 변경 충돌)
     */
    public static byte[] check(
            Path vaultRoot,
            NoteScanner.NoteRecord note,
            String providedToken,
            String writeSnippet
    ) {
        Path contentPath = note.contentFilePath();
        String targetRelPath = vaultRoot.relativize(contentPath).toString().replace('\\', '/');

        // 7.2의 2번: 본문 파일 읽기
        // 없음, 1,000,000바이트 초과, 엄격한 UTF-8이 아님, 입출력 오류
        byte[] fileBytes;
        try {
            if (!Files.exists(contentPath) || !Files.isRegularFile(contentPath)) {
                throw new WlException(500, "노트 본문 파일을 못 읽어 " + writeSnippet + " 않았다: " + targetRelPath);
            }
            long size = Files.size(contentPath);
            if (size > 1_000_000) {
                throw new WlException(500, "노트 본문 파일을 못 읽어 " + writeSnippet + " 않았다: " + targetRelPath);
            }
            fileBytes = Files.readAllBytes(contentPath);
            try {
                StrictUtf8.decodeStrict(fileBytes);
            } catch (Exception e) {
                throw new WlException(500, "노트 본문 파일을 못 읽어 " + writeSnippet + " 않았다: " + targetRelPath);
            }
        } catch (WlException e) {
            throw e;
        } catch (IOException e) {
            throw new WlException(500, "노트 본문 파일을 못 읽어 " + writeSnippet + " 않았다: " + targetRelPath);
        }

        // 포맷 확인 (1.2, 7.2 면제 규칙)
        String fileName = contentPath.getFileName().toString();
        boolean isJsonFormat = "note.json".equals(fileName) ||
                (!"comments.json".equals(fileName) && !"ask.json".equals(fileName) && fileName.endsWith(".json"));

        // 7.2의 3번: 토큰이 없고, 본문 파일 이름이 json 포맷이 아님 (MISSING)
        if (providedToken == null || providedToken.isEmpty()) {
            if (!isJsonFormat) {
                throw new WlException(412, buildMissingMessage(targetRelPath));
            }
            return fileBytes;
        }

        // 7.2의 4번: 토큰이 f1:로 시작하지 않음 (MALFORMED)
        if (!providedToken.startsWith("f1:")) {
            throw new WlException(412, buildMalformedMessage(targetRelPath));
        }

        // 7.2의 5번: 읽은 바이트로 계산한 토큰과 글자 그대로 다름 (STALE)
        String currentToken = DocToken.compute(fileBytes);
        if (!providedToken.equals(currentToken)) {
            throw new WlException(412, buildStaleMessage(targetRelPath));
        }

        return fileBytes;
    }

    private static String buildMissingMessage(String targetRelPath) {
        return "선결 조건 토큰이 안 왔어요. 아무것도 쓰지 않았습니다: " + targetRelPath + "\n" +
                "get_doc 으로 이 노트를 읽고, 그 응답의 precondition 을 f1 인자에 그대로 실어 다시 부릅니다.\n" +
                "방금 만들거나 방금 고친 노트면 그 쓰기 응답이 이미 다음 토큰을 줍니다. 다시 읽지 말고 그 값을 씁니다.";
    }

    private static String buildMalformedMessage(String targetRelPath) {
        return "선결 조건 토큰의 모양이 아니에요. 아무것도 쓰지 않았습니다: " + targetRelPath + "\n" +
                "get_doc 응답의 precondition 을 글자 그대로 f1 인자에 되돌립니다. 계산해서 만드는 값이 아닙니다.\n" +
                "해시를 직접 구해 넣지 않습니다. 이 대조가 묻는 것은 그 파일을 방금 읽었는가 입니다.";
    }

    private static String buildStaleMessage(String targetRelPath) {
        return "다른 쪽이 이 노트를 먼저 고쳤어요. 아무것도 쓰지 않았습니다: " + targetRelPath + "\n" +
                "get_doc 으로 다시 읽고, 그 새 본문 위에 자기 변경을 다시 얹은 뒤, 그 응답의 precondition 을 f1 로 실어 보냅니다.\n" +
                "f1 만 갈아 같은 본문으로 다시 부르지 않습니다. 그러면 남이 쓴 글이 사라집니다.";
    }
}
