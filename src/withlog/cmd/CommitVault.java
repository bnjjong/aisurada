package withlog.cmd;

import withlog.git.VaultGit;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

/** 31.5: 표식/색인/작성자 기록 없이 재시도 시각을 무시해 1번 시도한다. */
public final class CommitVault {
    private CommitVault() {}
    public static Map<String, Object> execute(Path vault, boolean serve) {
        var attempt = VaultGit.forRequest(vault, serve).commit("restore");
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("outcome", attempt.outcome());
        if (attempt.blocked() != null) result.put("blocked", attempt.blocked());
        return result;
    }
}
