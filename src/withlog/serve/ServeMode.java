package withlog.serve;

import withlog.config.EngineConfig;
import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;
import withlog.frame.FrameException;
import withlog.frame.RequestFrame;
import withlog.frame.ResponseFrame;
import withlog.text.Trim;
import withlog.vault.VaultKey;
import withlog.vault.VaultPath;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.StandardProtocolFamily;
import java.net.UnixDomainSocketAddress;
import java.nio.channels.Channels;
import java.nio.channels.ClosedChannelException;
import java.nio.channels.FileChannel;
import java.nio.channels.FileLock;
import java.nio.channels.ServerSocketChannel;
import java.nio.channels.SocketChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.time.Instant;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;

/**
 * withlog serve 모드 엔진 프로세스의 생명주기를 관리합니다. (설명서 3.2)
 */
public final class ServeMode {

    private static final AtomicBoolean tornDown = new AtomicBoolean(false);
    private static final CountDownLatch teardownDone = new CountDownLatch(1);
    private static final AtomicBoolean running = new AtomicBoolean(true);

    private static ServerSocketChannel serverChannel;
    private static Path socketPath;
    private static Path runDir;
    private static String vaultKey;
    private static FileLock lock;
    private static FileChannel lockChannel;
    private static final AtomicInteger openConnections = new AtomicInteger(0);

    /**
     * serve 모드로 진입하여 소켓 서버를 엽니다.
     */
    public static void run(String vaultRoot, EngineConfig config) {
        String userHome = System.getProperty("user.home");
        vaultKey = VaultKey.compute(vaultRoot);
        String hash16 = VaultKey.computeHash16(vaultRoot);

        // 1. 소켓 경로 결정
        String rawSocket = System.getenv("WITHLOG_SIDECAR_SOCKET");
        if (rawSocket == null || Trim.isBlank(rawSocket)) {
            rawSocket = userHome + "/Library/Caches/withlog/wl-" + hash16 + ".sock";
        } else {
            rawSocket = Trim.trim(rawSocket);
        }
        socketPath = Paths.get(VaultPath.normalize(rawSocket));

        byte[] socketPathBytes = socketPath.toString().getBytes(StandardCharsets.UTF_8);
        if (socketPathBytes.length > 102) {
            System.err.println("소켓 경로가 102 바이트를 넘는다(" + socketPathBytes.length + " 바이트): " + socketPath + ". WITHLOG_SIDECAR_SOCKET 로 더 짧은 자리를 지정한다.");
            System.exit(1);
        }

        // 2. 시작 시각
        Instant startedAt = Instant.now();

        // 3. 잠금 파일 획득
        runDir = Paths.get(userHome, "Library", "Application Support", "withlog", "run");
        Path lockFile = runDir.resolve(vaultKey + ".lock");
        try {
            Files.createDirectories(runDir);
            lockChannel = FileChannel.open(lockFile, StandardOpenOption.READ, StandardOpenOption.WRITE, StandardOpenOption.CREATE);
            lock = lockChannel.tryLock(0, Long.MAX_VALUE, false);
            if (lock == null) {
                // 이미 다른 프로세스가 잠금을 쥐고 있음 -> 경고 로그 1줄 후 종료 코드 0
                System.err.println("경고: 이미 다른 프로세스가 잠금을 쥐고 있다: " + lockFile);
                System.exit(0);
            }
        } catch (Exception e) {
            System.err.println("잠금 파일을 열거나 잠그지 못했다: " + e.getMessage());
            System.exit(1);
        }

        // 셧다운 훅: 잠금을 잡은 직후에 건다. 그래야 뜨는 도중(실행 상태 파일을 쓴 뒤, bind 전)에 신호를 받아도
        // 3.2의 「내려가는 차례」가 돌아 실행 상태 파일을 지우고 잠금을 놓는다. 신호로 끝나면 128 + 신호 번호다
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            doTeardown();
        }, "wl-shutdown-hook"));

        // 4. 활동 시각 초기화
        Instant initialLastActiveAt = Instant.now();
        AtomicReference<Instant> lastActiveAtRef = new AtomicReference<>(initialLastActiveAt);
        AtomicLong lastActiveNanoRef = new AtomicLong(System.nanoTime());

        // 5. 실행 상태 파일 처음 쓰기
        long pid = ProcessHandle.current().pid();
        String builtCommit = readBuiltCommit();
        AtomicReference<String> lastWrittenJsonRef = new AtomicReference<>(null);

        try {
            StateFile state = new StateFile(pid, vaultRoot, socketPath.toString(), startedAt, initialLastActiveAt, 0, builtCommit);
            String written = StateFile.writeIfChanged(runDir, vaultKey, state, null);
            lastWrittenJsonRef.set(written);
        } catch (Exception e) {
            System.err.println("실행 상태 파일을 쓰지 못했다: " + e.getMessage());
            doTeardown();
            System.exit(1);
        }

        // 6. 소켓 폴더 생성 및 기존 파일 정리
        try {
            if (socketPath.getParent() != null) {
                Files.createDirectories(socketPath.getParent());
            }
            if (Files.exists(socketPath, LinkOption.NOFOLLOW_LINKS)) {
                if (Files.isDirectory(socketPath, LinkOption.NOFOLLOW_LINKS)) {
                    try {
                        Files.delete(socketPath);
                    } catch (DirectoryNotEmptyException dne) {
                        System.err.println("소켓 자리에 안이 찬 폴더가 있어서 지우지 못했다: " + socketPath);
                        doTeardown();
                        System.exit(1);
                    }
                } else {
                    Files.delete(socketPath);
                }
            }
        } catch (Exception e) {
            System.err.println("소켓 자리를 정리하지 못했다: " + e.getMessage());
            doTeardown();
            System.exit(1);
        }

        // 7. 소켓 바인드
        try {
            UnixDomainSocketAddress address = UnixDomainSocketAddress.of(socketPath);
            serverChannel = ServerSocketChannel.open(StandardProtocolFamily.UNIX);
            serverChannel.bind(address);
            System.err.println("소켓을 열었다: " + socketPath);
        } catch (Exception e) {
            System.err.println("소켓 바인드 실패: " + e.getMessage());
            try { Files.deleteIfExists(socketPath); } catch (Exception ignored) {}
            doTeardown();
            System.exit(1);
        }

        // 9. 정비 스레드 시작 (5초 간격)
        Thread maintThread = new Thread(() -> {
            while (running.get()) {
                try {
                    Thread.sleep(5000);
                } catch (InterruptedException e) {
                    break;
                }
                if (!running.get()) break;

                if (openConnections.get() > 0) {
                    lastActiveAtRef.set(Instant.now());
                    lastActiveNanoRef.set(System.nanoTime());
                }

                // 실행 상태 파일 갱신
                try {
                    StateFile currentState = new StateFile(pid, vaultRoot, socketPath.toString(), startedAt,
                            lastActiveAtRef.get(), openConnections.get(), builtCommit);
                    String updated = StateFile.writeIfChanged(runDir, vaultKey, currentState, lastWrittenJsonRef.get());
                    if (updated != null) {
                        lastWrittenJsonRef.set(updated);
                    }
                } catch (Exception e) {
                    System.err.println("경고: 정비 중 상태 파일 작성 실패: " + e.getMessage());
                }

                // 유휴 판정
                long idleMinutes = config.idleTimeoutMinutes();
                if (idleMinutes > 0 && openConnections.get() == 0) {
                    long elapsedNanos = System.nanoTime() - lastActiveNanoRef.get();
                    long limitNanos = idleMinutes * 60L * 1_000_000_000L;
                    if (elapsedNanos >= limitNanos) {
                        // 유휴 종료
                        doTeardown();
                        System.exit(0);
                    }
                }
            }
        }, "wl-maintenance");
        maintThread.setDaemon(true);
        maintThread.start();

        // 10. 연결 수신 루프
        ExecutorService workerPool = Executors.newCachedThreadPool();
        try {
            while (running.get()) {
                SocketChannel clientChannel;
                try {
                    clientChannel = serverChannel.accept();
                } catch (ClosedChannelException e) {
                    break;
                } catch (IOException e) {
                    if (!running.get()) break;
                    System.err.println("소켓 accept 실패: " + e.getMessage());
                    doTeardown();
                    System.exit(0);
                    break;
                }

                openConnections.incrementAndGet();
                workerPool.submit(() -> {
                    try {
                        handleConnection(clientChannel, vaultRoot, pid);
                    } finally {
                        openConnections.decrementAndGet();
                        lastActiveAtRef.set(Instant.now());
                        lastActiveNanoRef.set(System.nanoTime());
                        try {
                            clientChannel.close();
                        } catch (IOException ignored) {}
                    }
                });
            }
        } finally {
            doTeardown();
        }
    }

    private static void handleConnection(SocketChannel clientChannel, String vaultRoot, long pid) {
        try {
            InputStream in = Channels.newInputStream(clientChannel);
            OutputStream out = Channels.newOutputStream(clientChannel);

            RequestFrame req;
            try {
                req = RequestFrame.read(in);
            } catch (FrameException fe) {
                if (fe.isProbing()) {
                    // 생존 확인 연결 (4.3): 조용히 닫음
                    return;
                }
                System.err.println("경고: 요청 프레임 오류: " + fe.getMessage());
                return;
            } catch (IOException e) {
                return;
            }

            long reqStart = System.currentTimeMillis();
            EngineResult result = EngineExecutor.execute(Paths.get(vaultRoot), req.args().toArray(new String[0]), true, reqStart);

            // stdout/stderr 크기 검사 (64 MiB 초과 시 연결 닫음)
            if (result.stdoutBytes().length > 67_108_864 || result.stderrBytes().length > 67_108_864) {
                System.err.println("경고: 응답 길이가 64 MiB를 초과하여 연결을 닫습니다.");
                return;
            }

            ResponseFrame resp = new ResponseFrame(5, (int) pid, result.status(), result.errorCode(),
                    result.stdoutBytes(), result.stderrBytes());
            resp.write(out);
            out.flush();
        } catch (Exception e) {
            System.err.println("경고: 연결 처리 중 입출력 오류: " + e.getMessage());
        }
    }

    /**
     * serve 모드 종료 절차 (설명서 3.2 내려가는 차례)
     */
    public static void doTeardown() {
        if (!tornDown.compareAndSet(false, true)) {
            // 다른 스레드가 내려가는 중이면 다 끝날 때까지 기다린다.
            // 유휴 종료는 데몬인 정비 스레드가 돌려서, 주 스레드가 먼저 끝나면 JVM이 실행 상태 파일을 지우기 전에 선다.
            try {
                teardownDone.await(10, TimeUnit.SECONDS);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
            return;
        }
        try {
            teardownSteps();
        } finally {
            teardownDone.countDown();
        }
    }

    private static void teardownSteps() {
        running.set(false);

        // 1. 새 연결을 안 받는다 (소켓 닫기)
        try {
            if (serverChannel != null && serverChannel.isOpen()) {
                serverChannel.close();
            }
        } catch (Exception ignored) {}

        // 소켓 파일 삭제 및 로그
        try {
            // 이 프로세스가 연 소켓만 지운다. bind 전에 내려가면 소켓 자리는 다음 엔진의 8번이 치운다
            if (serverChannel != null && socketPath != null && Files.deleteIfExists(socketPath)) {
                System.err.println("소켓을 닫았다: " + socketPath);
            }
        } catch (Exception ignored) {}

        // 2. 열린 연결이 0이 되거나 5초가 지날 때까지 50밀리초 간격으로 대기
        long startWait = System.currentTimeMillis();
        while (openConnections.get() > 0 && (System.currentTimeMillis() - startWait) < 5000) {
            try {
                Thread.sleep(50);
            } catch (InterruptedException ignored) {
                break;
            }
        }

        // 3. shutdown git 커밋: 결정 2에 따라 생략

        // 4. 실행 상태 파일 삭제
        try {
            if (runDir != null && vaultKey != null) {
                Files.deleteIfExists(runDir.resolve(vaultKey + ".json"));
            }
        } catch (Exception ignored) {}

        // 5. 잠금을 놓고 잠금 파일을 닫는다 (잠금 파일은 안 지운다)
        try {
            if (lock != null && lock.isValid()) {
                lock.release();
            }
        } catch (Exception ignored) {}
        try {
            if (lockChannel != null && lockChannel.isOpen()) {
                lockChannel.close();
            }
        } catch (Exception ignored) {}
    }

    private static String readBuiltCommit() {
        try (InputStream in = ServeMode.class.getResourceAsStream("/META-INF/wl-build-commit.txt")) {
            if (in != null) {
                String c = new String(in.readAllBytes(), StandardCharsets.UTF_8).strip();
                if (!c.isBlank()) {
                    return c;
                }
            }
        } catch (Exception ignored) {}
        return null;
    }
}
