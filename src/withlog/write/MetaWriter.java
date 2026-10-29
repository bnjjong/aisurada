package withlog.write;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.Frontmatter;
import withlog.vault.NoteScanner;
import withlog.vault.StrictUtf8;

import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * 설명서 15.5 메타 쓰기 파이프라인.
 * 선결 조건 검증(7.2), 프론트매터 파싱, 아젠다 및 세션 복원, 도장 판정(1.10),
 * 100만자 크기 상한 검사, 원자적 쓰기(7.4), 색인 갱신을 순서대로 수행합니다.
 */
public final class MetaWriter {

    private MetaWriter() {}

    public record WriteResult(
            String path,
            String preconditionToken,
            StampJudgment.JudgmentResult stampResult
    ) {}

    /**
     * 노트의 메타데이터를 파일에 원자적으로 반영합니다.
     *
     * @param vaultRoot vault 루트 절대 경로
     * @param note 대상 노트 레코드
     * @param providedToken 제공된 f1 토큰 (생략/공백이면 null)
     * @param favorite 새 즐겨찾기 상태
     * @param favoriteCategory 새 즐겨찾기 라벨
     * @param evaluation 새 평가 맵 (null이면 제거 또는 평가 없음)
     * @param links 새 연결 목록
     * @param isToolSurface 도구 표면 호출 여부
     * @param currentSessionId 도구 표면 세션 식별자
     * @return 쓰기 결과 DTO (path, preconditionToken, stampResult)
     */
    public static WriteResult write(
            Path vaultRoot,
            NoteScanner.NoteRecord note,
            String providedToken,
            boolean favorite,
            String favoriteCategory,
            Map<String, Object> evaluation,
            List<Map<String, String>> links,
            boolean isToolSurface,
            String currentSessionId
    ) {
        Path contentPath = note.contentFilePath();
        String targetRelPath = vaultRoot.relativize(contentPath).toString().replace('\\', '/');

        // 1. 선결 조건 토큰 판정 (7.2 step 2, 3, 4, 5)
        byte[] fileBytes = PreconditionChecker.check(vaultRoot, note, providedToken, "메타를 저장하지");

        // 2. 프론트매터 및 본문 분리 (1.6)
        String content;
        try {
            content = StrictUtf8.decodeStrict(fileBytes);
        } catch (Exception e) {
            throw new WlException(500, "노트 본문 파일을 못 읽어 메타를 저장하지 않았다: " + targetRelPath);
        }
        FileParts fileParts = split(content);
        String yamlPart = fileParts.yamlPart();
        String body = fileParts.body();
        boolean fenceFound = fileParts.fenceFound();

        // 4. 프론트매터 확인 (7.2 step 6)
        if (!fenceFound || yamlPart == null || yamlPart.length() > 16_000) {
            throw new WlException(500, "프론트매터가 없어 메타를 저장하지 않았다: " + targetRelPath);
        }

        Map<String, Object> rawYamlMap = Frontmatter.parse(yamlPart);
        if (rawYamlMap == null) {
            throw new WlException(500, "프론트매터가 없어 메타를 저장하지 않았다: " + targetRelPath);
        }

        // 5. 프론트매터 재구성 (15.5 5번)
        // agenda: 파일에 적힌 글자, 없거나 문자열 아니면 path의 2번째 조각
        Object rawAgenda = rawYamlMap.get("agenda");
        String finalAgenda;
        if (rawAgenda instanceof String sa && !sa.isEmpty()) {
            finalAgenda = sa;
        } else {
            String[] parts = targetRelPath.split("/");
            finalAgenda = (parts.length >= 2) ? parts[1] : note.meta().agendaSlug();
        }

        // sessionId: 파일에 적힌 값(다듬어 앞 128자)
        Object rawSid = rawYamlMap.get("sessionId");
        String finalSessionId = null;
        if (rawSid instanceof String ss) {
            String trimmed = Trim.trim(ss);
            if (!trimmed.isEmpty()) {
                finalSessionId = Utf16.cut(trimmed, 128);
            }
        }

        // 11.6: 색인을 거친 제목과 태그는 짝 없는 서로게이트가 ?로 저장돼 있다.
        // 즐겨찾기 분류와 평가는 부르는 쪽이 색인에서 온 값에만 asIndexed를 걸어 넘긴다(15.9의 4번)
        String indexedCategory = favoriteCategory;
        Map<String, Object> indexedEvaluation = evaluation;
        List<String> indexedTags = note.meta().tags() == null ? null
                : note.meta().tags().stream().map(MetaWriter::asIndexed).toList();

        // 도장 판정 (1.10)
        StampJudgment.JudgmentResult judgment = StampJudgment.judge(
                note.format(),
                body,
                rawYamlMap,
                note.meta().id(),
                finalAgenda,
                note.meta().type(),
                asIndexed(note.meta().title()),
                note.meta().created(),
                finalSessionId,
                note.meta().mins(),
                note.meta().tier(),
                note.meta().editedBy(),
                note.meta().editedAt(),
                note.meta().editedSessionId(),
                indexedTags,
                favorite,
                indexedCategory,
                note.meta().template(),
                indexedEvaluation,
                links,
                isToolSurface,
                currentSessionId
        );

        String newFileText = judgment.fileText();

        // 7. 저장 상한 검증 (1.6 714행, 723행: UTF-16 길이가 1,000,000을 넘으면 실패)
        if (newFileText.length() > 1_000_000) {
            throw new WlException(500, "노트가 저장 상한을 넘어 저장하지 않았다: " + targetRelPath);
        }

        // 원자적 파일 쓰기 (7.4)
        byte[] writeBytes = newFileText.getBytes(StandardCharsets.UTF_8);
        withlog.git.VaultSnapshot.beforeWrite(vaultRoot, note.id(), contentPath);
        AtomicWriter.writeAtomic(contentPath, writeBytes);

        // 8. 쓴 뒤의 색인 맞추기(15.5 8번)는 할 일이 없다. 이 엔진은 부를 때마다 디스크를 훑는다(8.6의 4번)

        // 새 선결 조건 토큰 계산
        String newToken = DocToken.compute(writeBytes);

        return new WriteResult(note.vaultRelPath(), newToken, judgment);
    }

    /** 본문 파일 글을 울타리 안의 YAML 조각과 본문으로 가른 결과입니다. */
    public record FileParts(String yamlPart, String body, boolean fenceFound) {}

    /**
     * 본문 파일 글에서 BOM을 떼고 울타리(1.6)로 프론트매터 조각과 본문을 가릅니다.
     * 본문은 닫는 울타리 뒤의 \r\n 1개와 이어서 \n 1개를 뗀 글입니다(1.6의 6번).
     */
    public static FileParts split(String content) {
        if (content.startsWith("\uFEFF")) {
            content = content.substring(1);
        }

        String yamlPart = null;
        String rawBody = "";
        boolean fenceFound = false;

        if (content.startsWith("<!--withlog")) {
            int closeIdx = content.indexOf("\n-->", 11);
            if (closeIdx != -1) {
                yamlPart = content.substring(11, closeIdx);
                rawBody = content.substring(closeIdx + 4);
                fenceFound = true;
            }
        } else if (content.startsWith("---")) {
            int firstLf = content.indexOf('\n', 3);
            if (firstLf != -1) {
                int closeIdx = content.indexOf("\n---", firstLf);
                if (closeIdx != -1) {
                    yamlPart = content.substring(firstLf + 1, closeIdx + 1);
                    rawBody = content.substring(closeIdx + 4);
                    fenceFound = true;
                }
            }
        }

        String body = rawBody;
        if (body.startsWith("\r\n")) {
            body = body.substring(2);
        }
        if (body.startsWith("\n")) {
            body = body.substring(1);
        }
        return new FileParts(yamlPart, body, fenceFound);
    }

    /** 색인(SQLite)에 UTF-8로 넣었다 꺼낸 글자처럼, 짝 없는 서로게이트를 ?로 바꿉니다(11.6). */
    /** 색인에서 온 평가의 메모에 asIndexed를 건 사본입니다. 평가가 없으면 null입니다. */
    public static Map<String, Object> asIndexed(Map<String, Object> evaluation) {
        if (evaluation == null || !(evaluation.get("note") instanceof String n)) {
            return evaluation;
        }
        Map<String, Object> copy = new LinkedHashMap<>(evaluation);
        copy.put("note", asIndexed(n));
        return copy;
    }

    public static String asIndexed(String s) {
        return s == null ? null : new String(Utf16.toUtf8BytesWithQuestionForUnpaired(s), StandardCharsets.UTF_8);
    }
}
