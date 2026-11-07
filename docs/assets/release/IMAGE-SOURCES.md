# 배포 가이드 이미지 출처와 생성 기록

사용 문서: [그림으로 보는 배포 가이드](../../release-guide.html)

이미지를 HTML 안에도 포함했습니다. HTML 파일 하나만 복사해도 인터넷 없이 모든 그림을 볼 수 있습니다. 아래 파일은 원본 관리용입니다.

## 이미지별 구분

| 파일 | 종류 | 설명 |
|---|---|---|
| [app-runtime-package.png](app-runtime-package.png) | 설명용 AI 삽화 | 앱·엔진·Java를 하나의 패키지로 전달하는 개념. 실제 앱 화면·제품 사진이 아닙니다. |
| [release-roadmap.png](release-roadmap.png) | 설명용 AI 삽화 | Java 동봉 → 서명·DMG → 설치 검증 → 자동화의 네 장면. 실제 설비·스크린샷이 아닙니다. |
| [github-download-assets.png](github-download-assets.png) | 실제 화면 캡처 | [공개된 v0.0.1 릴리스](https://github.com/bnjjong/aisurada/releases/tag/v0.0.1)의 Assets 영역. 브라우저에서 DMG 행에 초록 강조를 추가한 후 캡처했습니다. 파일명·용량·다운로드 목록은 실제 페이지 내용입니다. |
| [macos-first-launch.svg](macos-first-launch.svg) | 화면 재구성 설명 그림 | 사용자가 겪은 차단 대화상자와 [Apple의 앱별 실행 허용 안내](https://support.apple.com/ko-kr/guide/mac-help/mh40616/mac)를 바탕으로 코드로 그린 그림입니다. 실제 시스템 설정 스크린샷이 아니며, 운영체제별 문구·위치는 다를 수 있습니다. |

생성 방식: **imagegen 내장 도구**. CLI나 별도 API 키를 사용하지 않았습니다.

## 패키지 삽화 생성 프롬프트

```text
Use case: product-mockup. Asset type: wide editorial illustration for a Korean desktop app deployment guide. Create one polished, tactile 3D isometric product illustration that visually explains one self-contained desktop app package traveling to a user's laptop. On an off-white warm background #f6f7f2, show an open forest-green software shipping box at center with three clearly distinct neatly fitted modules: a miniature desktop app window, a small abstract engine/gears module, and a glowing green runtime capsule bearing a simple coffee cup pictogram. All three modules fit together INSIDE the same box. On the right, an elegant silver laptop with an abstract cream-and-green notes app on its screen, simple note lines only, no readable interface text. A subtle curved dotted green path connects the package and laptop, suggesting install and use. Refined premium educational editorial render, soft daylight, paper and matte ceramic materials, subtle shadows, ample breathing room, balanced 16:9 landscape composition. This is a conceptual illustration, not a screenshot or a photograph of an actual product. No logos, no brand marks, no Latin text, no Korean text, no badges, no text anywhere, no people, no watermarks, no decorative clutter. Colors forest green, sage, cream, silver with a small warm amber accent. Readable at half-page width. Produce a real bitmap illustration, not a code diagram.
```

## 배포 과정 삽화 생성 프롬프트

```text
Use case: productivity-visual. Asset type: landscape illustrated four-step software release roadmap for an HTML guide. Create a wide single bitmap containing four clearly separated left-to-right isometric tabletop vignettes on a consistent warm cream (#f6f7f2) studio background. Vignette 1: open forest-green software box holding a miniature application window, a tiny gear and a coffee-cup runtime module fitted together, representing bundle Java. Vignette 2: the same closed box next to a simple stamp and a paper disk sleeve, representing sign and build an installer. Vignette 3: silver laptop displaying an abstract green note window and a small magnifying glass inspecting it, representing install and test. Vignette 4: a compact elegant assembly conveyor with several identical small forest-green software boxes coming out, representing automate repeat builds. Connect adjacent vignettes with three thin sage arrows. Each vignette takes one equal quarter of the image. Generous white space around objects; no panel borders; no confusing extra objects. A refined tactile 3D editorial illustration, matte paper, ceramic, brushed silver, soft daylight, forest green and sage with small muted amber accents. Consistent scale and camera angle across all four vignettes. Landscape aspect ratio about 3:1. Absolutely no letters, numbers, titles, captions, logos, badges, seals of approval, green checkmarks or watermarks. These are conceptual pictures rather than screenshots; actual labels will be added in HTML outside the image.
```
