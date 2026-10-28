package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Uri;
import withlog.vault.*;

import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * get_doc 및 get-doc 명령 실행기.
 * 설명서 11장에 따라 단일 노트 조회 및 규격 JSON 응답을 생성합니다.
 */
public final class GetDoc {
    private static final Pattern ID_PATTERN = withlog.text.IdShape.NOTE;

    private GetDoc() {}

    /**
     * get_doc 또는 get-doc 명령을 실행하여 응답 객체(Map)를 생성합니다.
     * @param vaultRoot vault 루트 절대 경로
     * @param parsed 파싱된 인자 객체
     * @param agendaTable 아젠다 매핑 테이블
     * @param scanResult 노트 훑기 결과
     * @return 규격에 맞춘 응답 Map
     */
    public static Map<String, Object> execute(
            Path vaultRoot,
            ParsedArgs parsed,
            AgendaTable agendaTable,
            NoteScanner.ScanResult scanResult
    ) {
        if (parsed.positionalArgs().isEmpty()) {
            throw new WlException(400, "인자가 하나 필요하다: get-doc <노트 id>");
        }

        String rawId = parsed.positionalArgs().get(0);
        if (rawId == null || Trim.isBlank(rawId)) {
            throw new WlException(400, "인자가 하나 필요하다: get-doc <노트 id>");
        }

        if (!ID_PATTERN.matcher(rawId).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rawId);
        }

        NoteScanner.NoteRecord note = scanResult.notesById().get(rawId);
        if (note == null) {
            throw new WlException(404, "Note not found: " + rawId);
        }

        // 11.5 & 11.9의 6·13번: 메인 노트의 HTML 첨부를 읽음 (입출력 오류면 예상 밖 실패)
        // 도구 표면에서 기록 층 노트를 WL-409로 거절하기 전에 이 읽기가 먼저임 (11.9 13번)
        AttachText.extract(vaultRoot, note);

        // 도구 표면이고 층이 record인 경우 WL-409 거절
        if (parsed.isToolSurface() && "record".equals(note.meta().tier())) {
            throw new WlException(409, "도구 표면이 읽을 수 없는 노트다(층=record): " + rawId +
                    ". 기계가 남긴 기록 노트는 앱에서만 열립니다.");
        }

        // 경로 조각 검사 (코멘트 경로 생성 시 빈 조각 검사, 7.4 및 11.4 16번)
        String[] pathSegments = note.vaultRelPath().split("/");
        Path current = vaultRoot;
        for (String seg : pathSegments) {
            current = PathJoin.join(current, seg);
        }

        // 코멘트 읽기 (단일 파일 노트는 absent)
        Map<String, Object> commentsEnvelope;
        if (note.isSingleFile()) {
            commentsEnvelope = new LinkedHashMap<>();
            commentsEnvelope.put("state", "absent");
        } else {
            Path commentFile = PathJoin.join(vaultRoot.resolve(note.vaultRelPath()), "comments.json");
            commentsEnvelope = CommentsReader.readComments(vaultRoot, commentFile);
        }

        // 사실층 읽기 (도구 표면만)
        String facts = null;
        if (parsed.isToolSurface()) {
            facts = FactsReader.readFacts(vaultRoot, note.meta().agendaSlug());
        }

        String agendaTitle = agendaTable.slugToTitle().getOrDefault(note.meta().agendaSlug(), "");

        return withlog.out.NoteRow.forGetDoc(
                vaultRoot,
                note,
                agendaTitle,
                facts,
                commentsEnvelope,
                parsed.isToolSurface()
        );
    }
}
