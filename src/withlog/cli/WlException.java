package withlog.cli;

/**
 * withlog 엔진의 규격화된 실패(WL 에러)를 나타내는 예외 클래스.
 * 설명서 6.5에 따라 WL 번호와 설명 문구를 포함하며, 종료 코드를 결정합니다.
 */
public class WlException extends RuntimeException {
    private final int wlCode;

    /**
     * WL 예외 생성자.
     * @param wlCode WL 오류 번호 (400, 404, 409, 412, 500 등)
     * @param message 오류 설명 문구
     */
    public WlException(int wlCode, String message) {
        super(message);
        this.wlCode = wlCode;
    }

    /**
     * WL 코드 번호를 반환합니다.
     * @return WL 오류 코드 번호
     */
    public int getWlCode() {
        return wlCode;
    }

    /**
     * 이 WL 예외에 대응하는 프로세스 종료 코드를 반환합니다.
     * WL-412는 5, 그 외(400, 404, 409, 500 등)는 3입니다.
     * @return 종료 코드 정수
     */
    public int getExitCode() {
        if (wlCode == 412) {
            return 5;
        }
        return 3;
    }

    /**
     * stderr에 출력할 규격 WL 줄(WL-xxx ...)을 반환합니다.
     * @return WL 출력 문자열
     */
    public String toWlLine() {
        return "WL-" + wlCode + " " + getMessage();
    }
}
