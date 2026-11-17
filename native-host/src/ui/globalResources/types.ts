export type ResourceType = 'jira' | 'github' | 'database' | 'grafana' | 'task' | 'custom';

export interface ProcessAgent {
  id: string;
  key: string;
  name: string;
  role: string;
  description: string;
  skills: string[];
  resources: string[];
  prompt: string;
  enabled: boolean;
  model: string;
  order: number;
}

export interface GlobalSkill {
  id: string;
  name: string;
  description: string;
  body: string;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

export interface GlobalResourceItem {
  id: string;
  name: string;
  type: ResourceType;
  isMock: boolean;
  endpoint: string;
  authType: 'none' | 'token' | 'basic' | 'oauth';
  config: string;
  description: string;
  status: 'active' | 'inactive' | 'mocking';
  createdAt: string;
  updatedAt: string;
}

export interface GlobalResourcesData {
  agents: ProcessAgent[];
  skills: GlobalSkill[];
  resources: GlobalResourceItem[];
  version: number;
}

export const DEFAULT_PROCESS_AGENTS: ProcessAgent[] = [
  {
    id: 'proc-orchestrator',
    key: 'orchestrator',
    name: '공통 개발 프로세스 오케스트레이터',
    role: '전체 개발 프로세스 라이프사이클 총괄 조율',
    description: '요구사항 확정부터 브랜치 관리, 검사, PR 발행 및 작업 자원 정리까지 개발 프로세스 7단계를 순차적으로 통제하고 관리합니다.',
    skills: ['gskill-ai-sync', 'gskill-project-manager'],
    resources: ['gres-jira', 'gres-github', 'gres-task'],
    prompt: `당신은 공통 개발 프로세스 총괄 오케스트레이터입니다.
각 작업 단계별로 전문 에이전트(티켓, 개발, QA, PR 리뷰&머지, 클린업, 리포트 등)를 지휘하며 프로세스 준수 여부를 확인하고 단계 전환을 통제합니다.`,
    enabled: true,
    model: 'pro',
    order: 1,
  },
  {
    id: 'proc-ticket',
    key: 'ticket',
    name: '티겟 에이전트',
    role: '요구사항 확인 및 이슈/티켓 생명주기 관리',
    description: '요구사항에 따른 작업 티켓을 조회·생성하고, 진행 상태 및 마일스톤 연동을 담당합니다.',
    skills: ['gskill-project-manager'],
    resources: ['gres-jira'],
    prompt: `당신은 티켓 관리 전문 에이전트입니다.
요구사항을 바탕으로 중복 없는 표준 형식의 작업 항목을 생성하고 진행 상황을 투명하게 기록합니다.`,
    enabled: true,
    model: 'inherit',
    order: 2,
  },
  {
    id: 'proc-commit-push',
    key: 'commit-push',
    name: '커밋 푸시 에이전트',
    role: '규격 커밋 메시지 작성 및 원격 저장소 푸시',
    description: '변경사항을 분석하여 표준 커밋 메시지(type(scope): subject [ID])를 생성하고 최신 기준 브랜치 위에 원격 push합니다.',
    skills: ['gskill-ai-commit'],
    resources: ['gres-github'],
    prompt: `당신은 커밋과 푸시를 전담하는 에이전트입니다.
반드시 변경된 파일만을 대상으로 논리적 단위 커밋을 생성하며, 규격 포맷을 철저히 지킵니다.`,
    enabled: true,
    model: 'inherit',
    order: 3,
  },
  {
    id: 'proc-dev',
    key: 'dev',
    name: '개발 에이전트',
    role: '비즈니스 로직 및 기능 구현',
    description: '확정된 요구사항에 따라 격리된 작업 공간에서 소스 코드, 설정, 의존성을 안전하게 구현·수정합니다.',
    skills: [],
    resources: ['gres-database', 'gres-task'],
    prompt: `당신은 소스 코드 구현 전문 개발 에이전트입니다.
프로젝트의 코딩 컨벤션과 구조를 준수하며 결함 없는 고품질 코드를 작성합니다.`,
    enabled: true,
    model: 'inherit',
    order: 4,
  },
  {
    id: 'proc-qa',
    key: 'qa',
    name: 'QA 에이전트',
    role: '단위·통합 테스트 및 품질 검증',
    description: '수정된 코드의 테스트 코드를 검토하고 회귀 버그 방지를 위한 정적 검사 및 테스트를 실행합니다.',
    skills: [],
    resources: ['gres-database'],
    prompt: `당신은 품질 보증(QA) 전문 에이전트입니다.
다양한 엣지 케이스와 오류 시나리오를 검증하고 테스트 커버리지를 보장합니다.`,
    enabled: true,
    model: 'inherit',
    order: 5,
  },
  {
    id: 'proc-pr-review-merge',
    key: 'pr-review-merge',
    name: 'PR 리뷰 & 머지 에이전트',
    role: '풀 리퀘스트 생성 및 코드 리뷰 점검',
    description: '최종 변경에 대해 PR을 작성하고 변경 위험도(Merge Danger) 및 증적(Evidence)을 종합 검토합니다.',
    skills: [],
    resources: ['gres-github'],
    prompt: `당신은 풀 리퀘스트(PR) 리뷰 및 머지 검토 에이전트입니다.
Summary, Evidence, Merge Danger 3요소를 꼼꼼히 점검하고 리뷰 의견을 제시합니다.`,
    enabled: true,
    model: 'inherit',
    order: 6,
  },
  {
    id: 'proc-issue-raise',
    key: 'issue-raise',
    name: '이슈 레이즈 에이전트',
    role: '발견된 잠재 결함 및 개선점 이슈 발행',
    description: '개발 또는 검증 중 발견된 부가 이슈, 기술 부채, 장애 요소를 추적 티켓으로 등록합니다.',
    skills: ['gskill-project-manager'],
    resources: ['gres-jira', 'gres-github'],
    prompt: `당신은 결함 및 기술 부채를 식별하고 이슈를 발행하는 에이전트입니다.
재현 경로와 영향도를 명확히 기술한 이슈를 등록합니다.`,
    enabled: true,
    model: 'inherit',
    order: 7,
  },
  {
    id: 'proc-cleanup',
    key: 'cleanup',
    name: '클린업 에이전트',
    role: '작업 워크트리 및 임시 자원 정리',
    description: '작업 완료 후 격리 워크트리, 테스트 프로세스, 임시 컨테이너 및 브랜치를 안전하게 정리합니다.',
    skills: [],
    resources: ['gres-task'],
    prompt: `당신은 작업 자원 정리 전문 에이전트입니다.
미커밋 변경이나 보관할 파일이 없는지 확인하고 이번 세션이 생성한 격리 자원만 선별 정리합니다.`,
    enabled: true,
    model: 'inherit',
    order: 8,
  },
  {
    id: 'proc-report',
    key: 'report',
    name: '리포트 에이전트',
    role: '작업 완료 요약 및 사용자 보고서 작성',
    description: '작업 결과, 바뀐 핵심 흐름, 검사 증적, 머지 후 사용자 조치 사항을 간결한 보고서로 제공합니다.',
    skills: [],
    resources: ['gres-grafana'],
    prompt: `당신은 작업 보고서 작성 전문 에이전트입니다.
사용자가 한눈에 이해할 수 있도록 결론 우선의 명확한 요약 보고서를 작성합니다.`,
    enabled: true,
    model: 'inherit',
    order: 9,
  },
  {
    id: 'proc-final-inspect',
    key: 'final-inspect',
    name: '최종 검수 에이전트',
    role: '프로세스 준수 여부 및 최종 무결성 검수',
    description: '빌드 성공 여부, 문서 최신화, 토큰 누출 방지 및 공통 규칙 체크리스트를 최종 검수합니다.',
    skills: ['gskill-ai-sync'],
    resources: ['gres-jira', 'gres-github'],
    prompt: `당신은 최종 검수 에이전트입니다.
프로젝트 공통 규칙의 체크리스트가 누락 없이 이행되었는지 엄격하게 최종 점검합니다.`,
    enabled: true,
    model: 'inherit',
    order: 10,
  },
];

export const DEFAULT_GLOBAL_SKILLS: GlobalSkill[] = [
  {
    id: 'gskill-ai-commit',
    name: 'ai-commit',
    description: '규격 커밋 메시지를 생성하고 원격 저장소에 안전하게 푸시합니다.',
    body: `# ai-commit 스킬
형식: type(scope): subject [Jira-ID]
1. 변경 파일 확인
2. 빌드 및 타입 검사
3. 규격 커밋 생성 및 push`,
    tags: ['git', 'commit', 'automation'],
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
  {
    id: 'gskill-ai-sync',
    name: 'ai-sync',
    description: '공유 AI 리소스(규칙, 컨벤션, 스킬, 가이드)를 동기화합니다.',
    body: `# ai-sync 스킬
공통 지침 및 컨벤션을 최신 상태로 동기화하고 변경사항을 적용합니다.`,
    tags: ['rules', 'sync', 'governance'],
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
  {
    id: 'gskill-project-manager',
    name: 'project-manager',
    description: '요구사항 기반으로 이슈 백로그 티켓을 생성하고 마일스톤을 관리합니다.',
    body: `# project-manager 스킬
요구사항을 분석하여 명확한 완료 기준을 가진 백로그 티켓을 발급합니다.`,
    tags: ['jira', 'issue', 'management'],
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
];

export const DEFAULT_GLOBAL_RESOURCES: GlobalResourceItem[] = [
  {
    id: 'gres-jira',
    name: 'jira (mock)',
    type: 'jira',
    isMock: true,
    endpoint: 'https://jira.company.mock/rest/api/2',
    authType: 'token',
    config: JSON.stringify({ projectKey: 'AIS', defaultBoardId: 101 }, null, 2),
    description: 'Jira 이슈 트래커 (Mock 모드: 가상 티켓 연동 및 상태 시뮬레이션)',
    status: 'mocking',
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
  {
    id: 'gres-task',
    name: 'aisurada task',
    type: 'task',
    isMock: false,
    endpoint: 'aisurada://tasks/internal',
    authType: 'none',
    config: JSON.stringify({ runner: 'native', parallelLimit: 4 }, null, 2),
    description: 'Aisurada 내장 태스크 러너 및 비동기 작업 큐',
    status: 'active',
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
  {
    id: 'gres-github',
    name: 'github',
    type: 'github',
    isMock: false,
    endpoint: 'https://api.github.com/repos/company/aisurada',
    authType: 'token',
    config: JSON.stringify({ baseBranch: 'main', prRequireReviews: 1 }, null, 2),
    description: 'GitHub 원격 저장소, 브랜치 관리 및 PR 리뷰 연동',
    status: 'active',
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
  {
    id: 'gres-database',
    name: 'database',
    type: 'database',
    isMock: false,
    endpoint: 'postgresql://localhost:5432/aisurada_dev',
    authType: 'basic',
    config: JSON.stringify({ schema: 'public', poolSize: 10 }, null, 2),
    description: '개발 및 테스트 환경 데이터베이스 인프라',
    status: 'active',
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
  {
    id: 'gres-grafana',
    name: 'grafana (mock)',
    type: 'grafana',
    isMock: true,
    endpoint: 'https://grafana.company.mock/api',
    authType: 'token',
    config: JSON.stringify({ dashboardId: 'build-metrics', alertWebhook: true }, null, 2),
    description: 'Grafana 모니터링 메트릭 및 대시보드 (Mock 모드)',
    status: 'mocking',
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
];
