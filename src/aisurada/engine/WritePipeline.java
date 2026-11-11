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

    private WritePipeline(NoteIndex index, VaultGit git, boolean tool, String session, String sessionAgent, ReentrantLock acquiredLock) {
        this.index = index; this.git = git; this.tool = tool; this.session = session; this.sessionAgent = sessionAgent;
        this.acquiredLock = acquiredLock;
    }

    public static ReentrantLock tryAcquireWriteLock(Path vault) {
        String vaultKey = vault.toAbsolutePath().normalize().toString();
        ReentrantLock lock = VAULT_WRITE_LOCKS.computeIfAbsent(vaultKey, k -> new ReentrantLock(true));
        return lock.tryLock() ? lock : null;
    }

    public static WritePipeline begin(Path vault, String command, boolean tool, boolean serve, String session) {
        return begin(vault, command, tool, serve, session, null);
    }

    /** [sessionAgent]는 세션을 연 에이전트 이름이다. 노트에 적는 도장이 이 값을 읽는다. 없으면 claude로 본다. */
    public static WritePipeline begin(Path vault, String command, boolean tool, boolean serve, String session, String sessionAgent) {
        if (!CommandRegistry.WRITE_COMMANDS.contains(command)) {
            return new WritePipeline(null, null, tool, session, sessionAgent, null);
        }
        String vaultKey = vault.toAbsolutePath().normalize().toString();
        ReentrantLock lock = VAULT_WRITE_LOCKS.computeIfAbsent(vaultKey, k -> new ReentrantLock(true));
        lock.lock();
        try {
            if (!command.equals("init-workspace")) WorkspaceMarker.check(vault);
            NoteIndex index = NoteIndex.reconciled(vault);
            VaultGit git = VaultGit.forRequest(vault, serve);
            git.beforeWrite(tool, session);
            WritePipeline scope = new WritePipeline(index, git, tool, session, sessionAgent, lock);
            CURRENT.set(scope);
            return scope;
        } catch (Throwable t) {
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
            if (acquiredLock != null && acquiredLock.isHeldByCurrentThread()) {
                acquiredLock.unlock();
            }
        }
    }
}
