package aisurada.session;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * 스키마 없이 protobuf 바이트를 필드 목록으로 읽는 가장 작은 읽개입니다.
 *
 * <p>Antigravity(agy)의 대화 기록은 SQLite 칸마다 protobuf 메시지를 넣어 두는데, 스키마 파일이 공개돼 있지 않습니다.
 * 그래서 필요한 필드 번호만 직접 짚어 읽습니다. 모르는 필드는 건너뛰므로 CLI가 필드를 더해도 읽기가 깨지지 않습니다.
 * 길이가 모자란 끝은 {@code tolerant}일 때 있는 만큼만 돌려줍니다(긴 도구 인자를 앞부분만 가져올 때 씁니다).
 */
final class Protobuf {

    private static final int VARINT = 0;
    private static final int FIXED64 = 1;
    private static final int LENGTH_DELIMITED = 2;
    private static final int FIXED32 = 5;

    private Protobuf() {}

    /** 필드 하나. 숫자 필드는 {@code number}, 길이가 있는 필드는 {@code bytes}에 값이 들어 있다. */
    record Field(int number, int wire, long number64, byte[] bytes) {
        boolean isMessage() {
            return wire == LENGTH_DELIMITED;
        }

        /** 길이가 있는 필드를 글로 읽는다. 숫자 필드면 null. */
        String text() {
            return wire == LENGTH_DELIMITED ? new String(bytes, StandardCharsets.UTF_8) : null;
        }
    }

    /**
     * 메시지를 필드 목록으로 읽는다. 형식이 틀리면 null이다.
     * [tolerant]가 참이면 마지막 길이 필드가 모자라도 있는 만큼을 돌려주고, 읽던 자리에서 멈춘다.
     */
    static List<Field> parse(byte[] data, boolean tolerant) {
        if (data == null) return null;
        List<Field> fields = new ArrayList<>();
        int[] pos = {0};
        while (pos[0] < data.length) {
            int start = pos[0];
            Long key = varint(data, pos);
            if (key == null) return tolerant ? fields : null;
            int number = (int) (key >>> 3);
            int wire = (int) (key & 7);
            if (number == 0 || number > 536_870_911) return tolerant && !fields.isEmpty() ? fields : null;
            switch (wire) {
                case VARINT -> {
                    Long value = varint(data, pos);
                    if (value == null) return tolerant ? fields : null;
                    fields.add(new Field(number, wire, value, null));
                }
                case FIXED64 -> {
                    if (pos[0] + 8 > data.length) return tolerant ? fields : null;
                    pos[0] += 8;
                    fields.add(new Field(number, wire, 0, null));
                }
                case FIXED32 -> {
                    if (pos[0] + 4 > data.length) return tolerant ? fields : null;
                    pos[0] += 4;
                    fields.add(new Field(number, wire, 0, null));
                }
                case LENGTH_DELIMITED -> {
                    Long length = varint(data, pos);
                    if (length == null || length < 0 || length > Integer.MAX_VALUE) return tolerant ? fields : null;
                    int available = data.length - pos[0];
                    if (length > available) {
                        if (!tolerant) return null;
                        fields.add(new Field(number, wire, 0, slice(data, pos[0], available)));
                        return fields;
                    }
                    fields.add(new Field(number, wire, 0, slice(data, pos[0], (int) (long) length)));
                    pos[0] += (int) (long) length;
                }
                default -> {
                    pos[0] = start;
                    return tolerant ? fields : null;
                }
            }
        }
        return fields;
    }

    /** 번호가 같은 첫 필드. 없으면 null. */
    static Field first(List<Field> fields, int number) {
        if (fields == null) return null;
        for (Field field : fields) {
            if (field.number() == number) return field;
        }
        return null;
    }

    /** 번호가 같은 모든 필드. */
    static List<Field> all(List<Field> fields, int number) {
        List<Field> found = new ArrayList<>();
        if (fields == null) return found;
        for (Field field : fields) {
            if (field.number() == number) found.add(field);
        }
        return found;
    }

    /** 번호가 같은 첫 길이 필드를 하위 메시지로 읽는다. 없거나 메시지가 아니면 null. */
    static List<Field> message(List<Field> fields, int number, boolean tolerant) {
        Field field = first(fields, number);
        return field == null || !field.isMessage() ? null : parse(field.bytes(), tolerant);
    }

    /** 번호가 같은 첫 길이 필드를 글로 읽는다. 없으면 null. */
    static String text(List<Field> fields, int number) {
        Field field = first(fields, number);
        return field == null ? null : field.text();
    }

    /** 번호가 같은 첫 숫자 필드. 없으면 null. */
    static Long number(List<Field> fields, int number) {
        Field field = first(fields, number);
        return field == null || field.wire() != VARINT ? null : field.number64();
    }

    private static byte[] slice(byte[] data, int from, int length) {
        byte[] out = new byte[length];
        System.arraycopy(data, from, out, 0, length);
        return out;
    }

    private static Long varint(byte[] data, int[] pos) {
        long value = 0;
        int shift = 0;
        while (pos[0] < data.length && shift < 64) {
            int b = data[pos[0]++] & 0xff;
            value |= (long) (b & 0x7f) << shift;
            if ((b & 0x80) == 0) return value;
            shift += 7;
        }
        return null;
    }
}
