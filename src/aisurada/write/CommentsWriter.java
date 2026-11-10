package aisurada.write;

import aisurada.cli.AisException;
import aisurada.out.JsonWriter;
import aisurada.vault.CommentsReader;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** 19.8: 읽힌 항목만 파일 전체로 다시 쓴다. 링크는 따라 쓰고 크기 상한은 추가하지 않는다. */
public final class CommentsWriter {
    private CommentsWriter() {}

    public static void write(Path file, List<Map<String, Object>> comments) {
        if (comments.isEmpty()) {
            try { Files.deleteIfExists(file); }
            catch (IOException e) { throw failed(file); }
            return;
        }
        CommentsReader.sort(comments);
        List<Map<String, Object>> ordered = new ArrayList<>();
        for (var item : comments) {
            Map<String, Object> row = new LinkedHashMap<>();
            for (String key : List.of("id", "createdAt", "editedAt", "anchor", "body")) {
                if (item.containsKey(key)) row.put(key, item.get(key));
            }
            ordered.add(row);
        }
        Path parent = file.toAbsolutePath().normalize().getParent();
        if (!Files.isDirectory(parent)) {
            try { Files.createDirectories(parent); }
            catch (IOException e) { throw new AisException(500, "폴더를 만들지 못했다: " + parent); }
        }
        Map<String, Object> document = new LinkedHashMap<>();
        document.put("v", 1);
        document.put("comments", ordered);
        // [B9] 설명서 19.7 원칙: 링크 너머 실제 파일을 건드리지 않고 링크 자체를 안전하게 교체 (toRealPath 제거)
        try {
            AtomicWriter.writeAtomic(file, JsonWriter.toFileBytes(document));
        } catch (Exception e) {
            throw failed(file);
        }
    }

    private static AisException failed(Path file) {
        return new AisException(500, "파일을 쓰지 못했다: " + file.toAbsolutePath().normalize());
    }
}
