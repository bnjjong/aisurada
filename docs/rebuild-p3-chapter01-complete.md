# 3부 1단계: 화면 빌드 기반

「withlog 3부 화면 설명서」의 0~1장을 기준으로 1부·2부 위에 React 화면을 연결했다. 이번 단계는 빌드 기반까지이며 vault 열기와 실제 편집은 아직 구현하지 않았다. 전체 순서는 [구현 계획](rebuild-p3-plan.md)에 있다.

## 구현

- 정적 `web/`과 자체 개발 서버를 React 19, TypeScript, Vite로 교체했다. 진입점은 `native-host/index.html`과 `native-host/src/main.tsx`다.
- `pnpm dev`와 `pnpm build`는 `noclaude` 모드를 쓴다. 현재 4부 구현이 없으므로 직접 production 모드로 빌드해도 4부 기능은 없다. 4부가 있는 대조군 빌드는 제공하지 않는다.
- 개발 서버는 `localhost:1420`에 뜨고 포트 충돌 시 실패한다. `TAURI_DEV_HOST`가 있으면 해당 호스트와 HMR 포트 1421을 사용한다.
- Vite의 `/` base와 SPA fallback을 사용한다. `/note-window/<id>` 직접 진입에도 같은 화면과 `/assets/...` 자원을 읽는다. 주소별 화면 선택과 노트 창 부팅은 후속 단계다.
- Tauri `frontendDist`는 `../dist`다. 기존 `beforeBuildCommand`가 React 화면과 Java jar를 모두 만든 뒤 앱에 담는다.
- 화면은 `@tauri-apps/api`를 import한다. `window.__TAURI__`에 기대지 않고 `isTauri()`로 호스트 유무를 구분한다.
- CSS는 별도 파일로 내보내며 원격 글꼴·그림·스크립트를 사용하지 않는다. 초기 화면은 기기 기본 글꼴과 라이트·다크 스타일을 쓴다.
- preview 서버는 Tauri의 번들 CSP를 응답 헤더로 적용한다. 개발 HMR과 번들 정책을 구분한다.

## 빌드 검사

Vite 빌드 후와 `pnpm check:screen [산출물 폴더]`에서 같은 검사를 실행한다. 다른 `--outDir`로 직접 빌드할 때도 적용된다.

HTML을 파싱해서 명시적인 `<head>`와 `</body>`, 인라인 스타일 0개, 외부 JS·CSS, 절대 자원 주소와 실제 파일을 확인한다. 인라인 이벤트·스크립트와 `<base>`·`<object>`도 거절한다. `assets/`의 하위 폴더까지 JS를 읽어 4부 전용 커맨드 9개가 문자열로 남아도 실패시킨다.

검사기 시험 6개는 정상 산출물 외에 대소문자·template 안의 인라인 CSS, 인라인 JS, 상대·원격·잘못된 확장자·없는 자원, 누락된 HTML 삽입 지점, 하위 worker에 남은 4부 이름 각각을 검사한다. 원본 `screen.mjs`를 대체했다고 주장하는 검사가 아니라 1장의 빌드 경계 검증이다.

## 검증

- `pnpm build`: 타입 검사와 빌드 성공. `index.html` 1개, 외부 JS·CSS 생성. 인라인 스타일 0개, 금지 커맨드 0개.
- `bash test-native.sh`: 화면 검사 6개, 기존 설정 검사 3개, Rust 검사 65개 성공. 격리된 하위 실행용 Rust 검사 1개는 원래대로 ignored다.
- `pnpm tauri build --debug --bundles app`: Java jar와 React 산출물을 포함한 macOS `withlog.app` 생성 성공.
- Ego 브라우저에서 번들 CSP를 단 preview의 `/`와 `/note-window/ndfixture-00000001`을 열어 React 렌더와 자원 로드를 확인했다. 별도의 전역 Tauri 객체 없이 렌더했다.
- 개발 서버의 깊은 주소도 정상 로드했다. 같은 개발 주소에 두 번째 서버를 띄우면 `Port 1420 is already in use`로 실패했다.
- `pnpm test:acceptance`: 기존 2부 실제 WKWebView 점검 26개 성공, Claude 호출 0회. 이 하네스는 자체 점검 화면을 사용하므로 새 React 화면의 실행 검증과 구분한다.

## 남은 범위

현재 화면은 초기 셸이다. 만들기·열기·샘플, 주소 13개와 탭, 편집기, 설정, 각 장의 받아들임 표식은 해당 단계를 구현하면서 붙인다. 원본 `screen.mjs`의 화면 점검이나 합치기 검사를 통과한 상태가 아니다.

브라우저 CSP 확인과 앱 번들 생성까지 검증했다. 생성한 앱을 실제 실행하여 WKWebView와 Tauri가 합성한 CSP를 확인한 것은 아니다. 2부 격리 하네스의 WKWebView 점검은 화면 자체와 별개다.

4부가 있는 production 대조군, 17장의 정확한 꽂이 인터페이스, 터미널 한글 글꼴의 셀 폭은 이번 단계에서 확정하지 않는다.
