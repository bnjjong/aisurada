# 2부 18장 완료 — 받아들임 검사

작성일: 2026-10-31

제공 PDF 78~82쪽의 재료·조건을 구현한 실제 WKWebView 하네스를 추가했다. 원본 scripts/parts/host.mjs는 제공되지 않아 native-host/scripts/acceptance.mjs와 acceptance/checks.js로 구성했다. 실행은 native-host에서 pnpm test:acceptance다. Tauri 2의 설정 덧씌우기, withGlobalTauri, --no-default-features를 사용한다.

매 회차 고유 앱 식별자와 임시 HOME·vault·빈 ZDOTDIR·엔진 소켓을 만들고 복원 목록을 심는다. Java 엔진은 직접 띄워 launchd를 건드리지 않는다. 사용자 설정·Keychain은 읽지 않는다. 성공 시 정리하고 실패 시 진단 자료를 남긴다. 선택한 절대 경로에 JSON 보고서도 저장할 수 있다.

24개 페이지 점검과 두 JSON 파일 점검, 총 26개가 실제 macOS WKWebView에서 통과했다. 확인한 항목은 열기 전 거절, workspace, 폴더 생성, vault 열기, 자기 쓰기, 첨부 바이트와 그림 범위, 기본 앱 열기 거절, md 판별·읽기·그림, HTML frame, Java 엔진 왕복·실패·4부 없음, PTY·통계, 빈 비밀 표·모양 검사, 노트 창 목록·복원·거절·열기·제거·닫기와 창 파일이다. 가짜 claude 호출도 0회였다. 원본 하네스 자체를 실행했다는 뜻은 아니다.

별도로 Rust 경계·동작 검사 65개와 Node 설정 검사 3개가 통과했고 macOS .app 빌드도 성공했다. Rust의 ignored IPC probe 하나는 격리된 실제 Java 왕복 검사가 자식 테스트로 호출하는 내부 진입점이다.

PDF의 손 확인 일곱 항목은 전체 완료로 표시하지 않는다. 실제 비밀 표·Keychain, 실제 메뉴, 기본 앱의 여는 갈래, 모니터 조합·yabai 데스크톱, 번들 스킴·md 연결, 개발 jar 선택의 GUI 확인, 편집기의 닫기 요청 저장·Cmd+W는 해당 환경 또는 3부 화면에서 확인해야 한다. 이번 회차는 자동 검증과 번들 생성까지 완료했다.
