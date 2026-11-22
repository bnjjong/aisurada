//! 골든 시험: 기준(옛 Java 엔진) 없이 맞대기 하네스 셋(`compat_check`, `cli_compat`, `mcp_compat`)을 녹화본과 견준다.
//!
//! 녹화본(`engine-rs/golden/*.ndjson`)은 Java와 Rust가 어긋남 0이던 순간의 기준 응답과 폴더 해시다. 같은 하네스를 기준 엔진 없이
//! 다시 돌려 응답과 파일 바이트를 견준다. TSK-9에서 생성·수정 시각을 담는 응답과 파일, 이에 영향을 받는 해시를 갱신했다.
//! TSK-10에서 프로젝트 응답과 파일에 유형을 추가한 변경도 반영했다.
//!
//! Java와 일치하는 녹화본은 `engine-rs/tools/golden/run.sh record`로 다시 만들 수 있다(JDK 21 필요).
//! 그 변경은 근거와 함께 커밋 본문에 적는다. 하네스를 읽는 법과 설계는 `tools/golden/run.sh` 머리말과 `examples/support/tape.rs`에 있다.

use std::path::Path;
use std::process::Command;

#[test]
fn rust_engine_matches_the_recorded_oracle_without_java() {
    let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["compat.ndjson", "cli.ndjson", "mcp.ndjson"] {
        assert!(
            engine.join("golden").join(name).is_file(),
            "녹화본 {name}이 없다. `engine-rs/tools/golden/run.sh record`로 만든다(JDK 21 필요)."
        );
    }
    let output = Command::new("bash")
        .arg(engine.join("tools/golden/run.sh"))
        .arg("replay")
        .output()
        .expect("bash를 못 돌렸다");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "골든 재생이 녹화본과 어긋났다.\n{text}"
    );
    // 어긋남 0을 글로도 확인한다(세 하네스 모두 「어긋남 0건」을 찍는다)
    assert!(
        text.matches("어긋남 0건").count() >= 3,
        "세 하네스가 모두 어긋남 0건을 찍지 않았다.\n{text}"
    );
}
