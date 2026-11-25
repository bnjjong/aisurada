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
        if (command === 'notifications_mark_read') return 2;
        if (command === 'read_drawio_attachment') return new Uint8Array([1, 2, 3]).buffer;
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

  // typed invocation: read_drawio_attachment with noteDir and name
  const bytes = await ctrl.callHost('read_drawio_attachment', { noteDir: 'nd1', name: 'diag.png' });
  assert.ok(bytes);

  // typed invocation: copy_dropped_attachment with noteDir and path
  await ctrl.callHost('copy_dropped_attachment', { noteDir: 'nd1', path: '/drop/file.png' });

  // typed invocation without args for optional argument commands
  await ctrl.callHost('github_status');
  const readCount = await ctrl.callHost('notifications_mark_read');
  assert.equal(readCount, 2);

  assert.equal(invocations.length, 7);
  assert.deepEqual(invocations[0], { command: 'is_workspace', args: { root: '/test/vault' } });
  assert.deepEqual(invocations[1], { command: 'read_external_md', args: { path: '/test/note.md' } });
  assert.deepEqual(invocations[2], { command: 'manager_webhook_secret', args: { name: 'slack', action: 'ensure' } });
  assert.deepEqual(invocations[3], { command: 'read_drawio_attachment', args: { noteDir: 'nd1', name: 'diag.png' } });
  assert.deepEqual(invocations[4], { command: 'copy_dropped_attachment', args: { noteDir: 'nd1', path: '/drop/file.png' } });
  assert.deepEqual(invocations[5], { command: 'github_status', args: undefined });
  assert.deepEqual(invocations[6], { command: 'notifications_mark_read', args: undefined });

  ctrl.dispose();
});
