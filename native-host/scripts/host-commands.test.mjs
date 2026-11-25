import test from 'node:test';
import assert from 'node:assert/strict';
import { WorkspaceController } from '../src/workspace/controller.ts';

function createMockServices(invocations = []) {
  return {
    storage: {
      getItem: () => null,
      setItem: () => {},
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
      invoke: async (command, args) => {
        invocations.push({ command, args });
        if (command === 'is_workspace') return true;
        if (command === 'read_external_md') return '# External Note';
        if (command === 'manager_webhook_secret') return 'whsec_test123';
        return {};
      },
      documentDir: async () => '/mock/docs',
      chooseDirectory: async () => null,
      confirmLeave: async () => true,
    },
  };
}

test('TypedHostCall: WorkspaceController.callHost passes typed command and arguments', async () => {
  const invocations = [];
  const ctrl = new WorkspaceController(createMockServices(invocations), { kind: 'main' }, '/');

  // typed invocation: is_workspace
  const isWs = await ctrl.callHost('is_workspace', { root: '/test/vault' });
  assert.equal(isWs, true);

  // typed invocation: read_external_md
  const md = await ctrl.callHost('read_external_md', { path: '/test/note.md' });
  assert.equal(md, '# External Note');

  // typed invocation: manager_webhook_secret
  const secret = await ctrl.callHost('manager_webhook_secret', { name: 'slack', action: 'ensure' });
  assert.equal(secret, 'whsec_test123');

  assert.equal(invocations.length, 3);
  assert.deepEqual(invocations[0], { command: 'is_workspace', args: { root: '/test/vault' } });
  assert.deepEqual(invocations[1], { command: 'read_external_md', args: { path: '/test/note.md' } });
  assert.deepEqual(invocations[2], { command: 'manager_webhook_secret', args: { name: 'slack', action: 'ensure' } });

  ctrl.dispose();
});

test('TypedHostCall: fallback generic invocation works for dynamic commands', async () => {
  const invocations = [];
  const ctrl = new WorkspaceController(createMockServices(invocations), { kind: 'main' }, '/');

  await ctrl.callHost('custom_extension_command', { extra: 42 });
  assert.equal(invocations.length, 1);
  assert.deepEqual(invocations[0], { command: 'custom_extension_command', args: { extra: 42 } });

  ctrl.dispose();
});
