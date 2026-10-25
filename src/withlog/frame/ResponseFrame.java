package withlog.frame;

import java.io.*;
import java.nio.ByteBuffer;

/**
 * 설명서 4.2 규격에 따른 소켓 응답 프레임 (WLG1) 읽기 및 쓰기.
 */
public final class ResponseFrame {

    public static final int MAGIC = 0x574C4731; // "WLG1"
    public static final int VERSION = 1;
    public static final int DB_SCHEMA = 5;
    public static final int MAX_BODY_LEN = 67_108_864; // 64 MiB

    private final int dbSchema;
    private final long pid;
    private final int status;
    private final int errorCode;
    private final byte[] stdoutBytes;
    private final byte[] stderrBytes;

    public ResponseFrame(int dbSchema, long pid, int status, int errorCode, byte[] stdoutBytes, byte[] stderrBytes) {
        this.dbSchema = dbSchema;
        this.pid = pid;
        this.status = status;
        this.errorCode = errorCode;
        this.stdoutBytes = stdoutBytes != null ? stdoutBytes : new byte[0];
        this.stderrBytes = stderrBytes != null ? stderrBytes : new byte[0];
    }

    public int dbSchema() { return dbSchema; }
    public long pid() { return pid; }
    public int status() { return status; }
    public int errorCode() { return errorCode; }
    public byte[] stdoutBytes() { return stdoutBytes; }
    public byte[] stderrBytes() { return stderrBytes; }

    /**
     * 엔진이 소켓으로 응답 프레임을 작성합니다.
     * stdout이나 stderr가 64 MiB 초과 시 아무것도 쓰지 않고 닫아야 하므로, 쓰기 전에 검증합니다.
     */
    public void write(OutputStream out) throws IOException {
        if (stdoutBytes.length > MAX_BODY_LEN || stderrBytes.length > MAX_BODY_LEN) {
            // 4.3: 64 MiB 초과 시 응답을 1바이트도 안 쓰고 닫음
            return;
        }

        byte[] header = new byte[24];
        ByteBuffer bb = ByteBuffer.wrap(header);
        bb.putInt(MAGIC);
        bb.putShort((short) VERSION);
        bb.putShort((short) dbSchema);
        bb.putInt((int) (pid & 0xFFFFFFFFL));
        bb.putShort((short) status);
        bb.putShort((short) errorCode);
        bb.putInt(stdoutBytes.length);
        bb.putInt(stderrBytes.length);

        out.write(header);
        if (stdoutBytes.length > 0) {
            out.write(stdoutBytes);
        }
        if (stderrBytes.length > 0) {
            out.write(stderrBytes);
        }
        out.flush();
    }

    /**
     * wl-cli가 응답 프레임을 스트리밍 방식으로 읽어 표준출력과 표준오류로 흘립니다 (4.2).
     * 머리 24바이트를 4 · 2 · 10 · 4 · 4 바이트로 나누어 받고, 받은 칸을 그 자리에서 즉시 잽니다.
     *
     * @param in 소켓 입력 스트림
     * @param stdoutPipe 표준출력으로 바이트를 흘릴 스트림
     * @param stderrPipe 표준오류로 바이트를 흘릴 스트림
     * @return HeaderInfo (status, errorCode, pid, dbSchema)
     */
    public static HeaderInfo streamRead(InputStream in, OutputStream stdoutPipe, OutputStream stderrPipe)
            throws IOException, FrameException {
        // 1. 매직 4바이트 수신 및 즉시 검증 (4.2 1번)
        byte[] magicBuf = new byte[4];
        readFully(in, magicBuf, 0, 4);
        int magic = ByteBuffer.wrap(magicBuf).getInt();
        if (magic != MAGIC) {
            String hex = (magic < 0 ? "-" : "") + Long.toHexString(Math.abs((long) magic));
            throw new FrameException("응답 프레임의 매직이 다르다. 기대=0x574c4731 실제=0x" + hex + ". 소켓 자리에 다른 프로그램이 붙어 있을 수 있다.");
        }

        // 2. 버전 2바이트 수신 및 즉시 검증 (4.2 2번)
        byte[] verBuf = new byte[2];
        readFully(in, verBuf, 0, 2);
        int version = ByteBuffer.wrap(verBuf).getShort() & 0xFFFF;
        if (version != VERSION) {
            throw new FrameException("프로토콜 버전이 다르다. 이쪽=" + VERSION + " 저쪽=" + version + ". 사이드카나 CLI 중 하나가 낡았다.");
        }

        // 3. 재지 않는 10바이트 (dbSchema 2B, pid 4B, status 2B, errorCode 2B) 수신 (4.2 3번)
        byte[] midBuf = new byte[10];
        readFully(in, midBuf, 0, 10);
        ByteBuffer midBb = ByteBuffer.wrap(midBuf);
        int dbSchema = midBb.getShort() & 0xFFFF;
        long pid = midBb.getInt() & 0xFFFFFFFFL;
        int status = midBb.getShort() & 0xFFFF;
        int errorCode = midBb.getShort() & 0xFFFF;

        // 4. stdout 길이 4바이트 수신 및 즉시 검증 (4.2 4번)
        byte[] stdoutLenBuf = new byte[4];
        readFully(in, stdoutLenBuf, 0, 4);
        int stdoutLen = ByteBuffer.wrap(stdoutLenBuf).getInt();
        if (stdoutLen < 0 || stdoutLen > MAX_BODY_LEN) {
            throw new FrameException("표준출력 길이가 0 과 " + MAX_BODY_LEN + " 사이가 아니다: " + stdoutLen);
        }

        // 5. stderr 길이 4바이트 수신 및 즉시 검증 (4.2 4번)
        byte[] stderrLenBuf = new byte[4];
        readFully(in, stderrLenBuf, 0, 4);
        int stderrLen = ByteBuffer.wrap(stderrLenBuf).getInt();
        if (stderrLen < 0 || stderrLen > MAX_BODY_LEN) {
            throw new FrameException("표준오류 길이가 0 과 " + MAX_BODY_LEN + " 사이가 아니다: " + stderrLen);
        }

        // 6. stdout 바이트 스트리밍 (64 KiB 단위) (4.2 6번)
        streamBody(in, stdoutPipe, stdoutLen);

        // 7. stderr 바이트 스트리밍 (64 KiB 단위) (4.2 6번)
        streamBody(in, stderrPipe, stderrLen);

        return new HeaderInfo(status, errorCode, pid, dbSchema);
    }

    private static void streamBody(InputStream in, OutputStream out, int totalLen) throws IOException, FrameException {
        if (totalLen == 0) return;
        byte[] buf = new byte[Math.min(totalLen, 65536)];
        int received = 0;
        while (received < totalLen) {
            int toRead = Math.min(buf.length, totalLen - received);
            int r = in.read(buf, 0, toRead);
            if (r == -1) {
                throw new FrameException("응답이 " + totalLen + " 바이트여야 하는데 " + received + " 바이트에서 끊겼다.");
            }
            out.write(buf, 0, r);
            received += r;
        }
        out.flush();
    }

    private static void readFully(InputStream in, byte[] b, int off, int len) throws IOException, FrameException {
        int read = 0;
        while (read < len) {
            int r = in.read(b, off + read, len - read);
            if (r == -1) {
                throw new FrameException("프레임이 다 오기 전에 끊겼다.");
            }
            read += r;
        }
    }

    public record HeaderInfo(int status, int errorCode, long pid, int dbSchema) {}
}
