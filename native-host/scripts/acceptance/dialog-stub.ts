// 점검 번들에서만 `@tauri-apps/plugin-dialog` 자리에 들어가는 대역이다(workspace-acceptance.mjs의 alias).
// 네이티브 대화 창은 이 점검이 눌러 줄 수 없고, Tauri가 `__TAURI_INTERNALS__`를 잠가 두어 호출을 가로챌 수도 없다.
// 제품 코드는 그대로 두고, 확인 창이 물은 글(`asked`)과 창 제목·단추 글자(`options`)를 쌓고 답은 점검이 `answer`로 정한다.
type ConfirmOptions = { title?: string; okLabel?: string; cancelLabel?: string } | undefined;
const state: { asked: string[]; options: ConfirmOptions[]; answer: boolean } = ((window as any).__AIS_DIALOG__ ??= { asked: [], options: [], answer: true });

export async function confirm(message: string, options?: ConfirmOptions): Promise<boolean> {
  state.asked.push(String(message));
  state.options.push(options);
  return state.answer;
}

export async function message(text: string): Promise<void> {
  state.asked.push(String(text));
  state.options.push(undefined);
}

export async function open(): Promise<null> {
  return null;
}
