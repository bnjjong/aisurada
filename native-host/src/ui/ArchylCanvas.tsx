import { useRef, useState, useEffect, useCallback, useMemo } from "react";
import type { ArchylElement, ArchylModel } from "../editor/archyl.ts";
import { updateArchylPositions } from "../editor/archyl.ts";

interface Position {
  x: number;
  y: number;
}

interface DraggingElement {
  id: string;
  startX: number;
  startY: number;
  origX: number;
  origY: number;
}

interface PanState {
  startX: number;
  startY: number;
  origPanX: number;
  origPanY: number;
}

const CARD_WIDTH = 240;
const CARD_HEIGHT = 130;
const PERSON_WIDTH = 220;
const PERSON_HEIGHT = 100;

export function ArchylCanvas({
  projectId,
  model,
  level = "context",
  onSelectElement,
  onElementUpdated,
}: {
  projectId: string;
  model: ArchylModel;
  level?: "context" | "container" | "component";
  onSelectElement?(element: ArchylElement): void;
  onElementUpdated?(): void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);

  // 1. 캔버스 뷰포트 상태 (줌 & 팬)
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState<Position>({ x: 50, y: 50 });
  const [panning, setPanning] = useState<PanState | null>(null);

  // 2. 요소별 로컬 위치 상태 (드래그 반영)
  const [positions, setPositions] = useState<Record<string, Position>>({});
  const [dragging, setDragging] = useState<DraggingElement | null>(null);

  // 현재 레벨에 표시할 요소 목록 필터링
  const visibleElements = useMemo(() => {
    if (level === "context") {
      return model.elements.filter((e) => e.kind === "system");
    }
    return model.elements.filter((e) => e.kind === level);
  }, [model, level]);

  const visibleIds = useMemo(
    () => new Set(visibleElements.map((e) => e.id)),
    [visibleElements],
  );

  // 관계 목록 필터링
  const visibleRelationships = useMemo(() => {
    return model.relationships.filter(
      (r) => visibleIds.has(r.source) && visibleIds.has(r.target),
    );
  }, [model, visibleIds]);

  // 모델이 바뀌었을 때 초기 위치 세팅
  useEffect(() => {
    const next: Record<string, Position> = {};
    visibleElements.forEach((e, idx) => {
      if (e.position && typeof e.position.x === "number" && typeof e.position.y === "number") {
        next[e.id] = { x: e.position.x, y: e.position.y };
      } else {
        // 기본 그리드 배치
        const col = idx % 3;
        const row = Math.floor(idx / 3);
        next[e.id] = { x: 100 + col * 320, y: 100 + row * 220 };
      }
    });
    setPositions(next);
  }, [visibleElements]);

  // 마우스 휠 줌
  const handleWheel = (e: React.WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      const delta = e.deltaY > 0 ? -0.1 : 0.1;
      setZoom((z) => Math.min(Math.max(z + delta, 0.4), 2.2));
    } else {
      // 일반 스크롤로 팬 이동
      setPan((p) => ({
        x: p.x - e.deltaX * 0.8,
        y: p.y - e.deltaY * 0.8,
      }));
    }
  };

  // 캔버스 드래그 (팬)
  const handleMouseDownCanvas = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest(".archyl-node")) return;
    setPanning({
      startX: e.clientX,
      startY: e.clientY,
      origPanX: pan.x,
      origPanY: pan.y,
    });
  };

  // 노드 드래그 시작
  const handleMouseDownNode = (e: React.MouseEvent, id: string) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    const cur = positions[id] || { x: 0, y: 0 };
    setDragging({
      id,
      startX: e.clientX,
      startY: e.clientY,
      origX: cur.x,
      origY: cur.y,
    });
  };

  // 마우스 이동
  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (dragging) {
        const dx = (e.clientX - dragging.startX) / zoom;
        const dy = (e.clientY - dragging.startY) / zoom;
        setPositions((prev) => ({
          ...prev,
          [dragging.id]: {
            x: Math.round(dragging.origX + dx),
            y: Math.round(dragging.origY + dy),
          },
        }));
      } else if (panning) {
        setPan({
          x: panning.origPanX + (e.clientX - panning.startX),
          y: panning.origPanY + (e.clientY - panning.startY),
        });
      }
    },
    [dragging, panning, zoom],
  );

  // 마우스 업 (드래그 종료 후 위치 클라우드 저장)
  const handleMouseUp = useCallback(() => {
    if (dragging) {
      const finalPos = positions[dragging.id];
      const elem = visibleElements.find((e) => e.id === dragging.id);
      if (finalPos && elem && projectId) {
        void updateArchylPositions({
          projectId,
          positions: [
            {
              id: dragging.id,
              type: elem.kind,
              x: finalPos.x,
              y: finalPos.y,
            },
          ],
        }).then(() => {
          onElementUpdated?.();
        });
      }
      setDragging(null);
    }
    if (panning) {
      setPanning(null);
    }
  }, [dragging, panning, positions, visibleElements, projectId, onElementUpdated]);

  // 두 노드 간 곡선 계산
  const renderRelationship = (rel: typeof model.relationships[0]) => {
    const sElem = visibleElements.find((e) => e.id === rel.source);
    const tElem = visibleElements.find((e) => e.id === rel.target);
    const sPos = positions[rel.source];
    const tPos = positions[rel.target];
    if (!sPos || !tPos || !sElem || !tElem) return null;

    const sW = sElem.type === "person" ? PERSON_WIDTH : CARD_WIDTH;
    const sH = sElem.type === "person" ? PERSON_HEIGHT : CARD_HEIGHT;
    const tW = tElem.type === "person" ? PERSON_WIDTH : CARD_WIDTH;
    const tH = tElem.type === "person" ? PERSON_HEIGHT : CARD_HEIGHT;

    const sCenter = { x: sPos.x + sW / 2, y: sPos.y + sH / 2 };
    const tCenter = { x: tPos.x + tW / 2, y: tPos.y + tH / 2 };

    // 베지어 곡선 제어점 계산
    const dx = tCenter.x - sCenter.x;
    const midX = (sCenter.x + tCenter.x) / 2;
    const midY = (sCenter.y + tCenter.y) / 2;

    // 약간의 곡률 부여
    const curveOffset = Math.min(Math.max(Math.abs(dx) * 0.2, 30), 80);
    const c1x = sCenter.x + (dx > 0 ? curveOffset : -curveOffset);
    const c1y = sCenter.y;
    const c2x = tCenter.x - (dx > 0 ? curveOffset : -curveOffset);
    const c2y = tCenter.y;

    const pathD = `M ${sCenter.x} ${sCenter.y} C ${c1x} ${c1y}, ${c2x} ${c2y}, ${tCenter.x} ${tCenter.y}`;

    return (
      <g key={rel.source + "-" + rel.target} className="archyl-rel-group">
        {/* 화살표 곡선 */}
        <path
          d={pathD}
          fill="none"
          stroke="#94a3b8"
          strokeWidth="2"
          strokeDasharray="5,5"
          markerEnd="url(#archyl-arrowhead)"
        />
        {/* 중앙 라벨 Pill */}
        {rel.label && (
          <foreignObject
            x={midX - 110}
            y={midY - 18}
            width={220}
            height={36}
            style={{ overflow: "visible" }}
          >
            <div className="archyl-rel-pill" title={rel.label}>
              <span>{rel.label}</span>
            </div>
          </foreignObject>
        )}
      </g>
    );
  };

  return (
    <div
      ref={containerRef}
      className="archyl-canvas-viewport"
      onWheel={handleWheel}
      onMouseDown={handleMouseDownCanvas}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
    >
      {/* 줌/팬이 적용되는 메인 무대 */}
      <div
        className="archyl-canvas-stage"
        style={{
          transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
          transformOrigin: "0 0",
        }}
      >
        {/* 관계 곡선 SVG 레이어 */}
        <svg className="archyl-canvas-svg">
          <defs>
            <marker
              id="archyl-arrowhead"
              markerWidth="10"
              markerHeight="7"
              refX="9"
              refY="3.5"
              orient="auto"
            >
              <polygon points="0 0, 10 3.5, 0 7" fill="#64748b" />
            </marker>
          </defs>
          {visibleRelationships.map(renderRelationship)}
        </svg>

        {/* C4 노드 카드들 */}
        {visibleElements.map((elem) => {
          const pos = positions[elem.id] || { x: 0, y: 0 };
          const isPerson = elem.type === "person";
          const isMainApp = elem.name === "aisurada" || elem.id.includes("aisurada");
          const isExternal = elem.isExternal || elem.type === "external_system";

          if (isPerson) {
            return (
              <div
                key={elem.id}
                className="archyl-node archyl-node-person"
                style={{
                  left: `${pos.x}px`,
                  top: `${pos.y}px`,
                  width: `${PERSON_WIDTH}px`,
                }}
                onMouseDown={(e) => handleMouseDownNode(e, elem.id)}
                onClick={() => onSelectElement?.(elem)}
              >
                <div className="person-avatar-circle">
                  <svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z" />
                  </svg>
                </div>
                <div className="person-card-body">
                  <div className="node-title">{elem.name}</div>
                  {elem.description && (
                    <div className="node-desc" title={elem.description}>
                      {elem.description}
                    </div>
                  )}
                </div>
              </div>
            );
          }

          return (
            <div
              key={elem.id}
              className={`archyl-node archyl-node-card ${
                isMainApp ? "is-main-system" : isExternal ? "is-external" : ""
              }`}
              style={{
                left: `${pos.x}px`,
                top: `${pos.y}px`,
                width: `${CARD_WIDTH}px`,
              }}
              onMouseDown={(e) => handleMouseDownNode(e, elem.id)}
              onClick={() => onSelectElement?.(elem)}
            >
              {/* 상단 뱃지 캡슐 */}
              <div className="node-badge-wrapper">
                {isExternal ? (
                  <span className="node-badge external">
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <circle cx="12" cy="12" r="10" />
                      <line x1="2" y1="12" x2="22" y2="12" />
                      <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
                    </svg>
                    EXTERNAL
                  </span>
                ) : (
                  <span className="node-badge system">
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <rect x="3" y="3" width="18" height="18" rx="2" />
                      <line x1="3" y1="9" x2="21" y2="9" />
                      <line x1="9" y1="21" x2="9" y2="9" />
                    </svg>
                    SYSTEM
                  </span>
                )}
              </div>

              {/* 본문 제목 & 설명 */}
              <div className="node-title">{elem.name}</div>
              {elem.description && (
                <div className="node-desc" title={elem.description}>
                  {elem.description}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* 캔버스 하단 좌측 줌 컨트롤 */}
      <div className="archyl-zoom-controls">
        <button
          type="button"
          title="확대"
          onClick={() => setZoom((z) => Math.min(z + 0.15, 2.2))}
        >
          +
        </button>
        <span className="zoom-level">{Math.round(zoom * 100)}%</span>
        <button
          type="button"
          title="축소"
          onClick={() => setZoom((z) => Math.max(z - 0.15, 0.4))}
        >
          -
        </button>
        <button
          type="button"
          title="화면 리셋"
          onClick={() => {
            setZoom(1);
            setPan({ x: 50, y: 50 });
          }}
        >
          리셋
        </button>
      </div>
    </div>
  );
}
