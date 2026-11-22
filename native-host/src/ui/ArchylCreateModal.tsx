import { useState } from "react";
import { Modal } from "./Modal.tsx";
import {
  createArchylComponent,
  createArchylContainer,
  createArchylRelationship,
  createArchylSystem,
} from "../editor/archyl.ts";
import type { ArchylModel } from "../editor/archyl.ts";

export type ArchylCreateMode = "element" | "relationship";

export function ArchylCreateModal({
  projectId,
  model,
  initialMode = "element",
  initialParentId,
  close,
  onCreated,
}: {
  projectId: string;
  model: ArchylModel;
  initialMode?: ArchylCreateMode;
  initialParentId?: string;
  close(): void;
  onCreated(): void;
}) {
  const [mode, setMode] = useState<ArchylCreateMode>(initialMode);
  const [kind, setKind] = useState<"system" | "container" | "component">("system");

  // System 폼 필드
  const [sysName, setSysName] = useState("");
  const [sysType, setSysType] = useState<"software_system" | "person" | "external_system">("software_system");
  const [sysExternal, setSysExternal] = useState(false);
  const [sysDesc, setSysDesc] = useState("");

  // Container 폼 필드
  const systems = model.elements.filter((e) => e.kind === "system");
  const [contSystemId, setContSystemId] = useState(
    initialParentId && systems.some((s) => s.id === initialParentId)
      ? initialParentId
      : systems[0]?.id || "",
  );
  const [contName, setContName] = useState("");
  const [contType, setContType] = useState("service");
  const [contDesc, setContDesc] = useState("");

  // Component 폼 필드
  const containers = model.elements.filter((e) => e.kind === "container");
  const [compContainerId, setCompContainerId] = useState(
    initialParentId && containers.some((c) => c.id === initialParentId)
      ? initialParentId
      : containers[0]?.id || "",
  );
  const [compName, setCompName] = useState("");
  const [compType, setCompType] = useState("service");
  const [compDesc, setCompDesc] = useState("");

  // Relationship 폼 필드
  const allElements = model.elements;
  const [relSourceId, setRelSourceId] = useState(allElements[0]?.id || "");
  const [relTargetId, setRelTargetId] = useState(allElements[1]?.id || allElements[0]?.id || "");
  const [relType, setRelType] = useState("uses");
  const [relDesc, setRelDesc] = useState("");

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function handleSubmit() {
    setBusy(true);
    setError("");
    try {
      if (mode === "element") {
        if (kind === "system") {
          if (!sysName.trim()) throw Error("시스템 이름을 입력해 주세요.");
          await createArchylSystem({
            projectId,
            name: sysName.trim(),
            type: sysType,
            isExternal: sysExternal || sysType === "external_system",
            description: sysDesc.trim() || undefined,
          });
        } else if (kind === "container") {
          if (!contSystemId) throw Error("상위 시스템을 선택해 주세요.");
          if (!contName.trim()) throw Error("컨테이너 이름을 입력해 주세요.");
          await createArchylContainer({
            systemId: contSystemId,
            name: contName.trim(),
            containerType: contType,
            description: contDesc.trim() || undefined,
          });
        } else {
          if (!compContainerId) throw Error("상위 컨테이너를 선택해 주세요.");
          if (!compName.trim()) throw Error("컴포넌트 이름을 입력해 주세요.");
          await createArchylComponent({
            containerId: compContainerId,
            name: compName.trim(),
            componentType: compType,
            description: compDesc.trim() || undefined,
          });
        }
      } else {
        if (!relSourceId || !relTargetId) throw Error("출발 요소와 대상 요소를 선택해 주세요.");
        if (relSourceId === relTargetId) throw Error("출발 요소와 대상 요소는 달라야 해요.");
        const sourceElem = allElements.find((e) => e.id === relSourceId);
        const targetElem = allElements.find((e) => e.id === relTargetId);
        if (!sourceElem || !targetElem) throw Error("선택한 요소를 찾을 수 없어요.");

        const typeMap = { system: 1, container: 2, component: 3 };
        await createArchylRelationship({
          projectId,
          sourceId: relSourceId,
          sourceType: typeMap[sourceElem.kind],
          targetId: relTargetId,
          targetType: typeMap[targetElem.kind],
          type: relType.trim() || "uses",
          description: relDesc.trim() || undefined,
        });
      }
      onCreated();
      close();
    } catch (err: any) {
      setError(err?.message || String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      title={mode === "element" ? "C4 요소 추가" : "C4 관계 연결"}
      close={close}
      className="ndf archyl-create-modal"
    >
      <div className="archyl-modal-tabs">
        <button
          type="button"
          className={`tab-btn ${mode === "element" ? "active" : ""}`}
          onClick={() => {
            setMode("element");
            setError("");
          }}
        >
          요소 추가
        </button>
        <button
          type="button"
          className={`tab-btn ${mode === "relationship" ? "active" : ""}`}
          onClick={() => {
            setMode("relationship");
            setError("");
          }}
        >
          관계 연결
        </button>
      </div>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          void handleSubmit();
        }}
        style={{ marginTop: "12px" }}
      >
        {mode === "element" ? (
          <>
            <label>
              요소 수준
              <select
                value={kind}
                onChange={(e) => {
                  setKind(e.target.value as any);
                  setError("");
                }}
              >
                <option value="system">시스템 (System Context)</option>
                <option value="container">컨테이너 (Container)</option>
                <option value="component">컴포넌트 (Component)</option>
              </select>
            </label>

            {kind === "system" && (
              <>
                <label>
                  이름 *
                  <input
                    type="text"
                    value={sysName}
                    onChange={(e) => setSysName(e.target.value)}
                    placeholder="예: 주문 서비스, 고객, 결제 게이트웨이"
                    autoFocus
                  />
                </label>
                <label>
                  유형
                  <select
                    value={sysType}
                    onChange={(e) => setSysType(e.target.value as any)}
                  >
                    <option value="software_system">소프트웨어 시스템</option>
                    <option value="person">사용자 / 사람 (Person)</option>
                    <option value="external_system">외부 시스템 (External System)</option>
                  </select>
                </label>
                {sysType !== "person" && (
                  <label style={{ display: "flex", alignItems: "center", gap: "8px", margin: "8px 0" }}>
                    <input
                      type="checkbox"
                      checked={sysExternal || sysType === "external_system"}
                      disabled={sysType === "external_system"}
                      onChange={(e) => setSysExternal(e.target.checked)}
                    />
                    외부 시스템으로 표시
                  </label>
                )}
                <label>
                  설명
                  <textarea
                    value={sysDesc}
                    onChange={(e) => setSysDesc(e.target.value)}
                    placeholder="시스템의 역할과 책임을 간략히 적어주세요."
                    rows={2}
                  />
                </label>
              </>
            )}

            {kind === "container" && (
              <>
                <label>
                  상위 시스템 *
                  <select
                    value={contSystemId}
                    onChange={(e) => setContSystemId(e.target.value)}
                  >
                    {systems.length === 0 && <option value="">등록된 시스템이 없어요</option>}
                    {systems.map((s) => (
                      <option key={s.id} value={s.id}>
                        {s.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  이름 *
                  <input
                    type="text"
                    value={contName}
                    onChange={(e) => setContName(e.target.value)}
                    placeholder="예: Web App, API Server, MySQL Database"
                    autoFocus
                  />
                </label>
                <label>
                  컨테이너 종류
                  <select
                    value={contType}
                    onChange={(e) => setContType(e.target.value)}
                  >
                    <option value="api">API / Backend</option>
                    <option value="web_app">웹 애플리케이션 (Web App)</option>
                    <option value="mobile_app">모바일 앱 (Mobile App)</option>
                    <option value="database">데이터베이스 (Database)</option>
                    <option value="cache">캐시 (Cache / Redis)</option>
                    <option value="message_queue">메시지 큐 (Kafka, SQS)</option>
                    <option value="service">마이크로서비스 (Service)</option>
                    <option value="function">서버리스 함수 (Function)</option>
                    <option value="other">기타 (Other)</option>
                  </select>
                </label>
                <label>
                  설명
                  <textarea
                    value={contDesc}
                    onChange={(e) => setContDesc(e.target.value)}
                    placeholder="컨테이너의 기술 스택 및 책임을 적어주세요."
                    rows={2}
                  />
                </label>
              </>
            )}

            {kind === "component" && (
              <>
                <label>
                  상위 컨테이너 *
                  <select
                    value={compContainerId}
                    onChange={(e) => setCompContainerId(e.target.value)}
                  >
                    {containers.length === 0 && <option value="">등록된 컨테이너가 없어요</option>}
                    {containers.map((c) => (
                      <option key={c.id} value={c.id}>
                        {c.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  이름 *
                  <input
                    type="text"
                    value={compName}
                    onChange={(e) => setCompName(e.target.value)}
                    placeholder="예: OrderController, PaymentService, UserRepository"
                    autoFocus
                  />
                </label>
                <label>
                  컴포넌트 종류
                  <select
                    value={compType}
                    onChange={(e) => setCompType(e.target.value)}
                  >
                    <option value="controller">컨트롤러 (Controller)</option>
                    <option value="service">서비스 (Service)</option>
                    <option value="repository">저장소 (Repository / DAO)</option>
                    <option value="middleware">미들웨어 (Middleware)</option>
                    <option value="handler">핸들러 (Handler)</option>
                    <option value="factory">팩토리 (Factory)</option>
                    <option value="utility">유틸리티 (Utility)</option>
                    <option value="model">도메인 모델 (Model)</option>
                    <option value="other">기타 (Other)</option>
                  </select>
                </label>
                <label>
                  설명
                  <textarea
                    value={compDesc}
                    onChange={(e) => setCompDesc(e.target.value)}
                    placeholder="컴포넌트의 상세 역할을 적어주세요."
                    rows={2}
                  />
                </label>
              </>
            )}
          </>
        ) : (
          <>
            <label>
              출발 요소 (Source) *
              <select
                value={relSourceId}
                onChange={(e) => setRelSourceId(e.target.value)}
              >
                {allElements.map((e) => (
                  <option key={e.id} value={e.id}>
                    [{e.kind}] {e.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              대상 요소 (Target) *
              <select
                value={relTargetId}
                onChange={(e) => setRelTargetId(e.target.value)}
              >
                {allElements.map((e) => (
                  <option key={e.id} value={e.id}>
                    [{e.kind}] {e.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              관계 유형
              <select
                value={relType}
                onChange={(e) => setRelType(e.target.value)}
              >
                <option value="uses">사용함 (uses)</option>
                <option value="calls">호출함 (calls)</option>
                <option value="depends_on">의존함 (depends_on)</option>
                <option value="reads_from">읽음 (reads_from)</option>
                <option value="writes_to">저장/기록함 (writes_to)</option>
                <option value="sends_events_to">이벤트 발행 (sends_events_to)</option>
              </select>
            </label>
            <label>
              설명 및 기술 프로토콜
              <input
                type="text"
                value={relDesc}
                onChange={(e) => setRelDesc(e.target.value)}
                placeholder="예: HTTPS / REST, gRPC, JDBC"
              />
            </label>
          </>
        )}

        {error && <p role="alert" className="form-error" style={{ color: "#d9534f", margin: "8px 0" }}>{error}</p>}

        <footer className="ndf-foot" style={{ marginTop: "16px", display: "flex", justifyContent: "flex-end", gap: "8px" }}>
          <button type="button" onClick={close} disabled={busy}>
            취소
          </button>
          <button type="submit" disabled={busy}>
            {busy ? "등록 중…" : mode === "element" ? "요소 등록" : "관계 연결"}
          </button>
        </footer>
      </form>
    </Modal>
  );
}
