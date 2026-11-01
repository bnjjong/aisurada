# withlog > 1부 vault 엔진 설명서 > 33장 설정과 기기 위 파일

최종 수정: 목요일 10/22 · 22:56

이 장은 엔진과 `wl-cli`가 읽는 설정 키와 환경변수, 그리고 vault 밖 기기 위에 만드는 파일의 자리와 이름을 정한다.

- 엔진 설정 키는 7개이고 모두 `withlog.` 머리다. 명령줄 `--키=값`이 JVM `-D`를 이기고, 그것이 환경변수를 이기고, 그것이 내장 기본값을 이긴다.
- 설정 값은 모드 2가지 모두 엔진이 뜰 때 읽는다. 그래서 serve 모드에서만 쓰는 키도 값이 틀리면 단발 모드가 종료 코드 1로 선다.
- `wl-cli`는 설정 키를 안 읽고 환경변수만 읽는다.
- 기기 위 파일의 이름은 모두 vault 키 1개에서 나오고, 자리는 지금 구현에서 모두 JVM `user.home` 아래다.
- 근거: 묶음의 실행기는 3.1의 「묶음의 환경」 표를 늘 같은 꼴로 준다(「묶음이 잰다」). `--withlog.vault=`, `--withlog.db.home=`, `-Duser.home`은 지금 엔진의 어댑터가 붙이는 꼴이다. 값 읽기와 이름 맞추기는 Spring Framework 6.2.8과 Spring Boot 3.5.3의 소스로, 나머지는 코드 읽기로 확인했다.

---

## 33.1 엔진 설정 키와 우선순위

| 키 | 환경변수 | 기본값 | 값 | 쓰는 모드 | 장 |
|---|---|---|---|---|---|
| `withlog.vault` | `WITHLOG_VAULT` | 없음. 필수 | 폴더 경로 | 단발과 serve | 3.1 |
| `withlog.db.home` | `WITHLOG_DB_HOME` | 빈 글. 그러면 `~/Library/Application Support/withlog/db` | 폴더 경로 | 단발과 serve | 8.2 |
| `withlog.notes.shard.by.month` | `WITHLOG_NOTES_SHARD_BY_MONTH` | 없음. 그러면 디스크가 정한다 | 불리언 | 단발과 serve | 1.3 |
| `withlog.idle.timeout.minutes` | `WITHLOG_IDLE_TIMEOUT_MINUTES` | 10 dev 프로파일은 0 | 64비트 정수 분. 0 이하는 끔 | serve | 3.2 |
| `withlog.vault.autocommit.enabled` | `WITHLOG_VAULT_AUTOCOMMIT_ENABLED` | `true` | 불리언 | 단발과 serve | 31장 |
| `withlog.vault.autocommit.idle.seconds` | `WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS` | `300` | 64비트 정수 초 | serve | 31.1 |
| `withlog.vault.snapshot.enabled` | `WITHLOG_VAULT_SNAPSHOT_ENABLED` | `false` | 불리언 | 단발과 serve | 31.6 |

「쓰는 모드」는 그 값이 동작을 바꾸는 모드다. 값 읽기는 모드 2가지 모두 뜰 때 해서(3.1의 3번), `withlog.idle.timeout.minutes`나 `withlog.vault.autocommit.idle.seconds`에 틀린 값을 주면 단발 모드도 종료 코드 1로 선다.
`withlog.vault.autocommit.enabled`가 단발 모드에 닿는 자리는 `commit-vault` 1개다. 단발 모드는 그 명령 말고는 커밋하지 않는다(3.1).

키 이름을 점으로만 잇고 하이픈을 안 쓰는 까닭은 환경변수 이름 때문이다. 점을 밑줄로 바꾸고 대문자로 올린 이름이 곧 환경변수인데, 키에 하이픈을 쓰면 환경변수 이름이 정해 둔 이름과 어긋난다.

설정 키가 아닌 환경 입력도 엔진에 닿는다.

| 입력 | 읽는 쪽 | 뜻 |
|---|---|---|
| `WITHLOG_SIDECAR_SOCKET` | serve 모드 | 소켓 자리(3.2). 설정 키로는 못 주고 환경변수로만 읽는다 |
| `TZ` | 엔진 | 프로세스 시간대. 「오늘」과 `+09:00` 딱지 규칙은 7.5에 있다. 묶음은 `Asia/Seoul`을 준다 |
| `LANG`, `LC_ALL` | 지금 엔진의 JVM | 단발 모드 stdout의 인코딩을 정한다(3.1). 다시 짓는 구현은 이 값과 상관없이 UTF-8로 쓴다 |
| `PATH` | 엔진 | vault git(31장)이 `git`을 이 경로에서 찾는다. launchd가 띄운 엔진은 launchd의 최소 `PATH`를 받는다 |
| JVM `user.home` | 지금 엔진 | `~`의 뜻(33.3) |
| JVM `java.io.tmpdir` | 지금 엔진 | SQLite 드라이버가 자기 네이티브 라이브러리를 푸는 자리라는 기록이 저장소 스크립트에 있다. 묶음은 `TMPDIR`과 맞춘다 |
| `SPRING_PROFILES_ACTIVE`, `--spring.profiles.active=` | 지금 엔진 | 프로파일(아래). JVM 구현만의 입력이다 |
| `SPRING_APPLICATION_JSON`, `--spring.application.json=` | 지금 엔진 | JSON으로 준 설정. 우선순위는 아래 1번과 2번 사이다. JVM 구현만의 입력이다 |

**우선순위.** 높은 것부터 이렇다. 앞이 값을 주면 뒤는 안 본다.

1. 명령줄의 `--<키>=<값>` 칸(3.1).
2. JVM 시스템 속성 `-D<키>=<값>`.
3. 운영체제 환경변수.
4. 실행 파일 안의 내장 설정. `dev` 프로파일이 켜지면 그 절이 기본 절을 이긴다.
5. 위 표의 기본값.

`dev` 프로파일은 지금 엔진에서 `SPRING_PROFILES_ACTIVE=dev`나 `--spring.profiles.active=dev`로 켜고, 그 절에는 `withlog.idle.timeout.minutes: 0` 1줄만 있다. 개발에는 launchd가 없어서 1번 죽으면 아무도 되살리지 않기 때문이다. 저장소의 어느 실행 스크립트도 이 프로파일을 안 켠다.

환경변수가 내장 설정을 이기는 까닭은 엔진을 띄우는 쪽(launchd plist와 실행 스크립트)이 어느 vault를 맡을지 알기 때문이다. 내장 설정에 vault 기본값을 안 적는 까닭은, 그 값이 환경변수와 겨루는 자리가 생기면 사람이 환경변수를 바꿔도 안 바뀌는 일이 생기기 때문이다. 틀린 폴더를 가리켜도 빈 vault처럼 조용히 성공하면 노트를 잃은 화면과 구분이 안 돼서, vault는 기본값 없이 필수로 둔다.

**JVM이 아닌 구현.** 2번(`-D`)과 4번의 프로파일, `SPRING_APPLICATION_JSON`은 Spring과 JVM이 준 입력이다. 앱, plist, 묶음, 실행 스크립트 어느 것도 이 입력을 안 써서(코드 읽기로 확인), JVM이 아닌 구현은 받지 않아도 된다. 지킬 것은 명령줄 1번, 환경변수 3번, 기본값 5번이고, 유휴 0을 원하는 개발 실행은 환경변수나 명령줄로 0을 준다.

**이름 맞추기.** 지금 엔진은 Spring Boot의 느슨한 이름 맞추기를 그대로 쓴다. 저장소가 잰 것은 2가지다. 실행 스크립트의 기록이 임시 vault 2개로 재현했다.

- `--WITHLOG.VAULT=/x`처럼 대소문자가 다른 명령줄 이름도 `withlog.vault`로 맞춘다.
- 같은 명령줄 안에 정확한 이름과 느슨한 이름이 함께 있으면 정확한 이름이 이긴다.

그 밖의 규칙은 Spring Boot 3.5.3의 소스로 읽은 것이고 재지 않았다. 명령줄과 `-D`의 이름은 `.`으로 나눈 조각마다 맞대고, 조각 수가 같아야 한다. 빈 조각과 맞출 글자가 1개도 없는 조각은 처음부터 버려서, `withlog..vault`도 조각 2개다. 조각 안에서는 자바의 1글자 소문자 바꾸기를 한 뒤 ASCII `a`부터 `z`와 `0`부터 `9`가 아닌 글자를 모두 건너뛰고 맞댄다. 그래서 `-`뿐 아니라 `_`와 한글 같은 글자도 무시되고, `---withlog.vault=/v`(이름은 `-withlog.vault`)도 맞는다. 같은 출처 안에서는 정확한 이름을 먼저 찾고, 없을 때만 느슨한 이름을 본다. 느슨한 이름 2개(`--WITHLOG.VAULT=/a`, `--Withlog.Vault=/b`)가 정확한 이름 없이 함께 오면 쉼표로 잇지 않고 1개만 쓰는데, 어느 쪽인지는 Spring Boot 안의 해시 집합 차례가 정해서 이 설명서는 정하지 않는다. 같은 느슨한 이름을 여러 번 주면 3.1처럼 쉼표로 잇는다. 환경변수는 `WITHLOG_VAULT`처럼 키를 대문자로 올리고 `.`을 `_`로 바꾼 이름을 먼저 찾고, 그다음 소문자 이름(`withlog_vault`)을 찾고, 그래도 없으면 이름을 `_`로 나눈 조각을 위와 같은 규칙으로 맞대는 변수(`WithLog_Vault` 같은 것)를 본다.

다시 짓는 구현은 부록 D가 정하기 전까지 정확한 이름만 받는다. 묶음, 앱, plist, 실행 스크립트가 모두 정확한 이름만 쓰고, 느슨한 맞추기는 격리를 뚫는 길로만 쓰였기 때문이다(실행 스크립트가 명령 뒤의 `--` 칸을 아예 막는 까닭이 이것이다). 3.1의 vault 문턱만은 지금 엔진도 이 규칙을 안 따르고, `WITHLOG_VAULT`와 글자 그대로의 `--withlog.vault=`만 본다(33.4).

**값 읽기.**

| 경우 | 결과 |
|---|---|
| 모르는 키 | 조용히 버린다. 오타 난 키는 아무 일도 안 한다 |
| 같은 키를 명령줄에 여러 번 | 모든 값을 argv 차례대로 `,`로 이은 글자 1개 |
| `--<키>` (`=` 없음) | 빈 글 |
| `withlog.vault` | 다듬고, 3.1의 경로 정리를 한다. 빈 글이거나 폴더가 아니면 엔진이 못 뜬다 |
| `withlog.db.home` | 공백뿐이면 기본 자리. 아니면 다듬지 않고 경로로 쓰고, 그 아래 `<VaultKey>/meta.db`를 붙여 3.1의 경로 정리를 한다. 상대 경로는 프로세스 작업 폴더 기준이다 |
| 불리언 | 앞뒤의 U+0020 이하 글자를 떼고, 대소문자를 안 가려 `true`, `on`, `yes`, `1`은 참, `false`, `off`, `no`, `0`은 거짓이다. 그 밖이면 엔진이 못 뜬다 |
| 정수 | 자바가 공백으로 보는 글자를 앞뒤와 가운데에서 모두 지운 뒤 읽는다. `+`나 `-` 부호와 앞의 0을 받고 10진수다. `0x`, `0X`, `#`(앞에 `-`만 허용)로 시작하면 16진수다. 밑줄과 소수점은 안 받고, 64비트를 넘거나 숫자로 안 읽히면 엔진이 못 뜬다 |
| 빈 글 | `withlog.db.home`은 기본 자리이고 `withlog.notes.shard.by.month`는 「안 줬다」다. 불리언 2개와 정수 2개는 엔진이 못 뜬다. `withlog.vault`는 3.1이다 |

빈 글 규칙은 환경변수에도 그대로 걸린다. 그래서 `WITHLOG_VAULT_SNAPSHOT_ENABLED=`처럼 빈 글로 정의된 환경변수가 있으면 엔진이 못 뜬다. 「자바가 공백으로 보는 글자」는 3.1의 유니코드 공백에서 U+00A0, U+2007, U+202F를 뺀 글자이고, 정수 읽기 규칙은 Spring Framework 6.2.8의 숫자 읽기다. 「엔진이 못 뜬다」는 3.1의 종료 코드 1이다.

묶음이 다시 짓는 구현에 주는 것은 케이스의 명령과 인자, 그리고 3.1의 「묶음의 환경」이다(「묶음이 잰다」). 구현은 그 환경에서 vault와 색인 자리를 받아야 하고, 받는 꼴은 구현의 어댑터가 정한다(34.5). 지금 `wl-cli`가 쓰는 plist는 serve 모드 엔진에 vault를 환경변수 `WITHLOG_VAULT`로만 주므로(3.4), 지금 `wl-cli`와 함께 쓰일 엔진은 그 환경변수를 읽어야 한다. `--withlog.vault=`와 `--withlog.db.home=`을 명령 뒤에서 받는 꼴은 지금 엔진의 어댑터와 1부 실행 스크립트가 쓰는 꼴이라, 그 스크립트를 그대로 쓰려면 받아야 한다. 쉼표 잇기와 그 밖의 값 규칙을 따를지는 부록 D에서 정한다.

Spring Boot 자신의 키(`logging.level.*`, `spring.*`)도 같은 길로 받는다. `logging.level.*`은 stderr 로그 양만 바꾸고, `spring.*`은 엔진을 다른 꼴로 띄울 수 있다. 2가지 모두 계약이 아니다.

---

## 33.2 `wl-cli` 환경변수

`wl-cli`는 설정 키를 안 읽고 아래 환경변수만 읽는다(코드 읽기로 확인).

| 변수 | 읽는 명령 | 없거나 틀리면 | 값 규칙 |
|---|---|---|---|
| `WITHLOG_VAULT` | 엔진 갈래와 `doctor`는 필수, `sweep`은 선택 | `WL-400 WITHLOG_VAULT 환경변수가 없다. …`, 종료 코드 2 | 다듬는다. `~`를 안 풀고, 상대 경로면 `wl-cli`의 작업 폴더 기준이다. 폴더가 있는지는 안 본다 |
| `CLAUDE_CODE_SESSION_ID` | 엔진 갈래 | 표식을 안 붙인다 | 다듬은 값이 비지 않을 때만 4.4의 표식으로 싣는다 |
| `WITHLOG_SIDECAR_SOCKET` | 엔진 갈래, `doctor`, plist 짓기 | vault 키로 정한 자리 | 다듬어 3.1의 경로 정리를 한다. 103바이트 상한 |
| `WITHLOG_SIDECAR_JAR` | plist 짓기, `install`의 원본, `doctor` | plist는 설치본 `bin/wl-sidecar.jar`, 그것도 없으면 띄울 때 `WL-400`과 종료 코드 2. `install`은 추론(3.5) | 다듬은 글자를 plist에 그대로 싣는다 |
| `WITHLOG_SIDECAR_JAVA` | plist 짓기, `doctor` | `wl-cli`를 돌리는 JVM의 `<java.home>/bin/java` | 다듬은 글자를 그대로 싣는다 |
| `WITHLOG_CLI_JAR` | `install`의 원본, `doctor` | `install`은 추론(3.5), `doctor`는 설치본 `bin/wl-cli.jar` | 다듬는다 |
| `WITHLOG_READY_TIMEOUT_SECONDS` | 엔진 갈래 | `30` | 64비트 양의 정수 초 |
| `WITHLOG_SIDECAR_LIMIT` | 엔진 갈래(띄울 때), `doctor` | `3` | 32비트 양의 정수 |
| `WITHLOG_SWEEP_QUIET_HOURS` | 엔진 갈래(하루 청소), `sweep` | `24` | 64비트 양의 정수 시간 |
| `WITHLOG_AGENT_SOCKET` | `doctor`의 4부 조각 | 4부 규칙의 자리 | 32.4 |

숫자 3개는 다듬은 뒤 정수로 읽는다. `+`나 `-` 부호와 앞의 0을 받아서 `+30`과 `030`은 30이다. 자릿수는 자바가 10진 숫자로 보는 글자이고, 전각 숫자 같은 유니코드 숫자도 받는다. 범위를 넘치거나, 숫자가 아니거나, 0 이하면 기본값으로 물러선다. 틀린 값을 알리지는 않는다. `WITHLOG_READY_TIMEOUT_SECONDS`가 그렇게 하는 까닭은 그 값이 성능 손잡이일 뿐 결과를 안 바꾸고, 오타 때문에 엔진을 못 띄우면 고칠 것은 환경변수인데 증상은 「백엔드가 죽었다」로 보이기 때문이다. 나머지 2개가 알리지 않는 까닭은 코드에 적혀 있지 않다.

`WITHLOG_SIDECAR_JAR`는 설치본보다 우선이라서, 셸에 넣어 두면 설치본이 영영 안 쓰이고 그 값이 plist에 굳는다. 급할 때 1번 못박는 용도다.

앱은 `wl-cli`를 띄울 때 `WITHLOG_VAULT`를 열린 vault로 덮고, `WITHLOG_SIDECAR_JAR`는 찾은 엔진 jar로 덮거나 지운다. 앱 자신의 환경에 그 값이 있으면 그 값을 넘기고, 없으면 개발 빌드와 빌드 트리 안의 번들에서는 지워서 `wl-cli`가 설치본을 쓰게 한다. 빌드가 덮는 jar 자리가 plist에 박히면 개발 회차가 곧 배포가 되기 때문이다. 앱은 뜰 때 Claude 셸에서 물려받은 `CLAUDE_CODE_SESSION_ID`를 자기 환경에서 지운다. 앱 자신은 `WITHLOG_CLI_JAR`와 `WITHLOG_SIDECAR_JAVA`로 `wl-cli.jar`와 자바를 찾고, 호출 1번을 120초까지 기다린다. 이 문단은 2부의 몫이고 이음매로만 적는다.

예전에 있던 `WITHLOG_BACKEND`, `WITHLOG_LEGACY_CLI`, `WITHLOG_LEGACY_NODE`는 옛 구현과 함께 지워져서 지금 아무것도 안 한다. 리포트 잡(`report`, `schedule`)의 환경변수는 0.1에 따라 안 다룬다.

**공백을 떼는 규칙.** 자리마다 비었다고 보는 판정과 떼는 글자가 다르다. 「유니코드 공백」은 3.1의 정의다.

| 자리 | 비었다고 보는 판정 | 떼는 글자 |
|---|---|---|
| 엔진의 vault 문턱(`WITHLOG_VAULT`) | 공백뿐 | 떼지 않고 판정만 한다 |
| 설정 `withlog.vault` | 다듬은 뒤 빈 글 | 앞뒤의 유니코드 공백 |
| 설정 `withlog.db.home` | 공백뿐이면 기본 자리 | 안 뗀다 |
| 불리언 설정 | 뗀 뒤 빈 글 | 앞뒤의 U+0020 이하 글자 |
| 정수 설정 | 뗀 뒤 빈 글 | 자바가 공백으로 보는 글자를 앞뒤와 가운데에서 모두 |
| `wl-cli`의 `WITHLOG_VAULT` | 공백뿐 | 앞뒤의 유니코드 공백 |
| `WITHLOG_SIDECAR_SOCKET`(엔진과 `wl-cli`), `WITHLOG_AGENT_SOCKET` | 공백뿐이면 기본 자리 | 앞뒤의 유니코드 공백 |
| `CLAUDE_CODE_SESSION_ID` | 다듬은 뒤 빈 글 | 앞뒤의 유니코드 공백 |
| `WITHLOG_SIDECAR_JAR`, `WITHLOG_SIDECAR_JAVA`, `WITHLOG_CLI_JAR` | 다듬은 뒤 빈 글이면 없음 | 앞뒤의 유니코드 공백 |
| 숫자 환경변수 3개 | 다듬은 뒤 숫자가 아니면 기본값 | 앞뒤의 유니코드 공백 |
| `install`의 `--cli`, `--sidecar` 다음 칸 | 다듬은 뒤 빈 글이거나 `--`로 시작하면 없음 | 앞뒤의 유니코드 공백 |
| 시각 파일 `last-sweep` | 다듬은 뒤 ISO 순간이 아니면 없음 | 앞뒤의 유니코드 공백 |

---

## 33.3 기기 위 파일과 JVM `user.home`

**vault 키.** 기기 위 파일의 이름은 vault마다 vault 키 1개에서 나온다(8.2와 같은 값).

1. vault 경로를 3.1의 규칙으로 정리한다. 절대 경로로 바꾸고 `.`과 `..`을 정리하며, 심볼릭 링크는 안 풀고 유니코드 정규화도 안 한다.
2. 그 글자의 UTF-8 바이트를 SHA-256으로 해시해 소문자 16진수 앞 16자를 얻는다. 이것이 「해시 16자」다.
3. 마지막 경로 조각(폴더 이름)을 로캘 없이 소문자로 바꾼다. 마지막 조각이 없는 `/`는 빈 글이다. 소문자 바꾸기는 JDK 21의 `String.toLowerCase(Locale.ROOT)`와 같은 전체 대응(여러 글자 대응 포함, 유니코드 15.0)이다.
4. 그 글의 UTF-16 단위마다 ASCII `a`부터 `z`와 `0`부터 `9`가 아니면 `-`로 바꾸고, 이어진 `-`를 1개로 줄이고, 앞뒤 `-`를 떼고, 앞 40자로 자르고, 끝 `-`를 다시 뗀다. 비면 `vault`다.
5. 키는 `<해시 16자>-<4번 결과>`이고, 모양은 `^[0-9a-f]{16}-[a-z0-9-]+$`이다.

소문자로 바꾸면 ASCII가 되는 글자가 2개 있어서 4번의 결과가 갈린다. U+212A(켈빈 기호)는 `k`가 되고, U+0130(점 있는 대문자 I)은 `i`와 U+0307 2글자가 되어 `i-`를 남긴다. 아래 예시는 이 규칙으로 계산해 맞춘 값이다. 경로의 한글은 NFC로 적은 글자다.

| vault 경로(예시 값) | vault 키 |
|---|---|
| `/Users/me/vault` | `17dc9196931802ce-vault` |
| `/Users/me/내 vault` | `1d02e04897d42d35-vault` |
| `/Users/me/노트` | `19d21c43bfe07f90-vault` |
| `/Users/me/My.Vault (copy)` | `a96d9f6b00188e4b-my-vault-copy` |
| `/Users/me/İstanbul Notes` | `db74ba7b0e906010-i-stanbul-notes` |
| `/Users/me/Kelvin (첫 글자가 U+212A)` | `8752c77aef4c9417-kelvin` |
| `/` | `8a5edab282632443-vault` |

해시를 앞에 두는 까닭은 백업 사본처럼 이름이 같은 vault 여럿이 같은 색인을 나눠 쓰면 안 되기 때문이다. 폴더 이름을 뒤에 붙이는 까닭은 사람이 Finder에서 어느 vault의 폴더인지 알아보게 하려는 데 있다. vault 안 파일에 UUID를 박지 않는 까닭은 `cp -R`로 복사한 사본이 같은 UUID를 들고 가서 vault 2개가 같은 색인을 번갈아 덮기 때문이다. 경로에서 키를 뽑으면 사본은 저절로 제 색인을 갖는다.

**파일 목록.** 모두 `~` 아래이고, 지금 구현의 `~`는 JVM `user.home`이다.

| 자리 | 이름 | 쓰는 쪽 | 지우는 때 | 장 |
|---|---|---|---|---|
| `~/Library/Application Support/withlog/db/<VaultKey>/` | `meta.db`. WAL 모드라 SQLite가 `meta.db-wal`, `meta.db-shm`을 곁에 둔다 | 엔진(단발과 serve) | 안 지운다. vault를 옮기면 옛 폴더가 고아로 남는다 | 8.2 |
| `~/Library/Application Support/withlog/run/` | `<VaultKey>.json` | serve 모드 엔진 | 정상으로 내려갈 때. 강제로 죽으면 남고, 같은 vault의 새 엔진이 덮거나 하루 청소가 문턱(기본 24시간) 넘게 조용한 것을 지운다 | 3.2 |
| 같은 폴더 | `<VaultKey>.json.tmp` | serve 모드 엔진 | 이름 바꾸기 직전까지만 산다 | 3.2 |
| 같은 폴더 | `<VaultKey>.lock` | serve 모드 엔진 | 안 지운다. 크기 0 | 3.2 |
| `~/Library/Application Support/withlog/logs/` | `sidecar-<VaultKey>.log` | launchd(엔진의 stdout과 stderr) | 안 지운다 | 3.4 |
| `~/Library/Application Support/withlog/bin/` | `wl-cli.jar`, `wl-sidecar.jar`. 설치 중에만 `wl-cli.jar.installing`, `wl-sidecar.jar.installing` | `wl-cli install` | 안 지운다 | 3.5 |
| `~/Library/Caches/withlog/` | `wl-<해시 16자>.sock` | serve 모드 엔진 | 받기를 멈출 때. 남은 것은 다음 serve가 bind 전에 지운다 | 3.2 |
| 같은 폴더 | `last-sweep` | `wl-cli` | 안 지운다 | 3.5 |
| `~/Library/LaunchAgents/` | `co.kcd.withlog.sidecar.<VaultKey>.plist` | `wl-cli` | `doctor --prune`이 vault 자리가 폴더가 아닌 것만 | 3.4 |
| `~/Library/Caches/withlog/` | `wl-agent-<해시 16자>.sock`과 `.log` | 4부 런타임 | 1부 몫이 아니다. `doctor`가 상태만 본다 | 32.4 |
| `~/.claude/projects/` | Claude Code 세션 전사 | Claude Code | 4부 명령이 읽기만 한다 | 32장 |

모든 폴더와 파일의 권한 비트는 프로세스의 umask를 따른다(3.2). 소켓을 `Caches`에 두는 까닭은 재부팅마다 새로 만들어져 시스템이 언제 비워도 되기 때문이다. 실행 상태 파일과 색인을 `Application Support`에 두는 까닭은 유령을 찾는 단서와 다시 짓는 데 시간이 드는 파생물을 시스템이 임의로 비우면 안 되기 때문이다. 시각 파일 `last-sweep`이 `run/`이 아니라 `Caches`에 있는 까닭은, 잃어도 청소가 1번 더 도는 것뿐이고 `run/`에 두면 유령 목록에 설명 안 되는 이름이 늘기 때문이다. 색인을 vault 밖에 두는 까닭은 1.1에 있다.

`WITHLOG_SIDECAR_SOCKET`은 소켓 자리를, `withlog.db.home`은 색인 뿌리를 옮긴다. 나머지 자리를 옮기는 설정은 없고 `~`만 옮긴다. `~/Library/Application Support/withlog/` 아래에는 리포트 잡의 파일도 함께 사는데 0.1에 따라 안 다룬다.

**`user.home`.** 지금 엔진과 `wl-cli`는 `~`를 JVM 속성 `user.home`으로 짓는다. JVM은 그 값을 `HOME` 환경변수가 아니라 운영체제 계정에서 얻는다(묶음 README가 적는 사실이다). 그래서 지금 구현의 `~`를 옮기려면 JVM 인자 `-Duser.home=<폴더>`를 준다.

- 엔진에서 `~`에 매인 자리는 색인 기본 자리, `run/` 폴더, 소켓 기본 자리, 4부의 전사 자리다.
- `wl-cli`에서는 그 모두와 `LaunchAgents`, `logs/`, `bin/`, `last-sweep`, launchd uid다.
- launchd 도메인의 uid는 `~` 폴더 주인의 uid다(3.3). `user.home`을 임시 폴더로 옮겨도 그 폴더의 주인이 같은 사용자면 launchd 도메인은 그대로 진짜 사용자의 도메인이다.

묶음의 실행기는 `HOME`을 입력 HOME으로 바꿔 주고, 지금 엔진의 어댑터는 JVM에 `-Duser.home=[HOME]`과 `-Djava.io.tmpdir=[TMPDIR]`를 준다. 안 주면 4부 세션 케이스가 실행하는 사람의 진짜 `~/.claude/projects`를 읽기 때문이고, 묶음 README가 다시 짠 JVM 구현에도 같은 인자를 요구한다. JVM이 아닌 엔진은 `HOME` 환경변수의 폴더를 `~`로 써야 묶음과 맞는다(34.5의 8번). 다시 짓는 `wl-cli`는 계정 홈을 쓴다(3.3, 32.4). 계정 홈은 보통 `HOME`과 같아서 엔진과 `wl-cli`가 같은 `~`를 본다. 2개가 갈리는 때는 계정 홈과 다른 `HOME`으로 serve 모드 엔진을 띄운 때뿐이고, 그때는 실행 상태 파일과 기본 소켓 자리가 서로 달라져 동시 상한과 다시 띄우기가 그 엔진을 못 본다. launchd가 띄운 엔진의 `HOME`이 무엇인지는 재지 않았다. 지금 JVM 엔진은 `HOME`과 상관없이 계정 홈을 써서 이 갈림이 없다. 읽기 레인은 vault와 `HOME`을 바꾸면 안 되고, 색인은 `WLX_STATE_DIR` 아래에 둔다(34장).

---

## 33.4 얼어 있는 특이점

다시 지을 때 지킬지 고칠지는 부록 D에서 정한다.

1. vault 문턱(3.1)은 `WITHLOG_VAULT`와 글자 그대로의 `--withlog.vault=`만 본다. 그래서 `-Dwithlog.vault=/v`만 주거나 `--WITHLOG.VAULT=/v`만 주면, 값 고르기는 그 값을 받을 수 있는데도 「환경변수가 없다」로 종료 코드 2다.
2. launchd가 띄운 serve 모드 엔진은 plist의 `WITHLOG_*` 2개만 받는다. 셸의 `WITHLOG_DB_HOME`, `WITHLOG_NOTES_SHARD_BY_MONTH`, `WITHLOG_IDLE_TIMEOUT_MINUTES`, `WITHLOG_VAULT_AUTOCOMMIT_*`는 단발 모드에만 닿아서, 같은 기기에서 단발 모드와 serve 모드의 설정이 갈릴 수 있다. launchd 도메인에 따로 심은 환경변수가 닿는지는 재지 않았다.
3. 설정 이름을 대소문자와 ASCII 영숫자가 아닌 글자를 무시하고 맞추고, 같은 출처 안에서는 정확한 이름이 이긴다(33.1). 느슨한 이름 2개가 함께 오면 어느 쪽이 이기는지 정해지지 않는다.
4. 불리언은 `on`, `off`, `yes`, `no`, `1`, `0`도 받는다.
5. 빈 글로 정의된 환경변수가 불리언이나 정수 키를 가리키면 엔진이 못 뜬다.
6. 정수 설정은 가운데의 공백까지 지우고 읽어서 `1 0`은 10이고, `#1F` 같은 16진수도 받는다.
7. `withlog.db.home`은 다듬지 않아서 앞에 공백이 든 값은 작업 폴더 아래의 이상한 상대 경로가 된다. launchd가 띄운 엔진의 작업 폴더는 재지 않았다.
8. `withlog.vault.autocommit.idle.seconds`가 0이나 음수면 쓰기 뒤 첫 정비(5초 안)에서 곧바로 커밋한다고 코드로 읽힌다(31.1). 실행해 보지는 않았다.
9. `WITHLOG_READY_TIMEOUT_SECONDS`, `WITHLOG_SIDECAR_LIMIT`, `WITHLOG_SWEEP_QUIET_HOURS`는 틀린 값을 알리지 않고 기본값을 쓴다. 너무 큰 값은 다른 실패가 된다(3.8의 21번과 22번).
10. vault 키는 경로 글자로 짓는다. 심볼릭 링크로 들어간 경로, 대소문자만 다른 경로, `/tmp`와 `/private/tmp`, 한글 이름의 NFC와 NFD는 같은 폴더라도 키가 달라서 색인, 잠금, 소켓, 레이블이 모두 따로 생기고 잠금이 서로를 못 막는다.
11. `wl-cli`는 `WITHLOG_VAULT`의 `~`를 풀지 않는다. 따옴표 안에 적은 `~/vault`는 작업 폴더 아래의 `~` 폴더로 읽힌다.
12. 셸의 `WITHLOG_SIDECAR_JAR`가 설치본을 이겨서 그 값이 plist에 굳는다(33.2).
