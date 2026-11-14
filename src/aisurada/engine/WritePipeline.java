package aisurada.engine;

import aisurada.cmd.CommandRegistry;
import aisurada.git.VaultGit;
import aisurada.vault.NoteIndex;
import aisurada.write.WorkspaceMarker;

import java.nio.file.Path;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.locks.ReentrantLock;

/** 7.1: 문법 검사 뒤 공통 준비를 하고, 명령의 성공/실패에 관계없이 쓴 쪽을 남긴다. */
public final class WritePipeline implements AutoCloseable {
    private static final ThreadLocal<WritePipeline> CURRENT = new ThreadLocal<>();
    // [B6] 1-가: vault별 쓰기 직렬화 FIFO 공정 잠금 (fair ReentrantLock)
    private static final ConcurrentHashMap<String, ReentrantLock> VAULT_WRITE_LOCKS = new ConcurrentHashMap<>();

    private final NoteIndex index;
    private final VaultGit git;
    private final boolean tool;
    private final String session;
    private final String sessionAgent;
    private final ReentrantLock acquiredLock;
    private final VaultFileLock fileLock;

    private WritePipeline(NoteIndex index, VaultGit git, boolean tool, String session, String sessionAgent, ReentrantLock acquiredLock, VaultFileLock fileLock) {
        this.index = index; this.git = git; this.tool = tool; this.session = session; this.sessionAgent = sessionAgent;
        this.acquiredLock = acquiredLock; this.fileLock = fileLock;
    }

    /** 잡은 쓰기 잠금: JVM 안 줄 세우기와 프로세스 간 파일 잠금. 쓴 쪽이 {@link #unlock()}으로 둘을 함께 놓습니다. */
    public static final class WriteLock {
        private final ReentrantLock lock;
        private final VaultFileLock file;
        private WriteLock(ReentrantLock lock, VaultFileLock file) { this.lock = lock; this.file = file; }
        public boolean isHeldByCurrentThread() { return lock.isHeldByCurrentThread(); }
        public void unlock() {
            try { if (file != null) file.close(); }
            finally { lock.unlock(); }
        }
    }

    /** 기다리지 않고 쓰기 잠금을 한 번 잡아 봅니다. 다른 스레드나 다른 프로세스가 쓰는 중이면 null입니다. */
    public static WriteLock tryAcquireWriteLock(Path vault) {
        String vaultKey = vault.toAbsolutePath().normalize().toString();
        ReentrantLock lock = VAULT_WRITE_LOCKS.computeIfAbsent(vaultKey, k -> new ReentrantLock(true));
        if (!lock.tryLock()) return null;
        try {
            // 이 스레드가 이미 쥔 잠금을 다시 잡는 경우에는 파일 잠금도 이미 쥐고 있다
            VaultFileLock file = lock.getHoldCount() == 1 ? VaultFileLock.tryAcquire(vault) : null;
            if (lock.getHoldCount() == 1 && file == null) { lock.unlock(); return null; }
            return new WriteLock(lock, file);
        } catch (RuntimeException e) {
            lock.unlock();
            throw e;
        }
    }

    public static WritePipeline begin(Path vault, String command, boolean tool, boolean serve, String session) {
        return begin(vault, command, tool, serve, session, null);
    }

    /** [sessionAgent]는 세션을 연 에이전트 이름이다. 노트에 적는 도장이 이 값을 읽는다. 없으면 claude로 본다. */
    public static WritePipeline begin(Path vault, String command, boolean tool, boolean serve, String session, String sessionAgent) {
        if (!CommandRegistry.WRITE_COMMANDS.contains(command)) {
            return new WritePipeline(null, null, tool, session, sessionAgent, null, null);
        }
        String vaultKey = vault.toAbsolutePath().normalize().toString();
        ReentrantLock lock = VAULT_WRITE_LOCKS.computeIfAbsent(vaultKey, k -> new ReentrantLock(true));
        lock.lock();
        VaultFileLock fileLock = null;
        try {
            // 같은 스레드가 다시 들어온 경우(holdCount > 1)에는 파일 잠금을 이미 쥐고 있다
            if (lock.getHoldCount() == 1) fileLock = VaultFileLock.acquire(vault);
            if (!command.equals("init-workspace")) WorkspaceMarker.check(vault);
            NoteIndex index = NoteIndex.reconciled(vault);
            VaultGit git = VaultGit.forRequest(vault, serve);
            git.beforeWrite(tool, session);
            WritePipeline scope = new WritePipeline(index, git, tool, session, sessionAgent, lock, fileLock);
            CURRENT.set(scope);
            return scope;
        } catch (Throwable t) {
            if (fileLock != null) fileLock.close();
            lock.unlock();
            throw t;
        }
    }

    /** 지금 요청 범위의 세션 에이전트 이름입니다. 범위가 없거나 지정이 없으면 null이고, 그때는 claude로 봅니다. */
    public static String currentSessionAgent() {
        WritePipeline scope = CURRENT.get();
        return scope == null ? null : scope.sessionAgent;
    }

    /** 직접 호출하는 명령도 표식과 맞추기를 지킨다. 실행기를 거친 요청은 준비한 색인을 재사용한다. */
    public static NoteIndex index(Path vault) {
        WritePipeline scope = CURRENT.get();
        if (scope != null && scope.index.vaultRoot().toAbsolutePath().normalize().equals(vault.toAbsolutePath().normalize()))
            return scope.index;
        WorkspaceMarker.check(vault);
        return NoteIndex.reconciled(vault);
    }

    @Override public void close() {
        try {
            if (git != null) {
                CURRENT.remove();
                git.afterWrite(tool, session);
            }
        } finally {
            try {
                if (fileLock != null) fileLock.close();
            } finally {
                if (acquiredLock != null && acquiredLock.isHeldByCurrentThread()) {
                    acquiredLock.unlock();
                }
            }
        }
    }
}
