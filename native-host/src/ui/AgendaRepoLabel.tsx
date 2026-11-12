// 아젠다 화면에 보이는 저장소 글. 아젠다가 프로젝트의 저장소(projectRepo)를 가리키면 그 항목을, 아니면 옛 repo 글을 보인다.
// 가리킨 이름이 프로젝트에서 사라졌으면 옛 repo 글로 물러나고 그 사실을 덧붙인다.
import type { Row } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { useGithub } from "./githubStore.ts";
import { agendaRepoView } from "./repoLinks.ts";

export function AgendaRepoLabel({ agenda, projects }: { agenda: Row; projects: Row[] }) {
  const github = useGithub();
  const view = agendaRepoView(agenda, projects, github.status?.cloneRoot);
  if (!view.dangling) return <>{view.label}</>;
  const lost = asText(agenda.projectRepo);
  return (
    <>
      {view.label}
      {view.label ? " " : ""}({lost}는 프로젝트에서 빠졌어요)
    </>
  );
}
