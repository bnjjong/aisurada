import test from 'node:test';
import assert from 'node:assert/strict';
import { setupPopoverDismiss } from '../src/ui/usePopoverDismiss.ts';

// 가상 DOM 및 리스너 환경 픽스처
function createEnv() {
  const docListeners = new Map();
  const winListeners = new Map();

  const add = (map, type, fn, capture) => {
    const key = `${type}:${Boolean(capture)}`;
    if (!map.has(key)) map.set(key, new Set());
    map.get(key).add(fn);
  };

  const remove = (map, type, fn, capture) => {
    const key = `${type}:${Boolean(capture)}`;
    map.get(key)?.delete(fn);
  };

  const trigger = (map, type, event, capture = false) => {
    const key = `${type}:${Boolean(capture)}`;
    const fns = map.get(key);
    if (fns) {
      for (const fn of [...fns]) fn(event);
    }
  };

  const fakeDoc = {
    addEventListener: (type, fn, capture) => add(docListeners, type, fn, capture),
    removeEventListener: (type, fn, capture) => remove(docListeners, type, fn, capture),
  };

  const fakeWin = {
    PointerEvent: function () {},
    addEventListener: (type, fn, capture) => add(winListeners, type, fn, capture),
    removeEventListener: (type, fn, capture) => remove(winListeners, type, fn, capture),
  };

  const prevDoc = globalThis.document;
  const prevWin = globalThis.window;
  const prevPointer = globalThis.PointerEvent;

  globalThis.document = fakeDoc;
  globalThis.window = fakeWin;
  globalThis.PointerEvent = fakeWin.PointerEvent;

  return {
    dispatchDoc: (type, event) => trigger(docListeners, type, event, false),
    dispatchWin: (type, event, capture = false) => trigger(winListeners, type, event, capture),
    listenerCounts: () => {
      let docCount = 0;
      for (const s of docListeners.values()) docCount += s.size;
      let winCount = 0;
      for (const s of winListeners.values()) winCount += s.size;
      return { docCount, winCount };
    },
    cleanup: () => {
      globalThis.document = prevDoc;
      globalThis.window = prevWin;
      globalThis.PointerEvent = prevPointer;
    },
  };
}

function mockElement(name = 'elem') {
  const children = new Set();
  const el = {
    name,
    contains(target) {
      return target === el || children.has(target);
    },
    addChild(child) {
      children.add(child);
    },
    focused: false,
    focus() {
      this.focused = true;
    },
  };
  return el;
}

test('setupPopoverDismiss: 외부 포인터 다운 시 onClose 호출', () => {
  const env = createEnv();
  try {
    const popoverEl = mockElement('popover');
    const outsideEl = mockElement('outside');
    let closed = 0;

    const cleanup = setupPopoverDismiss({
      ref: { current: popoverEl },
      onClose: () => closed++,
    });

    // 내부 클릭 -> 닫히지 않음
    env.dispatchDoc('pointerdown', { target: popoverEl });
    assert.equal(closed, 0);

    // 외부 클릭 -> 닫힘
    env.dispatchDoc('pointerdown', { target: outsideEl });
    assert.equal(closed, 1);

    cleanup();
    // 정리 후 클릭 -> 호출되지 않음
    env.dispatchDoc('pointerdown', { target: outsideEl });
    assert.equal(closed, 1);
  } finally {
    env.cleanup();
  }
});

test('setupPopoverDismiss: 앵커 ref 클릭 시 onClose 무시', () => {
  const env = createEnv();
  try {
    const popoverEl = mockElement('popover');
    const anchorEl = mockElement('anchor');
    let closed = 0;

    const cleanup = setupPopoverDismiss({
      ref: { current: popoverEl },
      anchorRef: { current: anchorEl },
      onClose: () => closed++,
    });

    env.dispatchDoc('pointerdown', { target: anchorEl });
    assert.equal(closed, 0);

    cleanup();
  } finally {
    env.cleanup();
  }
});

test('setupPopoverDismiss: shouldIgnoreTarget 술어 존중', () => {
  const env = createEnv();
  try {
    const popoverEl = mockElement('popover');
    const badgeEl = mockElement('badge');
    badgeEl.closest = (sel) => (sel === '.badge' ? badgeEl : null);
    let closed = 0;

    const cleanup = setupPopoverDismiss({
      ref: { current: popoverEl },
      onClose: () => closed++,
      shouldIgnoreTarget: (target) => Boolean(target.closest?.('.badge')),
    });

    env.dispatchDoc('pointerdown', { target: badgeEl });
    assert.equal(closed, 0);

    cleanup();
  } finally {
    env.cleanup();
  }
});

test('setupPopoverDismiss: Escape 키다운 시 onClose 및 이벤트 전파 방지', () => {
  const env = createEnv();
  try {
    const popoverEl = mockElement('popover');
    const anchorEl = mockElement('anchor');
    let closed = 0;

    const cleanup = setupPopoverDismiss({
      ref: { current: popoverEl },
      anchorRef: { current: anchorEl },
      onClose: () => closed++,
      restoreFocus: true,
      captureEscape: true,
    });

    let defaultPrevented = false;
    let stopped = false;
    const escEvent = {
      key: 'Escape',
      preventDefault: () => {
        defaultPrevented = true;
      },
      stopPropagation: () => {
        stopped = true;
      },
    };

    env.dispatchWin('keydown', escEvent, true);
    assert.equal(closed, 1);
    assert.equal(defaultPrevented, true);
    assert.equal(stopped, true);
    assert.equal(anchorEl.focused, true);

    cleanup();
  } finally {
    env.cleanup();
  }
});

test('setupPopoverDismiss: 스크롤 및 리사이즈 처리 (내부 스크롤 무시)', () => {
  const env = createEnv();
  try {
    const popoverEl = mockElement('popover');
    const popoverInner = mockElement('inner');
    popoverEl.addChild(popoverInner);
    let closed = 0;

    const cleanup = setupPopoverDismiss({
      ref: { current: popoverEl },
      onClose: () => closed++,
      closeOnScroll: true,
      closeOnResize: true,
    });

    // 팝오버 내부 스크롤 -> 닫히지 않음
    env.dispatchWin('scroll', { target: popoverInner }, true);
    assert.equal(closed, 0);

    // 윈도우 스크롤 -> 닫힘
    env.dispatchWin('scroll', { target: mockElement('window-body') }, true);
    assert.equal(closed, 1);

    // 리사이즈 -> 닫힘
    env.dispatchWin('resize', {}, false);
    assert.equal(closed, 2);

    cleanup();
  } finally {
    env.cleanup();
  }
});

test('setupPopoverDismiss: enabled = false 시 리스너 비활성화', () => {
  const env = createEnv();
  try {
    const popoverEl = mockElement('popover');
    let closed = 0;

    const cleanup = setupPopoverDismiss({
      ref: { current: popoverEl },
      onClose: () => closed++,
      enabled: false,
    });

    assert.deepEqual(env.listenerCounts(), { docCount: 0, winCount: 0 });
    env.dispatchDoc('pointerdown', { target: mockElement('outside') });
    assert.equal(closed, 0);

    cleanup();
  } finally {
    env.cleanup();
  }
});
