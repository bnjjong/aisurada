# 2부 9장 완료 — Markdown 열기

작성일: 2026-10-31

제공 PDF 40~42쪽의 커맨드 `resolve_md_target`, `read_external_md`, `allow_external_images`를 구현하고 invoke 처리기에 등록했다. 파일은 실재 경로로 판단하고 확장자는 대소문자를 가리지 않는다. 알려진 vault는 받은 글자의 끝 `/`만 떼어 응답하고, 모르는 vault는 가까운 조상의 workspace 표식으로 찾는다.

노트는 notes/ 아래의 정확한 note.md만 고른다. 머리 8,192바이트에서 좁은 id 문법을 읽고 sub/를 따라 루트부터의 사슬을 만든다. id를 하나라도 못 읽으면 외부 Markdown으로 내린다. 외부 글은 2,097,152바이트 이하의 엄격한 UTF-8이며 BOM과 줄 끝을 보존한다. 상대 그림은 질의·fragment를 떼고 실재 경로가 문서 폴더 아래인 그림만 중복 없이 파일별로 허용한다.

Rust 검사에서 하위 사슬, 머리 BOM·CRLF·따옴표·들여쓰기·id 위치 한도, UTF-8 오류, 크기 초과, 이미지 중복과 심볼릭 링크 탈출을 확인했다. 실제 WKWebView의 md-target·external-md·external-images 점검도 통과했다. 허용한 그림은 asset 주소로 읽히고 다른 폴더 그림은 차단됐다. 허용 범위 누적과 외부 md 경로를 제한하지 않는 기존 동작을 유지한다.
