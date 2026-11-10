package aisurada.cmd;

/**
 * 설명서 부록 A 및 B 규격: 명령과 도구 표면의 메타데이터를 통합 정의하는 레코드.
 *
 * @param name      명령 이름 (예: "create-doc")
 * @param toolName  도구 이름 (짝 도구가 없으면 null, 예: "create_doc")
 * @param isWrite   쓰기 여부 (쓰기 잠금 대상)
 * @param isPart4   4부(Claude 연동) 명령 여부
 * @param usage     사용법 설명 한 줄
 */
public record CommandSpec(
        String name,
        String toolName,
        boolean isWrite,
        boolean isPart4,
        String usage
) {}
