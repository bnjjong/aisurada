// 프로젝트 폼의 「저장소」 옆 「왜 필요해요?」가 여는 안내 팝업.
// 저장소를 왜 적는지, 어떻게 쓰이고 어디에 보관되는지, 앱이 GitHub에서 무엇을 하는지를 지금 구현된 동작 그대로만 적는다.
// 아직 없는 기능(앱 안 개발)은 「아직 없다」고 밝힌다. 동작이 바뀌면 이 글도 같이 고친다.
import { Modal } from "./Modal.tsx";

/** 받는 폴더를 글로 보인다. 연결 상태를 아직 모르면 앱 전용 폴더라고만 한다. */
export function cloneFolderText(cloneRoot?: string): string {
  return cloneRoot ? `${cloneRoot.replace(/\/+$/, "")}/<소유자>/<이름>` : "앱 전용 폴더(~/Library/Application Support/aisurada/repos/<소유자>/<이름>)";
}

export function RepoGuideFlow() {
  // 프로젝트의 목록에서 아젠다가 하나를 고르고, 고른 저장소의 폴더로 세션과 화면이 이어진다.
  return (
    <figure className="repo-guide-figure">
      <svg
        viewBox="0 0 560 112"
        role="img"
        aria-label="프로젝트가 저장소 목록을 가지고, 아젠다가 그중 하나를 고르면, 그 폴더로 세션과 화면이 이어진다"
      >
        <defs>
          <marker id="repo-guide-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
            <path d="M0 0 L10 5 L0 10 z" fill="currentColor" />
          </marker>
        </defs>
        <g fill="none" stroke="currentColor" strokeWidth="1.4">
          <rect x="2" y="24" width="152" height="60" rx="10" />
          <rect x="204" y="24" width="152" height="60" rx="10" />
          <rect x="406" y="24" width="152" height="60" rx="10" />
          <line x1="156" y1="54" x2="202" y2="54" markerEnd="url(#repo-guide-arrow)" />
          <line x1="358" y1="54" x2="404" y2="54" markerEnd="url(#repo-guide-arrow)" />
        </g>
        <g fill="currentColor" fontSize="13" textAnchor="middle">
          <text x="78" y="50">프로젝트</text>
          <text x="78" y="68" opacity="0.7">저장소 목록</text>
          <text x="280" y="50">아젠다</text>
          <text x="280" y="68" opacity="0.7">그중 하나를 고름</text>
          <text x="482" y="50">세션 묶기 · 화면 표시</text>
          <text x="482" y="68" opacity="0.7">고른 저장소의 폴더로</text>
          <text x="179" y="44" fontSize="11" opacity="0.7">고름</text>
          <text x="381" y="44" fontSize="11" opacity="0.7">폴더로</text>
        </g>
      </svg>
      <figcaption>프로젝트가 저장소를 모아 두면, 아젠다가 그중 하나를 골라 쓰고, 고른 저장소의 폴더로 세션과 화면이 이어져요.</figcaption>
    </figure>
  );
}

export function RepoGuide({ close, cloneRoot, connected }: { close: () => void; cloneRoot?: string; connected: boolean }) {
  return (
    <Modal title="프로젝트 저장소 안내" close={close} className="repo-guide">
      <div className="repo-guide-body">
        <p>
          <strong>프로젝트의 저장소는 ‘이 프로젝트의 코드가 어디에 있는지’를 앱에 알려 주는 목록이에요.</strong> 꼭 적어야 하는 것은
          아니에요. 적지 않아도 노트와 아젠다는 그대로 쓸 수 있어요.
        </p>
        <RepoGuideFlow />

        <section>
          <h3>왜 적어 두나요?</h3>
          <ul>
            <li>아젠다가 이 목록에서 저장소 하나를 고르면, 아젠다 화면에 어느 저장소의 작업인지 보여요.</li>
            <li>터미널이나 AI(Claude Code 등) 세션의 작업 폴더가 그 저장소 폴더 안이면, 그 아젠다의 세션으로 자동으로 묶여 보여요.</li>
            <li>앞으로 이 앱 안에서 코드를 읽고 고치는 개발 기능을 붙일 때 이 목록이 기준이 돼요. 그 기능은 아직 없어요.</li>
          </ul>
        </section>

        <section>
          <h3>적는 방법은 두 가지예요</h3>
          <ul>
            <li>
              <strong>이 컴퓨터의 폴더</strong>: 이미 받아 둔 폴더를 그대로 써요. 앱은 그 폴더를 복사하거나 바꾸지 않아요.
            </li>
            <li>
              <strong>GitHub 저장소</strong>(<code>소유자/이름</code>): 앱이 대신 받아 둬요. 받은 폴더는 <code>{cloneFolderText(cloneRoot)}</code>에
              생기고, 이미 있으면 다시 받지 않아요.
            </li>
            <li>둘 다 적으면 폴더를 먼저 쓰고, 받지 않아요.</li>
          </ul>
        </section>

        <section>
          <h3>GitHub 연결은 언제 필요한가요?</h3>
          <p>
            폴더만 적을 때는 필요 없어요. GitHub 저장소를 앱이 받으려면 설정의 「GitHub 연결」에서 내가 만든 GitHub App을 한 번
            연결해야 해요. 그 App을 설치한 저장소만 보이고 받을 수 있어요.{" "}
            <strong>{connected ? "지금은 연결되어 있어요." : "지금은 연결되어 있지 않아요."}</strong>
          </p>
        </section>

        <section>
          <h3>어떻게 관리되나요?</h3>
          <ul>
            <li>
              목록은 프로젝트 파일(노트 폴더의 <code>.aisurada/projects/</code> 아래, <code>repos:</code> 한 줄)에 저장돼요. 노트와 함께
              옮겨 가요. 다만 폴더 경로는 이 컴퓨터 기준이라 다른 컴퓨터에서는 맞지 않을 수 있어요. 여러 컴퓨터에서 쓴다면 GitHub
              저장소로 적는 편이 안전해요.
            </li>
            <li>개인 키는 노트 폴더에 남기지 않고 이 Mac의 키체인에만 보관해요.</li>
            <li>지금 앱이 GitHub에서 하는 일은 저장소를 보고 받는 것(읽기)뿐이에요. 코드를 올리거나 바꾸지 않아요.</li>
            <li>목록에서 「빼기」를 해도 이미 받아 둔 폴더는 지워지지 않아요. GitHub 연결을 끊어도 같아요. 끊으면 키체인의 키와 앱 정보만 지워져요.</li>
            <li>아젠다가 고른 저장소를 프로젝트에서 빼면, 그 아젠다는 옛 「저장소」 글(있으면)로 돌아가고 화면에 안내가 떠요.</li>
          </ul>
        </section>

        <div className="repo-guide-actions">
          <button type="button" className="button-primary" onClick={close}>
            알겠어요
          </button>
        </div>
      </div>
    </Modal>
  );
}
