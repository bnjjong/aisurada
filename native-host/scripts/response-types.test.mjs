import test from "node:test";
import assert from "node:assert/strict";
import {
  explainEngineError,
  extractErrorKind,
  extractErrorMessage,
} from "../src/ui/engineClient.ts";
import { isMissingCommand } from "../src/ui/runnerStore.ts";
import { EMPTY_ANSWER_ERROR, humanizeAskError, isBlankAnswer } from "../src/plugs/claude/askText.ts";

test("extractErrorKind recognizes structured kind and AIS-### codes", () => {
  assert.equal(extractErrorKind({ kind: "conflict" }), "conflict");
  assert.equal(extractErrorKind({ kind: "command_not_found" }), "command_not_found");
  assert.equal(extractErrorKind({ stderr: "AIS-412 precondition failed" }), "precondition_failed");
  assert.equal(extractErrorKind({ stderr: "AIS-409 conflict occurred" }), "conflict");
  assert.equal(extractErrorKind({ stderr: "AIS-404 resource not found" }), "not_found");
  assert.equal(extractErrorKind({ stderr: "AIS-400 invalid argument" }), "invalid_argument");
  assert.equal(extractErrorKind(new Error("random")), undefined);
});

test("isMissingCommand detects command_not_found by structured kind regardless of message text", () => {
  assert.equal(isMissingCommand({ kind: "command_not_found", message: "arbitrary text" }), true);
  assert.equal(isMissingCommand({ kind: "other_kind", message: "arbitrary text" }), false);
  // fallback for legacy string error
  assert.equal(isMissingCommand(new Error("unknown command: runner_status")), true);
  assert.equal(isMissingCommand(new Error("permission denied")), false);
});

test("humanizeAskError branches by kind even if message text is changed or localized", () => {
  // Even if message text has nothing to do with the error type
  assert.match(
    humanizeAskError({ kind: "no_cli", message: "random unhelpful message" }).message,
    /Claude CLI\(claude\)를 찾지 못했어요/,
  );
  assert.match(
    humanizeAskError({ kind: "login_required", message: "authorization error 123" }).message,
    /Claude 로그인이 필요해요/,
  );
  assert.match(
    humanizeAskError({ kind: "timeout", message: "took 9999ms" }).message,
    /응답 시간이 너무 오래 걸려/,
  );
  assert.match(
    humanizeAskError({ kind: "cancelled", message: "user aborted" }).message,
    /사용자에 의해 질문이 중단되었어요/,
  );
  assert.match(
    humanizeAskError({ kind: "not_found", message: "absent" }).message,
    /항목을 찾을 수 없어요/,
  );

  // Fallback string matching still works
  assert.match(
    humanizeAskError("spawn claude ENOENT").message,
    /Claude CLI\(claude\)를 찾지 못했어요/,
  );
  assert.match(
    humanizeAskError("token expired, please login").message,
    /Claude 로그인이 필요해요/,
  );
});

test("explainEngineError maintains Korean messages when structured error kinds are provided", () => {
  const conflict = explainEngineError({ kind: "precondition_failed", message: "concurrent modification" });
  assert.match(conflict, /다른 곳\(AI나 다른 창\)에서 먼저 고쳤어요/);

  const timeout = explainEngineError({ kind: "timeout", message: "slow backend" });
  assert.match(timeout, /응답 시간이 너무 오래 걸려/);
});

test("TSK-4: a blank ok answer is told apart from a real one, and its error reads as a notice", () => {
  assert.equal(isBlankAnswer(true, ""), true);
  assert.equal(isBlankAnswer(true, "  \n"), true);
  assert.equal(isBlankAnswer(true, undefined), true);
  assert.equal(isBlankAnswer(true, "답"), false);
  assert.equal(isBlankAnswer(false, ""), false, "a failure keeps its own error");
  const info = humanizeAskError(EMPTY_ANSWER_ERROR);
  assert.equal(info.kind, "empty_answer");
  assert.match(info.message, /답을 주지 않았어요/);
  // 호스트 글 뒤에 표준오류가 붙어 login 따위가 들어 있어도 같은 안내가 먼저다
  assert.equal(humanizeAskError("claude 가 종료 코드 0 으로 끝났지만 답이 비어 있다. 도구 실행이 실패했을 수 있다. login expired").kind, "empty_answer");
});
