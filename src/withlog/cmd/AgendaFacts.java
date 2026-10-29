package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.engine.WritePipeline;
import withlog.text.Trim;
import withlog.vault.AgendaReader;
import withlog.vault.AgendaTable;
import withlog.vault.FactsReader;
import withlog.vault.NoteIndex;
import withlog.write.AtomicWriter;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

/** 22장 명령 전용 사실층 편집. 읽기와 쓰기는 같은 응답 키를 쓴다. */
public final class AgendaFacts {
    public static final String GET_USAGE = "get-agenda-facts <아젠다 id>";
    public static final String UPDATE_USAGE =
            "update-agenda-facts <아젠다 id> facts=<지금도 참인 것. 빈 값이면 지운다>";

    private AgendaFacts() {}

    public static Map<String, Object> get(Path vaultRoot, ParsedArgs parsed) {
        String id = AgendaArgs.requireId(parsed, GET_USAGE);
        // 색인이 없는 판(8.6): 노트 전체를 훑지 않고 아젠다 파일만 읽는다.
        AgendaTable.AgendaInfo agenda = AgendaArgs.find(AgendaReader.scanAgendas(vaultRoot, false).table(), id);
        FactsReader.ReadResult facts = FactsReader.readForEdit(vaultRoot, agenda.slug());
        Map<String, Object> result = envelope(agenda);
        if (facts.facts() != null) result.put("facts", facts.facts());
        if (facts.tooLarge()) result.put("tooLargeToEdit", true);
        return result;
    }

    public static Map<String, Object> update(Path vaultRoot, ParsedArgs parsed) {
        NoteIndex index = WritePipeline.index(vaultRoot);
        String id = AgendaArgs.requireId(parsed, UPDATE_USAGE);
        if (!parsed.namedOptions().containsKey("facts")) {
            throw new WlException(400, "facts 가 필요하다. 사실층을 지우려면 빈 값을 준다.");
        }
        String facts = Trim.trim(parsed.namedOptions().get("facts"));
        if (facts.length() > FactsReader.LIMIT) {
            throw new WlException(400, "사실층이 상한을 넘었다: " + facts.length() +
                    " 자. 상한은 " + FactsReader.LIMIT + " 자다.");
        }
        AgendaTable.AgendaInfo agenda = AgendaArgs.find(index, id);
        Path file = vaultRoot.resolve(FactsReader.relativePath(agenda.slug()));
        if (facts.isEmpty()) {
            try {
                // 링크를 따라가지 않는다. 빈 폴더도 지우며, 부모가 일반 파일이면 실패한다.
                Files.deleteIfExists(file);
            } catch (IOException e) {
                throw new WlException(500, "파일을 지우지 못했다: " + file.toAbsolutePath().normalize());
            }
        } else {
            AtomicWriter.writeAtomic(file, (facts + "\n").getBytes(StandardCharsets.UTF_8));
        }
        Map<String, Object> result = envelope(agenda);
        if (!facts.isEmpty()) result.put("facts", facts);
        return result;
    }

    private static Map<String, Object> envelope(AgendaTable.AgendaInfo agenda) {
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("id", agenda.id());
        result.put("slug", agenda.slug());
        result.put("path", FactsReader.relativePath(agenda.slug()));
        result.put("limit", FactsReader.LIMIT);
        result.put("editLimit", FactsReader.EDIT_LIMIT);
        return result;
    }
}
