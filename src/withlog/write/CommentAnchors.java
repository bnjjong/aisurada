package withlog.write;

import java.util.regex.Pattern;

/** 19.8: 키워드의 대소문자는 무시하되 잡힌 id는 글자 그대로 비교한다. */
public final class CommentAnchors {
    private static final Pattern MARKER = Pattern.compile("<!--(/?)wlc:(cm[a-z0-9]+-[a-z0-9]{1,8})-->",
            Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE);
    private CommentAnchors() {}

    public static String strip(String body, String commentId) {
        var matcher = MARKER.matcher(body);
        StringBuilder result = new StringBuilder(body.length());
        while (matcher.find()) {
            matcher.appendReplacement(result, matcher.group(2).equals(commentId) ? "" : java.util.regex.Matcher.quoteReplacement(matcher.group()));
        }
        return matcher.appendTail(result).toString();
    }
}
