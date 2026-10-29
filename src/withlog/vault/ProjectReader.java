package withlog.vault;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.write.IdGenerator;

import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * 프로젝트 폴더(.withlog/projects/) 훑기. 설명서 2.2의 프로젝트 파일 읽기 규칙입니다.
 * 지금 엔진은 프로젝트를 색인에만 두고 이 파일을 안 읽지만, 색인이 없는 판은 파일에서 읽을 수밖에 없다(8.6).
 * 그래서 2.2가 「다시 짓는 구현의 출발점」으로 적은 엔진 소스의 읽개 규칙을 따른다.
 */
public final class ProjectReader {

    /** 2.2의 크기 상한. 아젠다(1,000,000바이트)보다 작다. */
    private static final long MAX_BYTES = 100_000;

    /**
     * 프로젝트 1개. 색인의 프로젝트 표 행 1개와 같은 칸이다(8.1).
     *
     * @param summary 본문. 닫는 울타리 줄의 줄바꿈 1개 뒤부터 받은 그대로다
     */
    public record ProjectInfo(String id, String slug, String title, String hue, String summary) {}

    private ProjectReader() {}

    /**
     * {@code .withlog/projects/} 아래 {@code .md} 파일을 이름 차례로 읽어 프로젝트를 id마다 1개로 모읍니다.
     * 폴더가 없거나 폴더가 아니면 빈 목록이다. 목록을 못 열면 WL-500이다.
     * <p>
     * 행 고르기는 색인 표에 이름 차례로 넣는 것과 같다. 같은 id의 파일 2개는 뒤 파일이 행을 덮고,
     * 이미 다른 id가 쓰는 slug를 든 파일은 slug 유일 제약에 걸린 것처럼 건너뛴다. 아젠다와 달리
     * 맞추기 전체를 세우지 않는다. 프로젝트 표는 색인 맞추기에 안 들어서(21.5) 그 파일 때문에 다른 명령이 서면 안 되기 때문이다.
     *
     * @param vaultRoot vault 루트 경로
     * @return id에서 프로젝트로. 차례는 행을 처음 넣은 차례다
     */
    public static Map<String, ProjectInfo> scanProjects(Path vaultRoot) {
        Path projectsDir = vaultRoot.resolve(".withlog/projects");
        Map<String, ProjectInfo> idToProject = new LinkedHashMap<>();
        if (!Files.isDirectory(projectsDir)) {
            return idToProject;
        }

        List<Path> files = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(projectsDir)) {
            for (Path entry : ds) {
                if (Files.isRegularFile(entry) && entry.getFileName().toString().endsWith(".md")) {
                    files.add(entry);
                }
            }
        } catch (IOException e) {
            throw new WlException(500, "프로젝트 폴더를 읽지 못했다: " + projectsDir.toAbsolutePath().normalize());
        }
        files.sort((a, b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));

        Map<String, String> slugToId = new HashMap<>();
        for (Path file : files) {
            ProjectInfo info = read(file);
            if (info == null) {
                continue;
            }
            String owner = slugToId.get(info.slug());
            if (owner != null && !owner.equals(info.id())) {
                continue;
            }
            ProjectInfo previous = idToProject.put(info.id(), info);
            if (previous != null) {
                slugToId.remove(previous.slug());
            }
            slugToId.put(info.slug(), info.id());
        }
        return idToProject;
    }

    /**
     * 파일 1개를 2.2로 읽습니다. 건너뛸 파일이면 null이다. 건너뛴 파일은 어느 응답에도 안 싣는다.
     * 2.1의 1번부터 9번과 같고 다른 곳은 3자리다. 크기 상한이 100,000바이트이고, id는 프로젝트 id 모양이고,
     * 제목이 비면 본문 첫 줄을 안 보고 바로 slug로 물러선다.
     */
    private static ProjectInfo read(Path file) {
        String content;
        try {
            if (Files.size(file) > MAX_BYTES) {
                return null;
            }
            content = StrictUtf8.decodeStrict(Files.readAllBytes(file));
        } catch (Exception e) {
            return null;
        }
        if (content.startsWith("﻿")) {
            content = content.substring(1);
        }
        AgendaReader.Fenced fenced = AgendaReader.splitFence(content);
        if (fenced == null) {
            return null;
        }
        Map<String, String> kv = AgendaReader.parseLineReader(fenced.frontmatter());

        String fileName = file.getFileName().toString();
        String slug = Trim.trim(kv.get("slug"));
        if (slug.isEmpty()) {
            slug = fileName.substring(0, fileName.length() - 3);
        }
        if (!AgendaReader.isValidSlugSegment(slug)) {
            return null;
        }
        String id = Trim.trim(kv.get("id"));
        if (!IdGenerator.isValidProjectId(id)) {
            return null;
        }
        String title = Trim.trim(kv.get("title"));
        if (title.isEmpty()) {
            title = slug;
        }
        String hue = Trim.trim(kv.get("hue"));
        if (!AgendaReader.HUES.contains(hue)) {
            hue = "blue";
        }
        return new ProjectInfo(id, slug, title, hue, summaryOf(fenced.body()));
    }

    /**
     * 본문에서 닫는 울타리 줄의 줄바꿈 1개(LF나 CRLF)를 뗀 글. 다듬지 않는다.
     * 지금 엔진의 색인은 받은 summary를 그대로 들어서(21.6), 엔진이 쓴 파일을 이렇게 읽어야 같은 글이 나온다.
     */
    private static String summaryOf(String body) {
        if (body.startsWith("\r\n")) {
            return body.substring(2);
        }
        if (body.startsWith("\n")) {
            return body.substring(1);
        }
        return body;
    }
}
