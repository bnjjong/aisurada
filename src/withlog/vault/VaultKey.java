package withlog.vault;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.Locale;

/**
 * Computes vault keys according to withlog specification sections 8.2 and 33.3.
 */
public final class VaultKey {

    private VaultKey() {}

    /**
     * Computes the vault key for the given normalized vault path.
     *
     * @param normalizedVaultPath normalized absolute vault path without trailing slash
     * @return vault key string of format ^[0-9a-f]{16}-[a-z0-9-]+$
     */
    public static String compute(String normalizedVaultPath) {
        String hash16 = computeHash16(normalizedVaultPath);
        String name40 = computeName40(normalizedVaultPath);
        return hash16 + "-" + name40;
    }

    /**
     * Computes the first 16 lowercase hex characters of SHA-256 of the normalized path.
     */
    public static String computeHash16(String normalizedVaultPath) {
        try {
            MessageDigest md = MessageDigest.getInstance("SHA-256");
            byte[] digest = md.digest(normalizedVaultPath.getBytes(StandardCharsets.UTF_8));
            StringBuilder sb = new StringBuilder(16);
            for (int i = 0; i < 8; i++) {
                sb.append(String.format("%02x", digest[i]));
            }
            return sb.toString();
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException("SHA-256 not available", e);
        }
    }

    /**
     * Computes the sanitized folder name piece (up to 40 characters).
     */
    public static String computeName40(String normalizedVaultPath) {
        if ("/".equals(normalizedVaultPath) || normalizedVaultPath.isEmpty()) {
            return "vault";
        }

        int lastSlash = normalizedVaultPath.lastIndexOf('/');
        String lastSegment;
        if (lastSlash >= 0) {
            lastSegment = normalizedVaultPath.substring(lastSlash + 1);
        } else {
            lastSegment = normalizedVaultPath;
        }

        if (lastSegment.isEmpty()) {
            return "vault";
        }

        // 3. 마지막 경로 조각(폴더 이름)을 로캘 없이 소문자로 바꾼다 (Locale.ROOT)
        String lower = lastSegment.toLowerCase(Locale.ROOT);

        // 4. UTF-16 단위마다 ASCII a-z, 0-9 아니면 -로 바꾸고
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < lower.length(); i++) {
            char c = lower.charAt(i);
            if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) {
                sb.append(c);
            } else {
                sb.append('-');
            }
        }

        // 이어진 -를 1개로 줄이고
        String s = sb.toString().replaceAll("-+", "-");

        // 앞뒤 -를 떼고
        while (s.startsWith("-")) {
            s = s.substring(1);
        }
        while (s.endsWith("-")) {
            s = s.substring(0, s.length() - 1);
        }

        // 앞 40자로 자르고
        if (s.length() > 40) {
            s = s.substring(0, 40);
        }

        // 끝 -를 다시 뗀다
        while (s.endsWith("-")) {
            s = s.substring(0, s.length() - 1);
        }

        // 비면 vault다
        if (s.isEmpty()) {
            return "vault";
        }
        return s;
    }
}
