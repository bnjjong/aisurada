# UI 점검 및 개선 백로그

`aisurada-UI-고칠-일-2026-11-10.pdf`(61건)의 검증 및 후속 UI 개선 항목을 기록한다.

## S(스타일·스킨) 묶음

### S17. 일 보기(캘린더 하루 화면) 카드 겹침 및 grid 스킨 투명 카드 결함 해결
- **심각도**: 상 (글자 중첩 및 가독성 훼손)
- **위치**: `native-host/src/ui/CalendarAgenda.tsx`, `native-host/src/styles.css`, `native-host/src/skin-grid.css`
- **원인**:
  1. `lanes()`는 60분(`SLOT = 60`, 48px)으로 자리를 잡으나 카드가 `.note-row`를 상속하여 패딩(18px)과 제목 길이로 92~164px로 늘어나 앞 카드를 덮음.
  2. 하루 전체의 최대 레인 수로 카드를 일괄 분할하여 단독 카드도 154px로 좁아져 제목이 과도하게 접힘.
  3. `skin-grid.css`에 `.tl-block` 배경 규칙이 없어 `.note-row` 투명 규칙이 이겨 카드 바탕이 투명해지고 글자끼리 겹침.
- **조치 사항 (사용자 결정 1-나, 2-가)**:
  1. **시간축 확대 및 높이 고정**: 1분 1.2px (60분 = 72px)로 눈금과 타임라인을 확대하고, 카드 높이를 72px 고정으로 설정.
  2. **제목 3줄 말줄임**: `.tl-block strong`에 `-webkit-line-clamp: 3` 및 `word-break: break-word`를 적용하여 72px 높이 내에서 깔끔하게 정돈.
  3. **클러스터별 동적 칸 수 계산**: `lanes()`에서 연쇄 겹침 묶음별로 칸 수를 계산(`counts`)하여, 단독 일정 카드는 전체 폭(90%)을 넓게 사용하도록 개선.
  4. **클래스 분리 및 스킨 배경 보강**: 카드에서 `.note-row`를 제거하고 `.day-timeline .tl-block`에 좌측 정렬·그림자 제거를 적용하며, `skin-grid.css`에 `.tl-block` 불투명 배경(`var(--soft)`)을 명시하여 전 스킨 겹침 0건 달성.
