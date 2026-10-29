package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.AttachText;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.write.AtomicWriter;
import withlog.write.PathConfinement;
import withlog.engine.WritePipeline;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * 설명서 18.8 write_attachment / write-attachment 명령.
 * 노트나 하위 노트 폴더의 attach/에 글 첨부 1개를 쓴다. 첨부는 「주인과 이름」이 열쇠라 같은 이름이면 덮는다.
 */
public final class WriteAttachment {

    private WriteAttachment() {}

    public static final String USAGE = "write-attachment <노트 id> [하위 노트 id] name=<파일명.확장자> content=<내용>";

    private static final Pattern NOTE_ID_PATTERN = withlog.text.IdShape.NOTE;
    private static final Pattern SUB_NOTE_ID_PATTERN = withlog.text.IdShape.SUB_NOTE;
    // 18.5: Java의 $라서 글 끝에 U+0085, U+2028, U+2029 1개가 붙어도 그 앞에서 맞는다
    private static final Pattern EXT_PATTERN = Pattern.compile("\\.[A-Za-z0-9]+$");
    private static final Set<String> TEXT_EXTENSIONS = Set.of(
            "svg", "html", "htm", "md", "markdown", "txt", "json", "csv", "tsv", "xml", "yaml", "yml"
    );

    /**
     * write_attachment / write-attachment 명령을 실행합니다. 검사 차례와 쓰는 차례는 18.8입니다.
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        boolean isTool = parsed.isToolSurface();

        // 2. 워크스페이스 표식, 3. 색인 맞추기 (7.1)

        NoteIndex noteIndex = WritePipeline.index(vaultRoot);

        // 5. 18.3의 8번부터 11번: 자리 인자
        List<String> pos = parsed.positionalArgs();
        if (pos.isEmpty() || pos.get(0) == null || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: " + USAGE);
        }
        String rootId = pos.get(0);
        String subId = pos.size() >= 2 && !Trim.isBlank(pos.get(1)) ? pos.get(1) : null;
        if (!NOTE_ID_PATTERN.matcher(rootId).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rootId);
        }
        if (subId != null && !SUB_NOTE_ID_PATTERN.matcher(subId).matches()) {
            throw new WlException(400, "하위 노트 id 모양이 아니다: " + subId);
        }

        // 6번, 7번: name과 content 옵션. 빈 글은 있는 것이다
        Map<String, String> opts = parsed.namedOptions();
        String rawName = isTool ? (String) parsed.toolOptions().get("name") : opts.get("name");
        if (rawName == null) {
            throw new WlException(400, "name 이 필요하다: " + USAGE);
        }
        String content = isTool ? (String) parsed.toolOptions().get("content") : opts.get("content");
        if (content == null) {
            throw new WlException(400, "content 이 필요하다: " + USAGE);
        }

        // 8번부터 10번: 이름 다듬기, 이름 규칙, 글 확장자
        String name = Trim.trim(rawName);
        checkName(name);
        String ext = name.substring(name.lastIndexOf('.') + 1).toLowerCase(Locale.ROOT);
        if (!TEXT_EXTENSIONS.contains(ext)) {
            throw new WlException(400, "텍스트로 쓸 수 없는 확장자다(이미지 같은 바이너리는 attach/ 에 직접 둔다): " + name);
        }

        // 11번: 주인이 색인에 있는지. 메인 노트는 get_doc과 같은 길로 읽어 HTML 첨부도 연다(18.5)
        NoteScanner.NoteRecord root = noteIndex.notesById().get(rootId);
        if (root == null) {
            throw new WlException(404, "Note not found: " + rootId);
        }
        NoteScanner.NoteRecord owner = root;
        if (subId != null) {
            owner = root.subNotes().get(subId);
            if (owner == null) {
                throw new WlException(404, "SubNote not found: " + rootId + "/" + subId);
            }
        } else {
            AttachText.extract(vaultRoot, root);
        }

        // 쓰는 차례 1~3: 색인의 경로, 단일 파일 판정(대소문자를 가린다)
        String ownerRel = owner.vaultRelPath();
        if (root.vaultRelPath() == null || ownerRel == null) {
            throw new WlException(409, "저장 위치를 모르는 노트라 첨부를 둘 수 없다: " + rootId);
        }
        String lastSegment = ownerRel.substring(ownerRel.lastIndexOf('/') + 1);
        if (lastSegment.endsWith(".md") || lastSegment.endsWith(".html") || lastSegment.endsWith(".json")) {
            throw new WlException(500, "폴더로 안 올라간 단일 파일 노트에는 첨부를 둘 수 없다(앱에서 한 번 열면 폴더가 된다): " + ownerRel);
        }

        // 4번: 경로로 못 짓는 이름(짝 없는 서로게이트)
        if (PathConfinement.hasUnusablePathCharacters(name)) {
            throw new WlException(400, "경로로 쓸 수 없는 값이다: " + name);
        }
        String relPath = ownerRel + "/attach/" + name;

        // 5번, 6번: 원자적 쓰기. 내용은 다듬지 않고, 짝 없는 서로게이트는 ? 1바이트
        AtomicWriter.writeAtomic(vaultRoot.resolve(relPath), Utf16.toUtf8BytesWithQuestionForUnpaired(content));

        // 7번(쓴 뒤 다시 읽기)은 할 일이 없다. 이 엔진은 부를 때마다 디스크를 훑는다(8.6의 4번)
        List<String> parentParts = new ArrayList<>();
        parentParts.add(rootId);
        if (subId != null) parentParts.add(subId);
        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("parent", String.join("/", parentParts));
        resp.put("name", name);
        resp.put("path", relPath);
        resp.put("ref", ListAttachments.makeRef(name));
        return resp;
    }

    /** 18.8의 9번: 이름 규칙을 이 차례로 보고 처음 걸린 1개만 낸다. */
    private static void checkName(String name) {
        if (name.isEmpty()) {
            throw new WlException(400, "첨부 파일명이 비었다");
        }
        if (name.length() > 120) {
            throw new WlException(400, "첨부 파일명이 너무 길다: " + name.length() + "자");
        }
        if (name.contains("/") || name.contains("\\")) {
            throw new WlException(400, "첨부 파일명에 경로 구분자가 있다: " + name);
        }
        if (name.contains("..")) {
            throw new WlException(400, "첨부 파일명에 상위 디렉터리 참조가 있다: " + name);
        }
        if (name.startsWith(".")) {
            throw new WlException(400, "첨부 파일명이 점으로 시작한다: " + name);
        }
        for (int i = 0; i < name.length(); i++) {
            if (name.charAt(i) < 0x20) {
                throw new WlException(400, "첨부 파일명에 제어문자가 있다");
            }
        }
        if (!EXT_PATTERN.matcher(name).find()) {
            throw new WlException(400, "첨부 파일명에 확장자가 없다: " + name);
        }
    }
}
