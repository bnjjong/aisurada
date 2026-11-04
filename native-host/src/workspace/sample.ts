import type { Agenda, Note, Workspace } from './types.ts';

// Content is newly written; only md1 + a markdown paragraph is a contract.
export function sampleWorkspace(): Workspace {
  const agendas: Agenda[] = [
    { id: 'ag-sample-work', slug: 'work', title: '프로젝트', hue: 'green' },
    { id: 'ag-sample-learn', slug: 'learn', title: '배움', hue: 'blue' },
    { id: 'ag-sample-daily', slug: 'daily', title: '일상', hue: 'amber' },
  ];
  const notes: Note[] = [
    { id: 'md1', title: 'aisurada에 오신 것을 환영해요', agenda: 'work', agendaId: 'ag-sample-work', format: 'markdown', created: '2026-06-04T09:00:00+09:00', date: '2026-06-04', time: '09:00', body: '# aisurada에 오신 것을 환영해요\n\n생각이 떠오른 순간부터 작업을 마치는 순간까지, 작은 기록으로 맥락을 이어가세요.\n\n이 샘플은 내 파일을 바꾸지 않고 자유롭게 둘러볼 수 있는 공간이에요.\n' },
    { id: 'md2', title: '오늘의 작은 배움', agenda: 'learn', agendaId: 'ag-sample-learn', format: 'markdown', created: '2026-06-04T11:00:00+09:00', date: '2026-06-04', time: '11:00', body: '# 오늘의 작은 배움\n\n배운 내용을 내 말로 정리하면 다음 작업에 다시 꺼내 쓰기 쉬워져요.\n' },
    { id: 'md3', title: '하루를 돌아보며', agenda: 'daily', agendaId: 'ag-sample-daily', format: 'markdown', created: '2026-06-04T17:00:00+09:00', date: '2026-06-04', time: '17:00', body: '# 하루를 돌아보며\n\n오늘 잘된 것 하나, 내일 이어갈 것 하나를 남겨 보세요.\n' },
  ];
  return { path: '', name: '샘플 워크스페이스', sample: true, today: '2026-06-04', agendas, notes,
    projects: [], templates: [], reports: [], trash: [], skipped: [], conflicts: [], noteReadFailed: false };
}
