package withlog.frame;

import java.io.*;
import java.nio.ByteBuffer;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * 설명서 4.1 규격에 따른 소켓 요청 프레임 (WLQ1) 읽기 및 쓰기.
 */
public final class RequestFrame {

    public static final int MAGIC = 0x574C5131; // "WLQ1"
    public static final int VERSION = 1;
    public static final int MAX_ARGS = 64;
    public static final int MAX_ARG_LEN = 1_048_576; // 1 MiB

    private final List<String> args;

    public RequestFrame(List<String> args) {
        if (args == null) {
            throw new IllegalArgumentException("args cannot be null");
        }
        if (args.size() > MAX_ARGS) {
            throw new IllegalArgumentException("arg count exceeds " + MAX_ARGS + ": " + args.size());
        }
        this.args = Collections.unmodifiableList(new ArrayList<>(args));
    }

    public List<String> args() {
        return args;
    }

    /**
     * InputStream에서 요청 프레임을 읽어 반환합니다.
     * 첫 바이트 수신 전 EOF인 경우 생존 확인(probing) 예외를 발생시킵니다.
     */
    public static RequestFrame read(InputStream in) throws IOException, FrameException {
        // 1. 매직 4바이트 읽기
        int b1 = in.read();
        if (b1 == -1) {
            throw new FrameException("Connection closed before first byte (probing)", true);
        }
        byte[] magicBuf = new byte[4];
        magicBuf[0] = (byte) b1;
        readFully(in, magicBuf, 1, 3);

        int magic = ByteBuffer.wrap(magicBuf).getInt();
        if (magic != MAGIC) {
            throw new FrameException(String.format("요청 프레임의 매직이 다르다. 기대=0x%08x 실제=0x%08x", MAGIC, magic));
        }

        // 2. 버전 2바이트
        byte[] shortBuf = new byte[2];
        readFully(in, shortBuf, 0, 2);
        int version = ByteBuffer.wrap(shortBuf).getShort() & 0xFFFF;
        if (version != VERSION) {
            throw new FrameException("프로토콜 버전이 다르다. 이쪽=" + VERSION + " 저쪽=" + version);
        }

        // 3. 인자 수 n 2바이트
        readFully(in, shortBuf, 0, 2);
        int n = ByteBuffer.wrap(shortBuf).getShort() & 0xFFFF;
        if (n > MAX_ARGS) {
            throw new FrameException("요청 인자가 " + MAX_ARGS + " 개를 넘는다: " + n);
        }

        // 4. 인자 읽기
        List<String> argsList = new ArrayList<>(n);
        byte[] intBuf = new byte[4];
        var decoder = StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPLACE)
                .onUnmappableCharacter(CodingErrorAction.REPLACE);

        for (int i = 0; i < n; i++) {
            readFully(in, intBuf, 0, 4);
            int len = ByteBuffer.wrap(intBuf).getInt();
            if (len < 0 || len > MAX_ARG_LEN) {
                throw new FrameException("인자 길이가 0 과 " + MAX_ARG_LEN + " 사이가 아니다: " + len);
            }

            byte[] argBytes = new byte[len];
            readFully(in, argBytes, 0, len);
            String argStr = decoder.decode(ByteBuffer.wrap(argBytes)).toString();
            argsList.add(argStr);
        }

        return new RequestFrame(argsList);
    }

    /**
     * OutputStream으로 요청 프레임을 작성합니다.
     */
    public void write(OutputStream out) throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);

        dos.writeInt(MAGIC);
        dos.writeShort(VERSION);
        dos.writeShort(args.size());

        for (String arg : args) {
            byte[] bytes = arg.getBytes(StandardCharsets.UTF_8);
            if (bytes.length > MAX_ARG_LEN) {
                throw new IllegalArgumentException("인자 1개의 UTF-8 바이트가 " + MAX_ARG_LEN + "를 넘는다: " + bytes.length);
            }
            dos.writeInt(bytes.length);
            dos.write(bytes);
        }
        dos.flush();
        out.write(baos.toByteArray());
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
}
