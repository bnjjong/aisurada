package withlog.frame;

/**
 * 프레임 읽기/검증 실패 예외.
 */
public class FrameException extends Exception {
    private final boolean isProbing;

    public FrameException(String message) {
        super(message);
        this.isProbing = false;
    }

    public FrameException(String message, boolean isProbing) {
        super(message);
        this.isProbing = isProbing;
    }

    public boolean isProbing() {
        return isProbing;
    }
}
