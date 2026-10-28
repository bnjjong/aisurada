package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.Frontmatter;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.vault.StrictUtf8;
import withlog.write.DocToken;
import withlog.write.FirstHeading;
import withlog.write.MetaWriter;
import withlog.write.PreconditionChecker;
import withlog.write.StampJudgment;
import withlog.write.SubNoteWriter;
import withlog.write.WarningsDetector;
import withlog.write.WorkspaceMarker;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * 설명서 17장 update_sub_note / update-sub-note 명령.
 * 하위 노트 1건의 제목, 본문, 태그를 갈아 끼우고, 다시 계산한 폴더 이름(1.5)으로 같은 `sub/` 안에서 폴더째 옮깁니다.
 */
public final class UpdateSubNote {

    private UpdateSubNote() {}

    public static final String USAGE = "update-sub-note <부모 노트 id> <하위 노트 id> [title=<제목>] [body=<본문>] "
            + "[tags=<JSON 배열. 전체 교체>] [f1=<직전에 읽은 토큰>]";

    /**
     * update_sub_note / update-sub-note 명령을 실행합니다. 검사 차례는 17.3, 쓰는 차례는 17.5의 고치기입니다.
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed, String sessionId) {
        Map<String, String> opts = parsed.namedOptions();

        // 3. 워크스페이스 표식, 색인 맞추기 (17.4의 2번)
        WorkspaceMarker.check(vaultRoot);
        NoteIndex noteIndex = NoteIndex.reconciled(vaultRoot);

        // 5. 자리 인자 2칸, 부모 참조, 하위 id의 3가지 검사, tags, 제목 공백 차례
        List<String> pos = parsed.positionalArgs();
        if (pos.isEmpty() || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: " + USAGE);
        }
        if (pos.size() < 2 || Trim.isBlank(pos.get(1))) {
            throw new WlException(400, "인자가 둘 필요하다: " + USAGE);
        }
        SubNoteChain.Parent parent = SubNoteChain.resolveParent(noteIndex, pos.get(0));
        NoteScanner.NoteRecord sub = SubNoteChain.resolveTarget(parent, pos.get(1));
        List<String> newTags = opts.containsKey("tags") ? CreateDoc.parseAndValidateTags(false, Map.of(), opts) : null;
        String givenTitle = opts.get("title");
        if (givenTitle != null && Trim.isBlank(givenTitle)) {
            throw new WlException(400, "하위 노트 제목이 비었다: " + sub.id());
        }
        String givenBody = opts.get("body");
        String f1Token = opts.get("f1") == null || Trim.trim(opts.get("f1")).isEmpty() ? null : Trim.trim(opts.get("f1"));

        // 17.5 고치기 2번: 제목은 준 제목, 본문만 줬으면 새 본문의 첫 헤딩, 그 밖은 옛 제목이다
        String title = givenTitle;
        if (title == null && givenBody != null && !"json".equals(sub.format())) {
            title = FirstHeading.extract(givenBody);
        }
        if (title == null) {
            title = MetaWriter.asIndexed(sub.meta().title());
        }
        List<String> tags = newTags != null ? newTags : SubNoteWriter.indexedTags(sub);

        // 3번: 루트에서 물려받는 아젠다
        String agenda = SubNoteWriter.rootAgenda(parent.root());

        // 4번: 본문 파일을 읽고 토큰을 맞대고(json 하위 노트만 면제) 프론트매터를 읽는다
        String snippet = givenBody != null ? "하위 노트 본문을 저장하지" : "하위 노트를 저장하지";
        String contentRel = vaultRoot.relativize(sub.contentFilePath()).toString().replace('\\', '/');
        byte[] fileBytes = PreconditionChecker.check(vaultRoot, sub, f1Token, snippet);
        MetaWriter.FileParts parts;
        try {
            parts = MetaWriter.split(StrictUtf8.decodeStrict(fileBytes));
        } catch (Exception e) {
            throw new WlException(500, "노트 본문 파일을 못 읽어 " + snippet + " 않았다: " + contentRel);
        }
        Map<String, Object> rawYamlMap = parts.fenceFound() && parts.yamlPart() != null && parts.yamlPart().length() <= 16_000
                ? Frontmatter.parse(parts.yamlPart()) : null;
        if (rawYamlMap == null) {
            throw new WlException(500, "프론트매터가 없어 " + snippet + " 않았다: " + contentRel);
        }
        String oldBody = parts.body();
        String body = givenBody != null ? givenBody : oldBody;

        // 5번: 하위 노트 키 11개로 짓고 도장은 1.10으로 정한다. B를 못 찍으면 여기서 WL-400이다
        StampJudgment.JudgmentResult judgment = StampJudgment.judge(
                sub.format(), body, oldBody, rawYamlMap,
                sub.id(), agenda, sub.meta().type(), title, sub.meta().created(), null, sub.meta().mins(), null,
                sub.meta().editedBy(), sub.meta().editedAt(), sub.meta().editedSessionId(),
                tags, false, null, null, null, sub.meta().links(),
                parsed.isToolSurface(), sessionId);

        // 6번, 7번: markdown 폴더 하위 노트만 쓴다. 8번, 9번: 정식 이름으로 옮긴 뒤 상한을 보고 쓴다
        SubNoteWriter.requireMarkdownFolder(sub);
        SubNoteWriter.Placed placed = SubNoteWriter.placeAndWrite(vaultRoot, sub, title, judgment.fileText());

        // 17.6 경고: 본문을 준 호출만 들여쓴 블록과 U+FFFD를 본다. 제목 불일치 경고는 없다(17.9의 4번)
        List<String> warnings = new ArrayList<>();
        if (givenBody != null) {
            String indented = WarningsDetector.detectIndentedBlocksWarning(givenBody);
            if (indented != null) warnings.add(indented);
            int grown = WarningsDetector.countBrokenChars(givenBody) - WarningsDetector.countBrokenChars(oldBody);
            if (grown > 0) warnings.add(WarningsDetector.brokenCharWarning(givenBody, grown));
        }

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", sub.id());
        resp.put("parent", parent.parentText());
        resp.put("title", title);
        resp.put("path", placed.folderRel());
        resp.put("precondition", DocToken.compute(placed.bytes()));
        if (!warnings.isEmpty()) {
            resp.put("warning", String.join("\n", warnings));
        }
        return resp;
    }
}
