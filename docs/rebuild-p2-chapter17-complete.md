# 2부 17장 완료 — 기기 파일과 개발 판

작성일: 2026-10-31

제공 PDF 75~77쪽의 파일·모드·환경 목록을 구현과 대조했다. 호스트가 앱 데이터 폴더에 직접 쓰는 파일은 main-window, note-windows, window-layouts 세 JSON이며, debug_assertions에 따라 .dev.json과 .json을 구분한다. 각각 2칸 JSON과 같은 폴더 임시 파일 뒤 이름 바꾸기를 사용하고, 못 읽은 파일은 빈 값으로 본다.

번들 자원 탐색은 Tauri의 custom-protocol 판정, 창 파일 이름은 Rust 디버그 판정으로 분리한다. 따라서 --debug 번들도 번들 jar를 찾고 .dev.json을 쓴다. vault 첨부와 새 워크스페이스 폴더만 사용자가 선택한 자리에 쓰며 WebKit 저장소를 호스트가 직접 다루지 않는다.

jar·java·HOME·USER·TERM·PATH와 Claude/cmux 정리의 용도를 README에 정리했다. 4부 session-images와 런타임은 만들지 않는다. 실제 검사 회차는 별도 앱 식별자와 임시 HOME으로 파일을 고립했다. 디버그 macOS .app 빌드와 자원 확인을 수행했으며, 앱 설치·launchd 등록은 하지 않았다.
