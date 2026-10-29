package withlog.git;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/** cat-file 응답은 본문의 LF/NUL이 아니라 헤더의 바이트 수로 자른다. */
final class GitBatch {
    private GitBatch() {}
    static byte[] input(List<String> specs) {
        return (String.join("\n", specs) + "\n").getBytes(StandardCharsets.UTF_8);
    }

    static List<byte[]> read(int count, byte[] output) {
        List<byte[]> bodies = new ArrayList<>();
        int offset = 0;
        for (int i = 0; i < count; i++) {
            int end = offset;
            while (end < output.length && output[end] != '\n') end++;
            if (end == output.length) break;
            String[] header = new String(output, offset, end - offset, StandardCharsets.UTF_8).split(" ");
            offset = end + 1;
            if (header.length != 3 || !header[1].equals("blob")) {
                bodies.add(null);
                // 비-blob도 크기를 싣고 본문을 내므로 다음 응답 전에 그 바이트를 건넌다.
                if (header.length == 3) {
                    try {
                        int size = Integer.parseInt(header[2]);
                        if (size < 0 || size >= output.length - offset) break;
                        offset += size + 1;
                    } catch (NumberFormatException e) { break; }
                }
                continue;
            }
            try {
                int size = Integer.parseInt(header[2]);
                if (size < 0 || size >= output.length - offset) break;
                bodies.add(Arrays.copyOfRange(output, offset, offset + size));
                offset += size + 1;
            } catch (NumberFormatException e) { break; }
        }
        while (bodies.size() < count) bodies.add(null);
        return bodies;
    }
}
