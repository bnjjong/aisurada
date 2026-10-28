package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.write.AtomicWriter;
import withlog.write.DocToken;
import withlog.write.FrontmatterWriter;
import withlog.write.IdGenerator;
import withlog.write.PathConfinement;
import withlog.write.SlugBuilder;
import withlog.write.SubNoteWriter;
import withlog.write.TestApparatus;
import withlog.write.WarningsDetector;
import withlog.write.WorkspaceMarker;

import java.nio.file.Path;
import java.time.LocalDateTime;
import java.time.ZoneId;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * 설명서 17장 create_sub_note / create-sub-note 명령.
 * 컨테이너(루트 노트나 하위 노트)의 `sub/` 아래에 새 폴더를 짓고 `note.md`를 씁니다.
 * 딛고 설 옛 바이트가 없어서 토큰을 안 받고 도장을 안 찍습니다(7.3).
 */
public final class CreateSubNote {

    private CreateSubNote() {}

    public static final String USAGE = "create-sub-note <부모 노트 id> title=<제목> body=<본문> [type=<유형>]";

    private static final List<String> TYPES = List.of("plan", "build", "report", "research", "meeting", "free");

    /**
     * create_sub_note / create-sub-note 명령을 실행합니다. 검사 차례는 17.3, 쓰는 차례는 17.5의 만들기입니다.
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        Map<String, String> opts = parsed.namedOptions();

        // 3. 워크스페이스 표식, 색인 맞추기 (17.4의 2번)
        WorkspaceMarker.check(vaultRoot);
        NoteIndex noteIndex = NoteIndex.reconciled(vaultRoot);

        // 4. 첫 자리 인자, 부모 참조, title, body, type, 제목 공백, 그다음 루트 노트 차례
        List<String> pos = parsed.positionalArgs();
        if (pos.isEmpty() || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: " + USAGE);
        }
        SubNoteChain.Parent parent = SubNoteChain.resolveParent(noteIndex, pos.get(0));
        String title = opts.get("title");
        if (title == null) {
            throw new WlException(400, "title 이 필요하다: " + USAGE);
        }
        String body = opts.get("body");
        if (body == null) {
            throw new WlException(400, "body 이 필요하다: " + USAGE);
        }
        String type = canonicalType(opts.getOrDefault("type", "free"));
        if (Trim.isBlank(title)) {
            throw new WlException(400, "하위 노트 제목이 비었다");
        }
        if (parent.root() == null) {
            throw new WlException(404, "Note not found: " + parent.rootId());
        }

        // 17.5 만들기 2번~5번: 새 id, 분 아래를 버린 created, 루트에서 물려받는 아젠다, 컨테이너 자리
        String subId = IdGenerator.nextSubNoteId();
        LocalDateTime now = TestApparatus.nowLocalDateTime(ZoneId.systemDefault());
        String created = CreateDoc.CREATED_DATE.format(now) + "T"
                + String.format("%02d:%02d", now.getHour(), now.getMinute()) + ":00+09:00";
        String agenda = SubNoteWriter.rootAgenda(parent.root());
        NoteScanner.NoteRecord container = parent.container();
        if (container.isSingleFile() || container.isExtensionFolder()) {
            throw new WlException(500, SubNoteWriter.SINGLE_FILE_MESSAGE + container.vaultRelPath());
        }

        // 6번: <컨테이너 자리>/sub/<폴더 이름>/note.md. 상한을 본 뒤 경로를 보고, 빈자리를 안 보고 쓴다(1.5)
        String folderRel = container.vaultRelPath() + "/sub/" + now.toLocalDate() + "-"
                + String.format("%02d%02d", now.getHour(), now.getMinute()) + "-"
                + SlugBuilder.build(title, 50) + "-" + CreateDoc.extractIdTail(subId);
        String frontmatter = FrontmatterWriter.buildFullNoteFrontmatter(
                subId, agenda, type, title, created, null, 0, null, null, null, null,
                null, false, null, null, null, null);
        String fileText = "---\n" + frontmatter + "---\n" + body;
        if (fileText.length() > 1_000_000) {
            throw new WlException(500, "노트가 저장 상한을 넘어 저장하지 않았다: " + folderRel);
        }
        if (PathConfinement.hasUnusablePathCharacters(folderRel)) {
            throw new WlException(400, "경로로 쓸 수 없는 값이다: " + folderRel);
        }
        PathConfinement.checkConfinement(vaultRoot, vaultRoot.resolve(folderRel));
        byte[] written = Utf16.toUtf8BytesWithQuestionForUnpaired(fileText);
        AtomicWriter.writeAtomic(vaultRoot.resolve(folderRel).resolve("note.md"), written);

        // 7번: 쓴 뒤 루트를 다시 읽는 일은 없다. 이 엔진은 부를 때마다 디스크를 훑는다(8.6의 4번)

        // 17.6 경고: 들여쓴 블록, 제목과 첫 헤딩 불일치, U+FFFD 차례. 같은 제목과 원본 요청 경고는 없다
        List<String> warnings = new ArrayList<>();
        String indented = WarningsDetector.detectIndentedBlocksWarning(body);
        if (indented != null) warnings.add(indented);
        String mismatch = WarningsDetector.detectTitleMismatchWarning(body, title);
        if (mismatch != null) warnings.add(mismatch);
        String broken = WarningsDetector.detectBrokenCharWarning(body);
        if (broken != null) warnings.add(broken);

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", subId);
        resp.put("parent", parent.parentText());
        resp.put("ref", parent.parentText() + "/" + subId);
        resp.put("title", title);
        resp.put("path", folderRel);
        resp.put("precondition", DocToken.compute(written));
        if (!warnings.isEmpty()) {
            resp.put("warning", String.join("\n", warnings));
        }
        return resp;
    }

    /** 13.3의 유형 6가지 가운데 대소문자를 안 가리고 맞는 정식 소문자입니다. 다듬지 않습니다. */
    private static String canonicalType(String raw) {
        for (String t : TYPES) {
            if (t.equalsIgnoreCase(raw)) {
                return t;
            }
        }
        throw new WlException(400, "모르는 유형 이다: " + raw + ". 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다.");
    }
}
