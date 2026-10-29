package withlog.engine;

/** 요청별 진단. serve에서도 사용자 stderr에 실리며 동시 요청의 진단은 섞이지 않는다. */
public final class Diagnostics implements AutoCloseable {
    private static final ThreadLocal<Diagnostics> CURRENT = new ThreadLocal<>();
    private final Diagnostics previous;
    private final StringBuilder text = new StringBuilder();
    public Diagnostics() { previous = CURRENT.get(); CURRENT.set(this); }
    public static void line(String line) {
        Diagnostics current = CURRENT.get();
        if (current == null) System.err.println(line); else current.text.append(line).append('\n');
    }
    public String text() { return text.toString(); }
    @Override public void close() { if (previous == null) CURRENT.remove(); else CURRENT.set(previous); }
}
