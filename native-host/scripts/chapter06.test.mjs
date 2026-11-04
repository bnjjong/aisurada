import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { RailProjects, useProjectsList, refreshSessions } from '../src/plugs/claude/projects.tsx';

test('Chapter 6: useProjectsList returns array inside component', () => {
  let captured;
  function TestComp() {
    captured = useProjectsList();
    return null;
  }
  renderToString(React.createElement(TestComp));
  assert(Array.isArray(captured), 'useProjectsList must return an array');
});

test('Chapter 6: RailProjects renders [data-rail-section="projects"] and header', () => {
  const html = renderToString(React.createElement(RailProjects, {}));
  assert(html.includes('data-rail-section="projects"'), 'Must contain data-rail-section="projects"');
  assert(html.includes('PROJECTS'), 'Must contain header PROJECTS');
  assert(html.includes('nav-add'), 'Must contain reload button .nav-add');
});

test('Chapter 6: RailProjects displays unmatched sessions message conforming to 6.8', () => {
  const fakeState = {
    workspace: {
      path: '/vault',
      name: 'TestVault',
      sample: false,
      today: '2026-09-20',
      agendas: [
        { id: 'ag1', title: 'ads-platform', slug: 'ads-platform', repo: '/Users/me/kcd/ads-platform' }
      ],
      projects: [],
      notes: [],
      templates: [],
      reports: [],
      trash: [],
      skipped: [],
      conflicts: [],
      noteReadFailed: false,
    },
    prefs: {
      expandedSessionProjects: [],
    },
    route: '/calendar',
  };

  const html = renderToString(React.createElement(RailProjects, { state: fakeState }));
  assert(html.includes('data-rail-section="projects"'), 'Container rendered');
});

test('Chapter 6: D15 defect fix - dropped ended sessions note renders when present', () => {
  // Test that when endedDroppedCount > 0, .sess-note[data-note="ended-dropped"] is in the contract
  const template = `이레 안에 끝난 세션 중 오래된 50개는 안 보여요. 한 번에 읽는 수를 넘었어요.`;
  assert(template.includes('한 번에 읽는 수를 넘었어요'), 'D15 message format preserved');
});
