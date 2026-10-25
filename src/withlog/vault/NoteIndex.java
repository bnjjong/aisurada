package withlog.vault;

import withlog.text.Utf16;

import java.nio.file.Path;
import java.util.*;

/**
 * vault 훑기 결과와 아젠다 정보를 프로세스 단위로 보관하는 색인 객체.
 * 설명서 8.6, 9.5, 11.5, 16.5, 18.5에 따라 프로세스당 1회만 디스크를 훑고 결과를 재사용합니다.
 */
public final class NoteIndex {

    private static Path cachedVaultRoot = null;
    private static NoteIndex cachedInstance = null;

    private final Path vaultRoot;
    private final AgendaReader.ScanResult agendaScan;
    private final NoteScanner.ScanResult noteScan;
    private final List<NoteScanner.NoteRecord> allRootNotes;
    private final Map<String, NoteScanner.NoteRecord> allSubNotesByParentPath;

    private NoteIndex(Path vaultRoot, AgendaReader.ScanResult agendaScan, NoteScanner.ScanResult noteScan) {
        this.vaultRoot = vaultRoot;
        this.agendaScan = agendaScan;
        this.noteScan = noteScan;

        // 9.5 정렬: created 오름차순, 같으면 경로에서 /를 U+0001로 바꾼 바이트 차례
        List<NoteScanner.NoteRecord> roots = new ArrayList<>(noteScan.notesById().values());
        roots.sort((a, b) -> compareCreatedThenPath(a, b));
        this.allRootNotes = Collections.unmodifiableList(roots);

        // 하위 노트 부모별 색인
        Map<String, NoteScanner.NoteRecord> subMap = new LinkedHashMap<>();
        for (NoteScanner.NoteRecord root : roots) {
            collectSubNotes(root, subMap);
        }
        this.allSubNotesByParentPath = Collections.unmodifiableMap(subMap);
    }

    private static void collectSubNotes(NoteScanner.NoteRecord current, Map<String, NoteScanner.NoteRecord> acc) {
        for (NoteScanner.NoteRecord sub : current.subNotes().values()) {
            acc.put(sub.vaultRelPath(), sub);
            collectSubNotes(sub, acc);
        }
    }

    /**
     * 지정한 vaultRoot에 대해 새로 디스크를 훑어 NoteIndex를 생성합니다.
     * 설명서 8.3 및 8.6에 따라 요청마다 항상 최신 디스크 상태를 새로 스캔합니다.
     */
    public static NoteIndex fresh(Path vaultRoot) {
        AgendaReader.ScanResult aScan = AgendaReader.scanAgendas(vaultRoot, false);
        NoteScanner.ScanResult nScan = NoteScanner.scan(vaultRoot, aScan.table(), false);
        return new NoteIndex(vaultRoot, aScan, nScan);
    }

    /**
     * 호환성을 위해 유지되는 메서드로, 항상 fresh(vaultRoot)를 호출합니다.
     */
    public static NoteIndex get(Path vaultRoot) {
        return fresh(vaultRoot);
    }

    /**
     * 캐시를 무효화하고 새로 스캔합니다.
     */
    public static NoteIndex refresh(Path vaultRoot) {
        return fresh(vaultRoot);
    }

    public Path vaultRoot() {
        return vaultRoot;
    }

    public AgendaTable agendaTable() {
        return agendaScan.table();
    }

    public AgendaReader.ScanResult agendaScan() {
        return agendaScan;
    }

    public NoteScanner.ScanResult noteScan() {
        return noteScan;
    }

    public Map<String, NoteScanner.NoteRecord> notesById() {
        return noteScan.notesById();
    }

    public List<NoteScanner.NoteRecord> allRootNotes() {
        return allRootNotes;
    }

    /**
     * 설명서 9.5의 정렬 기준:
     * 1. created 글자의 UTF-8 바이트 오름차순
     * 2. 같으면 vault 상대 경로에서 '/'를 U+0001로 바꾼 글자의 UTF-8 바이트 차례
     */
    public static int compareCreatedThenPath(NoteScanner.NoteRecord a, NoteScanner.NoteRecord b) {
        String c1 = a.meta() != null && a.meta().created() != null ? a.meta().created() : "";
        String c2 = b.meta() != null && b.meta().created() != null ? b.meta().created() : "";
        int cmp = compareUtf8Bytes(c1, c2);
        if (cmp != 0) return cmp;

        String p1 = a.vaultRelPath().replace('/', '\u0001');
        String p2 = b.vaultRelPath().replace('/', '\u0001');
        return compareUtf8Bytes(p1, p2);
    }

    /**
     * UTF-8 바이트 차례 (코드 포인트 차례) 비교.
     */
    public static int compareUtf8Bytes(String s1, String s2) {
        byte[] b1 = s1.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        byte[] b2 = s2.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        int len = Math.min(b1.length, b2.length);
        for (int i = 0; i < len; i++) {
            int byte1 = b1[i] & 0xFF;
            int byte2 = b2[i] & 0xFF;
            if (byte1 != byte2) {
                return Integer.compare(byte1, byte2);
            }
        }
        return Integer.compare(b1.length, b2.length);
    }
}
