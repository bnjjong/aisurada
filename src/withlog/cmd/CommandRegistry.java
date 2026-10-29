package withlog.cmd;

import java.util.*;

/**
 * 도구 29개와 명령 61개 및 사용법 목록을 관리하는 레지스트리.
 * 설명서 부록 A 및 B 규격을 준수합니다.
 */
public final class CommandRegistry {

    private CommandRegistry() {}

    public static final Set<String> TOOL_NAMES = Collections.unmodifiableSet(new LinkedHashSet<>(List.of(
            "create_doc", "update_doc", "set_favorite", "evaluate_doc", "link_docs",
            "list_docs", "search_docs", "list_tags", "get_doc", "delete_doc",
            "list_trash", "restore_doc", "purge_doc", "plan_day", "list_reports",
            "get_report", "list_templates", "get_template", "save_report", "save_template",
            "create_sub_note", "list_sub_notes", "get_sub_note", "update_sub_note", "delete_sub_note",
            "write_attachment", "list_attachments", "list_comments", "export_markdown"
    )));

    public static final Set<String> COMMAND_NAMES = Collections.unmodifiableSet(new LinkedHashSet<>(List.of(
            "reindex", "reindex-paths", "list-docs", "search-docs", "get-doc",
            "list-sub-notes", "get-sub-note", "list-attachments", "list-comments", "list-tags",
            "list-trash", "list-reports", "get-report", "list-templates", "get-template",
            "list-agendas", "list-projects", "create-project", "update-project", "delete-project",
            "save-report", "save-template", "set-favorite", "evaluate-doc", "link-docs",
            "write-attachment", "delete-doc", "restore-doc", "purge-doc", "create-sub-note",
            "update-sub-note", "delete-sub-note", "export-markdown", "create-doc", "update-doc",
            "empty-trash", "create-agenda", "update-agenda", "delete-agenda", "save-ritual-template",
            "delete-doc-template", "init-workspace", "create-comment", "update-comment", "delete-comment",
            "shard-notes", "verify-shard", "plan-day", "check-notes", "history",
            "commit-vault", "get-agenda-facts", "get-ask", "append-ask-turn", "list-chats",
            "append-chat-turn", "update-agenda-facts", "facts-brief", "read-session", "session-titles",
            "recent-sessions"
    )));

    public static final Set<String> PART4_COMMANDS = Collections.unmodifiableSet(new LinkedHashSet<>(List.of(
            "get-ask", "append-ask-turn", "list-chats", "append-chat-turn",
            "read-session", "session-titles", "recent-sessions"
    )));

    public static final List<String> USAGE_LINES = List.of(
            "reindex",
            "reindex-paths <vault 상대 경로> [경로...]",
            "list-docs [agenda=<slug 또는 이름>] [type=<유형>] [tag=<태그,태그>] [favorite=true] [favoriteCategory=<라벨>] [date=<YYYY-MM-DD>] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [limit=<건수. 0 이면 전부>] [bodies=true. 본문까지 싣는다. 기본은 메타만]",
            "search-docs <검색어> [agenda=<slug 또는 이름>] [type=<유형>] [tag=<태그,태그>] [favorite=true] [favoriteCategory=<라벨>] [date=<YYYY-MM-DD>] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [limit=<건수. 기본 20, 최대 200>]",
            "get-doc <노트 id>",
            "list-sub-notes <부모 노트 id> [bodies=true. 본문까지 싣는다. 기본은 메타만]",
            "get-sub-note <부모 노트 id> <하위 노트 id>",
            "list-attachments <노트 id> [하위 노트 id]",
            "list-comments <노트 id> [하위 노트 id]",
            "list-tags [query=<태그 이름의 일부>] [limit=<받을 태그 수. 1 이상>]",
            "list-trash",
            "list-reports [kind=morning|evening|weekly] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [bodies=true. 본문과 메모까지 싣는다. 기본은 유무만]",
            "get-report <회고 id. 예: rv-evening-2026-06-04>",
            "list-templates [category=doc|ritual] [type=<유형. 주면 리추얼이 빠진다>] [bodies=true. 본문까지 싣는다. 기본은 메타만]",
            "get-template <템플릿 id>",
            "list-agendas",
            "list-projects",
            "create-project title=<이름> [hue=<색>] [summary=<한 문단 설명>]",
            "update-project <프로젝트 id> [title=<이름>] [hue=<색>] [summary=<한 문단 설명>]",
            "delete-project <프로젝트 id>",
            "save-report kind=<morning|evening|weekly> date=<YYYY-MM-DD> [body=<다듬은 본문>] [memo=<개인 메모>]",
            "save-template [id=<고칠 템플릿 id. 없으면 새로 만든다>] [name=<이름. 새로 만들 때 필수>] [type=<plan|build|report|research|meeting|free. 새로 만들 때 기본 plan>] [desc=<언제 쓰는 템플릿인지>] [body=<본문. 새로 만들 때 필수>]",
            "set-favorite <노트 id> [favorite=true|false. 안 주면 지금 값을 뒤집는다] [category=<라벨. 빈 값이면 지운다>] [f1=<직전에 읽은 토큰>]",
            "evaluate-doc <노트 id> [score=<0~100>] [note=<평가 메모>] [clear=true] [f1=<직전에 읽은 토큰>]",
            "link-docs <출발 노트 id> <대상 노트 id> [kind=direct|indirect|attach] [remove=true] [f1=<직전에 읽은 토큰>]",
            "write-attachment <노트 id> [하위 노트 id] name=<파일명.확장자> content=<내용>",
            "delete-doc <노트 id>",
            "restore-doc <휴지통 항목 id. list-trash 로 조회한다>",
            "purge-doc <휴지통 항목 id. list-trash 로 조회한다>",
            "create-sub-note <부모 노트 id> title=<제목> body=<본문> [type=<유형>]",
            "update-sub-note <부모 노트 id> <하위 노트 id> [title=<제목>] [body=<본문>] [tags=<JSON 배열. 전체 교체>] [f1=<직전에 읽은 토큰>]",
            "delete-sub-note <부모 노트 id> <하위 노트 id>",
            "export-markdown",
            "create-doc agenda=<slug 또는 이름> type=<유형> title=<제목> body=<본문> [date=<YYYY-MM-DD>] [time=<HH:MM>] [mins=<분>] [format=markdown] [links=<JSON 배열>] [tags=<JSON 배열>] [template=<템플릿 id>] [tier=reading|record]",
            "update-doc <노트 id> [title=<제목>] [body=<본문>] [tags=<JSON 배열. 전체 교체>] [template=<템플릿 id>] [date=<YYYY-MM-DD>] [time=<HH:MM>] [mins=<분>] [f1=<직전에 읽은 토큰>]",
            "empty-trash",
            "create-agenda title=<이름> [hue=<색>] [repo=<저장소>] [summary=<한 문단 설명>] [project=<프로젝트 slug>]",
            "update-agenda <아젠다 id> [title=<이름>] [hue=<색>] [repo=<저장소. 빈 값이면 지운다>] [summary=<한 문단 설명>] [project=<프로젝트 slug. 빈 값이면 미분류로>]",
            "delete-agenda <아젠다 id>",
            "save-ritual-template <리추얼 템플릿 id> [name=<이름>] [desc=<언제 쓰는 양식인지>] [body=<본문>]",
            "delete-doc-template <문서 템플릿 id>",
            "init-workspace [name=<워크스페이스 이름>] [templates=<빌트인 템플릿 JSON 배열>]",
            "create-comment <노트 id> [하위 노트 id] body=<본문> [anchor_quote=<가리킨 글>] [comment_id=<식별자. 안 주면 발급한다>]",
            "update-comment <노트 id> [하위 노트 id] comment_id=<코멘트 id> body=<본문>",
            "delete-comment <노트 id> [하위 노트 id] comment_id=<코멘트 id>",
            "shard-notes [apply=true]",
            "verify-shard [file=<매핑 파일 이름>]",
            "plan-day [date=<YYYY-MM-DD>] [agenda=<slug 또는 이름. 주면 저장한다>] [dryRun=true]",
            "check-notes [date=<YYYY-MM-DD>]",
            "history <노트 id> [번호] [commit=<커밋>] [query=<글자>]",
            "commit-vault",
            "get-agenda-facts <아젠다 id>",
            "get-ask <노트 id> [하위 노트 id]",
            "append-ask-turn <노트 id> [하위 노트 id] session-id=<대화 id> turn-id=<회차 id> mark-id=<밑줄 id> asked-at=<시각> [quote=<물은 대목>] question=<질문> answer=<답> [error=<실패 까닭>] model=<모델> effort=<노력> elapsed-ms=<밀리초> [session-lines=<줄 수>] [images=<attach/이름,…>]",
            "list-chats <노트 id> [하위 노트 id]",
            "append-chat-turn <노트 id> [하위 노트 id] chat-id=<대화 id> source-session=<세션 id> turn-id=<회차 id> asked-at=<시각> question=<질문> answer=<답> [error=<실패 까닭>] model=<모델> effort=<노력> elapsed-ms=<밀리초> [images=<attach/이름,…>]",
            "update-agenda-facts <아젠다 id> facts=<지금도 참인 것. 빈 값이면 지운다>",
            "facts-brief [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [min-new-file=<노트 수>]",
            "read-session <세션 id> [limit=<개수>] [before=<순번>] [urls=true] [note=<노트 id>] [author=<노트 id>,...]",
            "session-titles <세션 id> [세션 id...]",
            "recent-sessions [days=<일 수>]"
    );

    /**
     * 이 판이 지은 명령인지 확인합니다. 읽기 명령 14개(list-agendas 포함)와 reindex, 쓰기 명령 12개(create_doc, update_doc, set_favorite,
     * evaluate_doc, link_docs, write_attachment, create_sub_note, update_sub_note, delete_sub_note, create-agenda, update-agenda,
     * delete-agenda)다. 아젠다 명령 4개는 명령 표면에만 있어서 밑줄 이름이 없다(20.2).
     */
    public static boolean isSupported(String name) {
        return "reindex".equals(name) ||
                "list-agendas".equals(name) || "create-agenda".equals(name) ||
                "update-agenda".equals(name) || "delete-agenda".equals(name) ||
                "create_doc".equals(name) || "create-doc".equals(name) ||
                "update_doc".equals(name) || "update-doc".equals(name) ||
                "write_attachment".equals(name) || "write-attachment".equals(name) ||
                "create_sub_note".equals(name) || "create-sub-note".equals(name) ||
                "update_sub_note".equals(name) || "update-sub-note".equals(name) ||
                "delete_sub_note".equals(name) || "delete-sub-note".equals(name) ||
                "set_favorite".equals(name) || "set-favorite".equals(name) ||
                "evaluate_doc".equals(name) || "evaluate-doc".equals(name) ||
                "link_docs".equals(name) || "link-docs".equals(name) ||
                "get_doc".equals(name) || "get-doc".equals(name) ||
                "get_sub_note".equals(name) || "get-sub-note".equals(name) ||
                "list_attachments".equals(name) || "list-attachments".equals(name) ||
                "list_comments".equals(name) || "list-comments".equals(name) ||
                "create-comment".equals(name) || "update-comment".equals(name) || "delete-comment".equals(name) ||
                "list_sub_notes".equals(name) || "list-sub-notes".equals(name) ||
                "list_docs".equals(name) || "list-docs".equals(name) ||
                "list_tags".equals(name) || "list-tags".equals(name) ||
                "search_docs".equals(name) || "search-docs".equals(name) ||
                "list_templates".equals(name) || "list-templates".equals(name) ||
                "get_template".equals(name) || "get-template".equals(name) ||
                "list_reports".equals(name) || "list-reports".equals(name) ||
                "get_report".equals(name) || "get-report".equals(name) ||
                "list_trash".equals(name) || "list-trash".equals(name);
    }

    /**
     * 설명서 32.1에 정의된 4부(Claude 연동) 명령 7개 여부를 확인합니다.
     *
     * @param name 명령 이름
     * @return 4부 명령이면 true
     */
    public static boolean isPart4Command(String name) {
        return PART4_COMMANDS.contains(name);
    }

    /**
     * 설명서 3.1에 명시된 모르는 명령 오류 문구를 생성합니다.
     */
    public static String buildUnknownCommandMessage(String name) {
        StringBuilder sb = new StringBuilder();
        sb.append("모르는 명령이다: ").append(name).append("\n");
        sb.append("도구 이름으로 부르려면 이렇게 한다: <도구> '<json>'\n");
        sb.append("  list_docs, search_docs, list_tags, get_doc, list_trash, list_reports, get_report, list_templates, get_template, list_sub_notes, get_sub_note, list_attachments\n");
        sb.append("사이드카 고유 명령은 이렇다.\n");
        for (String line : USAGE_LINES) {
            sb.append("  ").append(line).append("\n");
        }
        return sb.toString();
    }
}
