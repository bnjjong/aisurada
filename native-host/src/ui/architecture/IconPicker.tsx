// 요소 편집 창의 「아이콘」 칸. 「자동」(빈 값)이거나 모음에서 하나를 고른다. 모음은 앱이 그린 선 아이콘과 서비스 로고다.
import { useMemo, useState } from 'react';
import { Icon } from '../Icon.tsx';
import { C4Icon, tileStyle } from './C4Icon.tsx';
import { LINE_GROUPS, LOGOS_GROUP_TITLE, LOGO_CHOICES, filterChoices, iconLabel, isKnownIcon, logoInk } from './icons.ts';
import type { IconGroup } from './icons.ts';
import { LOGOS } from './logos.ts';

export function IconPicker({ value, auto, onChange }: {
  /** 고른 아이콘 이름. 빈 글이면 자동이다. */
  value: string;
  /** 자동일 때 카드에 쓰일 아이콘 이름(미리보기에 쓴다) */
  auto: string;
  onChange(next: string): void;
}) {
  const [query, setQuery] = useState('');
  const shown = isKnownIcon(value) ? value : auto;
  const groups = useMemo<IconGroup[]>(() => {
    const all: IconGroup[] = [...LINE_GROUPS, { title: LOGOS_GROUP_TITLE, items: LOGO_CHOICES }];
    return all
      .map(group => ({ ...group, items: filterChoices(group.items, query) }))
      .filter(group => group.items.length > 0);
  }, [query]);

  return (
    <div className="c4-icon-field" role="group" aria-label="아이콘">
      <span className="c4-icon-label">아이콘</span>
      <div className="c4-icon-now">
        <span className="c4-icon-preview" data-icon={shown} style={tileStyle(shown)}><C4Icon name={shown} size={18} /></span>
        <button type="button" className="c4-icon-auto" aria-pressed={value === ''} onClick={() => onChange('')}>자동 (이름과 기술에서 고름)</button>
        <label className="c4-icon-search">
          <Icon name="search" size={13} />
          <input type="search" aria-label="아이콘 검색" placeholder="아이콘 찾기" value={query} onChange={event => setQuery(event.target.value)} />
        </label>
      </div>
      <div className="c4-icon-scroll">
        {groups.length === 0 && <p className="c4-note-empty">찾는 아이콘이 없어요.</p>}
        {groups.map(group => (
          <div key={group.title} className="c4-icon-group">
            <h4>{group.title}</h4>
            <div className="c4-icon-grid">
              {group.items.map(item => (
                <button
                  key={item.name}
                  type="button"
                  className="c4-icon-opt"
                  data-icon={item.name}
                  aria-pressed={value === item.name}
                  aria-label={item.label}
                  title={item.label}
                  style={LOGOS[item.name] ? { color: logoInk(LOGOS[item.name].hex) } : undefined}
                  onClick={() => onChange(item.name)}
                >
                  <C4Icon name={item.name} size={16} />
                </button>
              ))}
            </div>
          </div>
        ))}
      </div>
      <small className="c4-hint">
        {value ? `고른 아이콘: ${iconLabel(value)}` : '고르지 않으면 종류에 맞는 기본 아이콘이에요. 이름이나 기술에 GitHub, DB 같은 단서가 있으면 그에 맞게 골라요.'}
      </small>
    </div>
  );
}
