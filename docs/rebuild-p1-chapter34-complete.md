# withlog 1부 다시 짜기 34장 완료 — 받아들임 검사

작성일: 2026-10-30

설명서 34장의 받아들임 묶음 `fixtures/expect`를 설명서만 보고 다시 지었다. 실행기(`run.mjs`)와 라이브러리 3개, 자가 점검용 가짜 구현, 변형 8가지, 이 엔진의 어댑터, 실행기를 재는 시험이다. 기준 판은 33장 커밋 `c38dad4`이다.

---

## 요약

| 항목 | 결과 |
|---|---|
| 범위 | 「실행기 다시 짓기」. 진짜 묶음(읽기 레인 460건, 나무 `fixture.v1`, `claude-home.v1`)과 지금 엔진 jar가 이 기기에 없어서, 실행기는 34.1~34.5대로 짓고 레인은 설명서 원문에 기대값이 글자 그대로 적힌 케이스로만 채웠다 |
| 묶음 | 레인 9파일 20건(성공 11건, 실패 9건), 입력 vault 나무 1벌(파일 143개, 폴더 140개), 보기 어댑터 1개 |
| 이 엔진 | 20건 모두 맞음. 실패 9건의 WL 번호도 6.5의 표와 같고, 준비 걸음과 케이스가 vault와 HOME을 바꾸지 않음(경고 0건) |
| 자가 점검 | 가짜 구현 20건 맞음, 변형 8가지 모두 잡음 |
| 실행기 시험 | `fixtures/runner-test.mjs` 57건 통과 |
| 엔진 소스 | 안 바꿈. 34.5의 계약 9가지를 이미 지켰다(아래 표) |

---

## 원본 요청

> /Users/df/Downloads/IMG_7774.HEIC … /Users/df/Downloads/IMG_7756.HEIC(사진 19장)
>
> 34장 개발 진행해줘.
>
> 끝나면 커밋 푸쉬도 진행해~

사진 19장은 withlog 앱에서 연 「withlog 1부 vault 엔진 설명서 > 34장 받아들임 검사」 노트다. 첫 장(34장 머리말과 34.1의 파일 표)과 마지막 장(34.5 끝과 34.6)을 로컬 설명서 13098~13561줄과 맞대 보니 같은 글이라 원문을 따랐다.

진짜 묶음이 없어서 범위를 여쭸고, 「실행기 다시 짓기」로 정했다.

---

## 지은 것

| 파일 | 하는 일 |
|---|---|
| `fixtures/expect/run.mjs` | 실행기. 34.1의 13걸음, 옵션 문법, 판정 9가지, `report.json`과 stderr 요약, 신호 처리, `--self-check`와 `--mutants` |
| `fixtures/expect/lib/tree.mjs` | 입력 나무의 꼴 검사, 펼치기, 다시 읽어 맞대기(경고 6갈래), 폴더를 나무로 담기 |
| `fixtures/expect/lib/template.mjs` | 자리표 4가지의 검사와 맞대기, JSON 값 비교(키 차례, 배열 길이, `Object.is`), 기대값 다시 찍기, 토큰 같음 |
| `fixtures/expect/lib/bundle.mjs` | 무결성(sha256, 목록에 없는 파일, 운영체제 파일 빼기), 레인 파일과 케이스의 꼴 |
| `fixtures/expect/selftest/replay-impl.mjs` | 자가 점검용 가짜 구현. 토큰을 엔진과 다른 규칙(`rp-` 머리)으로 짓고, 변형 8가지를 흉내 낸다 |
| `fixtures/expect/bundle.json` | 파일마다 sha256, 케이스 수, 케이스마다 설명서의 출처 줄, 고친 기록 |
| `fixtures/expect/lanes/read/*.json` | 읽기 레인 9파일 |
| `fixtures/expect/trees/rebuild-fixture.v1.json` | 입력 vault. 다시 짜기 개발 폴더의 픽스처 vault를 담았다 |
| `fixtures/expect/adapters/withlog-jar.json` | 지금 엔진 jar를 부르는 보기 어댑터(34.5 그대로) |
| `fixtures/expect/README.md`, `.gitattributes` | 계약을 줄여 적은 글, 줄 끝 변환 끄기 |
| `fixtures/aisurada.adapter.json` | 이 엔진의 어댑터. 34.5의 9번대로 묶음 밖에 둔다 |
| `fixtures/build-bundle.mjs` | 나무를 다시 담고 `bundle.json`의 목록을 다시 적는 도구. 묶음 밖에 둔다 |
| `fixtures/runner-test.mjs` | 실행기를 재는 시험 |
| `.gitignore` | 빌드용 `lib/` 무시 규칙이 `fixtures/expect/lib/`까지 삼켜서 그 폴더만 다시 넣었다 |

### 레인 20건

| 레인 파일 | 케이스 | 설명서 |
|---|---|---|
| `get_doc.json` | `ndfixture-00000004`(34.1의 케이스 그대로), `ndfixture-00000003`, `missing` | 34.1, 11장, 6.5 |
| `get_sub_note.json` | 손자 `snfixture-00000006`, `missing-child` | 11.8, 6.5 |
| `list_docs.json` | `type-research`, `empty-range`, `unknown-agenda` | 9장, 6.5 |
| `list_sub_notes.json` | `ndfixture-00000001`, `ndfixture-00000002`, `missing` | 16장, 6.5 |
| `list_attachments.json` | `ndfixture-00000001`, 3단 경로, `missing` | 18장, 6.5 |
| `list_reports.json` | `all` | 25장 |
| `get_report.json` | `missing` | 34.1, 6.5 |
| `list_templates.json` | `all` | 24.6 |
| `get_template.json` | `t-legacy`, `t-morning`, `missing` | 24.6, 6.5 |

- 성공 11건은 설명서가 「묶음이 잰다」나 실제 값으로 적은 예시 블록을 그대로 옮겼다. 설명서의 vault `/Users/me/vault`는 `⟦vault⟧`와 `⟦vault-uri⟧`로, `precondition` 값은 그 노트 파일 경로의 `⟦token:…⟧`으로 바꿨다.
- 실패 9건은 6.5의 표(「묶음은 1부 실패 9건을 종료 코드 3과 빈 stdout으로 잰다」)의 argv다. 진짜 묶음은 번호를 안 보지만(34.6의 1번), 이 판은 그 표의 WL 번호를 `wl` 칸으로 미리 담았다. 34.3이 적은 증분 2의 일이다.
- `get_template/t-morning` 1건은 실행기의 `stdoutText` 갈래를 묶음 안에서 재려고 글로 담았다. 진짜 묶음의 `stdoutText`는 4부 3건이다.

---

## 34.5 계약과 이 엔진

| 번호 | 계약 | 이 엔진 |
|---|---|---|
| 1 | 실행 파일을 어댑터 `prefix`가 절대 경로로 가리킨다 | `${WLX_IMPL_JAVA}`와 `${WLX_IMPL_JAR}`로 준다 |
| 2 | argv는 `prefix`, 케이스, `suffix` 차례이고 `--` 칸은 설정이다 | `--withlog.vault=${WITHLOG_VAULT}`를 `suffix`로 준다 |
| 3 | vault는 `WITHLOG_VAULT`에서 읽고 그 글자가 자리표와 맞아야 한다 | 20건의 `deepLink`와 경로가 맞았다 |
| 4 | 제 상태는 `WLX_STATE_DIR`에 두고 vault와 HOME은 안 바꾼다 | 색인이 없어 상태가 없고, 회차 뒤 경고 0건 |
| 5 | 같은 vault를 동시에 읽는 부름을 견딘다 | `--jobs` 기본값(논리 CPU 수에서 2를 뺀 값을 2 이상 8 이하로)으로 나란히 불러 맞았다 |
| 6 | 준비 걸음 `reindex`는 0으로 끝난다 | 맞음 |
| 7 | stdout은 로캘과 상관없이 BOM 없는 UTF-8이다 | 엔진이 응답을 바이트로 짓고 `System.out.write`로 그대로 써서 로캘을 안 탄다 |
| 8 | JVM은 `-Duser.home=${HOME}`을 둔다 | 어댑터 `prefix`에 있다 |
| 9 | 어댑터는 묶음 밖에 둔다 | `fixtures/aisurada.adapter.json` |

---

## 설명서가 정하지 않아 짐작한 것

- **실패 문구와 경고 글**: 34.1은 꼴만 적어서(`실행기가 못 돌았다: <까닭>`, `vault <상대 경로>: <까닭>`) 까닭 글은 지었다. 경고는 6갈래(파일이 사라짐, 바이트가 바뀜, 새 파일, 일반 파일이 아닌 것, 빈 새 폴더, 폴더가 사라짐)를 가린다.
- **변형 대상의 토큰 짝**: 34.1은 「토큰 1개인 JSON 객체 1건, 그 토큰을 같이 쓰는 다른 케이스 1건」을 처음 맞는 것으로 고른다고 적는다. 첫 후보의 토큰을 같이 쓰는 케이스가 없을 수 있어서, 짝이 있는 후보 가운데 처음 것을 고른다.
- **자가 점검의 결과 자리**: `<out>/replay/`와 `<out>/mutant-<이름>/`에 저마다 `report.json`을 쓴다. 변형 회차는 그 변형의 대상 케이스만 부른다.
- **`--out` 정리**: 겹침으로 설 때 이번에 만든 폴더만 깊은 차례로 지운다.
- **`--lanes`의 없는 레인**: 레인 폴더가 없으면 「걸리는 케이스가 없다」로 선다.

---

## 시험

| 시험 | 결과 |
|---|---|
| 이 엔진 회차(`--engine fixtures/aisurada.adapter.json`) | 20/20, 경고 0건, 구현 호출 21번 |
| 자가 점검(`--self-check --mutants`) | 20/20, 변형 8가지 모두 잡음 |
| `node fixtures/runner-test.mjs` | 57/57 |

`fixtures/runner-test.mjs`는 시험용 작은 묶음과 대본대로 답하는 가짜 엔진을 임시 폴더에 지어서 이것을 잰다.
- 옵션 문법(모르는 옵션, `--jobs=4`, 값이 빠진 옵션, 다시 준 옵션, 빈 목록 항목, 정수가 아닌 값, 함께 못 쓰는 옵션 등)과 `" 4.0 "`을 정수로 받는 것
- 무결성(sha256이 다름, 목록에 없는 파일, 목록의 파일이 없음, 운영체제 파일은 안 셈)
- `--out`(비어 있지 않음, 폴더가 아님, 묶음 안이고 폴더를 안 남김, `WITHLOG_VAULT` 안과 그 자체, 링크로 돌아 들어감, 대소문자만 다른 경로)
- 거르개(항목마다와 함께 적용한 0건)와 케이스 꼴(stdout 꼴 2개, 모르는 자리표, 겹친 id, 밖을 가리키는 나무 경로), 어댑터(없음, 꼴, 빈 prefix, 없는 `${이름}`, 이름 기본값)와 준비 걸음(못 띄움, 0이 아님)
- 판정 9가지 가운데 `spawn`과 `timeout`을 뺀 7가지의 검사 이름과 `detail`, `report.json`의 키 차례와 `totals`, 요약 줄
- 구현 환경(부모의 `WITHLOG_VAULT`와 다른 변수가 안 닿음, `WLX_IMPL_*`가 넘어감, 빈 stdin, 작업 폴더)과 vault를 바꾼 케이스의 경고
- SIGTERM을 받으면 143으로 끝나고 도는 구현의 프로세스 그룹을 끝내는 것
- 진짜 묶음의 자가 점검과 이 엔진 회차

케이스 수준의 `spawn`은 준비 걸음과 같은 `prefix`를 써서 따로 만들기 어렵고, `timeout`은 120초라서 시험에서 뺐다. 준비 걸음의 못 띄움은 잰다.

---

## 그대로 둔 것

- 진짜 묶음의 읽기 레인 460건과 나무 `fixture.v1`, `claude-home.v1`(4부 세션 케이스용)은 없다. 받으면 `lanes/`와 `trees/`를 바꾸고 `node fixtures/build-bundle.mjs`로 `bundle.json`을 다시 적는다. 개발 폴더의 픽스처 vault는 파일 143개로, 설명서의 `fixture.v1`(파일 149개)과 다르다.
- 34.4의 `fixtures/golden`(함수 단위 규칙 쌍 11파일)은 기대값이 지금 엔진의 합의 값이라 이 기기에서 다시 뜰 수 없어 짓지 않았다.
- 34.3의 증분 2(게이트를 이 실행기로, 읽기 레인이 vault를 바꾸면 어긋남으로)와 증분 3(쓰기 축 319건을 쓰기 레인으로)은 설명서가 아직 정하지 않은 일이다. 다만 실패 케이스의 `wl` 칸은 이 판이 미리 채웠다.
- 34.6의 특이점 8가지는 실행기가 그대로 따른다. 예를 들어 읽기 레인이 vault를 바꿔도 경고로만 남기고, 토큰은 모양과 같음만 본다.

기록은 개발 폴더의 `spec-gaps-19차.md`에 있다.
