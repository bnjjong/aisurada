# 2부 1장 구현 — 번들과 설정

작성일: 2026-10-30

제공받은 2부 PDF 12~17쪽을 기준으로 `native-host/`에 Tauri 호스트를 추가했다. 식별자 `co.kcd.withlog`, 제품명, withlog 스킴, md/markdown 연결, 지연 생성 main 설정, 두 CSP, 빈 asset 범위와 main/note 권한을 반영했다. fs/watch, dialog, opener, deep-link의 판은 문서대로 고정했다.

이 저장소의 Java 산출물 경로에 맞춰 자원 원본만 조정했다. 번들에서는 `Resources/wl-cli.jar`, `Resources/wl-sidecar.jar`로 배치한다. 실 비밀 표가 있으면 빌드에 포함하고 없으면 빈 견본을 쓰며, 두 경로 모두 빌드 변경 추적 대상이다. 실 표는 git에서 제외한다.

원본 아이콘과 4부 데몬 소스는 이 저장소에 없어 임시 SVG 아이콘을 만들었고 wl-agent 자원은 포함하지 않았다. 4부 자원이 있는 원본과 이 부분은 다르다. 3부 화면은 아직 제공되지 않아 외부 스타일 파일만 사용하는 최소 화면을 연결했다.

설정·권한·CSP 계약 테스트 3개와 실제 Tauri 컴파일을 통과했다. `pnpm tauri build --debug --bundles app`으로 macOS 번들을 만들고 Info.plist의 식별자·스킴·문서 연결과 Resources의 두 jar SHA-256 일치를 확인했다. 빌드 명령과 후속 범위는 `native-host/README.md`에 적었다. macOS 파일 연결의 기본 앱 선택과 딥링크를 사용자 앱에 등록하는 작업은 수행하지 않는다.
