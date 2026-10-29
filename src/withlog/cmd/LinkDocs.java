package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.write.MetaWriter;
import withlog.engine.WritePipeline;

import java.nio.file.Path;
import java.util.*;
import java.util.regex.Pattern;

/**
 * 설명서 15장 link_docs 및 link-docs 명령 구현체.
 * 두 노트 간의 연결(direct, indirect, attach)을 추가하거나 제거하며(remove=true),
 * 이미 있거나 지울 연결이 없으면 파일을 열지 않고 changed: false로 반환합니다.
 */
public final class LinkDocs {

    public static final String USAGE =
            "link-docs <출발 노트 id> <대상 노트 id> [kind=direct|indirect|attach] [remove=true] [f1=<직전에 읽은 토큰>]";

    private static final Pattern NOTE_ID_PATTERN = withlog.text.IdShape.NOTE;
    private static final Set<String> ALLOWED_KINDS = Set.of("direct", "indirect", "attach");

    private LinkDocs() {}

    /**
     * link_docs / link-docs 명령을 실행합니다.
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed, String sessionId) {
        boolean isTool = parsed.isToolSurface();

        // 3. 색인 맞추기 (15.4 row 3). 맞추기를 세우는 파일이 있으면 쓰기도 선다(8.7의 4번)
        NoteIndex noteIndex = WritePipeline.index(vaultRoot);

        // 4. from 검증 (15.4 row 4, 6)
        String rawFrom;
        if (isTool) {
            rawFrom = (String) parsed.toolOptions().get("from");
        } else {
            rawFrom = parsed.positionalArgs().isEmpty() ? null : parsed.positionalArgs().get(0);
        }

        if (rawFrom == null || Trim.isBlank(rawFrom)) {
            throw new WlException(400, "인자가 하나 필요하다: " + USAGE);
        }

        // 5. to 검증 (15.4 row 5, 7)
        String rawTo;
        if (isTool) {
            rawTo = (String) parsed.toolOptions().get("to");
        } else {
            rawTo = parsed.positionalArgs().size() >= 2 ? parsed.positionalArgs().get(1) : null;
        }

        if (rawTo == null || Trim.isBlank(rawTo)) {
            throw new WlException(400, "대상 노트 id 가 필요하다: " + USAGE);
        }

        // 6. from 모양 검증
        if (!NOTE_ID_PATTERN.matcher(rawFrom).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rawFrom);
        }

        // 7. to 모양 검증
        if (!NOTE_ID_PATTERN.matcher(rawTo).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rawTo);
        }

        String fromId = rawFrom;
        String toId = rawTo;

        // 13. kind 옵션 파싱 (15.4 row 13)
        String canonicalKind = "direct";
        if (isTool) {
            if (parsed.toolOptions().containsKey("kind")) {
                String k = (String) parsed.toolOptions().get("kind");
                String trimmed = Trim.trim(k);
                if (!trimmed.isEmpty()) {
                    String lower = trimmed.toLowerCase(Locale.ROOT);
                    if (!ALLOWED_KINDS.contains(lower)) {
                        throw new WlException(400, "모르는 관계 종류 이다: " + trimmed + ". 쓸 수 있는 것은 [direct, indirect, attach] 뿐이다.");
                    }
                    canonicalKind = lower;
                }
            }
        } else {
            if (parsed.namedOptions().containsKey("kind")) {
                String k = parsed.namedOptions().get("kind");
                String trimmed = Trim.trim(k);
                if (!trimmed.isEmpty()) {
                    String lower = trimmed.toLowerCase(Locale.ROOT);
                    if (!ALLOWED_KINDS.contains(lower)) {
                        throw new WlException(400, "모르는 관계 종류 이다: " + trimmed + ". 쓸 수 있는 것은 [direct, indirect, attach] 뿐이다.");
                    }
                    canonicalKind = lower;
                }
            }
        }

        // 14. remove 옵션 파싱 (15.4 row 14)
        boolean remove = false;
        if (isTool) {
            if (parsed.toolOptions().containsKey("remove")) {
                Object rVal = parsed.toolOptions().get("remove");
                if (rVal instanceof Boolean b) {
                    remove = b;
                }
            }
        } else {
            if (parsed.namedOptions().containsKey("remove")) {
                String rStr = parsed.namedOptions().get("remove");
                String trimmed = Trim.trim(rStr);
                if (!trimmed.isEmpty()) {
                    if ("true".equals(trimmed)) {
                        remove = true;
                    } else if ("false".equals(trimmed)) {
                        remove = false;
                    } else {
                        throw new WlException(400, "remove 은 true 나 false 여야 한다: " + trimmed);
                    }
                }
            }
        }

        // 15. 자기 자신과 연결 검증 (15.4 row 15)
        if (fromId.equals(toId)) {
            throw new WlException(400, "자기 자신과는 연결할 수 없다: " + fromId);
        }

        // f1 토큰 파싱
        String f1Token = null;
        if (isTool) {
            Object rawF1 = parsed.toolOptions().get("f1");
            if (rawF1 instanceof String s) {
                String trimmed = Trim.trim(s);
                if (!trimmed.isEmpty()) {
                    f1Token = trimmed;
                }
            }
        } else {
            String rawF1 = parsed.namedOptions().get("f1");
            if (rawF1 != null) {
                String trimmed = Trim.trim(rawF1);
                if (!trimmed.isEmpty()) {
                    f1Token = trimmed;
                }
            }
        }

        // 16. 색인에서 from 노트 검색 (15.4 row 16)
        NoteScanner.NoteRecord note = noteIndex.notesById().get(fromId);
        if (note == null) {
            throw new WlException(404, "Note not found: " + fromId);
        }

        // 기존 연결 여부 확인 (15.5)
        boolean exists = false;
        if (note.meta().links() != null) {
            for (Map<String, String> l : note.meta().links()) {
                if (toId.equals(l.get("to")) && canonicalKind.equalsIgnoreCase(l.get("kind"))) {
                    exists = true;
                    break;
                }
            }
        }

        // 변경 없는 호출 처리 (15.5)
        if (!remove && exists) {
            Map<String, Object> resp = new LinkedHashMap<>();
            resp.put("id", fromId);
            resp.put("changed", false);
            resp.put("note", "이미 있는 연결이에요.");
            return resp;
        }

        if (remove && !exists) {
            Map<String, Object> resp = new LinkedHashMap<>();
            resp.put("id", fromId);
            resp.put("changed", false);
            resp.put("note", "제거할 연결이 없어요.");
            return resp;
        }

        // 새 연결 목록 구성
        List<Map<String, String>> updatedLinks = new ArrayList<>();
        if (note.meta().links() != null) {
            for (Map<String, String> l : note.meta().links()) {
                if (toId.equals(l.get("to")) && canonicalKind.equalsIgnoreCase(l.get("kind"))) {
                    if (!remove) {
                        updatedLinks.add(l);
                    }
                } else {
                    updatedLinks.add(l);
                }
            }
        }
        if (!remove) {
            Map<String, String> newLink = new LinkedHashMap<>();
            newLink.put("to", toId);
            newLink.put("kind", canonicalKind);
            updatedLinks.add(newLink);
        }

        // 메타 쓰기 실행
        MetaWriter.WriteResult writeResult = MetaWriter.write(
                vaultRoot,
                note,
                f1Token,
                note.meta().favorite(),
                MetaWriter.asIndexed(note.meta().favoriteCategory()),
                MetaWriter.asIndexed(note.meta().evaluation()),
                updatedLinks,
                isTool,
                sessionId
        );

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", fromId);
        resp.put("to", toId);
        resp.put("kind", canonicalKind);
        resp.put("removed", remove);
        resp.put("changed", true);
        resp.put("path", writeResult.path());
        resp.put("precondition", writeResult.preconditionToken());

        // 대상 노트가 색인에 없으면 경고 추가 (15.5, 15.6)
        if (!remove && !noteIndex.notesById().containsKey(toId)) {
            resp.put("warning", "대상 문서 id를 vault에서 찾지 못했어요(" + toId + ") — 연결은 저장되지만 대상이 생길 때까지 그래프엔 안 보일 수 있어요.");
        }

        return resp;
    }
}
