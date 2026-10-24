package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Uri;
import withlog.text.Utf16;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;

import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.regex.Pattern;

/**
 * list_attachments 및 list-attachments 명령 실행기.
 * 설명서 18장에 따라 노트 또는 하위 노트의 첨부 목록 및 본문 참조 조각(ref)을 생성합니다.
 */
public final class ListAttachments {

    private static final Pattern NOTE_ID_PATTERN = Pattern.compile("^nd[0-9a-z]+-[0-9a-f]{8}$");
    private static final Pattern SUB_NOTE_ID_PATTERN = Pattern.compile("^sn[0-9a-z]+-[0-9a-f]{8}$");
    private static final Pattern ATTACH_EXT_PATTERN = Pattern.compile("\\.[A-Za-z0-9]+$");

    private static final Set<String> IMAGE_EXTENSIONS = Set.of(
            "svg", "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"
    );

    private ListAttachments() {}

    public static Map<String, Object> execute(
            Path vaultRoot,
            ParsedArgs parsed,
            NoteIndex noteIndex
    ) {
        List<String> pos = parsed.positionalArgs();

        // 1. 공통 인자 검사
        if (pos.isEmpty() || pos.get(0) == null || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: list-attachments <노트 id> [하위 노트 id]");
        }

        String rawRootId = pos.get(0);
        String rawSubId = pos.size() >= 2 ? pos.get(1) : null;

        // 명령 표면에서 2번째 인자가 공백뿐이면 없는 것으로 봄 (18.3 9번)
        if (rawSubId != null && Trim.isBlank(rawSubId)) {
            rawSubId = null;
        }

        // 2. id 모양 검증
        if (!NOTE_ID_PATTERN.matcher(rawRootId).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rawRootId);
        }
        if (rawSubId != null && !SUB_NOTE_ID_PATTERN.matcher(rawSubId).matches()) {
            throw new WlException(400, "하위 노트 id 모양이 아니다: " + rawSubId);
        }

        // 3. 색인 조회
        NoteScanner.NoteRecord rootNote = noteIndex.notesById().get(rawRootId);
        if (rootNote == null) {
            throw new WlException(404, "Note not found: " + rawRootId);
        }

        NoteScanner.NoteRecord targetSub = null;
        if (rawSubId != null) {
            targetSub = rootNote.subNotes().get(rawSubId);
            if (targetSub == null) {
                throw new WlException(404, "SubNote not found: " + rawRootId + "/" + rawSubId);
            }
        }

        NoteScanner.NoteRecord owner = targetSub != null ? targetSub : rootNote;
        String reconstructedParent = targetSub != null ? (rawRootId + "/" + rawSubId) : rawRootId;

        // 4. 첨부 파일 탐색
        List<Map<String, String>> attachmentsList = new ArrayList<>();

        if (!owner.isSingleFile()) {
            Path ownerDir = vaultRoot.resolve(owner.vaultRelPath());
            Path attachDir = ownerDir.resolve("attach");

            if (Files.exists(attachDir) && Files.isDirectory(attachDir)) {
                List<String> validNames = new ArrayList<>();
                try (DirectoryStream<Path> ds = Files.newDirectoryStream(attachDir)) {
                    for (Path entry : ds) {
                        String name = entry.getFileName().toString();
                        if (isValidAttachmentName(name, entry)) {
                            validNames.add(name);
                        }
                    }
                } catch (IOException ignored) {
                    // 18.5 7518행: 폴더 목록을 못 읽어도 0개이고 실패하지 않는다
                }

                validNames.sort(Utf16::compare);

                for (String name : validNames) {
                    String ref = makeRef(name);
                    Map<String, String> item = new LinkedHashMap<>();
                    item.put("name", name);
                    item.put("ref", ref);
                    attachmentsList.add(item);
                }
            }
        }

        // 5. 응답 구성 (18.6 표 순서)
        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("parent", reconstructedParent);
        resp.put("attachments", attachmentsList);
        return resp;
    }

    private static boolean isValidAttachmentName(String name, Path path) {
        // 1. 일반 파일인지 (심볼릭 링크는 가리키는 것이 일반 파일이면 센다)
        if (!Files.isRegularFile(path)) {
            return false;
        }

        // 2. .tmp로 안 끝남 (대소문자 구분)
        if (name.endsWith(".tmp")) {
            return false;
        }

        // 3. 길이 1자 이상 120자 이하 (UTF-16 코드 단위)
        if (name.isEmpty() || name.length() > 120) {
            return false;
        }

        // 4. /, \, .., U+0020 미만 글자가 없음
        if (name.contains("/") || name.contains("\\") || name.contains("..")) {
            return false;
        }
        for (int i = 0; i < name.length(); i++) {
            if (name.charAt(i) < ' ') {
                return false;
            }
        }

        // 5. .으로 시작하지 않음
        if (name.startsWith(".")) {
            return false;
        }

        // 6. 어딘가에서 \.[A-Za-z0-9]+$ 매치
        if (!ATTACH_EXT_PATTERN.matcher(name).find()) {
            return false;
        }

        return true;
    }

    private static String makeRef(String name) {
        // 1. 그림 여부 판별
        int lastDot = name.lastIndexOf('.');
        String ext = (lastDot != -1) ? name.substring(lastDot + 1).toLowerCase(Locale.ROOT) : "";
        boolean isImage = IMAGE_EXTENSIONS.contains(ext);

        // 2. 라벨: 이름에서 ]를 모두 지운 글
        String label = name.replace("]", "");

        // 3. 주소: attach/ 뒤에 encodeURIComponent(name) 후 '(' -> %28, ')' -> %29
        String enc = Uri.encodeURIComponent(name).replace("(", "%28").replace(")", "%29");
        String url = "attach/" + enc;

        // 4. 조립
        if (isImage) {
            return "![" + label + "](" + url + ")";
        } else {
            return "[" + label + "](" + url + ")";
        }
    }
}
