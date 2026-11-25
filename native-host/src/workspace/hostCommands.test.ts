import type { TypedHostCall } from './hostCommands';

// 컴파일 타임 타입 검사 검증용 테스트 (tsc --noEmit 시 실행됨)
declare const call: TypedHostCall;

// 1. 올바른 호출들 - 에러 없이 컴파일 통과해야 함
void call('is_workspace', { root: '/test/vault' });
void call('github_clone', { fullName: 'bnjjong/aisurada' });
void call('github_status');
void call('notifications_mark_read');
void call('notifications_mark_read', { ids: ['item1'] });
void call('resolve_md_target', { path: 'doc.md', knownRoots: ['/root'] });
void call('read_drawio_attachment', { noteDir: 'nd1', name: 'diag.png' });
void call('copy_dropped_attachment', { noteDir: 'nd1', path: '/drop/file.png' });
void call('show_in_main', { path: '/test/note.md' });
void call('sidecar_call', { argv: ['--version'] });
void call('exit_app');

// 2. 인자 이름 오타 - @ts-expect-error 로 잡혀야 함
// @ts-expect-error is_workspace는 root를 받으며 path는 허용되지 않는다
void call('is_workspace', { path: '/test/vault' });

// @ts-expect-error github_clone은 fullName을 받으며 repoFullName은 허용되지 않는다
void call('github_clone', { repoFullName: 'bnjjong/aisurada' });

// @ts-expect-error read_drawio_attachment는 noteDir, name을 받으며 path는 허용되지 않는다
void call('read_drawio_attachment', { path: '/test/diag.png' });

// @ts-expect-error copy_dropped_attachment는 noteDir, path를 받으며 source, target은 허용되지 않는다
void call('copy_dropped_attachment', { source: 'a', target: 'b' });

// @ts-expect-error resolve_md_target의 knownRoots는 필수다
void call('resolve_md_target', { path: 'doc.md' });

// 3. 미등록 커맨드 호출 - @ts-expect-error 로 잡혀야 함
// @ts-expect-error no_such_command는 HostCommandMap에 등록되지 않은 명령이다
void call('no_such_command', { anything: 1 });
