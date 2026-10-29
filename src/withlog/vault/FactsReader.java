package withlog.vault;

import withlog.text.Trim;

import java.io.BufferedReader;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * 사실층(.withlog/facts/<slug>.md) 파일 읽기 유틸리티.
 * 22.5의 엄격한 UTF-8 읽기. JDK 21 BufferedReader의 앞선 해독 범위까지 계약에 포함된다.
 */
public final class FactsReader {
    public static final int LIMIT = 2000;
    public static final int EDIT_LIMIT = LIMIT * 5;
    public static final String OVER_LIMIT_MESSAGE =
            "이 아젠다의 사실층이 상한(" + LIMIT + " 자)을 넘어 안 싣는다. .withlog/facts/ 아래 파일을 상한 아래로 줄이면 다시 실린다.";
    private static final System.Logger LOG = System.getLogger(FactsReader.class.getName());

    public record ReadResult(String facts, boolean tooLarge) {}

    private FactsReader() {}

    /**
     * 지정한 아젠다 slug의 사실층 파일을 읽어 get_doc의 agendaFacts 값을 반환합니다.
     * @param vaultRoot vault 루트 경로
     * @param agendaSlug 아젠다 slug
     * @return 실을 문자열 (사실층 내용 또는 초과 안내 문구), 실을 내용이 없으면 null
     */
    public static String readFacts(Path vaultRoot, String agendaSlug) {
        ReadResult result = read(vaultRoot, agendaSlug, LIMIT);
        return result.tooLarge() ? OVER_LIMIT_MESSAGE : result.facts();
    }

    /** 편집에는 원문만 준다. 상한을 넘으면 잘린 글이나 AI용 안내 문장을 주지 않는다. */
    public static ReadResult readForEdit(Path vaultRoot, String agendaSlug) {
        return read(vaultRoot, agendaSlug, EDIT_LIMIT);
    }

    public static String relativePath(String agendaSlug) {
        return ".withlog/facts/" + agendaSlug + ".md";
    }

    private static ReadResult read(Path vaultRoot, String agendaSlug, int limit) {
        if (agendaSlug == null || agendaSlug.isEmpty()) {
            return new ReadResult(null, false);
        }

        Path factsFile = vaultRoot.resolve(relativePath(agendaSlug));
        if (!Files.isRegularFile(factsFile)) {
            return new ReadResult(null, false);
        }

        char[] buffer = new char[limit + 1];
        int totalRead = 0;
        try (BufferedReader reader = Files.newBufferedReader(factsFile, StandardCharsets.UTF_8)) {
            // 전체 읽기로 바꾸지 않는다. 8,192단위의 앞선 해독과 서로게이트 분할도 22.5의 계약이다.
            while (totalRead < buffer.length) {
                int read = reader.read(buffer, totalRead, buffer.length - totalRead);
                if (read == -1) {
                    break;
                }
                totalRead += read;
            }
        } catch (IOException e) {
            LOG.log(System.Logger.Level.WARNING, "사실층 파일을 읽지 못했다: " + factsFile + ": " + e.getMessage());
            return new ReadResult(null, false);
        }

        String raw = new String(buffer, 0, totalRead);
        String trimmed = Trim.trim(raw);
        if (trimmed.isEmpty()) {
            return new ReadResult(null, false);
        }

        if (trimmed.length() > limit) {
            return new ReadResult(null, true);
        }
        return new ReadResult(trimmed, false);
    }
}
