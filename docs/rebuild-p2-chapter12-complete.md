# 2부 12장 완료 — 노트 창

작성일: 2026-10-31

제공 PDF 51~55쪽의 커맨드 여덟 개를 구현했다. open_note_window는 ASCII id 1~64바이트로 note- 이름표를 만들고, 여는 잠금으로 중복 생성을 막는다. 이미 있으면 앞으로 가져오고, 새 창은 note:last 또는 받은 geometry에 열린 창 수만큼 비켜 배치한다. 실제 생성과 열린 목록 방송은 main 스레드에서 수행한다.

vault별 복원 목록은 note-windows.dev.json에 읽고 고쳐 쓰는 잠금과 원자적 저장을 적용한다. 같은 vault의 기존 키를 보존하고 순서를 유지한다. 창 제거는 모든 vault 칸에서 id를 빼지만 앱 종료 중에는 목록을 남긴다. remember는 실제 호출 창의 기존 칸만 고친다. restore는 목록에 있는 유효 id만 중복 없이 포커스 없이 만들고, 기억한 데스크톱으로 보낸 뒤 main을 앞으로 가져온다.

close_note_windows는 닫기 요청만 하고, destroy_current_window는 호출한 노트 창만 직접 없앤다. show_in_main은 앱 안 경로만 검사해 main에 navigate를 보내고 rail·sub를 그대로 전달한다.

실제 WKWebView에서 미리 심은 창 복원, 중복·없는 id 건너뛰기, 노트 창의 vault 변경 거절, remember·자기 쓰기·main 이동·자기 제거, 목록 방송, 복원 목록 삭제와 나머지 창 닫기가 통과했다. 실제 편집기의 닫기 요청 저장과 Cmd+W는 3부 화면을 연결한 뒤 손 확인한다.
