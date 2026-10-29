package withlog.engine;

import withlog.cmd.CommandRegistry;
import withlog.git.VaultGit;
import withlog.vault.NoteIndex;
import withlog.write.WorkspaceMarker;

import java.nio.file.Path;

/** 7.1: 문법 검사 뒤 공통 준비를 하고, 명령의 성공/실패에 관계없이 쓴 쪽을 남긴다. */
public final class WritePipeline implements AutoCloseable {
    private static final ThreadLocal<WritePipeline> CURRENT = new ThreadLocal<>();
    private final NoteIndex index;
    private final VaultGit git;
    private final boolean tool;
    private final String session;

    private WritePipeline(NoteIndex index, VaultGit git, boolean tool, String session) {
        this.index = index; this.git = git; this.tool = tool; this.session = session;
    }

    public static WritePipeline begin(Path vault, String command, boolean tool, boolean serve, String session) {
        if (!CommandRegistry.WRITE_COMMANDS.contains(command)) return new WritePipeline(null, null, tool, session);
        if (!command.equals("init-workspace")) WorkspaceMarker.check(vault);
        NoteIndex index = NoteIndex.reconciled(vault);
        VaultGit git = VaultGit.forRequest(vault, serve);
        git.beforeWrite(tool, session);
        WritePipeline scope = new WritePipeline(index, git, tool, session);
        CURRENT.set(scope);
        return scope;
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
        if (git == null) return;
        CURRENT.remove();
        git.afterWrite(tool, session);
    }
}
