# 2부 4장 완료 — vault 자리 커맨드 네 개

작성일: 2026-10-30

제공받은 PDF 24~27쪽을 기준으로 allow_workspace, vault_root, is_workspace, create_workspace_dir를 구현하고 Tauri invoke 처리기에 등록했다.

allow_workspace는 main 창만 부를 수 있다. 현재 vault 잠금을 끝까지 쥐고 노트 창 유무와 같은 vault인지 검사한 뒤 파일 범위, `.withlog` 파일 범위, asset 범위를 열고 값을 바꾼다. 현재 값은 받은 글자 그대로 보존하며, 처음에는 null이다. 경로 마디 비교나 양쪽 canonicalize 결과로 같은 vault를 판정한다. 노트 창이 열려 있을 때 다른 vault로 바꾸기는 거절한다. 범위 열기에 실패하면 이전 vault 값을 유지한다.

is_workspace는 `.withlog/workspace.json`이 파일인지만 보고 JSON을 읽지 않는다. create_workspace_dir는 절대 부모 경로, 이름 마디, 점으로 시작하는지, 부모 canonicalize, 하위 경로 여부 순서로 검사한다. 이미 있는 폴더는 성공하고 파일은 실패한다. 앞뒤 공백과 `a/`, `a//`, `a/.`의 반환 표기를 보존한다. 현재 Rust에서 없는 `a/.`를 직접 create_dir_all 하면 실패해, 실제 생성에는 검사한 경로 마디를 사용하고 응답에는 원래 이름을 붙인 경로를 사용했다.

Rust 테스트 17개와 설정 테스트 3개를 통과했다. 실제 임시 파일·심볼릭 링크와 Tauri MockRuntime의 invoke 처리기를 사용했다. null 응답, 부른 창 제한, 잠금 안의 전환과 실패 처리, 노트 창이 있는 첫 열기·재열기·다른 vault 거절, 파일/asset 범위, `.withlog` 범위, 기존 범위 유지, 없는 폴더 열기, 이름 경계, 파일인 표식, 생성 결과를 검사했다. macOS `.app` 빌드도 확인했다.

문서의 특이점을 유지한다. 범위는 누적되며 vault 값을 지우는 커맨드는 없다. allow_workspace는 존재 여부나 워크스페이스 표식을 검사하지 않는다. 실제 macOS 창과 Finder를 통한 수동 검증 및 18장의 전체 확인 페이지 실행은 하지 않았다.
