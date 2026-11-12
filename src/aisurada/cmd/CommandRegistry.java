package aisurada.cmd;

import java.util.*;

/**
 * 도구 34개와 명령 66개 및 사용법 목록을 관리하는 통합 레지스트리.
 * 단일 CommandSpec 테이블을 기반으로 목록과 메타데이터를 자동 생성합니다.
 */
public final class CommandRegistry {

    private CommandRegistry() {}

    public static final List<CommandSpec> SPECS = List.of(
            new CommandSpec("reindex", null, false, false, "reindex"),
            new CommandSpec("reindex-paths", null, false, false, "reindex-paths <vault 상대 경로> [경로...]"),
            new CommandSpec("list-docs", "list_docs", false, false, "list-docs [agenda=<slug 또는 이름>] [type=<유형>] [tag=<태그,태그>] [favorite=true] [favoriteCategory=<라벨>] [date=<YYYY-MM-DD>] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [limit=<건수. 0 이면 전부>] [bodies=true. 본문까지 싣는다. 기본은 메타만]"),
            new CommandSpec("search-docs", "search_docs", false, false, "search-docs <검색어> [agenda=<slug 또는 이름>] [type=<유형>] [tag=<태그,태그>] [favorite=true] [favoriteCategory=<라벨>] [date=<YYYY-MM-DD>] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [limit=<건수. 기본 20, 최대 200>]"),
            new CommandSpec("get-doc", "get_doc", false, false, "get-doc <노트 id>"),
            new CommandSpec("list-sub-notes", "list_sub_notes", false, false, "list-sub-notes <부모 노트 id> [bodies=true. 본문까지 싣는다. 기본은 메타만]"),
            new CommandSpec("get-sub-note", "get_sub_note", false, false, "get-sub-note <부모 노트 id> <하위 노트 id>"),
            new CommandSpec("list-attachments", "list_attachments", false, false, "list-attachments <노트 id> [하위 노트 id]"),
            new CommandSpec("list-comments", "list_comments", false, false, "list-comments <노트 id> [하위 노트 id]"),
            new CommandSpec("list-tags", "list_tags", false, false, "list-tags [query=<태그 이름의 일부>] [limit=<받을 태그 수. 1 이상>]"),
            new CommandSpec("list-trash", "list_trash", false, false, "list-trash"),
            new CommandSpec("list-templates", "list_templates", false, false, "list-templates [category=doc|ritual] [type=<유형. 주면 리추얼이 빠진다>] [bodies=true. 본문까지 싣는다. 기본은 메타만]"),
            new CommandSpec("get-template", "get_template", false, false, "get-template <템플릿 id>"),
            new CommandSpec("list-agendas", null, false, false, "list-agendas"),
            new CommandSpec("list-projects", null, false, false, "list-projects"),
            new CommandSpec("create-project", null, true, false, "create-project title=<이름> [hue=<색>] [summary=<한 문단 설명>]"),
            new CommandSpec("update-project", null, true, false, "update-project <프로젝트 id> [title=<이름>] [hue=<색>] [summary=<한 문단 설명>]"),
            new CommandSpec("delete-project", null, true, false, "delete-project <프로젝트 id>"),
            new CommandSpec("save-template", "save_template", true, false, TemplateCommands.SAVE_USAGE),
            new CommandSpec("set-favorite", "set_favorite", true, false, "set-favorite <노트 id> [favorite=true|false. 안 주면 지금 값을 뒤집는다] [category=<라벨. 빈 값이면 지운다>] [f1=<직전에 읽은 토큰>]"),
            new CommandSpec("evaluate-doc", "evaluate_doc", true, false, "evaluate-doc <노트 id> [score=<0~100>] [note=<평가 메모>] [clear=true] [f1=<직전에 읽은 토큰>]"),
            new CommandSpec("link-docs", "link_docs", true, false, "link-docs <출발 노트 id> <대상 노트 id> [kind=direct|indirect|attach] [remove=true] [f1=<직전에 읽은 토큰>]"),
            new CommandSpec("write-attachment", "write_attachment", true, false, "write-attachment <노트 id> [하위 노트 id] name=<파일명.확장자> content=<내용>"),
            new CommandSpec("delete-doc", "delete_doc", true, false, "delete-doc <노트 id>"),
            new CommandSpec("restore-doc", "restore_doc", true, false, "restore-doc <휴지통 항목 id. list-trash 로 조회한다>"),
            new CommandSpec("purge-doc", "purge_doc", true, false, "purge-doc <휴지통 항목 id. list-trash 로 조회한다>"),
            new CommandSpec("create-sub-note", "create_sub_note", true, false, "create-sub-note <부모 노트 id> title=<제목> body=<본문> [type=<유형>]"),
            new CommandSpec("update-sub-note", "update_sub_note", true, false, "update-sub-note <부모 노트 id> <하위 노트 id> [title=<제목>] [body=<본문>] [tags=<JSON 배열. 전체 교체>] [f1=<직전에 읽은 토큰>]"),
            new CommandSpec("delete-sub-note", "delete_sub_note", true, false, "delete-sub-note <부모 노트 id> <하위 노트 id>"),
            new CommandSpec("export-markdown", "export_markdown", true, false, "export-markdown"),
            new CommandSpec("create-doc", "create_doc", true, false, "create-doc agenda=<slug 또는 이름> type=<유형> title=<제목> body=<본문> [date=<YYYY-MM-DD>] [time=<HH:MM>] [format=markdown] [links=<JSON 배열>] [tags=<JSON 배열>] [template=<템플릿 id>] [tier=reading|record]"),
            new CommandSpec("update-doc", "update_doc", true, false, "update-doc <노트 id> [title=<제목>] [body=<본문>] [tags=<JSON 배열. 전체 교체>] [template=<템플릿 id>] [date=<YYYY-MM-DD>] [time=<HH:MM>] [f1=<직전에 읽은 토큰>]"),
            new CommandSpec("empty-trash", null, true, false, "empty-trash"),
            new CommandSpec("create-agenda", null, true, false, "create-agenda title=<이름> [hue=<색>] [repo=<저장소>] [summary=<한 문단 설명>] [project=<프로젝트 slug>]"),
            new CommandSpec("update-agenda", null, true, false, "update-agenda <아젠다 id> [title=<이름>] [hue=<색>] [repo=<저장소. 빈 값이면 지운다>] [summary=<한 문단 설명>] [project=<프로젝트 slug. 빈 값이면 미분류로>]"),
            new CommandSpec("delete-agenda", null, true, false, "delete-agenda <아젠다 id>"),
            new CommandSpec("save-ritual-template", null, true, false, TemplateCommands.RITUAL_USAGE),
            new CommandSpec("delete-doc-template", null, true, false, TemplateCommands.DELETE_USAGE),
            new CommandSpec("init-workspace", null, true, false, "init-workspace [name=<워크스페이스 이름>] [templates=<빌트인 템플릿 JSON 배열>]"),
            new CommandSpec("create-comment", null, true, false, "create-comment <노트 id> [하위 노트 id] body=<본문> [anchor_quote=<가리킨 글>] [comment_id=<식별자. 안 주면 발급한다>]"),
            new CommandSpec("update-comment", null, true, false, "update-comment <노트 id> [하위 노트 id] comment_id=<코멘트 id> body=<본문>"),
            new CommandSpec("delete-comment", null, true, false, "delete-comment <노트 id> [하위 노트 id] comment_id=<코멘트 id>"),
            new CommandSpec("shard-notes", null, true, false, "shard-notes [apply=true]"),
            new CommandSpec("verify-shard", null, false, false, "verify-shard [file=<매핑 파일 이름>]"),
            new CommandSpec("plan-day", "plan_day", true, false, "plan-day [date=<YYYY-MM-DD>] [agenda=<slug 또는 이름. 주면 저장한다>] [dryRun=true]"),
            new CommandSpec("check-notes", null, true, false, "check-notes [date=<YYYY-MM-DD>]"),
            new CommandSpec("history", null, false, false, "history <노트 id> [번호] [commit=<커밋>] [query=<글자>]"),
            new CommandSpec("commit-vault", null, false, false, "commit-vault"),
            new CommandSpec("get-agenda-facts", null, false, false, AgendaFacts.GET_USAGE),
            new CommandSpec("get-ask", null, false, true, "get-ask <노트 id> [하위 노트 id]"),
            new CommandSpec("append-ask-turn", null, true, true, "append-ask-turn <노트 id> [하위 노트 id] session-id=<대화 id> turn-id=<회차 id> mark-id=<밑줄 id> asked-at=<시각> [quote=<물은 대목>] question=<질문> answer=<답> [error=<실패 까닭>] model=<모델> effort=<노력> elapsed-ms=<밀리초> [session-lines=<줄 수>] [images=<attach/이름,…>]"),
            new CommandSpec("list-chats", null, false, true, "list-chats <노트 id> [하위 노트 id]"),
            new CommandSpec("append-chat-turn", null, true, true, "append-chat-turn <노트 id> [하위 노트 id] chat-id=<대화 id> source-session=<세션 id> turn-id=<회차 id> asked-at=<시각> question=<질문> answer=<답> [error=<실패 까닭>] model=<모델> effort=<노력> elapsed-ms=<밀리초> [images=<attach/이름,…>]"),
            new CommandSpec("update-agenda-facts", null, true, false, AgendaFacts.UPDATE_USAGE),
            new CommandSpec("facts-brief", null, true, false, FactsBrief.USAGE),
            new CommandSpec("read-session", null, false, true, "read-session <세션 id> [limit=<개수>] [before=<순번>] [urls=true] [note=<노트 id>] [author=<노트 id>,...]"),
            new CommandSpec("session-titles", null, false, true, "session-titles <세션 id> [세션 id...]"),
            new CommandSpec("list-resources", "list_resources", false, false, ResourceCommands.LIST_USAGE),
            new CommandSpec("get-resource", "get_resource", false, false, ResourceCommands.GET_USAGE),
            new CommandSpec("save-resource", "save_resource", true, false, ResourceCommands.SAVE_USAGE),
            new CommandSpec("delete-resource", "delete_resource", true, false, ResourceCommands.DELETE_USAGE),
            new CommandSpec("restore-resource", "restore_resource", true, false, ResourceCommands.RESTORE_USAGE),
            new CommandSpec("set-resource-ref", "set_resource_ref", true, false, ResourceCommands.REF_USAGE),
            new CommandSpec("usable-resources", "usable_resources", false, false, ResourceCommands.USABLE_USAGE),
            new CommandSpec("list-resource-versions", "list_resource_versions", false, false, ResourceCommands.VERSIONS_USAGE),
            new CommandSpec("recent-sessions", null, false, true, "recent-sessions [days=<일 수>]")
    );

    public static final Set<String> WRITE_COMMANDS;
    public static final Set<String> TOOL_NAMES;
    public static final Set<String> COMMAND_NAMES;
    public static final Set<String> PART4_COMMANDS;
    public static final List<String> USAGE_LINES;

    static {
        Set<String> writes = new LinkedHashSet<>();
        Set<String> tools = new LinkedHashSet<>();
        Set<String> commands = new LinkedHashSet<>();
        Set<String> part4 = new LinkedHashSet<>();
        List<String> usages = new ArrayList<>();

        for (CommandSpec s : SPECS) {
            commands.add(s.name());
            usages.add(s.usage());
            if (s.isWrite()) writes.add(s.name());
            if (s.toolName() != null) tools.add(s.toolName());
            if (s.isPart4()) part4.add(s.name());
        }

        WRITE_COMMANDS = Collections.unmodifiableSet(writes);
        TOOL_NAMES = Collections.unmodifiableSet(tools);
        COMMAND_NAMES = Collections.unmodifiableSet(commands);
        PART4_COMMANDS = Collections.unmodifiableSet(part4);
        USAGE_LINES = Collections.unmodifiableList(usages);
    }

    public static boolean isSupported(String name) {
        return COMMAND_NAMES.contains(name) || TOOL_NAMES.contains(name);
    }

    public static boolean isPart4Command(String name) {
        return PART4_COMMANDS.contains(name);
    }

    public static String buildUnknownCommandMessage(String name) {
        StringBuilder sb = new StringBuilder();
        sb.append("모르는 명령이다: ").append(name).append("\n");
        sb.append("도구 이름으로 부르려면 이렇게 한다: <도구> '<json>'\n");
        sb.append("  list_docs, search_docs, list_tags, get_doc, list_trash, list_templates, get_template, list_sub_notes, get_sub_note, list_attachments\n");
        sb.append("사이드카 고유 명령은 이렇다.\n");
        for (String line : USAGE_LINES) {
            sb.append("  ").append(line).append("\n");
        }
        return sb.toString();
    }
}
