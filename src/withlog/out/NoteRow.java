package withlog.out;

import withlog.text.Uri;
import withlog.vault.NoteScanner;

import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * 노트 및 하위 노트의 응답 행(Map)을 설명서 규격(9.6, 10.6, 11.6, 11.8, 16.6)의
 * 엄격한 키 순서에 맞춰 생성하는 유틸리티.
 */
public final class NoteRow {

    private NoteRow() {}

    /**
     * get_doc (11.6) 상세 응답 객체 생성.
     */
    public static Map<String, Object> forGetDoc(
            Path vaultRoot,
            NoteScanner.NoteRecord note,
            String agendaTitle,
            String agendaFacts,
            Map<String, Object> commentsEnvelope,
            boolean isToolSurface
    ) {
        boolean isJson = "json".equals(note.format());
        boolean suppressBodyAndTokens = isJson || note.isExtensionFolder();

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", note.meta().id());
        resp.put("agenda", note.meta().agendaSlug());
        resp.put("agendaTitle", agendaTitle);

        if (isToolSurface && agendaFacts != null) {
            resp.put("agendaFacts", agendaFacts);
        }

        resp.put("type", note.meta().type());
        resp.put("title", note.meta().title());
        resp.put("created", note.meta().created());

        if (!suppressBodyAndTokens && note.meta().sessionId() != null) {
            resp.put("sessionId", note.meta().sessionId());
        }

        resp.put("date", note.meta().date());
        resp.put("time", note.meta().time());
        resp.put("mins", note.meta().mins());

        if (!isToolSurface && "record".equals(note.meta().tier())) {
            resp.put("tier", "record");
        }

        if (!suppressBodyAndTokens && note.meta().editedBy() != null && note.meta().editedAt() != null) {
            resp.put("editedBy", note.meta().editedBy());
            resp.put("editedAt", note.meta().editedAt());
            if (note.meta().editedSessionId() != null && !"app".equals(note.meta().editedBy())) {
                resp.put("editedSessionId", note.meta().editedSessionId());
            }
        }

        resp.put("format", note.format());

        if (!suppressBodyAndTokens && note.parsedFile().token() != null) {
            resp.put("precondition", note.parsedFile().token());
        }

        resp.put("comments", commentsEnvelope);
        resp.put("body", suppressBodyAndTokens ? "" : note.parsedFile().body());
        resp.put("links", note.meta().links());
        resp.put("tags", note.meta().tags());

        if (note.meta().favorite()) {
            resp.put("favorite", true);
        }
        if (note.meta().favoriteCategory() != null) {
            resp.put("favoriteCategory", note.meta().favoriteCategory());
        }
        if (note.meta().template() != null) {
            resp.put("template", note.meta().template());
        }
        if (note.meta().evaluation() != null) {
            resp.put("evaluation", note.meta().evaluation());
        }

        resp.put("path", note.vaultRelPath());

        String deepLink = "withlog://doc/" + Uri.encodeURIComponent(note.meta().id()) +
                "?ws=" + Uri.encodeURIComponent(vaultRoot.toString());
        resp.put("deepLink", deepLink);

        return resp;
    }

    /**
     * list_docs (9.6) 목록 줄 객체 생성.
     */
    public static Map<String, Object> forListDocs(
            NoteScanner.NoteRecord note,
            String agendaTitle,
            boolean isToolSurface,
            boolean includeBodies,
            String attachText
    ) {
        boolean isJson = "json".equals(note.format());
        boolean suppressBodyAndTokens = isJson || note.isExtensionFolder();

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", note.meta().id());
        resp.put("agenda", note.meta().agendaSlug());
        resp.put("agendaTitle", agendaTitle);
        resp.put("type", note.meta().type());
        resp.put("title", note.meta().title());
        resp.put("created", note.meta().created());

        if (!suppressBodyAndTokens && note.meta().sessionId() != null) {
            resp.put("sessionId", note.meta().sessionId());
        }

        resp.put("date", note.meta().date());
        resp.put("time", note.meta().time());
        resp.put("mins", note.meta().mins());

        if (!isToolSurface && "record".equals(note.meta().tier())) {
            resp.put("tier", "record");
        }

        if (!suppressBodyAndTokens && note.meta().editedBy() != null && note.meta().editedAt() != null) {
            resp.put("editedBy", note.meta().editedBy());
            resp.put("editedAt", note.meta().editedAt());
            if (note.meta().editedSessionId() != null && !"app".equals(note.meta().editedBy())) {
                resp.put("editedSessionId", note.meta().editedSessionId());
            }
        }

        resp.put("format", note.format());

        if (!suppressBodyAndTokens && note.parsedFile().token() != null) {
            resp.put("precondition", note.parsedFile().token());
        }

        if (includeBodies) {
            resp.put("body", suppressBodyAndTokens ? "" : note.parsedFile().body());
            if (attachText != null && !attachText.isEmpty()) {
                resp.put("attachText", attachText);
            }
        }

        resp.put("links", note.meta().links());

        // 9.6: tags는 비어있지 않을 때만 싣는다 (비면 키째 제외)
        if (note.meta().tags() != null && !note.meta().tags().isEmpty()) {
            resp.put("tags", note.meta().tags());
        }

        if (note.meta().favorite()) {
            resp.put("favorite", true);
        }
        if (note.meta().favoriteCategory() != null) {
            resp.put("favoriteCategory", note.meta().favoriteCategory());
        }
        if (note.meta().template() != null) {
            resp.put("template", note.meta().template());
        }
        if (note.meta().evaluation() != null) {
            resp.put("evaluation", note.meta().evaluation());
        }

        resp.put("path", note.vaultRelPath());
        return resp;
    }

    /**
     * search_docs (10.6) 목록 줄 객체 생성.
     */
    public static Map<String, Object> forSearchDocs(
            NoteScanner.NoteRecord note,
            String agendaTitle,
            boolean isToolSurface,
            String matchedIn,
            String snippet
    ) {
        // search_docs는 bodies=false 상태의 list_docs 줄 뒤에 matchedIn, snippet을 덧붙임
        Map<String, Object> resp = forListDocs(note, agendaTitle, isToolSurface, false, null);
        resp.put("matchedIn", matchedIn);
        if ("body".equals(matchedIn) && snippet != null) {
            resp.put("snippet", snippet);
        }
        return resp;
    }

    /**
     * list_sub_notes (16.6) 목록 줄 객체 생성.
     */
    public static Map<String, Object> forListSubNotes(
            NoteScanner.NoteRecord sub,
            String parentRef,
            boolean includeBodies
    ) {
        boolean isJson = "json".equals(sub.format());

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", sub.id());
        resp.put("ref", parentRef + "/" + sub.id());
        resp.put("type", sub.meta().type());
        resp.put("title", sub.meta().title());
        resp.put("created", sub.meta().created());
        resp.put("date", sub.meta().date());
        resp.put("time", sub.meta().time());
        resp.put("mins", sub.meta().mins());

        if (!isJson && sub.meta().editedBy() != null && sub.meta().editedAt() != null) {
            resp.put("editedBy", sub.meta().editedBy());
            resp.put("editedAt", sub.meta().editedAt());
            if (sub.meta().editedSessionId() != null && !"app".equals(sub.meta().editedBy())) {
                resp.put("editedSessionId", sub.meta().editedSessionId());
            }
        }

        resp.put("format", sub.format());

        if (!isJson && sub.parsedFile().token() != null) {
            resp.put("precondition", sub.parsedFile().token());
        }

        if (includeBodies) {
            resp.put("body", isJson ? "" : sub.parsedFile().body());
        }

        resp.put("links", sub.meta().links());
        // 16.6: 하위 노트 목록에서는 tags가 비어도 []로 늘 싣는다!
        resp.put("tags", sub.meta().tags());
        resp.put("path", sub.vaultRelPath());

        return resp;
    }

    /**
     * get_sub_note (11.8) 상세 응답 객체 생성.
     */
    public static Map<String, Object> forGetSubNote(
            NoteScanner.NoteRecord sub,
            String normalizedParent,
            Map<String, Object> commentsEnvelope
    ) {
        boolean isJson = "json".equals(sub.format());

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", sub.id());
        resp.put("parent", normalizedParent);
        resp.put("type", sub.meta().type());
        resp.put("title", sub.meta().title());
        resp.put("created", sub.meta().created());
        resp.put("date", sub.meta().date());
        resp.put("time", sub.meta().time());
        resp.put("mins", sub.meta().mins());

        if (!isJson && sub.meta().editedBy() != null && sub.meta().editedAt() != null) {
            resp.put("editedBy", sub.meta().editedBy());
            resp.put("editedAt", sub.meta().editedAt());
            if (sub.meta().editedSessionId() != null && !"app".equals(sub.meta().editedBy())) {
                resp.put("editedSessionId", sub.meta().editedSessionId());
            }
        }

        resp.put("format", sub.format());

        if (!isJson && sub.parsedFile().token() != null) {
            resp.put("precondition", sub.parsedFile().token());
        }

        resp.put("comments", commentsEnvelope);
        resp.put("body", isJson ? "" : sub.parsedFile().body());
        resp.put("links", sub.meta().links());
        resp.put("tags", sub.meta().tags());
        resp.put("path", sub.vaultRelPath());

        return resp;
    }
}
