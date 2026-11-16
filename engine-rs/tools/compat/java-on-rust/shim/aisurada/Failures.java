package aisurada;
public final class Failures {
    public static int count;
    public static void record(Object label) { count++; System.out.println("FAIL: " + String.valueOf(label).replace("\n", " | ")); }
    public static void summary() { System.out.println("FAILURES=" + count); }
}
