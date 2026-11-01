import { useEffect, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { markdownHTML, imageSources, ReadMarkdown } from "./ReadMarkdown.tsx";
import type { SanitizedHTML } from "./ReadMarkdown.tsx";
import { relativeFile } from "./model.ts";
import { editorHost } from "../editor/platform.ts";
import { Page } from "./OtherPages.tsx";
export function ExternalFile({
  path,
  controller,
}: {
  path: string;
  controller: WorkspaceController;
}) {
  const [result, setResult] = useState<{ html?: SanitizedHTML; error?: string }>({});
  useEffect(() => {
    let live = true;
    if (path)
      void controller
        .callHost<string>("read_external_md", { path })
        .then(async (body) => {
          if (typeof body !== "string") throw Error("파일을 읽을 수 없어요");
          const html = markdownHTML(body, true),
            srcs = imageSources(html);
          if (srcs.length)
            await controller
              .callHost("allow_external_images", { mdPath: path, srcs })
              .catch((error) =>
                console.warn("그림 범위를 열지 못했어요", error),
              );
          if (live) setResult({ html });
        })
        .catch((error) => {
          if (live)
            setResult({
              error:
                typeof error === "string"
                  ? error
                  : error instanceof Error
                    ? error.message
                    : "파일을 읽을 수 없어요",
            });
        });
    return () => {
      live = false;
    };
  }, [path, controller]);
  const host = editorHost();
  return (
    <Page
      title={path ? path.split("/").pop() || path : "이 파일을 열 수 없어요"}
    >
      <span className="format-badge">읽기 전용</span>
      <p className="file-path">{path}</p>
      {!path ? (
        <p className="ext-md-error">열 파일 경로가 없어요</p>
      ) : result.error ? (
        <p className="ext-md-error" role="alert">
          {result.error}
        </p>
      ) : result.html === undefined ? (
        <p>불러오는 중이에요</p>
      ) : (
        <ReadMarkdown
          preparedHTML={result.html}
          directory={path.slice(0, path.lastIndexOf("/"))}
          native={controller.isNative()}
          external
          onLink={(url) => {
            if (/^[a-z][a-z\d+.-]*:/i.test(url)) {
              if (/^(https?:|mailto:|tel:)/i.test(url)) void host.openURL(url);
              else return false;
              return;
            }
            const next = relativeFile(path, url);
            if (next)
              controller.navigate("/file?path=" + encodeURIComponent(next));
          }}
        />
      )}
    </Page>
  );
}
