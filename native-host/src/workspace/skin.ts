import type { Skin, StoragePort } from './types.ts';
import { asRow } from './types.ts';
import { KEYS, readJSON } from './storage.ts';

export interface SkinOption {
  readonly id: Skin;
  readonly label: string;
  readonly hint: string;
}

/**
 * 설정에서 고를 수 있는 화면 모양 목록. 순서가 곧 화면에 나오는 순서다.
 */
export const SKIN_OPTIONS: readonly SkinOption[] = [
  {
    id: 'tactile',
    label: '물성',
    hint: '눌리는 것은 떠 있고, 쓰는 것은 파여 있는 둥근 모양이에요.',
  },
  {
    id: 'grid',
    label: '격자',
    hint: '4px 격자와 또렷한 먹선, 각진 모서리로 정확하게 그린 모양이에요.',
  },
  {
    id: 'glass',
    label: '유리',
    hint: '반투명 유리 패널과 은은한 빛번짐으로 가볍게 그린 모양이에요.',
  },
  {
    id: 'modern',
    label: '모던',
    hint: '얇은 선과 차분한 회색, 작은 곡률로 정돈한 모양이에요.',
  },
  {
    id: 'classic',
    label: '기본',
    hint: '얇은 테두리와 평평한 면으로 그린 이전 모양이에요.',
  },
] as const;

/** 지원하는 모든 스킨 식별자 목록 */
export const SKINS: readonly Skin[] = SKIN_OPTIONS.map((o) => o.id);

/** 기본 스킨 식별자 */
export const DEFAULT_SKIN: Skin = 'tactile';

/** 전달된 값이 유효한 Skin인지 판별하는 타입 가드 */
export function isValidSkin(value: unknown): value is Skin {
  return typeof value === 'string' && SKINS.includes(value as Skin);
}

/** 주어진 스킨 식별자의 옵션 메타데이터를 찾는다. 알 수 없는 값이면 기본 스킨 옵션을 돌려준다. */
export function findSkinOption(skin: unknown): SkinOption {
  return SKIN_OPTIONS.find((s) => s.id === skin) ?? SKIN_OPTIONS[0];
}

/**
 * 저장소에서 사용자가 고른 스킨을 읽는다.
 * 고른 적이 없거나 알 수 없는 값이면 기본 스킨('tactile')을 반환한다.
 */
export function readSkin(storage: StoragePort): Skin {
  const skin = asRow(asRow(readJSON(storage, KEYS.prefs))?.state)?.skin;
  return isValidSkin(skin) ? skin : DEFAULT_SKIN;
}
