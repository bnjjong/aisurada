# 2부 부록 대조 기록

작성일: 2026-10-31

부록 A의 2부 커맨드 29개를 main invoke 처리기에 모두 등록했다. async는 sidecar_call, write_attachment_bytes, announce_self_write, open_note_window, restore_note_windows, secret_table_probe 여섯 개다. 나머지 23개는 동기다. 4부 커맨드 아홉 개는 없는 판으로 답한다.

부록 B의 오류 단계와 거절 방식을 각 모듈에 반영했다. 필수·타입 인자는 Tauri가 먼저 검사하고, 파일 경계·크기·UTF-8·id·내부 경로와 빈 비밀 표는 커맨드가 검사한다. Java 명령 실패는 sidecar_call의 status·stdout·stderr 값으로 반환한다. wlframe 거절은 빈 HTTP 본문이며 이유 글을 싣지 않는다. 계약인 열기 전 문구는 실제 webview에서 맞댔다.

부록 C에서 고친다고 정한 C16·C19는 이전 장에 반영했고, C51은 이번 PTY의 인스턴스·세대 검사로 반영했다. 기존의 범위 누적, raw vault 글자, 좁은 노트 id 읽기, attach 마디 판정, 위치가 화면 밖인 노트 창 크기 보존, 비밀 probe 제한 뒤 자식 존속 등은 유지했다. 실제 4부 자원이 없는 재구현에서 켠 판과 원본 아이콘·소스 검사까지 제공하지 않는 차이는 각 완료 문서에 명시했다.

부록은 별도 제품 기능을 더하지 않으며, 자동 점검의 통과가 PDF 18.4의 손 확인을 대신하지 않는다.
