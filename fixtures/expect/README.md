# fixtures/expect — 받아들임 묶음

다시 지은 withlog 1부 엔진이 지금 엔진과 같은 답을 내는지 가르는 묶음이다. 실행기가 케이스마다 엔진을 셸 없이 불러 종료 코드와 stdout 바이트를 맞댄다. 규칙의 정본은 「withlog 1부 vault 엔진 설명서」 34장이고, 이 글은 그 계약을 줄여 적는다.

> 이 판은 설명서만 보고 다시 지은 묶음이다. 실행기(`run.mjs`, `lib/`, `selftest/`)는 34.1의 규칙을 따른다. 진짜 묶음의 읽기 레인 460건과 나무 `fixture.v1`, `claude-home.v1`이 이 저장소에 없어서, 레인에는 설명서 원문에 기대값이 글자 그대로 적힌 1부 케이스 29건만 담고, 입력 vault는 다시 짜기 개발 폴더의 픽스처 vault(`trees/rebuild-fixture.v1.json`)를 담았다. 진짜 묶음을 받으면 `lanes/`와 `trees/`를 바꾸고 `node fixtures/build-bundle.mjs`로 `bundle.json`을 다시 적는다.

## 부르는 법

```text
node fixtures/expect/run.mjs --engine <어댑터 .json> --out <폴더> [--lanes read] [--parts 1,4] [--only <id 머리>,...] [--jobs <n>]
node fixtures/expect/run.mjs --self-check [--mutants] --out <폴더> [--jobs <n>]
```

- 옵션은 `--jobs 4`처럼 이름 뒤 칸에 값을 둔 꼴만 받는다. 모르는 옵션, 값이 빠진 옵션, 다시 준 옵션, 빈 목록 항목, 정수가 아닌 값은 종료 코드 2다.
- `--out`은 늘 필수이고, 있으면 빈 폴더여야 한다. 묶음 폴더나 부모 셸의 `WITHLOG_VAULT`와 겹치면 선다. 겹침은 장치와 inode로 잰다.
- `--lanes`, `--parts`, `--only`의 항목은 항목마다 1건이라도 걸려야 하고, 함께 적용한 결과도 1건 이상이어야 한다.
- `--jobs`의 기본값은 논리 CPU 수에서 2를 뺀 값을 2 이상 8 이하로 묶은 값이다.
- `--self-check`는 받은 argv의 기대값을 그대로 찍는 가짜 구현(`selftest/replay-impl.mjs`)으로 묶음 자신을 돈다. `--mutants`는 그 가짜를 일부러 틀리게 만든 변형 8가지를 실행기가 잡는지 본다.

| 종료 코드 | 뜻 |
|---|---|
| 0 | 고른 케이스가 모두 맞다 |
| 1 | 어긋난 케이스가 있다 |
| 2 | 실행기가 못 돌았다. stderr는 `실행기가 못 돌았다: <까닭>`이다 |

## 회차

1. 옵션 → 묶음 무결성(`bundle.json`의 sha256) → `--out` → `--jobs` → 어댑터와 거르개 → 모든 케이스의 꼴과 자리표 → 거르개마다 걸리는 케이스.
2. 레인, 입력 vault, 입력 HOME, 준비 걸음이 같은 레인 파일을 1벌로 묶고, 1벌마다 `<out>/<레인>-<번호>/` 아래 `vault`, `home`, `state`, `tmp`를 만든다.
3. 입력 나무를 펼치고, 준비 걸음 `["reindex"]`를 부르고(종료 코드 0이어야 한다), 케이스를 `--jobs`만큼 나란히 부른다.
4. 읽기 레인이면 vault와 HOME을 다시 읽어 입력 나무와 바이트로 맞대고 다르면 경고로 남긴다.
5. `<out>/report.json`을 쓰고 stderr에 요약을 찍는다.

구현 환경은 새로 짓는다. 부모 셸에서는 `PATH`와 `WLX_IMPL_`로 시작하는 변수만 넘어가고, `WITHLOG_VAULT`, `HOME`, `TMPDIR`, `WLX_STATE_DIR`, `TZ=Asia/Seoul`, `LANG=LC_ALL=en_US.UTF-8`을 준다. 구현은 제 프로세스 그룹으로 뜨고 1번에 120초까지 기다린다.

## 판정

처음 걸린 검사 이름이 남는다: `spawn`, `timeout`, `code`, `wl`(기대값에 `wl`이 있을 때만), `utf8`, `stdout`, `token`(출력 1개 안), `format`(JSON 값은 같은데 바이트가 다름), `token`(케이스 사이).

자리표는 `⟦vault⟧`, `⟦vault-uri⟧`(`encodeURIComponent`), `⟦home⟧`, `⟦token:<vault 상대 경로>⟧` 4가지다. 토큰 값은 계산하지 않고, 공백 글자와 `"`, `\`, `⟦`, `⟧`가 없는 글자 1개 이상인지와 같은 경로의 토큰이 늘 같은 값인지만 본다.

## 어댑터

```json
{
  "format": "wlx-adapter",
  "version": 1,
  "surface": "engine",
  "name": "my-engine",
  "prefix": ["${WLX_IMPL_BIN}"],
  "suffix": []
}
```

`prefix`와 `suffix`의 `${이름}`(`[A-Z_][A-Z0-9_]*`)은 구현 환경에서 채운다. 어댑터는 묶음 폴더 밖에 둔다. 안에 두면 목록에 없는 파일이라 무결성 검사가 선다. `adapters/withlog-jar.json`은 지금 엔진 jar를 부르는 보기다.
