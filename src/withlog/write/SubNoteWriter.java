package withlog.write;

import withlog.cli.WlException;
import withlog.text.Utf16;
import withlog.vault.Frontmatter;
import withlog.vault.NoteScanner;
import withlog.vault.StrictUtf8;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.OffsetDateTime;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * 설명서 17.5와 7.3의 하위 노트 쓰기 부품.
 * 루트에서 물려받는 아젠다, 하위 노트 키 11개(1.7), 폴더 이름(1.5), 정식 이름으로 옮긴 뒤 쓰기를 맡습니다.
 * `update_sub_note`와 하위 노트 컨테이너의 카드 걷기가 같은 걸음을 지나게 합니다.
 */
public final class SubNoteWriter {

    /** 옛 단일 파일 자리에는 하위 노트를 둘 수도 고칠 수도 없다는 문구의 앞부분입니다(17.4, 17.9의 6번). */
    public static final String SINGLE_FILE_MESSAGE =
            "폴더로 안 올라간 단일 파일 노트에는 하위 노트를 둘 수 없다(앱에서 한 번 열면 폴더가 된다): ";

    private SubNoteWriter() {}

    /** 정식 자리에 쓴 결과입니다. [folderRel]은 옮긴 뒤 폴더의 vault 상대 경로이고 [bytes]는 쓴 바이트입니다. */
    public record Placed(String folderRel, byte[] bytes) {}

    /**
     * 하위 노트 파일에 적을 아젠다 slug입니다(17.5 만들기의 4번).
     * 하위 노트에는 `agenda`를 안 읽으므로, 컨테이너가 하위 노트여도 루트 노트 본문 파일의 `agenda` 글자를 봅니다.
     * 그 글자가 빈 글이 아닌 문자열이 아니거나 루트 파일을 1.6대로 못 읽으면, 실패하지 않고 루트 `path`의 2번째 조각을 씁니다.
     */
    public static String rootAgenda(NoteScanner.NoteRecord root) {
        String fromFile = readAgendaText(root.contentFilePath());
        if (fromFile != null) {
            return fromFile;
        }
        String[] segs = root.vaultRelPath().split("/");
        return segs.length >= 2 ? segs[1] : root.meta().agendaSlug();
    }

    private static String readAgendaText(Path file) {
        try {
            if (file == null || !Files.isRegularFile(file) || Files.size(file) > 1_000_000) {
                return null;
            }
            MetaWriter.FileParts parts = MetaWriter.split(StrictUtf8.decodeStrict(Files.readAllBytes(file)));
            if (!parts.fenceFound() || parts.yamlPart() == null || parts.yamlPart().length() > 16_000) {
                return null;
            }
            Map<String, Object> map = Frontmatter.parse(parts.yamlPart());
            return map != null && map.get("agenda") instanceof String s && !s.isEmpty() ? s : null;
        } catch (IOException e) {
            return null;
        }
    }

    /**
     * 하위 노트 파일 글을 옛 도장 그대로 짓습니다(1.10의 A). 도장을 안 찍는 카드 걷기가 씁니다.
     * 키는 `id`, `agenda`, `type`, `title`, `created`, `mins`, 도장 3개, `tags`, `links`뿐이라 다른 키는 사라집니다.
     */
    public static String fileText(NoteScanner.NoteRecord sub, String agenda, String title, List<String> tags, String body) {
        String frontmatter = FrontmatterWriter.buildFullNoteFrontmatter(
                sub.id(), agenda, sub.meta().type(), title, sub.meta().created(), null, sub.meta().mins(), null,
                sub.meta().editedBy(), sub.meta().editedAt(), sub.meta().editedSessionId(),
                tags, false, null, null, null, sub.meta().links());
        return "---\n" + frontmatter + "---\n" + body;
    }

    /** 색인의 태그를 1.8로 정규화된 값 그대로, 짝 없는 서로게이트는 `?`로 바꿔 돌려줍니다(11.6). */
    public static List<String> indexedTags(NoteScanner.NoteRecord note) {
        return note.meta().tags() == null ? null : note.meta().tags().stream().map(MetaWriter::asIndexed).toList();
    }

    /** 1.5의 폴더 이름 `<날짜>-<시분>-<slug>-<꼬리>`입니다. 날짜와 시분은 `created`의 오프셋 그대로입니다. */
    public static String folderName(String created, String title, String id) {
        OffsetDateTime at = OffsetDateTime.parse(created);
        return at.toLocalDate() + "-" + String.format("%02d%02d", at.getHour(), at.getMinute())
                + "-" + SlugBuilder.build(title, 50) + "-" + idTail(id);
    }

    /** id에서 ASCII 글자와 숫자만 남긴 끝 4자입니다. 모자라면 있는 만큼, 없으면 `0`입니다(1.5). */
    private static String idTail(String id) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < id.length(); i++) {
            char c = id.charAt(i);
            if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) {
                sb.append(c);
            }
        }
        if (sb.length() == 0) {
            return "0";
        }
        return sb.length() > 4 ? sb.substring(sb.length() - 4) : sb.toString();
    }

    /**
     * 17.5 고치기의 8번과 9번입니다. 같은 `sub/` 안의 정식 이름을 짓고, 경로로 못 지으면 아무것도 안 바꾸고 서고,
     * 다르면 폴더째 옮긴 뒤(7.4) 저장 상한을 보고 본문 파일을 씁니다. 상한이 옮기기 뒤라서 걸리면 폴더만 옮겨집니다(17.9의 9번).
     */
    public static Placed placeAndWrite(Path vaultRoot, NoteScanner.NoteRecord sub, String title, String fileText) {
        String currentRel = sub.vaultRelPath();
        String canonicalRel = currentRel.substring(0, currentRel.lastIndexOf('/') + 1)
                + folderName(sub.meta().created(), title, sub.id());
        if (PathConfinement.hasUnusablePathCharacters(canonicalRel)) {
            throw new WlException(400, "경로로 쓸 수 없는 값이다: " + canonicalRel);
        }
        PathConfinement.checkConfinement(vaultRoot, vaultRoot.resolve(canonicalRel));
        String finalRel = NoteMover.placeFolder(vaultRoot, currentRel, canonicalRel);
        if (fileText.length() > 1_000_000) {
            throw new WlException(500, "노트가 저장 상한을 넘어 저장하지 않았다: " + finalRel);
        }
        byte[] bytes = Utf16.toUtf8BytesWithQuestionForUnpaired(fileText);
        Path target = vaultRoot.resolve(finalRel).resolve(sub.contentFilePath().getFileName().toString());
        withlog.git.VaultSnapshot.beforeWrite(vaultRoot, sub.id(), target);
        AtomicWriter.writeAtomic(target, bytes);
        return new Placed(finalRel, bytes);
    }

    /**
     * 하위 노트 쓰기가 받지 않는 모양을 거절합니다(17.5 고치기의 6번, 7번).
     * markdown이 아니면 옛 CLI처럼 마크다운 글을 `note.json`에 적어 본문을 잃게 되고, `sub/` 바로 아래 옛 `.md` 파일은 폴더로 올리지 않습니다.
     */
    public static void requireMarkdownFolder(NoteScanner.NoteRecord sub) {
        if (!"markdown".equals(sub.format())) {
            throw new WlException(500, "markdown 이 아닌 하위 노트는 저장하지 않는다(포맷="
                    + sub.format().toUpperCase(Locale.ROOT) + "): " + sub.id());
        }
        if (sub.isSingleFile()) {
            throw new WlException(500, SINGLE_FILE_MESSAGE + sub.vaultRelPath());
        }
    }
}
