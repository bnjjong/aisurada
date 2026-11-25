import test from 'node:test';
import assert from 'node:assert/strict';
import { WorkspaceController } from '../src/workspace/controller.ts';

function createMockServices() {
  const store = new Map();
  return {
    storage: {
      getItem: (k) => store.get(k) ?? null,
      setItem: (k, v) => store.set(k, String(v)),
    },
    files: {
      exists: async () => false,
      readDir: async () => [],
      readTextFile: async () => '',
      writeTextFile: async () => {},
      mkdir: async () => {},
      rename: async () => {},
      remove: async () => {},
    },
    host: {
      invoke: async () => ({}),
      documentDir: async () => '/mock/docs',
      chooseDirectory: async () => null,
      confirmLeave: async () => true,
    },
  };
}

test('WorkspaceController implements WorkspaceNavigation surface', () => {
  const ctrl = new WorkspaceController(createMockServices(), { kind: 'main' }, '/');

  assert.equal(typeof ctrl.navigate, 'function');
  assert.equal(typeof ctrl.locationChanged, 'function');
  assert.equal(typeof ctrl.selectTab, 'function');
  assert.equal(typeof ctrl.closeTab, 'function');
  assert.equal(typeof ctrl.closeActiveTab, 'function');
  assert.equal(typeof ctrl.reorderTab, 'function');

  ctrl.dispose();
});

test('WorkspaceController implements WorkspacePreferences surface', () => {
  const ctrl = new WorkspaceController(createMockServices(), { kind: 'main' }, '/');

  assert.equal(typeof ctrl.preference, 'function');
  assert.equal(typeof ctrl.refreshTheme, 'function');
  assert.equal(typeof ctrl.zoomBy, 'function');
  assert.equal(typeof ctrl.applyScale, 'function');
  assert.equal(typeof ctrl.settings, 'function');

  ctrl.dispose();
});

test('WorkspaceController implements WorkspaceHostBridge surface', () => {
  const ctrl = new WorkspaceController(createMockServices(), { kind: 'main' }, '/');

  assert.equal(typeof ctrl.isNative, 'function');
  assert.equal(typeof ctrl.callHost, 'function');
  assert.equal(typeof ctrl.callEngine, 'function');
  assert.equal(typeof ctrl.pickDirectory, 'function');
  assert.equal(typeof ctrl.chooseFile, 'function');

  ctrl.dispose();
});

test('WorkspaceController implements WorkspaceLifecycle surface', () => {
  const ctrl = new WorkspaceController(createMockServices(), { kind: 'main' }, '/');

  assert.equal(typeof ctrl.boot, 'function');
  assert.equal(typeof ctrl.open, 'function');
  assert.equal(typeof ctrl.create, 'function');
  assert.equal(typeof ctrl.sample, 'function');
  assert.equal(typeof ctrl.openRecent, 'function');
  assert.equal(typeof ctrl.forgetRecent, 'function');
  assert.equal(typeof ctrl.launcher, 'function');
  assert.equal(typeof ctrl.refresh, 'function');
  assert.equal(typeof ctrl.dispose, 'function');
  assert.equal(typeof ctrl.waitForIdle, 'function');

  ctrl.dispose();
});
