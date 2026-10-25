package withlog.engine;

/**
 * 엔진 명령 실행 결과 객체.
 * 설명서 3.1 및 4.2에 따라 상태(종료 코드), 오류 코드, stdout 바이트, stderr 바이트를 담습니다.
 */
public record EngineResult(
        int status,
        int errorCode,
        byte[] stdoutBytes,
        byte[] stderrBytes
) {
    public static final byte[] EMPTY_BYTES = new byte[0];

    public static EngineResult success(byte[] stdoutBytes) {
        return new EngineResult(0, 0, stdoutBytes, EMPTY_BYTES);
    }

    public static EngineResult unknownCommand(byte[] stderrBytes) {
        return new EngineResult(2, 0, EMPTY_BYTES, stderrBytes);
    }

    public static EngineResult wlError(int status, int errorCode, byte[] stderrBytes) {
        return new EngineResult(status, errorCode, EMPTY_BYTES, stderrBytes);
    }

    public static EngineResult internalError(int status, int errorCode, byte[] stderrBytes) {
        return new EngineResult(status, errorCode, EMPTY_BYTES, stderrBytes);
    }
}
