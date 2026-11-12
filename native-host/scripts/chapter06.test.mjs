import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { useSessionList } from '../src/plugs/claude/sessionStore.ts';
import { Rail } from '../src/ui/Rail.tsx';

const agenda = (id, repo, extra = {}) => ({ id, slug: id, title: id, repo, ...extra });
const workspace = (agendas, projects = []) => ({ path: '/vault', name: 'v', sample: false, today: '2026-11-04', agendas, projects, notes: [], templates: [], trash: [], skipped: [], conflicts: [], noteReadFailed: false });

test('6: useSessionList returns an array inside a component', () => {
  let captured;
  function TestComp() {
    captured = useSessionList();
    return null;
  }
  renderToString(React.createElement(TestComp));
  assert(Array.isArray(captured));
});

test('Rail activates selected agenda and does not keep calendar active on agenda route', () => {
  const ws = workspace([agenda('ag1', '', { title: 'AI 개발 철학' })]);
  const fakeController = {
    navigate: () => {},
    filterAgendas: () => {},
    form: () => {},
    preference: () => {},
    settings: () => {},
    launcher: () => {},
  };
  const agendaState = {
    workspace: ws,
    prefs: { collapsedProjects: [] },
    hiddenAgendas: [],
    theme: 'dark',
    route: '/agenda/ag1',
  };
  const htmlAgenda = renderToString(React.createElement(Rail, { controller: fakeController, state: agendaState }));
  assert.doesNotMatch(htmlAgenda, /class="nav-item active">캘린더/);
  assert.match(htmlAgenda, /class="rail-agenda active/);

  const calState = {
    workspace: ws,
    prefs: { collapsedProjects: [] },
    hiddenAgendas: [],
    theme: 'dark',
    route: '/calendar',
  };
  const htmlCal = renderToString(React.createElement(Rail, { controller: fakeController, state: calState }));
  assert.match(htmlCal, /class="nav-item active">캘린더/);
  assert.doesNotMatch(htmlCal, /class="rail-agenda active/);
});
