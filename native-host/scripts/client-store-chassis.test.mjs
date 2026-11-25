import test from "node:test";
import assert from "node:assert/strict";
import {
  createExternalStore,
  explainEngineError,
  extractErrorMessage,
  isRecord,
} from "../src/ui/engineClient.ts";

test("isRecord recognizes plain objects and rejects primitives, arrays, null", () => {
  assert.equal(isRecord({}), true);
  assert.equal(isRecord({ a: 1 }), true);
  assert.equal(isRecord([]), false);
  assert.equal(isRecord(null), false);
  assert.equal(isRecord(undefined), false);
  assert.equal(isRecord("str"), false);
  assert.equal(isRecord(123), false);
});

test("extractErrorMessage extracts message from various shapes", () => {
  assert.equal(extractErrorMessage({ stderr: "AIS-400 Bad request" }), "AIS-400 Bad request");
  assert.equal(extractErrorMessage(new Error("Network failed")), "Network failed");
  assert.equal(extractErrorMessage("raw string error"), "raw string error");
  assert.equal(extractErrorMessage(404), "404");
});

test("explainEngineError handles AIS-412 conflict and standard AIS errors", () => {
  // AIS-412
  const conflict = explainEngineError({ stderr: "AIS-412 f1 precondition failed" });
  assert.match(conflict, /다른 곳\(AI나 다른 창\)에서 먼저 고쳤어요/);

  // standard AIS code
  const notFound = explainEngineError({ stderr: "AIS-404 Note not found: ndm123" });
  assert.equal(notFound, "Note not found: ndm123");

  // custom classifier override
  const custom = explainEngineError(
    { stderr: "AIS-409 이미 바뀌었다" },
    (stderr) => (/AIS-409/.test(stderr) ? "상태가 먼저 바뀌었습니다." : undefined),
  );
  assert.equal(custom, "상태가 먼저 바뀌었습니다.");
});

test("createExternalStore manages state, notifies listeners, and supports functional updates", () => {
  const initial = { count: 0, tag: "init" };
  const store = createExternalStore(
    initial,
    (a, b) => a.count === b.count && a.tag === b.tag,
  );

  assert.deepEqual(store.getSnapshot(), initial);

  let notified = 0;
  const unsubscribe = store.subscribe(() => {
    notified += 1;
  });

  // Direct publish
  store.publish({ count: 1, tag: "init" });
  assert.equal(notified, 1);
  assert.deepEqual(store.getSnapshot(), { count: 1, tag: "init" });

  // Same state ignores notification
  store.publish({ count: 1, tag: "init" });
  assert.equal(notified, 1);

  // Functional update
  store.publish((prev) => ({ count: prev.count + 1, tag: "updated" }));
  assert.equal(notified, 2);
  assert.deepEqual(store.getSnapshot(), { count: 2, tag: "updated" });

  // Unsubscribe
  unsubscribe();
  store.publish({ count: 10, tag: "done" });
  assert.equal(notified, 2);
  assert.equal(store.getSnapshot().count, 10);
});
