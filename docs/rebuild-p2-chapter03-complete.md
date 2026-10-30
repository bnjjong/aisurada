# 2부 3장 완료 — 화면과 주고받는 규약

작성일: 2026-10-30

제공받은 PDF 20~23쪽을 기준으로 `native-host/src-tauri/src/ipc.rs`에 공통 통신 규약을 추가했다. Tauri가 JSON 인자와 성공·오류 응답을 처리하며, 4장의 커맨드 네 개를 실제 invoke 처리기에 등록했다. 동기 커맨드는 Tauri의 main 스레드 실행 규칙을 따른다.

바이트 전달은 raw 본문과 숫자 배열 JSON을 같은 `Vec<u8>`로 읽는다. 범위 밖 정수·소수·불리언·중첩 배열·객체·null은 거절하고, 빈 raw 본문과 빈 배열은 허용한다. `x-wl-*` 헤더는 ASCII 확인 후 퍼센트 인코딩을 UTF-8로 푼다. `+`를 공백으로 바꾸지 않는다. 채널은 UTF-8 해독 없이 받은 조각을 표준 base64와 끝 채움으로 보낸다.

이벤트 이름 네 개와 camelCase 페이로드 형식을 공통 코드에 뒀다. 메뉴 이벤트는 `EventTarget::WebviewWindow`를 명시해 창 하나에만 보낸다. navigate의 rail/sub는 없을 때 null이고 self-write는 relPath를 쓴다. 실제 이벤트를 만드는 후속 커맨드는 각 장에서 연결한다.

`cargo test --locked`의 Rust 테스트 17개와 설정 테스트 3개가 통과했다. Tauri MockRuntime의 실제 invoke 처리기를 사용해 누락·타입 오류·없는 커맨드·camelCase를 검사하고, raw/JSON 대체 경로와 헤더를 통해 시험 커맨드를 호출했다. 실제 Tauri 채널의 base64와 창별 이벤트 대상도 검증했다. macOS `.app` 빌드를 확인했다.

write_attachment_bytes, pty_start 및 나머지 후속 커맨드는 이번에 등록하지 않았다. 이를 대신하는 가짜 제품 커맨드도 넣지 않았다. 바이트·채널의 공통 함수와 시험용 커맨드는 각각 제품 코드와 테스트 전용 코드로 구분했다. 18장의 전체 받아들임 검사나 실제 webview의 JS 이벤트 루프를 실행한 결과는 아니다.
