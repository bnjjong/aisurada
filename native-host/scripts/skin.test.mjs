import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import {
  DEFAULT_SKIN,
  SKINS,
  SKIN_OPTIONS,
  findSkinOption,
  isValidSkin,
  readSkin,
} from '../src/workspace/skin.ts';
import { LookSettings, SkinOptionCard, SkinPicker } from '../src/ui/LookSettings.tsx';

function createStorage(map = {}) {
  const data = new Map(Object.entries(map));
  return {
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => data.set(k, String(v)),
  };
}

test('SKIN_OPTIONS and SKINS: 5개 스킨이 순서대로 일치하게 정의된다', () => {
  assert.equal(SKIN_OPTIONS.length, 5);
  const expectedIds = ['tactile', 'grid', 'glass', 'modern', 'classic'];
  assert.deepEqual(SKINS, expectedIds);
  assert.deepEqual(
    SKIN_OPTIONS.map((o) => o.id),
    expectedIds
  );

  for (const option of SKIN_OPTIONS) {
    assert.equal(typeof option.id, 'string');
    assert.equal(typeof option.label, 'string');
    assert.equal(typeof option.hint, 'string');
    assert.ok(option.label.length > 0);
    assert.ok(option.hint.length > 0);
  }
});

test('DEFAULT_SKIN and isValidSkin: 기본값은 tactile이며 유효성을 엄격히 판별한다', () => {
  assert.equal(DEFAULT_SKIN, 'tactile');
  assert.equal(isValidSkin('tactile'), true);
  assert.equal(isValidSkin('grid'), true);
  assert.equal(isValidSkin('glass'), true);
  assert.equal(isValidSkin('modern'), true);
  assert.equal(isValidSkin('classic'), true);

  assert.equal(isValidSkin(''), false);
  assert.equal(isValidSkin('unknown'), false);
  assert.equal(isValidSkin('dark'), false);
  assert.equal(isValidSkin(null), false);
  assert.equal(isValidSkin(undefined), false);
  assert.equal(isValidSkin(123), false);
});

test('findSkinOption: 스킨 메타데이터를 조회하고 알 수 없는 값은 기본 옵션으로 대체한다', () => {
  const tactile = findSkinOption('tactile');
  assert.equal(tactile.id, 'tactile');
  assert.equal(tactile.label, '물성');

  const grid = findSkinOption('grid');
  assert.equal(grid.id, 'grid');
  assert.equal(grid.label, '격자');

  const fallback = findSkinOption('nonexistent');
  assert.equal(fallback.id, DEFAULT_SKIN);
  assert.equal(fallback.label, '물성');
});

test('readSkin: 저장소 상태에 따라 저장된 스킨 또는 기본값을 안전하게 읽는다', () => {
  // 빈 저장소
  assert.equal(readSkin(createStorage()), DEFAULT_SKIN);

  // 정상 저장된 값
  const classicStorage = createStorage({
    'aisurada.prefs.v1': JSON.stringify({ state: { skin: 'classic' }, version: 1 }),
  });
  assert.equal(readSkin(classicStorage), 'classic');

  // 알 수 없는 스킨 값
  const unknownStorage = createStorage({
    'aisurada.prefs.v1': JSON.stringify({ state: { skin: 'neon' }, version: 1 }),
  });
  assert.equal(readSkin(unknownStorage), DEFAULT_SKIN);

  // 깨진 JSON
  const brokenStorage = createStorage({
    'aisurada.prefs.v1': '{ broken',
  });
  assert.equal(readSkin(brokenStorage), DEFAULT_SKIN);
});

test('SkinPicker and LookSettings: 5개의 스킨 라디오 단추와 설명이 렌더링된다', () => {
  const pickerHtml = renderToStaticMarkup(
    React.createElement(SkinPicker, {
      skin: 'grid',
      onChange: () => {},
    })
  );

  assert.ok(pickerHtml.includes('role="radiogroup"'));
  assert.ok(pickerHtml.includes('data-setting="skin"'));
  assert.ok(pickerHtml.includes('data-skin-preview="grid"'));
  assert.ok(pickerHtml.includes('data-skin-preview="tactile"'));
  assert.ok(pickerHtml.includes('data-skin-preview="glass"'));
  assert.ok(pickerHtml.includes('data-skin-preview="modern"'));
  assert.ok(pickerHtml.includes('data-skin-preview="classic"'));

  // 5개의 라디오 옵션이 존재하는지 확인
  const matches = pickerHtml.match(/class="set-skin-option"/g);
  assert.equal(matches?.length, 5);

  // 현재 선택된 스킨의 힌트 표시
  assert.ok(pickerHtml.includes('격자'));
  assert.ok(pickerHtml.includes('4px 격자'));

  // 전체 LookSettings 렌더링 확인
  const lookHtml = renderToStaticMarkup(
    React.createElement(LookSettings, {
      controller: { preference: () => {} },
      state: { skin: 'modern', typography: { scale: 100, font: 'system', terminalFont: 'mono' } },
    })
  );
  assert.ok(lookHtml.includes('모던'));
  assert.ok(lookHtml.includes('data-setting="scale"'));
});
