import test from "node:test";
import assert from "node:assert/strict";
import {
  readPreferences,
  writePreferences,
  DEFAULT_PREFERENCES,
} from "../src/workspace/storage.ts";
import { defaultPrefs, preferences } from "../src/ui/model.ts";

function createMemoryStorage(initial = {}) {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (key) => map.get(key) ?? null,
    setItem: (key, val) => {
      map.set(key, val);
    },
    removeItem: (key) => {
      map.delete(key);
    },
    clear: () => {
      map.clear();
    },
  };
}

test("readPreferences yields 21 enumerable keys and type-safe alias getters", () => {
  const storage = createMemoryStorage();
  const prefs = readPreferences(storage);

  // Exact 21 keys from Chapter 16
  const keys = Object.keys(prefs);
  assert.equal(keys.length, 21);

  // Enumerable fields match default
  assert.equal(prefs.theme, "light");
  assert.equal(prefs.titleDatePrefix, true);
  assert.equal(prefs.showRecordTier, false);
  assert.equal(prefs.mermaidShowSource, false);
  assert.equal(prefs.docSortMode, "structure");
  assert.equal(prefs.docSortDir, "desc");

  // Non-enumerable UI alias getters match standard fields
  assert.equal(prefs.titleDate, prefs.titleDatePrefix);
  assert.equal(prefs.showRecords, prefs.showRecordTier);
  assert.equal(prefs.diagrams, prefs.mermaidShowSource);
  assert.equal(prefs.agendaSort, prefs.docSortMode);
  assert.equal(prefs.agendaDesc, prefs.docSortDir === "desc");
  assert.deepEqual(prefs.previewSize, { width: 720, height: 480 });
});

test("writePreferences accepts legacy alias keys and updates canonical fields", () => {
  const storage = createMemoryStorage();

  // Update via aliases
  writePreferences(storage, {
    titleDate: false,
    showRecords: true,
    diagrams: true,
    agendaSort: "time",
    agendaDesc: false,
    previewSize: { width: 900, height: 600 },
  });

  const prefs = readPreferences(storage);
  assert.equal(prefs.titleDatePrefix, false);
  assert.equal(prefs.titleDate, false);
  assert.equal(prefs.showRecordTier, true);
  assert.equal(prefs.showRecords, true);
  assert.equal(prefs.mermaidShowSource, true);
  assert.equal(prefs.diagrams, true);
  assert.equal(prefs.docSortMode, "time");
  assert.equal(prefs.agendaSort, "time");
  assert.equal(prefs.docSortDir, "asc");
  assert.equal(prefs.agendaDesc, false);
  assert.equal(prefs.previewWidth, 900);
  assert.equal(prefs.previewHeight, 600);
  assert.deepEqual(prefs.previewSize, { width: 900, height: 600 });
});

test("model defaultPrefs and preferences function adhere to FullPreferences", () => {
  assert.equal(defaultPrefs.titleDate, true);
  assert.equal(defaultPrefs.showRecords, false);
  assert.equal(defaultPrefs.diagrams, false);
  assert.equal(defaultPrefs.agendaSort, "structure");
  assert.equal(defaultPrefs.agendaDesc, true);

  const storage = createMemoryStorage();
  const p = preferences(storage);
  assert.equal(p.theme, "light");
  assert.equal(p.titleDatePrefix, true);
  assert.equal(p.titleDate, true);
});
