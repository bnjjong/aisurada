package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.TemplateReader;

import java.nio.file.Path;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * get_template 및 get-template 명령 실행기.
 * 설명서 24장에 따라 id로 템플릿을 찾아 본문까지 상세 정보를 반환합니다.
 */
public final class GetTemplate {

    private static final Pattern ID_PATTERN = Pattern.compile("^[A-Za-z0-9._-]{1,128}$");

    private GetTemplate() {}

    public static Map<String, Object> execute(
            Path vaultRoot,
            ParsedArgs parsed
    ) {
        String id;
        if (parsed.isToolSurface()) {
            Map<String, Object> opts = parsed.toolOptions();
            id = (opts != null) ? (String) opts.get("id") : null;
            if (id == null) {
                throw new WlException(400, "인자 검증 실패(get_template): id 가 필요하다.");
            }
        } else {
            if (parsed.positionalArgs().isEmpty()) {
                throw new WlException(400, "인자가 하나 필요하다: get-template <템플릿 id>");
            }
            if (parsed.positionalArgs().size() > 1) {
                throw new WlException(400, "get-template 이 받는 자리 인자는 1 개인데 " +
                        parsed.positionalArgs().size() + " 개를 받았다: get-template <템플릿 id>");
            }
            id = parsed.positionalArgs().get(0);
        }

        // 값 검증 (다듬지 않는다)
        if (id.isEmpty() || Trim.isBlank(id)) {
            throw new WlException(400, "인자가 하나 필요하다: get-template <템플릿 id>");
        }
        if (!ID_PATTERN.matcher(id).matches()) {
            throw new WlException(400, "템플릿 id 모양이 아니다: " + id);
        }

        // 템플릿 스캔
        TemplateReader.ScanResult scan = TemplateReader.scan(vaultRoot);

        // 훑기 차례에서 처음 찾은 것을 고름 (24.5 line 9947)
        for (TemplateReader.TemplateRecord t : scan.templates()) {
            if (id.equals(t.id())) {
                return ListTemplates.formatRow(t, true);
            }
        }

        throw new WlException(404, "Template not found: " + id);
    }
}
