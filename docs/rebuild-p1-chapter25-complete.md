# 25장 완료 — 회고 명령 3개

작성일: 2026-10-29

기준은 사용자가 제공한 `withlog 1부 vault 엔진 설명서 25~30장.pdf` 2~15쪽이다. 기존 `list_reports`/`list-reports`, `get_report`/`get-report`를 점검하고 `save_report`/`save-report`를 구현했다.

## 구현

- `cmd/SaveReport.java`: 워크스페이스와 공통 쓰기 전 훑기를 거친 뒤 종류, 날짜 순서로 검사한다. 종류는 다듬고 로캘 없이 소문자로 읽으며 날짜는 ASCII 네 자리 연도의 모양만 검사한다. `2026-02-30`과 `2026-13-45`도 저장할 수 있다.
- 본문과 메모는 받은 글을 보존하고, 공백뿐인 칸은 뺀다. 두 칸 모두 없으면 파일을 지운다. 저장은 기존 파일을 읽지 않고 통째로 덮는 원자적 쓰기이며, JSON 키는 body 다음 memo, 끝 LF는 1개다. 짝 없는 서로게이트는 파일에서 소문자 JSON 탈출로 보존한다.
- `args/ArgsParser.java`: 도구의 필수 문자열 kind/date와 선택 문자열 body/memo를 차례로 검사한다. body/memo는 각각 200,000 UTF-16 단위까지 받고 명령 표면에는 그 상한을 걸지 않는다.
- 기존 읽기의 YAML 1.1, 껍데기 WL-404와 못 읽음 WL-500 구별, nullable 상세 응답, 목록의 incomplete 봉투를 유지했다. 목록의 진단은 `engine/Diagnostics.java`를 통해 소켓 응답의 stderr에도 전달한다.

## 검증

`test/withlog/Chapter25Test.java`의 102개 검증이 `bash test.sh`에 포함된다. 저장 JSON 바이트, 본문 끝 줄바꿈, 전체 교체, 반복 삭제, 달력에 없는 날짜, BOM과 YAML, 잘못된 타입, 탭·슬래시 탈출·제어 문자, YAML 줄 접기, 200,001단위 거절, 1MB 초과 저장 후 읽기 실패, 검증 순서와 소문자로 바꾸기 전 오류 표시를 확인했다. 실제 serve와 wl-cli에서도 회고 저장·읽기·봉투 진단을 왕복했다.

## 유지한 경계

25.9의 미결정 호환 동작을 유지한다. 날짜는 달력을 보지 않고, 저장은 읽기에서 WL-500인 파일도 덮으며, YAML 해석 때문에 일부 제어 문자와 줄 구분 문자의 왕복 결과가 달라진다. 자동 회고 생성 잡은 PDF가 정한 대로 범위 밖이다.

## 전체 검증 근거

`bash test.sh`의 전체 1,660개 검증이 통과했다. 장별 수와 실제 serve·wl-cli 40개 왕복 검증, PDF 예시 픽스처 사본의 대조 결과는 [검증 기록](review-repros/chapter25-30-verification-2026-10-29.json)에 담았다. 원본 Kotlin 엔진과 차등 실행한 결과는 아니며, 명세를 기준으로 Java 구현을 검증한 결과다.
