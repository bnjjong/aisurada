package withlog.vault;

import withlog.text.Trim;

import java.io.BufferedReader;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * 사실층(.withlog/facts/<slug>.md) 파일 읽기 유틸리티.
 * 설명서 2.3 및 11.6에 따라 앞 2,001자를 읽고 2,000자 초과 시 규격 안내 문구를 반환합니다.
 */
public final class FactsReader {
    private static final String OVER_LIMIT_MESSAGE =
            "이 아젠다의 사실층이 상한(2000 자)을 넘어 안 싣는다. .withlog/facts/ 아래 파일을 상한 아래로 줄이면 다시 실린다.";

    private FactsReader() {}

    /**
     * 지정한 아젠다 slug의 사실층 파일을 읽어 get_doc의 agendaFacts 값을 반환합니다.
     * @param vaultRoot vault 루트 경로
     * @param agendaSlug 아젠다 slug
     * @return 실을 문자열 (사실층 내용 또는 초과 안내 문구), 실을 내용이 없으면 null
     */
    public static String readFacts(Path vaultRoot, String agendaSlug) {
        if (agendaSlug == null || agendaSlug.isEmpty()) {
            return null;
        }

        Path factsFile = vaultRoot.resolve(".withlog/facts").resolve(agendaSlug + ".md");
        if (!Files.isRegularFile(factsFile)) {
            return null;
        }

        char[] buffer = new char[2001];
        int totalRead = 0;
        try (BufferedReader reader = Files.newBufferedReader(factsFile, StandardCharsets.UTF_8)) {
            while (totalRead < 2001) {
                int read = reader.read(buffer, totalRead, 2001 - totalRead);
                if (read == -1) {
                    break;
                }
                totalRead += read;
            }
        } catch (Exception e) {
            // 깨진 UTF-8 또는 입출력 오류 시 키를 뺀다 (11.6 3번)
            return null;
        }

        if (totalRead == 0) {
            return null;
        }

        String raw = new String(buffer, 0, totalRead);
        String trimmed = Trim.trim(raw);
        if (trimmed.isEmpty()) {
            return null;
        }

        if (trimmed.length() > 2000) {
            return OVER_LIMIT_MESSAGE;
        }
        return trimmed;
    }
}
