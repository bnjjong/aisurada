# 8-1 완료 검토

검토일: 2026-10-27

결론: 8-1의 핵심 계약 구현과 검증을 확인했으며 진행을 막는 문제는 발견하지 못했다. 8-2 재조정 코디네이터로 진행할 수 있다.

## 확인 범위

- reindex 응답 필수 필드·정수·scanned 관계, skipped 6종·conflicts 2종·reason 부재, 전체 배열 원소 검사.
- reindex 호출의 빈 argv와 full 모드 제한, reindex-paths의 원문 argv·빈 입력·full/paths 응답 수용.
- 순수 validator와 기존 decodeResponse 조합, 추가 키와 원본 응답 보존.
- 공개 export, README와 사용 예시 테스트, 명세/합성 fixture의 출처 구분.

## 실행 결과

- npm test: 12개 파일·175개 테스트 통과.
- npm run build: TypeScript·Vite 빌드 성공.
- 추가 독립 확인: 8.4 명세에서 직접 추출한 JSON 블록+LF와 FIXTURE_REINDEX_FULL_SPEC의 UTF-8 바이트 일치.
- 추가 mock 확인: WL-400·500, serve 3/0의 예상 밖 실패, in_flight 전송 실패의 분류·원문·복구 지침 전달.

타입 선언의 필드 순서 자체가 실제 JSON 바이트를 강제하지는 않는다. 이번 확인은 소비 계층과 fixture 계약이며, 실제 엔진·SQLite·파일 감시·공식 수용 묶음 검증은 아니다.

## 문서 반영과 남은 범위

사용자가 지정한 Downloads의 aisurada 엔진 해부도.html에서 8-1 구현 내역을 확인하고 별도 검토 결과를 추가했다. 기존 Java 엔진 본문과 앵커·스크립트를 유지하고 수정 전 백업을 만들었다. 저장소 사본은 docs/artifact-updates/aisurada-updated-2026-10-27.html이다.

8-2와 8-3은 후속 계획이다. 7-1의 미분류 쓰기 실패 및 미정의 종료/WL 조합 보완은 이번 변경과 별개로 남아 있다. 애플리케이션 소스는 수정하지 않았다.
