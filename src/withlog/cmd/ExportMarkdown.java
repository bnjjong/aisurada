package withlog.cmd;

import withlog.cli.WlException;
import withlog.engine.WritePipeline;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.AttachText;
import withlog.vault.NoteScanner;
import withlog.write.*;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;

/** 27장: 읽기 층 루트 노트의 마크다운 본문을 고정 내부 폴더에 내보낸다. */
public final class ExportMarkdown {
    private ExportMarkdown() {}
    private record Export(String path, String body) {}

    public static Map<String, Object> execute(Path vault) {
        var index = WritePipeline.index(vault);
        List<Export> exports = new ArrayList<>();
        int total = 0, html = 0, json = 0, unsafe = 0, exported = 0;
        for (NoteScanner.NoteRecord note : index.allRootNotes()) {
            if (!"reading".equals(note.meta().tier())) continue;
            total++;
            AttachText.extract(vault, note);
            if ("html".equals(note.format())) { html++; continue; }
            if ("json".equals(note.format())) { json++; continue; }
            String path = note.vaultRelPath();
            if (Trim.isBlank(path)) throw new WlException(409, "저장 자리를 모르는 노트가 있어 내보내지 않았다. 색인을 다시 만들어야 한다: " + note.id());
            // 확장자로 단일 파일을 판정하는 기존 읽기 경계도 유지한다(27.9의 9번).
            boolean single = path.matches("(?is).*\\.(md|html|json)$");
            Path body = single ? vault.resolve(path) : note.contentFilePath();
            exports.add(new Export(path, DerivedBodyWriter.read(body).body()));
        }
        Path dest = vault.resolve(".withlog/md-export").toAbsolutePath().normalize();
        TreeRemover.remove(dest);
        try { Files.createDirectories(dest); }
        catch (IOException e) { throw new WlException(500, "폴더를 만들지 못했다: " + dest); }
        for (Export item : exports) {
            String relative = item.path().startsWith("notes/") ? item.path().substring(6) : item.path();
            relative = relative.replaceFirst("(?i)\\.(md|html|json)$", "") + ".md";
            if (Trim.isBlank(relative) || relative.startsWith("/") || relative.contains("..") || relative.contains("\\") || relative.chars().anyMatch(c -> c < 32)) {
                unsafe++; continue;
            }
            PathConfinement.checkSegment(dest, relative);
            Path target = dest.resolve(relative).normalize(); PathConfinement.checkConfinement(dest, target);
            AtomicWriter.writeAtomic(target, Utf16.toUtf8BytesWithQuestionForUnpaired(item.body())); exported++;
        }
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("dest", dest.toString()); result.put("exported", exported); result.put("totalNotes", total); result.put("skippedHtml", html);
        if (unsafe != 0) result.put("skippedUnsafe", unsafe);
        if (json != 0) result.put("skippedJson", json);
        return result;
    }
}
