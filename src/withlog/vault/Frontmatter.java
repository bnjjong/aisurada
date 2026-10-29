package withlog.vault;

import org.yaml.snakeyaml.LoaderOptions;
import org.yaml.snakeyaml.Yaml;
import org.yaml.snakeyaml.constructor.SafeConstructor;
import org.yaml.snakeyaml.error.YAMLException;
import org.yaml.snakeyaml.nodes.Tag;
import org.yaml.snakeyaml.resolver.Resolver;

import java.util.Map;

/**
 * SnakeYAML 2.4 기반의 안전 프론트매터 파서.
 * 설명서 1.8에 따라 암시적 타임스탬프 해석을 비활성화하고 안전 생성자(SafeConstructor)를 사용합니다.
 */
public final class Frontmatter {

    private static class NoTimestampResolver extends Resolver {
        @Override
        protected void addImplicitResolvers() {
            addImplicitResolver(Tag.BOOL, BOOL, "yYnNtTfFoO", 10);
            addImplicitResolver(Tag.INT, INT, "-+0123456789");
            addImplicitResolver(Tag.FLOAT, FLOAT, "-+0123456789.");
            addImplicitResolver(Tag.MERGE, MERGE, "<", 10);
            addImplicitResolver(Tag.NULL, NULL, "~nN\0", 10);
            addImplicitResolver(Tag.NULL, EMPTY, null, 10);
            // TIMESTAMP(시각 해석)는 의도적으로 제외
            addImplicitResolver(Tag.YAML, YAML, "!&*", 10);
        }
    }

    private static final LoaderOptions LOADER_OPTIONS;
    static {
        LOADER_OPTIONS = new LoaderOptions();
        LOADER_OPTIONS.setMaxAliasesForCollections(50);
        // 1.8: 스칼라까지 노드마다 깊이를 세어 깊이 52의 노드에서 선다. SnakeYAML은 상한+1까지 받아서 50이다
        LOADER_OPTIONS.setNestingDepthLimit(50);
    }

    private Frontmatter() {}

    /** 휴지통 등 파일 전체를 읽을 때도 암시적 시각 해석을 끈 같은 읽개를 쓴다. */
    public static Yaml yamlWithoutTimestamps(LoaderOptions options) {
        return new Yaml(new SafeConstructor(options), new org.yaml.snakeyaml.representer.Representer(new org.yaml.snakeyaml.DumperOptions()), new org.yaml.snakeyaml.DumperOptions(), options, new NoTimestampResolver());
    }

    /**
     * YAML 조각을 파싱하여 Map으로 반환합니다.
     * 설명서 1.8 및 1.15의 6번에 따라 YAMLException만 잡아서 null(프론트매터 없음)을 반환하고,
     * NumberFormatException 등 Java 런타임 예외는 잡지 않고 전파합니다.
     * @param yamlText YAML 텍스트 조각
     * @return 파싱된 Map 객체, 파싱 실패 시 null
     */
    @SuppressWarnings("unchecked")
    public static Map<String, Object> parse(String yamlText) {
        if (yamlText == null || yamlText.isEmpty()) {
            return null;
        }

        Yaml yaml = yamlWithoutTimestamps(LOADER_OPTIONS);

        Object loaded;
        try {
            loaded = yaml.load(yamlText);
        } catch (YAMLException e) {
            // 잡히는 실패: 문법 오류, 별칭/중첩 상한 초과, 모르는 태그 등 -> 프론트매터 없음
            return null;
        }
        // 그 외 NumberFormatException, ClassCastException 등은 전파됨 (1.15의 6번)

        if (loaded instanceof Map<?, ?> map) {
            return (Map<String, Object>) map;
        }
        return null;
    }
}
