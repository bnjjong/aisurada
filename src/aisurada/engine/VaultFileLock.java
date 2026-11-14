package aisurada.engine;

import aisurada.cli.AisException;

import java.io.IOException;
import java.nio.channels.FileChannel;
import java.nio.channels.FileLock;
import java.nio.channels.OverlappingFileLockException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;

/**
 * 쓰기 잠금의 프로세스 간 부분입니다. JVM 안 줄 세우기(공정 ReentrantLock)만으로는 따로 도는 프로세스, 예를 들어
 * 앱 안의 Rust 엔진과 이 사이드카가 같은 vault에 동시에 쓰는 것을 못 막으므로, vault 밖의 잠금 파일에 파일 잠금을 겁니다.
 *
 * <p>잠금 파일은 vault 내용이 아니라서 vault 밖에 둡니다. 폴더는 시스템 속성 {@code aisurada.lock.dir}, 환경 변수
 * {@code AISURADA_LOCK_DIR}, 없으면 {@code ~/Library/Application Support/aisurada/locks}이고, 파일 이름은 vault 실제 경로의
 * SHA-256 16진수에 {@code .lock}입니다. Rust 엔진의 {@code write::lock}과 같은 규칙이며, macOS에서는 Rust의
 * {@code flock}과 이 클래스가 쓰는 {@code FileChannel.lock}이 서로를 막습니다.
 */
final class VaultFileLock implements AutoCloseable {
    private final FileChannel channel;
    private final FileLock lock;

    private VaultFileLock(FileChannel channel, FileLock lock) {
        this.channel = channel;
        this.lock = lock;
    }

    static Path lockDir() {
        String property = System.getProperty("aisurada.lock.dir");
        if (property != null && !property.isBlank()) return Path.of(property);
        String env = System.getenv("AISURADA_LOCK_DIR");
        if (env != null && !env.isBlank()) return Path.of(env);
        return Path.of(System.getProperty("user.home", ""), "Library", "Application Support", "aisurada", "locks");
    }

    static Path lockFile(Path vault) {
        Path real;
        try { real = vault.toRealPath(); }
        catch (IOException e) { real = vault.toAbsolutePath().normalize(); }
        try {
            byte[] digest = MessageDigest.getInstance("SHA-256").digest(real.toString().getBytes(StandardCharsets.UTF_8));
            return lockDir().resolve(HexFormat.of().formatHex(digest) + ".lock");
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    /** 다른 쪽이 놓을 때까지 기다려서 잡습니다. */
    static VaultFileLock acquire(Path vault) { return open(vault, true); }

    /** 기다리지 않고 한 번 잡아 봅니다. 다른 쪽이 쥐고 있으면 null입니다. */
    static VaultFileLock tryAcquire(Path vault) { return open(vault, false); }

    private static VaultFileLock open(Path vault, boolean wait) {
        Path file = lockFile(vault);
        FileChannel channel = null;
        try {
            Files.createDirectories(file.getParent());
            channel = FileChannel.open(file, StandardOpenOption.CREATE, StandardOpenOption.WRITE);
            while (true) {
                try {
                    FileLock lock = wait ? channel.lock() : channel.tryLock();
                    if (lock == null) { channel.close(); return null; }
                    return new VaultFileLock(channel, lock);
                } catch (OverlappingFileLockException e) {
                    // 같은 JVM의 다른 스레드가 쥐고 있다(vault 경로를 달리 적어 줄이 갈린 경우). 놓을 때까지 기다린다.
                    if (!wait) { channel.close(); return null; }
                    try { Thread.sleep(25); }
                    catch (InterruptedException ie) { Thread.currentThread().interrupt(); channel.close(); throw new AisException(500, "쓰기 잠금을 기다리다 멈췄다: " + file); }
                }
            }
        } catch (IOException e) {
            if (channel != null) { try { channel.close(); } catch (IOException ignored) {} }
            throw new AisException(500, "쓰기 잠금 파일을 못 열었다: " + file + ": " + e.getMessage());
        }
    }

    @Override public void close() {
        try { lock.release(); } catch (IOException ignored) {}
        try { channel.close(); } catch (IOException ignored) {}
    }
}
