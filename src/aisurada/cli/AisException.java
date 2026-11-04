package aisurada.cli;

/**
 * aisurada 엔진의 규격화된 실패(AIS 에러)를 나타내는 예외 클래스.
 * 설명서 6.5에 따라 AIS 번호와 설명 문구를 포함하며, 종료 코드를 결정합니다.
 */
public class AisException extends RuntimeException {
    private final int aisCode;

    /**
     * AIS 예외 생성자.
     * @param aisCode AIS 오류 번호 (400, 404, 409, 412, 500 등)
     * @param message 오류 설명 문구
     */
    public AisException(int aisCode, String message) {
        super(message);
        this.aisCode = aisCode;
    }

    /**
     * AIS 코드 번호를 반환합니다.
     * @return AIS 오류 코드 번호
     */
    public int getAisCode() {
        return aisCode;
    }

    /**
     * 이 AIS 예외에 대응하는 프로세스 종료 코드를 반환합니다.
     * AIS-412는 5, 그 외(400, 404, 409, 500 등)는 3입니다.
     * @return 종료 코드 정수
     */
    public int getExitCode() {
        if (aisCode == 412) {
            return 5;
        }
        return 3;
    }

    /**
     * stderr에 출력할 규격 AIS 줄(AIS-xxx ...)을 반환합니다.
     * @return AIS 출력 문자열
     */
    public String toAisLine() {
        return "AIS-" + aisCode + " " + getMessage();
    }
}
