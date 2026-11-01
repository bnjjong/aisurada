import { useEffect, useMemo, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { asText } from "../workspace/types.ts";
import type { Note, WorkspaceState } from "../workspace/types.ts";
import { docAddress } from "../navigation/routes.ts";
import { NoteReading } from "../navigation/NoteReading.tsx";
import { Settings } from "../ui/Settings.tsx";
import { Forms } from "../ui/Forms.tsx";
import { visibleNotes, displayTitle, TYPE_NAMES } from "../ui/model.ts";
import { findNotes, emptySearch, searchTrim } from "../ui/search.ts";

export function InteractionOverlays({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  return (
    <>
      {state.settingsOpen && <Settings controller={controller} state={state} />}
      {state.form && <Forms controller={controller} state={state} />}
      {state.subNotes && (
        <SubNotePages
          key={`${state.subNotes.root}/${state.subNotes.ids.join("/")}`}
          controller={controller}
          state={state}
        />
      )}
      {state.searchOpen && (
        <QuickSearch controller={controller} state={state} />
      )}
    </>
  );
}

function QuickSearch({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  const [query, setQuery] = useState(""),
    [selected, setSelected] = useState(0);
  const [initial] = useState(() => ({
    recent: controller.recentNotes.list(state.workspace!.path),
    live: controller.recentNotes.mounted(),
  }));
  const results = useMemo(() => {
    const notes = visibleNotes(state.workspace!.notes, state.prefs.showRecords);
    return searchTrim(query)
      ? findNotes(notes, state.workspace!.agendas, query)
      : emptySearch(notes, initial.recent, initial.live);
  }, [query, state.workspace, state.prefs.showRecords]);
  const matches = results.slice(0, 20);
  const open = (note: Note) => {
    controller.closeSearch();
    controller.navigate(docAddress(note.id));
  };
  return (
    <div className="cmdk-overlay" onClick={controller.closeSearch}>
      <section
        className="cmdk-panel"
        role="dialog"
        aria-modal="true"
        aria-label="노트 빠른 검색"
        onClick={(event) => event.stopPropagation()}
      >
        <input
          className="cmdk-input"
          autoFocus
          role="combobox"
          aria-label="노트 검색"
          aria-expanded="true"
          aria-controls="cmdk-results"
          aria-activedescendant={
            matches[selected] ? `cmdk-result-${selected}` : undefined
          }
          placeholder="노트 찾기…"
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setSelected(0);
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              controller.closeSearch();
            }
            if (event.key === "ArrowDown" || event.key === "ArrowUp") {
              event.preventDefault();
              setSelected((index) =>
                Math.max(
                  0,
                  Math.min(
                    matches.length - 1,
                    index + (event.key === "ArrowDown" ? 1 : -1),
                  ),
                ),
              );
            }
            if (event.key === "Enter" && matches[selected]) {
              event.preventDefault();
              open(matches[selected].note);
            }
          }}
        />
        <div className="cmdk-results" id="cmdk-results" role="listbox">
          {matches.length ? (
            matches.map((hit, index) => (
              <button
                id={`cmdk-result-${index}`}
                className="cmdk-item"
                role="option"
                aria-selected={selected === index}
                key={hit.note.id}
                onMouseEnter={() => setSelected(index)}
                onClick={() => open(hit.note)}
              >
                {hit.group &&
                  (index === 0 || matches[index - 1].group !== hit.group) && (
                    <small className="cmdk-group">{hit.group}</small>
                  )}
                <strong>{displayTitle(hit.note, state.prefs.titleDate)}</strong>
                {hit.snippet && (
                  <span className="cmdk-snippet">
                    {hit.snippet
                      .split(
                        new RegExp(
                          "(" +
                            query
                              .trim()
                              .replace(/[.*+?^${}()|[\]\\]/g, "\\$&") +
                            ")",
                          "ig",
                        ),
                      )
                      .map((part, i) =>
                        part.toLowerCase() === query.trim().toLowerCase() ? (
                          <mark key={i}>{part}</mark>
                        ) : (
                          part
                        ),
                      )}
                  </span>
                )}
                <small>
                  {state.workspace!.agendas.find(
                    (a) => a.id === hit.note.agendaId,
                  )?.title || "삭제된 아젠다"}{" "}
                  · {TYPE_NAMES[asText(hit.note.type)]} ·{" "}
                  {hit.note.date.slice(5)}
                </small>
              </button>
            ))
          ) : (
            <p className="cmdk-empty">일치하는 노트가 없어요.</p>
          )}
          {searchTrim(query) && results.length > 20 && (
            <p className="cmdk-more">
              {results.length - 20}건이 더 있어요. 검색어를 좁혀 보세요
            </p>
          )}
        </div>
      </section>
    </div>
  );
}

function SubNotePages({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  const trail = state.subNotes!;
  const [failure, setFailure] = useState("");
  useEffect(() => {
    let current = true;
    const release: (() => void)[] = [];
    void (async () => {
      const root = state.workspace!.notes.find(
        (note) => note.id === trail.root,
      );
      if (!root) throw new Error("상위 노트를 찾을 수 없어요.");
      let parent = trail.root;
      for (const id of trail.ids) {
        if (!current) return;
        release.push(controller.data.retainSubNotes(parent));
        const notes = await controller.data.loadSubNotes(parent);
        if (!notes.some((note) => note.id === id))
          throw new Error("하위 노트를 찾을 수 없어요.");
        parent = id;
      }
    })().catch((error) => {
      if (current) setFailure(String(error));
    });
    return () => {
      current = false;
      release.forEach((stop) => stop());
    };
  }, [controller, trail.root, trail.ids.join("/")]);
  const parent = trail.ids.length > 1 ? trail.ids.at(-2)! : trail.root;
  const top = state.vault.subNotes[parent]?.find(
    (note) => note.id === trail.ids.at(-1),
  );
  return (
    <div
      className="subnote-overlay subnote-fullscreen"
      onClick={() => controller.openSubNotes(trail.root, [])}
    >
      <section
        className="subnote-page"
        role="dialog"
        aria-modal="true"
        aria-label="하위 노트"
        onClick={(event) => event.stopPropagation()}
      >
        <header>
          <nav>
            {trail.ids.length > 1 && (
              <button onClick={controller.popSubNote}>한 단계 뒤로</button>
            )}
            <button
              onClick={() => {
                while (controller.getSnapshot().subNotes)
                  controller.popSubNote();
              }}
            >
              {state.workspace?.notes.find((n) => n.id === trail.root)?.title ||
                trail.root}
            </button>
            {trail.ids.map((id, index) => (
              <button
                key={id}
                onClick={() =>
                  controller.openSubNotes(
                    trail.root,
                    trail.ids.slice(0, index + 1),
                  )
                }
              >
                {Object.values(state.vault.subNotes)
                  .flat()
                  .find((n) => n.id === id)?.title || id}
              </button>
            ))}
          </nav>
          <button
            aria-label="하위 페이지 닫기"
            onClick={() => controller.openSubNotes(trail.root, [])}
          >
            닫기
          </button>
        </header>
        {failure ? (
          <div className="missing-subnote">
            <p role="alert">{failure}</p>
            <button onClick={controller.popSubNote}>한 단계 뒤로</button>
          </div>
        ) : top ? (
          <NoteReading note={top} controller={controller} parent={parent} />
        ) : (
          <p role="status">하위 노트를 여는 중…</p>
        )}
      </section>
    </div>
  );
}
