package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.Frontmatter;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.vault.StrictUtf8;
import withlog.write.AtomicWriter;
import withlog.write.DocToken;
import withlog.write.MetaWriter;
import withlog.write.NoteMover;
import withlog.write.PathConfinement;
import withlog.write.PreconditionChecker;
import withlog.write.ShardLayout;
import withlog.write.SlugBuilder;
import withlog.write.StampJudgment;
import withlog.write.WarningsDetector;
import withlog.write.WorkspaceMarker;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDate;
import java.time.LocalTime;
import java.time.OffsetDateTime;
import java.time.ZoneOffset;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * 설명서 14장 update_doc / update-doc 명령.
 * 이미 있는 노트 1건의 제목, 본문, 태그, 템플릿, 날짜, 시각, 소요 시간을 고치고,
 * 그 값으로 다시 계산한 정식 자리(1.3, 1.5)로 노트를 옮깁니다.
 */
public final class UpdateDoc {

    private UpdateDoc() {}

    public static final String USAGE = "update-doc <노트 id> [title=<제목>] [body=<본문>] [tags=<JSON 배열. 전체 교체>] " +
            "[template=<템플릿 id>] [date=<YYYY-MM-DD>] [time=<HH:MM>] [mins=<분>] [f1=<직전에 읽은 토큰>]";

    private static final Pattern NOTE_ID_PATTERN = Pattern.compile("^nd[0-9a-z]+-[0-9a-f]{8}$");

    /**
     * update_doc / update-doc 명령을 실행합니다. 검사 차례는 14.3, 쓰는 차례는 14.5입니다.
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed, String sessionId) {
        boolean isTool = parsed.isToolSurface();
        Map<String, Object> toolOpts = parsed.toolOptions();
        Map<String, String> opts = parsed.namedOptions();

        // 4. 워크스페이스 표식, 5. 색인 맞추기 (14.4의 2번, 3번)
        WorkspaceMarker.check(vaultRoot);
        NoteIndex noteIndex = NoteIndex.reconciled(vaultRoot);

        // 7. 값 읽기: id, id 모양, tags, date, time, mins, 그다음 제목 공백과 음수 소요 시간
        String rawId = isTool ? (String) toolOpts.get("id")
                : (parsed.positionalArgs().isEmpty() ? null : parsed.positionalArgs().get(0));
        if (rawId == null || Trim.isBlank(rawId)) {
            throw new WlException(400, "인자가 하나 필요하다: " + USAGE);
        }
        if (!NOTE_ID_PATTERN.matcher(rawId).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rawId);
        }
        String noteId = rawId;

        boolean tagsGiven = isTool ? toolOpts.containsKey("tags") : opts.containsKey("tags");
        List<String> newTags = tagsGiven ? CreateDoc.parseAndValidateTags(isTool, toolOpts, opts) : null;

        String rawDate = isTool ? null : opts.get("date");
        LocalDate givenDate = rawDate != null ? CreateDoc.validateAndParseDate(rawDate) : null;
        String rawTime = isTool ? null : opts.get("time");
        LocalTime givenTime = rawTime != null ? CreateDoc.validateAndParseTime(rawTime) : null;

        Integer givenMins = null;
        if (!isTool && opts.containsKey("mins")) {
            String trimmedMins = Trim.trim(opts.get("mins"));
            if (!trimmedMins.isEmpty()) {
                givenMins = CreateDoc.parseMins(trimmedMins);
            }
        }

        String givenTitle = isTool ? (String) toolOpts.get("title") : opts.get("title");
        if (givenTitle != null && Trim.isBlank(givenTitle)) {
            throw new WlException(400, "노트 제목이 비었다: " + noteId);
        }
        if (givenMins != null && givenMins < 0) {
            throw new WlException(400, "노트 소요 시간이 음수다: " + givenMins);
        }

        String givenBody = isTool ? (String) toolOpts.get("body") : opts.get("body");
        String rawTemplate = isTool ? (String) toolOpts.get("template") : opts.get("template");
        String f1Token = null;
        Object rawF1 = isTool ? toolOpts.get("f1") : opts.get("f1");
        if (rawF1 instanceof String s && !Trim.trim(s).isEmpty()) {
            f1Token = Trim.trim(s);
        }

        // 8. 색인에서 노트를 찾는다 (14.4의 12번)
        NoteScanner.NoteRecord note = noteIndex.notesById().get(noteId);
        if (note == null) {
            throw new WlException(404, "Note not found: " + noteId);
        }
        if (note.vaultRelPath() == null || note.contentFilePath() == null) {
            throw new WlException(409, "저장 위치를 모르는 노트라 고칠 수 없다: " + noteId);
        }

        // 14.5의 2번: 준 칸만 갈아 낀 다음 값. 못 고치는 값은 색인의 값이 따라온다
        String format = note.format();
        String title = givenTitle != null ? givenTitle : MetaWriter.asIndexed(note.meta().title());
        List<String> tags = newTags != null ? newTags
                : (note.meta().tags() == null ? null : note.meta().tags().stream().map(MetaWriter::asIndexed).toList());
        String template = note.meta().template();
        if (rawTemplate != null) {
            String trimmedTpl = Trim.trim(rawTemplate);
            if (CreateDoc.TEMPLATE_ID_PATTERN.matcher(trimmedTpl).matches()) {
                template = trimmedTpl;
            }
        }
        int mins = givenMins != null ? givenMins : note.meta().mins();

        // 14.3: 날짜와 시각은 준 쪽만 갈아 끼우고, 오프셋은 노트의 지금 값을 지킨다
        OffsetDateTime current = OffsetDateTime.parse(note.meta().created());
        String created;
        LocalDate newDate;
        LocalTime newTime;
        if (givenDate == null && givenTime == null) {
            created = note.meta().created();
            newDate = current.toLocalDate();
            newTime = current.toLocalTime();
        } else {
            newDate = givenDate != null ? givenDate : current.toLocalDate();
            newTime = givenTime != null ? givenTime : LocalTime.of(current.getHour(), current.getMinute());
            created = CreateDoc.CREATED_DATE.format(newDate) + "T"
                    + String.format("%02d:%02d:00", newTime.getHour(), newTime.getMinute())
                    + offsetText(current.getOffset());
        }

        // 14.5의 3번: json 노트에 본문을 주면 파일을 읽기 전에 선다
        if ("json".equals(format) && givenBody != null) {
            throw new WlException(500, "Lexical JSON 노트의 본문은 고칠 수 없다(마크다운을 editorState 로 바꾸는 일이라 사이드카에 없다): " + noteId);
        }

        // 14.5의 4번, 5번: 본문 파일을 읽고 토큰을 맞댄다
        byte[] fileBytes = PreconditionChecker.check(vaultRoot, note, f1Token, "고치지");
        String contentRel = vaultRoot.relativize(note.contentFilePath()).toString().replace('\\', '/');

        // 6번: 프론트매터
        String content;
        try {
            content = StrictUtf8.decodeStrict(fileBytes);
        } catch (Exception e) {
            throw new WlException(500, "노트 본문 파일을 못 읽어 고치지 않았다: " + contentRel);
        }
        MetaWriter.FileParts parts = MetaWriter.split(content);
        if (!parts.fenceFound() || parts.yamlPart() == null || parts.yamlPart().length() > 16_000) {
            throw new WlException(500, "프론트매터가 없어 고치지 않았다: " + contentRel);
        }
        Map<String, Object> rawYamlMap = Frontmatter.parse(parts.yamlPart());
        if (rawYamlMap == null) {
            throw new WlException(500, "프론트매터가 없어 고치지 않았다: " + contentRel);
        }
        String oldBody = parts.body();

        // 7번: 새 자리의 아젠다 slug. 파일의 agenda가 빈 글이 아닌 문자열이면 그 글자 그대로다
        String oldRel = note.vaultRelPath();
        String agendaSlug;
        if (rawYamlMap.get("agenda") instanceof String sa && !sa.isEmpty()) {
            agendaSlug = sa;
        } else {
            String[] segs = oldRel.split("/");
            agendaSlug = segs.length >= 2 ? segs[1] : note.meta().agendaSlug();
        }

        // 8번: 쓸 본문, 9번: 파일의 sessionId(다듬어 앞 128자)
        String body = givenBody != null ? givenBody : oldBody;
        String fileSessionId = null;
        if (rawYamlMap.get("sessionId") instanceof String ss && !Trim.trim(ss).isEmpty()) {
            fileSessionId = Utf16.cut(Trim.trim(ss), 128);
        }

        // 10번: 파일 글자와 도장(1.10). 비교에는 본문도 든다
        String indexedCategory = MetaWriter.asIndexed(note.meta().favoriteCategory());
        Map<String, Object> evaluation = note.meta().evaluation();
        if (evaluation != null && evaluation.get("note") instanceof String en) {
            evaluation = new LinkedHashMap<>(evaluation);
            evaluation.put("note", MetaWriter.asIndexed(en));
        }
        StampJudgment.JudgmentResult judgment = StampJudgment.judge(
                format, body, oldBody, rawYamlMap,
                noteId, agendaSlug, note.meta().type(), title, created, fileSessionId, mins, note.meta().tier(),
                note.meta().editedBy(), note.meta().editedAt(), note.meta().editedSessionId(),
                tags, note.meta().favorite(), indexedCategory, template, evaluation, note.meta().links(),
                isTool, sessionId);
        String fileText = judgment.fileText();

        // 11번: 자리 갈래 (1.3 부모 폴더, 1.5 폴더 이름)
        String parent = ShardLayout.isSharded(vaultRoot)
                ? "notes/" + agendaSlug + "/" + CreateDoc.CREATED_MONTH.format(newDate)
                : "notes/" + agendaSlug;
        String folderName = newDate + "-" + String.format("%02d%02d", newTime.getHour(), newTime.getMinute())
                + "-" + SlugBuilder.build(title, 50) + "-" + CreateDoc.extractIdTail(noteId);
        String contentName = note.contentFilePath().getFileName().toString();

        List<String> warnings = new ArrayList<>();
        String finalRel;
        Path writeTarget;
        if (!note.isSingleFile()) {
            // 폴더 노트: 경로 검사, 빈자리 고르기, 옮기기 뒤에 상한을 보고 쓴다(14.4의 21~25번)
            String canonicalRel = parent + "/" + folderName;
            checkPath(vaultRoot, canonicalRel);
            finalRel = NoteMover.placeFolder(vaultRoot, oldRel, canonicalRel);
            checkSize(fileText, finalRel);
            writeTarget = vaultRoot.resolve(finalRel).resolve(contentName);
            write(writeTarget, fileText);
        } else if ("html".equals(format)) {
            // 옛 .html 단일 파일: 이름만 바꾼다. 상한, 경로, 쓰기 차례이고 같은 이름 파일은 덮는다
            finalRel = parent + "/" + folderName + ".html";
            checkSize(fileText, finalRel);
            checkPath(vaultRoot, finalRel);
            writeTarget = vaultRoot.resolve(finalRel);
            write(writeTarget, fileText);
            if (!finalRel.equals(oldRel)) {
                removeOld(vaultRoot, oldRel, warnings);
            }
        } else {
            // 옛 .md, .json 단일 파일: 빈자리를 안 보고 새 폴더에 쓴 뒤 옛 파일을 지운다
            finalRel = parent + "/" + folderName;
            checkSize(fileText, finalRel);
            checkPath(vaultRoot, finalRel);
            writeTarget = vaultRoot.resolve(finalRel).resolve("json".equals(format) ? "note.json" : "note.md");
            write(writeTarget, fileText);
            removeOld(vaultRoot, oldRel, warnings);
        }
        byte[] written = Utf16.toUtf8BytesWithQuestionForUnpaired(fileText);

        // 14.6 경고: 못 치운 옛 자리, 들여쓴 블록, 제목과 첫 헤딩, U+FFFD 차례
        if (givenBody != null && !"html".equals(format)) {
            String w = WarningsDetector.detectIndentedBlocksWarning(givenBody);
            if (w != null) warnings.add(w);
        }
        if ("markdown".equals(format)) {
            String w = WarningsDetector.detectTitleMismatchWarning(body, title);
            if (w != null) warnings.add(w);
        }
        if (givenBody != null) {
            int grown = WarningsDetector.countBrokenChars(givenBody) - WarningsDetector.countBrokenChars(oldBody);
            if (grown > 0) warnings.add(WarningsDetector.brokenCharWarning(givenBody, grown));
        }

        // 12번: 쓴 뒤의 다시 읽기는 할 일이 없다. 이 엔진은 부를 때마다 디스크를 훑는다(8.6의 4번)
        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", noteId);
        resp.put("path", finalRel);
        resp.put("precondition", DocToken.compute(written));
        resp.put("renamed", !finalRel.equals(oldRel));
        if (!warnings.isEmpty()) {
            resp.put("warning", String.join("\n", warnings));
        }
        return resp;
    }

    /** 1.9의 오프셋 꼴: 0이면 Z, 아니면 +HH:MM이나 -HH:MM입니다. */
    private static String offsetText(ZoneOffset offset) {
        return offset.getTotalSeconds() == 0 ? "Z" : offset.getId();
    }

    /** 14.4의 21번: 경로로 못 짓는 이름과 vault 밖을 가리키는 아젠다 글자(7.4의 경로 검사). */
    private static void checkPath(Path vaultRoot, String rel) {
        if (PathConfinement.hasUnusablePathCharacters(rel)) {
            throw new WlException(400, "경로로 쓸 수 없는 값이다: " + rel);
        }
        PathConfinement.checkConfinement(vaultRoot, vaultRoot.resolve(rel));
    }

    /** 14.4의 24번: 파일 전체가 1,000,000 UTF-16 코드 단위를 넘으면 안 쓴다(7.4의 크기 상한). */
    private static void checkSize(String fileText, String rel) {
        if (fileText.length() > 1_000_000) {
            throw new WlException(500, "노트가 저장 상한을 넘어 저장하지 않았다: " + rel);
        }
    }

    private static void write(Path target, String fileText) {
        AtomicWriter.writeAtomic(target, Utf16.toUtf8BytesWithQuestionForUnpaired(fileText));
    }

    /** 옛 단일 파일을 지웁니다. 못 지우면 응답을 실패로 바꾸지 않고 경고로 알립니다(14.5). */
    private static void removeOld(Path vaultRoot, String oldRel, List<String> warnings) {
        try {
            Files.deleteIfExists(vaultRoot.resolve(oldRel));
        } catch (IOException e) {
            warnings.add("옛 노트 삭제 실패 — 같은 id가 둘 남았을 수 있어요(정리 필요): " + oldRel);
        }
    }
}
