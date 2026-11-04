package withlog.session;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;

import java.nio.file.Path;
import java.util.*;

/**
 * 4부 세션 전사 명령 처리기.
 * read-session, session-titles, recent-sessions 명령을 실행합니다.
 */
public final class SessionCommands {

    public static final String READ_SESSION_USAGE =
            "read-session <세션 id> [limit=<개수>] [before=<순번>] [urls=true] [note=<노트 id>] [author=<노트 id>,...]";

    private SessionCommands() {}

    /**
     * read-session 명령 실행.
     *
     * @param parsed 파싱된 명령 인자
     * @return 규격화된 세션 대화 및 메타데이터 Map
     */
    public static Map<String, Object> executeReadSession(ParsedArgs parsed) {
        List<String> pos = parsed.positionalArgs();
        if (pos.isEmpty()) {
            throw new WlException(400, "인자가 하나 필요하다: " + READ_SESSION_USAGE);
        }

        String sessionId = pos.get(0).trim();
        validateSessionId(sessionId);

        Map<String, String> opts = parsed.namedOptions();

        // 1. limit
        int limit = 100;
        String rawLimit = opts.get("limit");
        if (rawLimit != null) {
            String trimmed = rawLimit.trim();
            if (!trimmed.isEmpty()) {
                try {
                    limit = Integer.parseInt(trimmed);
                } catch (NumberFormatException e) {
                    throw new WlException(400, "limit 은 숫자여야 한다: " + rawLimit);
                }
            }
        }

        // 2. before
        Integer before = null;
        String rawBefore = opts.get("before");
        if (rawBefore != null) {
            String trimmed = rawBefore.trim();
            if (!trimmed.isEmpty()) {
                try {
                    before = Integer.parseInt(trimmed);
                } catch (NumberFormatException e) {
                    throw new WlException(400, "before 은 숫자여야 한다: " + rawBefore);
                }
            }
        }

        // 3. urls
        boolean includeUrls = false;
        String rawUrls = opts.get("urls");
        if (rawUrls != null) {
            String trimmed = rawUrls.trim();
            if (!trimmed.isEmpty()) {
                if ("true".equalsIgnoreCase(trimmed)) {
                    includeUrls = true;
                } else if ("false".equalsIgnoreCase(trimmed)) {
                    includeUrls = false;
                } else {
                    throw new WlException(400, "urls 은 true 나 false 여야 한다: " + rawUrls);
                }
            }
        }

        // 4. note
        String noteId = null;
        String rawNote = opts.get("note");
        if (rawNote != null) {
            String trimmed = rawNote.trim();
            if (!trimmed.isEmpty()) {
                validateNoteId(trimmed);
                noteId = trimmed;
            }
        }

        // 5. author
        List<String> authorNotes = null;
        String rawAuthor = opts.get("author");
        if (rawAuthor != null) {
            String trimmed = rawAuthor.trim();
            if (!trimmed.isEmpty()) {
                String[] parts = trimmed.split(",", -1);
                if (parts.length < 1 || parts.length > 8) {
                    throw new WlException(400, "author 의 노트 id 는 1개에서 8개까지예요: " + parts.length);
                }
                authorNotes = new ArrayList<>();
                for (String part : parts) {
                    // 7.8, 7.15.7: 조각의 앞뒤 공백은 안 걷는다
                    validateNoteId(part);
                    authorNotes.add(part);
                }
            }
        }

        Path projectsDir = SessionTranscript.getClaudeProjectsDir();
        SessionTranscript.ReadSessionResult res = SessionTranscript.readSession(
                projectsDir, sessionId, limit, before, includeUrls, noteId, authorNotes
        );

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("sessionId", res.sessionId());
        resp.put("file", res.filePath());

        Map<String, Object> src = new LinkedHashMap<>();
        src.put("bytes", res.bytes());
        src.put("lines", res.lines());
        src.put("skipped", res.skipped());
        src.put("messages", res.messages());
        resp.put("source", src);

        resp.put("hasMore", res.hasMore());
        resp.put("urls", res.urls());
        resp.put("runningAgents", res.runningAgents());
        resp.put("todos", res.todos());
        resp.put("runState", res.runState());
        if (rawNote != null && !rawNote.trim().isEmpty()) {
            resp.put("noteMentions", res.noteMentions());
        }
        if (rawAuthor != null && !rawAuthor.trim().isEmpty()) {
            resp.put("author", res.author());
        }
        resp.put("turns", res.turns());

        return resp;
    }

    /**
     * session-titles 명령 실행.
     *
     * @param parsed 파싱된 명령 인자
     * @return 세션 제목 목록 Map
     */
    public static Map<String, Object> executeSessionTitles(ParsedArgs parsed) {
        List<String> pos = parsed.positionalArgs();
        if (pos.size() > 200) {
            throw new WlException(400, "세션을 한 번에 200개까지 물을 수 있어요: " + pos.size());
        }

        // 읽기 전에 모두 검증 (7.10)
        for (String id : pos) {
            validateSessionId(id.trim());
        }

        if (pos.isEmpty()) {
            return Map.of("sessions", Collections.emptyList());
        }

        Path projectsDir = SessionTranscript.getClaudeProjectsDir();
        List<Map<String, Object>> sessions = new ArrayList<>();
        for (String id : pos) {
            String sId = id.trim();
            SessionTranscript.SessionTitleInfo info = SessionTranscript.readTitleInfo(projectsDir, sId);
            Map<String, Object> item = new LinkedHashMap<>();
            item.put("sessionId", info.sessionId());
            item.put("found", info.found());
            item.put("title", info.title());
            item.put("branch", info.branch());
            item.put("cwd", info.cwd());
            sessions.add(item);
        }

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("sessions", sessions);
        return resp;
    }

    /**
     * recent-sessions 명령 실행.
     *
     * @param parsed 파싱된 명령 인자
     * @return 최근 세션 목록 Map
     */
    public static Map<String, Object> executeRecentSessions(ParsedArgs parsed) {
        int days = 7;
        Map<String, String> opts = parsed.namedOptions();
        String rawDays = opts.get("days");
        if (rawDays != null) {
            String trimmed = rawDays.trim();
            if (!trimmed.isEmpty()) {
                try {
                    days = Integer.parseInt(trimmed);
                } catch (NumberFormatException e) {
                    throw new WlException(400, "days 은 숫자여야 한다: " + rawDays);
                }
                if (days < 1 || days > 30) {
                    throw new WlException(400, "days 는 1 부터 30 사이여야 해요: " + rawDays);
                }
            }
        }

        Path projectsDir = SessionTranscript.getClaudeProjectsDir();
        SessionTranscript.RecentSessionsResult res = SessionTranscript.recentSessions(projectsDir, days);

        List<Map<String, Object>> sessList = new ArrayList<>();
        for (SessionTranscript.RecentSessionEntry e : res.sessions()) {
            Map<String, Object> item = new LinkedHashMap<>();
            item.put("sessionId", e.sessionId());
            item.put("lastActiveAt", e.lastActiveAt());
            item.put("startedAt", e.startedAt());
            item.put("title", e.title());
            item.put("branch", e.branch());
            item.put("cwd", e.cwd());
            sessList.add(item);
        }

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("days", res.days());
        resp.put("dropped", res.dropped());
        resp.put("sessions", sessList);
        return resp;
    }

    private static void validateSessionId(String id) {
        if (id == null || !SessionTranscript.SESSION_ID_PATTERN.matcher(id).matches()) {
            throw new WlException(400, "세션 식별자 모양이 아니에요: " + id);
        }
    }

    private static void validateNoteId(String id) {
        if (id == null || !SessionTranscript.NOTE_ID_PATTERN.matcher(id).matches()) {
            throw new WlException(400, "노트 id 모양이 아니에요: " + id);
        }
    }
}
