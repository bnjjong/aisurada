import { $addUpdateTag, $getNodeByKey, $getRoot, $isParagraphNode, HISTORY_MERGE_TAG } from "lexical";
import type { LexicalEditor, LexicalNode } from "lexical";
import { ListItemNode, $isListItemNode, $isListNode } from "@lexical/list";
import type { ListNode } from "@lexical/list";
import { $isCallout } from "./nodes.tsx";
import type { CalloutNode } from "./nodes.tsx";

/**
 * 결정 블록(`> [!decision]`)과 「답 다 했음」 블록(`> [!decided]`)의 전용 모양입니다.
 * AI가 노트에 쓴 「정할 것」에 사람이 편집기에서 답하기 쉽게 하려는 것으로, 형식은 엔진의 `note::decision`과 같습니다.
 * 그리는 DOM에 표시(답 상태, 추천, 빈 답 줄)만 붙이고 문서 글은 바꾸지 않습니다. 하나만 고르기(체크를 켜면 같은 블록의 다른
 * 체크가 풀림)만 문서를 고치며, 노트를 열 때(`ais:import`)와 되돌리기에서는 고치지 않습니다.
 */
export const DECISION_KIND = "decision";
export const DECISION_DONE_KIND = "decided";
/** 하나만 고르기가 낸 고침에 붙는 표시. 그 고침이 다시 이 규칙을 부르지 않게 합니다. */
const SINGLE_CHOICE_TAG = "ais:decision-choice";
const ANSWER = /^\s*답\s*[:：]\s*/;
const RECOMMENDED = /^\s*\S+?\s*[(（]추천[)）]/;

const kindOf = (callout: CalloutNode) => callout.getKind().toLowerCase();

/** [node]를 감싼 가장 가까운 콜아웃 */
function $calloutOf(node: LexicalNode): CalloutNode | null {
  for (const parent of node.getParents()) if ($isCallout(parent)) return parent;
  return null;
}

/** 콜아웃 바로 아래 목록들의 항목(중첩 목록을 담은 그릇 항목은 뺍니다) */
function $optionsOf(callout: CalloutNode): ListItemNode[] {
  return callout
    .getChildren()
    .filter($isListNode)
    .flatMap((list: ListNode) => list.getChildren().filter($isListItemNode))
    .filter((item) => !item.getChildren().some($isListNode));
}

/** 결정 블록에 답이 있는지: 체크가 하나라도 있거나 `답:` 줄에 글이 있으면 답한 것입니다. 엔진의 `answered`와 같은 규칙입니다. */
export function isAnswered(checked: boolean, answerLines: readonly string[]): boolean {
  return checked || answerLines.some((line) => line.replace(ANSWER, "").trim() !== "");
}

/** 선택지 글이 추천안(`가(추천): …`)인가 */
export const isRecommended = (text: string) => RECOMMENDED.test(text);

/** [editor]에 결정 블록의 하나만 고르기와 모양 표시를 붙이고, 떼는 함수를 돌려줍니다. */
export function registerDecisionBlocks(editor: LexicalEditor): () => void {
  const removeChoice = editor.registerMutationListener(ListItemNode, (mutations, { updateTags, prevEditorState }) => {
    if (updateTags.has("ais:import") || updateTags.has("historic") || updateTags.has(SINGLE_CHOICE_TAG)) return;
    const updated = [...mutations].filter(([, kind]) => kind === "updated").map(([key]) => key);
    if (!updated.length) return;
    const wasChecked = new Set(
      prevEditorState.read(() =>
        updated.filter((key) => {
          const node = $getNodeByKey(key);
          return $isListItemNode(node) && node.getChecked() === true;
        }),
      ),
    );
    const turnedOn = editor.getEditorState().read(() =>
      updated.filter((key) => {
        if (wasChecked.has(key)) return false;
        const node = $getNodeByKey(key);
        if (!$isListItemNode(node) || node.getChecked() !== true) return false;
        const callout = $calloutOf(node);
        return callout !== null && kindOf(callout) === DECISION_KIND;
      }),
    );
    if (!turnedOn.length) return;
    editor.update(
      () => {
        $addUpdateTag(SINGLE_CHOICE_TAG);
        for (const key of turnedOn) {
          const node = $getNodeByKey(key);
          const list = node?.getParent();
          if (!$isListItemNode(node) || !$isListNode(list)) continue;
          for (const sibling of list.getChildren())
            if ($isListItemNode(sibling) && sibling.getKey() !== key && sibling.getChecked()) sibling.setChecked(false);
        }
      },
      { tag: HISTORY_MERGE_TAG },
    );
  });

  const removeLook = editor.registerUpdateListener(({ editorState }) => {
    editorState.read(() => {
      for (const callout of $getRoot().getChildren().filter($isCallout)) {
        const kind = kindOf(callout);
        if (kind !== DECISION_KIND && kind !== DECISION_DONE_KIND) continue;
        const element = editor.getElementByKey(callout.getKey());
        if (!element) continue;
        const options = $optionsOf(callout);
        const checked = options.some((item) => item.getChecked() === true);
        if (kind === DECISION_DONE_KIND) {
          element.dataset.decisionState = checked ? "sent" : "ready";
          element.dataset.decisionLabel = checked ? "체크함 · 저장되면 AI가 이어 가요" : "체크하면 AI가 이어 가요";
          continue;
        }
        const answers = callout.getChildren().filter((child) => $isParagraphNode(child) && ANSWER.test(child.getTextContent()));
        const answered = isAnswered(checked, answers.map((paragraph) => paragraph.getTextContent()));
        element.dataset.decisionState = answered ? "answered" : "waiting";
        element.dataset.decisionLabel = answered ? "답함" : "답 기다림";
        for (const item of options)
          editor.getElementByKey(item.getKey())?.classList.toggle("lex-decision-recommended", isRecommended(item.getTextContent()));
        for (const paragraph of answers) {
          const line = editor.getElementByKey(paragraph.getKey());
          line?.classList.add("lex-decision-answer");
          line?.classList.toggle("is-empty", paragraph.getTextContent().replace(ANSWER, "").trim() === "");
        }
      }
    });
  });

  return () => {
    removeChoice();
    removeLook();
  };
}
