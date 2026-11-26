import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { LINE_NAMES, LOGO_NAMES, filterChoices, iconLabel, isKnownIcon, lineIconName, logoInk, logoTone, LINE_GROUPS, LOGO_CHOICES } from '../src/ui/architecture/icons.ts';
import { LOGOS } from '../src/ui/architecture/logos.ts';

const rust = readFileSync(new URL('../../engine-rs/crates/ais-domain/src/c4/icons.rs', import.meta.url), 'utf8');
const rustNames = name => [...rust.match(new RegExp(`pub const ${name}: \\[&str; \\d+\\] = \\[(.*?)\\];`, 's'))[1].matchAll(/"([^"]+)"/g)].map(match => match[1]);

test('화면의 아이콘 이름은 엔진이 받는 이름과 같다(순서까지 이름순)', () => {
  assert.deepEqual([...LINE_NAMES].sort(), rustNames('BUILTIN'), '선 아이콘 이름이 엔진 목록과 다르다');
  assert.deepEqual([...LOGO_NAMES].sort(), rustNames('LOGOS'), '로고 이름이 엔진 목록과 다르다(scripts/gen-logos.mjs를 다시 돌린다)');
});

test('선 아이콘은 모두 Icon.tsx에 그림이 있고, 로고는 모두 경로와 색이 있다', () => {
  const source = readFileSync(new URL('../src/ui/Icon.tsx', import.meta.url), 'utf8');
  const keys = new Set([...source.matchAll(/^  (\w+): /gm)].map(match => match[1]));
  for (const name of LINE_NAMES) assert.ok(keys.has(lineIconName(name)), `Icon.tsx에 ${name} 그림이 없다`);
  for (const [slug, logo] of Object.entries(LOGOS)) {
    assert.ok(/^[Mm]/.test(logo.path) && logo.title, `${slug}: 경로나 이름이 비었다`);
    assert.match(logo.hex, /^[0-9A-Fa-f]{6}$/, `${slug}: 색이 6자리 16진수가 아니다`);
  }
  assert.equal(lineIconName('git-branch'), 'gitBranch');
});

test('이름 묶음에는 중복이 없고 라벨이 있다', () => {
  const all = [...LINE_NAMES, ...LOGO_NAMES];
  assert.equal(new Set(all).size, all.length, '이름이 겹친다');
  assert.ok(LINE_GROUPS.every(group => group.items.length > 0));
  assert.equal(LOGO_CHOICES.length, LOGO_NAMES.length);
  assert.equal(iconLabel('github'), 'GitHub');
  assert.equal(iconLabel('person'), '사람');
  assert.equal(iconLabel('없는-이름'), '없는-이름');
});

test('아는 이름만 아이콘으로 쓰고, 모르는 이름은 쓰지 않는다', () => {
  assert.ok(isKnownIcon('github') && isKnownIcon('git-branch'));
  assert.ok(!isKnownIcon('GitHub') && !isKnownIcon('future-icon') && !isKnownIcon('') && !isKnownIcon(null) && !isKnownIcon(undefined));
});

test('검색은 이름이나 한글 라벨에 들어 있으면 남기고, 비면 그대로다', () => {
  const names = query => filterChoices(LOGO_CHOICES, query).map(item => item.name);
  assert.deepEqual(names('tele'), ['telegram']);
  assert.ok(names('GIT').includes('github') && names('git').includes('gitlab'), '대소문자를 가리지 않는다');
  assert.equal(names('  ').length, LOGO_CHOICES.length);
  const line = filterChoices(LINE_GROUPS.flatMap(group => group.items), '대화');
  assert.deepEqual(line.map(item => item.name), ['chat']);
});

test('밝은 브랜드 색은 어두운 글자색을, 어두운 색은 흰 글자색을 쓴다', () => {
  assert.equal(logoTone('F7DF1E').ink, '#1f2328', '노랑에는 어두운 로고');
  assert.equal(logoTone('181717').ink, '#ffffff');
  assert.equal(logoTone('26A5E4').from, '#26A5E4');
  assert.match(logoTone('26A5E4').to, /^#[0-9a-f]{6}$/);
  assert.equal(logoInk('181717'), 'var(--ink)', '검정에 가까운 로고는 기본 글자색');
  assert.equal(logoInk('26A5E4'), '#26A5E4');
});
