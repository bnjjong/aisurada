// 승인을 기다리는 변경이 있다는 안내 조각들. 글로벌 리소스 화면의 배너·카드와 에이전트·스킬 편집창이 같이 쓴다.
// 문구는 스킬·에이전트 페이지(ResourcesPage)의 배너와 맞춘다.

/** 화면 맨 위 배너. 승인을 기다리는 변경이 없으면 아무것도 그리지 않는다. */
export function ApprovalBanner({ count, onOpen }: { count: number; onOpen: () => void }) {
  if (count <= 0) return null;
  return (
    <div className="apv-banner" role="status">
      <span>
        <strong>승인이 필요한 변경이 {count}건 있어요.</strong> AI가 고친 스킬·에이전트는 사람이 승인해야 적용돼요. 그때까지는 이전 판이 쓰여요.
      </span>
      <button type="button" className="button-primary" onClick={onOpen}>
        승인 대기함 열기
      </button>
    </div>
  );
}

/** 카드 이름 옆의 「직접 고쳐짐」 표시. */
export function PendingChip() {
  return (
    <span className="apv-chip apv-fixed" title="승인한 뒤에 파일이 직접 고쳐졌어요. 승인하기 전까지는 승인해 둔 판이 쓰여요.">
      직접 고쳐짐
    </span>
  );
}

/** 카드 안의 안내. 왜 옛 내용이 보이는지 알리고 승인 대기함으로 이어 준다. */
export function PendingNote({ onOpen }: { onOpen: () => void }) {
  return (
    <p className="res-desc apv-pending-note">
      ⚠ 승인을 기다리는 변경이 있어요. 지금 보이는 것은 승인해 둔 옛 판이에요.{" "}
      <button
        type="button"
        className="apv-link"
        onClick={(e) => {
          e.stopPropagation();
          onOpen();
        }}
      >
        승인 대기함에서 보기 ›
      </button>
    </p>
  );
}

/** 편집창 맨 위의 안내. 이 창의 값이 승인해 둔 옛 판이라 저장하면 대기 중인 변경을 덮어쓴다는 것을 알린다. */
export function EditPendingNotice({ noun, field, onOpen }: { noun: "에이전트" | "스킬"; field: "지침" | "본문"; onOpen: () => void }) {
  return (
    <div className="apv-banner" role="status">
      <span>
        <strong>이 {noun}에는 승인을 기다리는 변경이 있어요.</strong> 지금 이 창에 보이는 {field}은 승인해 둔 옛 판이에요. 이 상태로 저장하면 대기 중인 새 {field}을 덮어써요.
      </span>
      <button type="button" onClick={onOpen}>
        승인 대기함에서 보기
      </button>
    </div>
  );
}
