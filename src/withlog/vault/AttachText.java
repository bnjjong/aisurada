package withlog.vault;

import withlog.text.Trim;
import withlog.text.Utf16;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 노트 첨부 파일의 HTML 평문 텍스트 추출기.
 * 설명서 10.5, 9.3, 18.5에 따라 4 MiB 이하의 HTML 첨부(.htm, .html)를 UTF-8로 읽고
 * HTML 마크업을 제거한 뒤 공백 1개로 연결하여 최대 60,000자로 반환합니다.
 */
public final class AttachText {

    private static final long MAX_FILE_SIZE = 4_194_304L; // 4 MiB
    private static final int MAX_ATTACH_TEXT_LEN = 60_000;

    private static final Pattern SCRIPT_STYLE_PATTERN = Pattern.compile(
            "<(script|style)[\\s\\S]*?</\\1>",
            Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE
    );
    private static final Pattern TAG_PATTERN = Pattern.compile("<[^>]+>");
    private static final Pattern APOS_PATTERN = Pattern.compile("&#0*39;");
    private static final Pattern WHITESPACE_PATTERN = Pattern.compile("\\s+");

    private AttachText() {}

    /**
     * 노트 폴더의 attach/ 아래 HTML 첨부 파일들의 평문을 추출하여 결합합니다.
     * @param vaultRoot vault 루트 경로
     * @param note 대상 노트 레코드
     * @return 결합된 첨부 텍스트 (없으면 null)
     */
    public static String extract(Path vaultRoot, NoteScanner.NoteRecord note) {
        if (note == null || note.isSingleFile()) {
            return null;
        }

        Path attachDir = vaultRoot.resolve(note.vaultRelPath()).resolve("attach");
        if (!Files.exists(attachDir) || !Files.isDirectory(attachDir)) {
            return null;
        }

        List<String> htmlNames = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(attachDir)) {
            for (Path p : ds) {
                String name = p.getFileName().toString();
                if (isHtmlAttachmentName(name)) {
                    htmlNames.add(name);
                }
            }
        } catch (IOException e) {
            // 18.5 7518행: attach/ 폴더 목록을 못 읽어도 0개이고 실패하지 않는다
            return null;
        }

        if (htmlNames.isEmpty()) {
            return null;
        }

        // 이름의 UTF-16 순서로 정렬 (10.5 2번)
        htmlNames.sort(Utf16::compare);

        List<String> plainTexts = new ArrayList<>();
        for (String name : htmlNames) {
            Path file = attachDir.resolve(name);
            if (!Files.isRegularFile(file)) {
                // 심볼릭 링크는 따라간다. 따라간 끝이 일반 파일이 아니거나(색인 뒤 지워진 것 포함) 건너뛴다
                continue;
            }
            try {
                long size;
                try {
                    size = Files.size(file);
                } catch (IOException e) {
                    // 10.5 2번: 크기를 못 읽으면 0바이트로 보고 읽기를 시도한다
                    size = 0L;
                }
                if (size > MAX_FILE_SIZE) {
                    continue;
                }
                byte[] bytes = Files.readAllBytes(file);
                String content = new String(bytes, StandardCharsets.UTF_8);
                String plain = htmlToPlainText(content);
                if (plain != null && !plain.isEmpty()) {
                    plainTexts.add(plain);
                }
            } catch (IOException e) {
                // 읽다 입출력 오류가 나면 건너뛰지 않고 명령 전체가 예상 밖 실패로 끝난다 (10.5, 10.9, 9.9, 11.9, 18.9)
                throw new RuntimeException("HTML attachment read error: " + e.getMessage(), e);
            }
        }

        if (plainTexts.isEmpty()) {
            return null;
        }

        String joined = String.join(" ", plainTexts);
        if (joined.length() > MAX_ATTACH_TEXT_LEN) {
            joined = joined.substring(0, MAX_ATTACH_TEXT_LEN);
        }
        return joined;
    }

    private static boolean isHtmlAttachmentName(String name) {
        String lower = name.toLowerCase(Locale.ROOT);
        if (!lower.endsWith(".html") && !lower.endsWith(".htm")) {
            return false;
        }
        // 이름은 1자 이상 뒤에 .htm 이나 .html 로 끝나야 함 (10.5 1번)
        int dot = name.lastIndexOf('.');
        if (dot <= 0) {
            return false;
        }
        // 점 앞에 U+0085, U+2028, U+2029 가 든 이름은 제외 (10.5 1번)
        char beforeDot = name.charAt(dot - 1);
        if (beforeDot == '\u0085' || beforeDot == '\u2028' || beforeDot == '\u2029') {
            return false;
        }
        return true;
    }

    /**
     * 설명서 10.5의 html 4단계 평문 추출 규칙:
     * 1. (i) <(script|style)[\s\S]*?</\1> -> 공백
     * 2. <[^>]+> -> 공백
     * 3. 엔티티 치환:
     *    &amp; -> &, &lt; -> <, &gt; -> >, &quot; -> "
     *    &#0*39; 및 &apos; -> '
     *    &nbsp; -> U+0020
     * 4. \s+ -> 공백
     * 5. 양 끝 5.3 다듬기
     */
    public static String htmlToPlainText(String html) {
        if (html == null || html.isEmpty()) {
            return "";
        }

        // 1. script / style 제거
        String step1 = SCRIPT_STYLE_PATTERN.matcher(html).replaceAll(" ");

        // 2. 태그 제거
        String step2 = TAG_PATTERN.matcher(step1).replaceAll(" ");

        // 3. 엔티티 치환 (순서대로)
        String step3 = step2.replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"");

        step3 = APOS_PATTERN.matcher(step3).replaceAll("'");
        step3 = step3.replace("&apos;", "'")
                .replace("&nbsp;", " ");

        // 4. \s+ -> 공백
        String step4 = WHITESPACE_PATTERN.matcher(step3).replaceAll(" ");

        // 5. 앞뒤 5.3 공백 제거
        return Trim.trim(step4);
    }
}
