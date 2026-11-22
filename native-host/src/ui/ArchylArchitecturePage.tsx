import { useEffect, useState, useCallback, useMemo } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { Icon } from "./Icon.tsx";
import {
  archylSource,
  getArchylModel,
  archylStatus,
  connectArchyl,
} from "../editor/archyl.ts";
import type { ArchylModel, ArchylView } from "../editor/archyl.ts";
import { renderDiagram } from "../editor/mermaid.tsx";
import { ArchylCreateModal } from "./ArchylCreateModal.tsx";
import type { ArchylCreateMode } from "./ArchylCreateModal.tsx";

export function ArchylArchitecturePage({
  slug,
  state,
  controller,
}: {
  slug: string;
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const ws = state.workspace;
  const project = ws?.projects.find((p) => p.slug === slug);
  const projectName = (project?.title as string) || slug;
  const archylProjectId = asText(project?.archylProjectId);

  const [connected, setConnected] = useState<boolean | null>(null);
  const [model, setModel] = useState<ArchylModel | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 뷰 레벨 상태
  const [level, setLevel] = useState<ArchylView["level"]>("context");
  const [selectedSystemId, setSelectedSystemId] = useState<string>("");
  const [selectedContainerId, setSelectedContainerId] = useState<string>("");

  // 다이어그램 렌더링 결과
  const [svg, setSvg] = useState<string>("");
  const [renderError, setRenderError] = useState<string | null>(null);
  const [showSource, setShowSource] = useState(false);
  const [copySuccess, setCopySuccess] = useState(false);

  // 생성 모달 상태
  const [createModal, setCreateModal] = useState<ArchylCreateMode | null>(null);

  // 1. Archyl 연결 상태 확인
  useEffect(() => {
    void archylStatus()
      .then(setConnected)
      .catch(() => setConnected(false));
  }, []);

  // 2. 모델 로드 함수
  const loadModel = useCallback(async () => {
    if (!archylProjectId) return;
    setLoading(true);
    setError(null);
    try {
      const data = await getArchylModel(archylProjectId);
      setModel(data);
    } catch (err: any) {
      setError(err?.message || String(err));
    } finally {
      setLoading(false);
    }
  }, [archylProjectId]);

  useEffect(() => {
    if (archylProjectId) {
      void loadModel();
    }
  }, [archylProjectId, loadModel]);

  // 시스템 및 컨테이너 목록
  const systems = useMemo(
    () => model?.elements.filter((e) => e.kind === "system") ?? [],
    [model],
  );
  const containers = useMemo(
    () =>
      model?.elements.filter(
        (e) => e.kind === "container" && (!selectedSystemId || e.parentId === selectedSystemId),
      ) ?? [],
    [model, selectedSystemId],
  );

  // 상위 선택 기본값 설정
  useEffect(() => {
    if (level === "container") {
      if (!selectedSystemId && systems.length > 0) {
        setSelectedSystemId(systems[0].id);
      }
    } else if (level === "component") {
      if (!selectedContainerId && containers.length > 0) {
        setSelectedContainerId(containers[0].id);
      }
    }
  }, [level, systems, containers, selectedSystemId, selectedContainerId]);

  // 3. 현재 뷰와 다이어그램 소스 생성
  const currentView: ArchylView | null = useMemo(() => {
    if (!archylProjectId) return null;
    if (level === "context") {
      return { projectId: archylProjectId, level: "context" };
    }
    if (level === "container") {
      if (!selectedSystemId) return null;
      return { projectId: archylProjectId, level: "container", elementId: selectedSystemId };
    }
    if (level === "component") {
      if (!selectedContainerId) return null;
      return { projectId: archylProjectId, level: "component", elementId: selectedContainerId };
    }
    return null;
  }, [archylProjectId, level, selectedSystemId, selectedContainerId]);

  const mermaidSource = useMemo(() => {
    if (!model || !currentView) return "";
    try {
      return archylSource(model, currentView);
    } catch (err: any) {
      return "";
    }
  }, [model, currentView]);

  // 4. Mermaid SVG 렌더링
  useEffect(() => {
    let active = true;
    if (!mermaidSource) {
      setSvg("");
      setRenderError(null);
      return;
    }
    setRenderError(null);
    void renderDiagram(mermaidSource, document.documentElement.dataset.theme || "system")
      .then((renderedSvg) => {
        if (active) setSvg(renderedSvg);
      })
      .catch((err: any) => {
        if (active) setRenderError(err?.message || "다이어그램을 렌더링하지 못했어요.");
      });
    return () => {
      active = false;
    };
  }, [mermaidSource]);

  // 클립보드 복사
  const handleCopySource = async () => {
    if (!mermaidSource) return;
    try {
      await navigator.clipboard.writeText(`\`\`\`mermaid\n${mermaidSource}\n\`\`\``);
      setCopySuccess(true);
      setTimeout(() => setCopySuccess(false), 2000);
    } catch {
      // ignore
    }
  };

  // 새 노트로 내보내기
  const handleExportToNote = async () => {
    if (!mermaidSource || !ws) return;
    const agenda = ws.agendas.find((a) => a.slug === slug) || ws.agendas[0];
    const agendaId = agenda ? agenda.id : "";
    const noteTitle = `${projectName} C4 ${
      level === "context" ? "시스템" : level === "container" ? "컨테이너" : "컴포넌트"
    } 아키텍처`;
    const noteBody = `## ${noteTitle}\n\n\`\`\`mermaid\n${mermaidSource}\n\`\`\`\n`;

    try {
      const res: any = await controller.callEngine({
        command: "create_doc",
        args: {
          agendaId,
          title: noteTitle,
          body: noteBody,
        },
      });
      if (res?.value?.id) {
        controller.navigate(`/doc/${encodeURIComponent(res.value.id)}`);
      }
    } catch (err) {
      // 오류 시 클립보드 복사로 대체
      void handleCopySource();
    }
  };

  if (connected === false) {
    return (
      <div className="archyl-page-container">
        <header className="archyl-page-header">
          <h1>{projectName} · C4 아키텍처</h1>
        </header>
        <div className="archyl-empty-card">
          <p>Archyl 서비스와 연결되어 있지 않아요.</p>
          <button
            type="button"
            className="action-btn primary"
            onClick={() => void connectArchyl().then(() => void archylStatus().then(setConnected))}
          >
            Archyl 연결하기
          </button>
        </div>
      </div>
    );
  }

  if (!archylProjectId) {
    return (
      <div className="archyl-page-container">
        <header className="archyl-page-header">
          <h1>{projectName} · C4 아키텍처</h1>
        </header>
        <div className="archyl-empty-card">
          <p>이 프로젝트에 연결된 Archyl 프로젝트가 아직 없어요.</p>
          <p className="archyl-hint">
            프로젝트 설정에서 Archyl 프로젝트를 선택해 연결해 주세요.
          </p>
          {project && (
            <button
              type="button"
              className="action-btn"
              onClick={() => controller.form({ kind: "project", id: project.id as string })}
            >
              프로젝트 설정 열기
            </button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="archyl-page-container">
      {/* 상단 헤더 & 컨트롤 바 */}
      <header className="archyl-page-header">
        <div className="archyl-header-title">
          <span className="archyl-icon-badge" aria-hidden="true">
            <Icon name="architecture" size={18} />
          </span>
          <h2>{projectName} C4 아키텍처</h2>
        </div>

        {/* 툴바 단추들 */}
        <div className="archyl-header-actions">
          <button
            type="button"
            className="action-btn"
            title="다이어그램 새로고침"
            onClick={() => void loadModel()}
            disabled={loading}
          >
            <Icon name="history" size={14} />
            {loading ? "불러오는 중…" : "새로고침"}
          </button>
          <button
            type="button"
            className="action-btn"
            title="새 요소 추가"
            onClick={() => setCreateModal("element")}
            disabled={loading || !model}
          >
            <Icon name="plus" size={14} />
            요소 추가
          </button>
          <button
            type="button"
            className="action-btn"
            title="요소 간 관계 연결"
            onClick={() => setCreateModal("relationship")}
            disabled={loading || !model || (model?.elements.length ?? 0) < 2}
          >
            관계 연결
          </button>
          <button
            type="button"
            className="action-btn"
            title="클립보드에 Mermaid 복사"
            onClick={() => void handleCopySource()}
            disabled={!mermaidSource}
          >
            {copySuccess ? "복사됨!" : "코드 복사"}
          </button>
          <button
            type="button"
            className="action-btn primary"
            title="새 노트에 다이어그램 삽입"
            onClick={() => void handleExportToNote()}
            disabled={!mermaidSource}
          >
            노트에 삽입
          </button>
        </div>
      </header>

      {/* 레벨 탭 및 상위 선택 바 */}
      <div className="archyl-view-controls">
        <div className="archyl-level-tabs">
          <button
            type="button"
            className={`level-tab ${level === "context" ? "active" : ""}`}
            onClick={() => setLevel("context")}
          >
            1. 시스템 컨텍스트 (System)
          </button>
          <button
            type="button"
            className={`level-tab ${level === "container" ? "active" : ""}`}
            onClick={() => setLevel("container")}
          >
            2. 컨테이너 (Container)
          </button>
          <button
            type="button"
            className={`level-tab ${level === "component" ? "active" : ""}`}
            onClick={() => setLevel("component")}
          >
            3. 컴포넌트 (Component)
          </button>
        </div>

        <div className="archyl-level-selectors">
          {level === "container" && (
            <label className="archyl-select-label">
              대상 시스템:
              <select
                value={selectedSystemId}
                onChange={(e) => setSelectedSystemId(e.target.value)}
              >
                {systems.length === 0 && <option value="">등록된 시스템이 없어요</option>}
                {systems.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name}
                  </option>
                ))}
              </select>
            </label>
          )}

          {level === "component" && (
            <label className="archyl-select-label">
              대상 컨테이너:
              <select
                value={selectedContainerId}
                onChange={(e) => setSelectedContainerId(e.target.value)}
              >
                {containers.length === 0 && <option value="">등록된 컨테이너가 없어요</option>}
                {containers.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
          )}

          <button
            type="button"
            className="source-toggle-btn"
            onClick={() => setShowSource(!showSource)}
          >
            {showSource ? "다이어그램 보기" : "Mermaid 소스 보기"}
          </button>
        </div>
      </div>

      {/* 본문 뷰 영역 */}
      <main className="archyl-viewport">
        {loading && (
          <div className="archyl-status-card">
            <p>Archyl에서 C4 모델을 불러오는 중이에요…</p>
          </div>
        )}

        {error && (
          <div className="archyl-status-card error">
            <p role="alert">{error}</p>
            <button type="button" className="action-btn" onClick={() => void loadModel()}>
              다시 시도
            </button>
          </div>
        )}

        {!loading && !error && model && model.elements.length === 0 && (
          <div className="archyl-empty-card">
            <p>등록된 아키텍처 요소가 아직 없어요.</p>
            <button
              type="button"
              className="action-btn primary"
              onClick={() => setCreateModal("element")}
            >
              + 첫 번째 시스템 등록하기
            </button>
          </div>
        )}

        {!loading && !error && model && model.elements.length > 0 && (
          <>
            {renderError && (
              <div className="archyl-status-card error">
                <p role="alert">다이어그램 생성 오류: {renderError}</p>
              </div>
            )}

            {showSource ? (
              <div className="archyl-source-view">
                <pre>
                  <code>{mermaidSource || "표시할 내용이 없어요."}</code>
                </pre>
              </div>
            ) : (
              <div
                className="archyl-diagram-wrapper"
                dangerouslySetInnerHTML={{ __html: svg }}
              />
            )}
          </>
        )}
      </main>

      {/* 요소/관계 생성 모달 */}
      {createModal && model && (
        <ArchylCreateModal
          projectId={archylProjectId}
          model={model}
          initialMode={createModal}
          initialParentId={
            level === "container"
              ? selectedSystemId
              : level === "component"
                ? selectedContainerId
                : undefined
          }
          close={() => setCreateModal(null)}
          onCreated={() => {
            void loadModel();
          }}
        />
      )}
    </div>
  );
}
