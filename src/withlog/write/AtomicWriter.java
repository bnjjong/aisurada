package withlog.write;

import withlog.cli.WlException;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Files;
import java.nio.file.LinkOption;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.util.Locale;

/**
 * 설명서 7.4 원자적 쓰기 엔진 컴포넌트.
 * 부모 디렉터리 생성, 임시 파일 작성 및 fsync, 원자적 이름 바꾸기를 수행합니다.
 */
public final class AtomicWriter {

    private AtomicWriter() {}

    /**
     * 바이트 데이터를 대상 파일에 원자적으로 기록합니다.
     *
     * @param target 대상 파일 경로
     * @param bytes  기록할 바이트 배열
     * @throws WlException 파일 기록 또는 폴더 생성 실패 시 WL-500
     */
    public static void writeAtomic(Path target, byte[] bytes) {
        Path normTarget = target.toAbsolutePath().normalize();
        Path parent = normTarget.getParent();

        // 부모 자리에 폴더가 아닌 것(일반 파일)이 있어도 만들기를 시도해 1번 문구로 선다
        if (parent != null && !Files.isDirectory(parent)) {
            try {
                Files.createDirectories(parent);
            } catch (IOException e) {
                throw new WlException(500, "폴더를 만들지 못했다: " + parent);
            }
        }

        String targetName = normTarget.getFileName().toString();
        Path tempPath = null;

        for (int attempt = 0; attempt < 3; attempt++) {
            long rand64 = TestApparatus.nextRandom64();
            String randBase36 = Long.toUnsignedString(rand64, 36).toLowerCase(Locale.ROOT);
            String tempName = targetName + "." + randBase36 + ".tmp";
            Path candidate = parent != null ? parent.resolve(tempName) : Path.of(tempName);

            if (!Files.exists(candidate, LinkOption.NOFOLLOW_LINKS)) {
                tempPath = candidate;
                break;
            }
        }

        if (tempPath == null) {
            throw new WlException(500, "임시 파일 이름을 3 번 골랐는데 다 이미 있었다: " + (parent != null ? parent : ""));
        }

        // 임시 파일 열기, 쓰기 및 fsync
        try (FileChannel channel = FileChannel.open(tempPath, StandardOpenOption.CREATE_NEW, StandardOpenOption.WRITE)) {
            ByteBuffer buffer = ByteBuffer.wrap(bytes);
            while (buffer.hasRemaining()) {
                channel.write(buffer);
            }
            channel.force(true);
        } catch (Exception e) {
            try {
                Files.deleteIfExists(tempPath);
            } catch (Exception ignored) {}
            throw new WlException(500, "파일을 쓰지 못했다: " + normTarget);
        }

        // 원자적 이름 바꾸기. 원자적이 아닌 옮기기로 다시 하지 않는다.
        // 그 길은 대상 자리의 빈 폴더를 지우고 파일을 둬서, 실패해야 할 쓰기가 성공한다
        try {
            Files.move(tempPath, normTarget, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (Exception e) {
            try {
                Files.deleteIfExists(tempPath);
            } catch (Exception ignored) {}
            throw new WlException(500, "파일을 쓰지 못했다: " + normTarget);
        }
    }
}
