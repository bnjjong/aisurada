package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.vault.CommentsReader;
import withlog.vault.NoteIndex;
import java.nio.file.Path;
import java.util.Map;

/** 19장: 대상 조회 뒤에는 코멘트 파일만 열어 세 상태 봉투를 반환한다. 층 게이트는 없다. */
public final class ListComments {
    public static final String USAGE = "list-comments <노트 id> [하위 노트 id]";
    private ListComments() {}

    public static Map<String, Object> execute(Path vault, ParsedArgs parsed, NoteIndex index) {
        var target = CommentTarget.resolve(vault, index, CommentTarget.parse(parsed, USAGE));
        return CommentsReader.readComments(vault, target.file());
    }
}
