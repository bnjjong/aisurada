import type { ResourceRow, SaveInput } from '../resourcesClient.ts';

export type ResourceType = 'jira' | 'github' | 'database' | 'grafana' | 'task' | 'custom';

export type AIEngine = 'claude' | 'codex' | 'gemini';

export const AI_ENGINES: readonly { id: AIEngine; label: string; company: string }[] = [
  { id: 'gemini', label: 'Gemini', company: 'Google' },
  { id: 'claude', label: 'Claude', company: 'Anthropic' },
  { id: 'codex', label: 'Codex', company: 'OpenAI' },
];

export interface ModelInfo {
  id: string;
  label: string;
}

export type ReasoningEffort = 'default' | 'low' | 'medium' | 'high' | 'xhigh' | 'max';

export interface EffortInfo {
  id: ReasoningEffort;
  label: string;
}

export const REASONING_EFFORTS: readonly EffortInfo[] = [
  { id: 'default', label: 'default (기본값)' },
  { id: 'low', label: 'low (최소 추론·고속)' },
  { id: 'medium', label: 'medium (표준 추론)' },
  { id: 'high', label: 'high (심층 추론)' },
  { id: 'xhigh', label: 'xhigh (최고 강도 추론)' },
  { id: 'max', label: 'max (최대 한계 추론)' },
] as const;

export const DEFAULT_EFFORT: ReasoningEffort = 'default';

export const ENGINE_MODELS: Record<AIEngine, ModelInfo[]> = {
  gemini: [
    { id: 'gemini-3.8-flash', label: 'Gemini 3.8 Flash (current, 코딩·에이전트 워크호스)' },
    { id: 'gemini-4-carbon', label: 'Gemini 4 Carbon (차세대 플래그십)' },
    { id: 'gemini-4-argon', label: 'Gemini 4 Argon (차세대 고성능 미드레인지)' },
    { id: 'gemini-4-hydrogen', label: 'Gemini 4 Hydrogen (차세대 초경량)' },
    { id: 'gemini-3-pro', label: 'Gemini 3 Pro (복합 심층 추론)' },
    { id: 'gemini-2.5-pro', label: 'Gemini 2.5 Pro (Thinking 내장 안정판)' },
    { id: 'inherit', label: 'inherit (기본 모델 상속)' },
  ],
  claude: [
    { id: 'claude-sonnet-5.5', label: 'Sonnet 5.5 (권장, 코딩·에이전트 최고 효율 표준)' },
    { id: 'claude-opus-5.5', label: 'Opus 5.5 (복합 고난도 작업 및 종합 지능)' },
    { id: 'claude-fable-5.2', label: 'Fable 5.2 (For your toughest challenges, 심층 한계 극복)' },
    { id: 'claude-haiku-5.5', label: 'Haiku 5.5 (초고속 즉답·경량 작업)' },
    { id: 'claude-sonnet-4.6', label: 'Claude Sonnet 4.6 (Thinking)' },
    { id: 'claude-opus-4.6', label: 'Claude Opus 4.6 (Thinking)' },
    { id: 'inherit', label: 'inherit (기본 모델 상속)' },
  ],
  codex: [
    { id: 'gpt-6.1-sol', label: 'GPT-6.1-Sol (current, 최신 코딩·일상 업무 워크호스)' },
    { id: 'gpt-6-astra', label: 'GPT-6-Astra (Frontier intelligence, 최고 난도 작업)' },
    { id: 'gpt-6.1-terra', label: 'GPT-6.1 Terra (Codex 전문 모델)' },
    { id: 'gpt-6-sol', label: 'GPT-6-Sol (이전 세대 워크호스 모델)' },
    { id: 'gpt-6-luna', label: 'GPT-6-Luna (경량·저비용 간편 작업)' },
    { id: 'gpt-oss-120b', label: 'GPT-OSS 120B (Medium 오픈소스 추론)' },
    { id: 'gpt-5.6-sol', label: 'GPT-5.6 Sol (구세대 워크호스)' },
    { id: 'inherit', label: 'inherit (기본 모델 상속)' },
  ],
};

export const MODEL_PRESETS: Record<AIEngine, string[]> = {
  gemini: ENGINE_MODELS.gemini.map((m) => m.id),
  claude: ENGINE_MODELS.claude.map((m) => m.id),
  codex: ENGINE_MODELS.codex.map((m) => m.id),
};

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
  engine: AIEngine;
  model: string;
  effort?: ReasoningEffort;
  fallbackEngine?: AIEngine;
  fallbackModel?: string;
  fallbackEffort?: ReasoningEffort;
  order: number;
  f1?: string;
  path?: string;
  /** `dirty`면 승인한 뒤에 파일이 직접 고쳐진 것이다. 이 경우 화면의 값은 승인해 둔 옛 판이다. */
  approval?: 'dirty';
}

export interface GlobalSkill {
  id: string;
  name: string;
  description: string;
  body: string;
  tags: string[];
  createdAt: string;
  updatedAt: string;
  f1?: string;
  path?: string;
  /** `dirty`면 승인한 뒤에 파일이 직접 고쳐진 것이다. 이 경우 화면의 값은 승인해 둔 옛 판이다. */
  approval?: 'dirty';
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
    engine: 'gemini',
    model: 'gemini-3.8-flash',
    effort: 'high',
    fallbackEngine: 'claude',
    fallbackModel: 'claude-sonnet-5.5',
    fallbackEffort: 'xhigh',
    order: 1,
  },
  {
    id: 'proc-ticket',
    key: 'ticket',
    name: '티켓 에이전트',
    role: '요구사항 확인 및 이슈/티켓 생명주기 관리',
    description: '요구사항에 따른 작업 티켓을 조회·생성하고, 진행 상태 및 마일스톤 연동을 담당합니다.',
    skills: ['gskill-project-manager'],
    resources: ['gres-jira'],
    prompt: `당신은 티켓 관리 전문 에이전트입니다.
요구사항을 바탕으로 중복 없는 표준 형식의 작업 항목을 생성하고 진행 상황을 투명하게 기록합니다.`,
    enabled: true,
    engine: 'claude',
    model: 'claude-sonnet-5.5',
    effort: 'medium',
    fallbackEngine: 'gemini',
    fallbackModel: 'gemini-3.8-flash',
    fallbackEffort: 'high',
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
    engine: 'codex',
    model: 'gpt-6.1-sol',
    effort: 'low',
    fallbackEngine: 'gemini',
    fallbackModel: 'gemini-3.8-flash',
    fallbackEffort: 'medium',
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
    engine: 'claude',
    model: 'claude-sonnet-5.5',
    effort: 'high',
    fallbackEngine: 'gemini',
    fallbackModel: 'gemini-3.8-flash',
    fallbackEffort: 'high',
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
    engine: 'gemini',
    model: 'gemini-3.8-flash',
    effort: 'high',
    fallbackEngine: 'claude',
    fallbackModel: 'claude-sonnet-5.5',
    fallbackEffort: 'high',
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
    engine: 'claude',
    model: 'claude-sonnet-5.5',
    effort: 'xhigh',
    fallbackEngine: 'gemini',
    fallbackModel: 'gemini-3.8-flash',
    fallbackEffort: 'high',
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
    engine: 'claude',
    model: 'claude-fable-5.2',
    effort: 'high',
    fallbackEngine: 'gemini',
    fallbackModel: 'gemini-3.8-flash',
    fallbackEffort: 'medium',
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
    engine: 'codex',
    model: 'gpt-6-luna',
    effort: 'low',
    fallbackEngine: 'gemini',
    fallbackModel: 'gemini-3.8-flash',
    fallbackEffort: 'low',
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
    engine: 'gemini',
    model: 'gemini-3.8-flash',
    effort: 'medium',
    fallbackEngine: 'claude',
    fallbackModel: 'claude-sonnet-5.5',
    fallbackEffort: 'medium',
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
    engine: 'gemini',
    model: 'gemini-4-carbon',
    effort: 'max',
    fallbackEngine: 'claude',
    fallbackModel: 'claude-opus-5.5',
    fallbackEffort: 'max',
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
    config: JSON.stringify({ owner: 'company', repo: 'aisurada' }, null, 2),
    description: 'GitHub 원격 저장소 및 PR/이슈 연동',
    status: 'active',
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
  {
    id: 'gres-database',
    name: 'database',
    type: 'database',
    isMock: true,
    endpoint: 'postgresql://localhost:5432/aisurada_dev',
    authType: 'basic',
    config: JSON.stringify({ maxPool: 10, timeoutMs: 5000 }, null, 2),
    description: '개발용 격리 데이터베이스 인스턴스',
    status: 'mocking',
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
  {
    id: 'gres-grafana',
    name: 'grafana',
    type: 'grafana',
    isMock: true,
    endpoint: 'https://grafana.internal.net',
    authType: 'token',
    config: JSON.stringify({ dashboardId: 'dev-metrics' }, null, 2),
    description: '작업 메트릭 및 빌드 모니터링 대시보드',
    status: 'mocking',
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  },
];

// --------------------------- 엔진 변환 헬퍼 ---------------------------

export function processAgentToSaveInput(agent: ProcessAgent): SaveInput {
  const effort = agent.effort && agent.effort !== 'default' ? agent.effort : undefined;
  const fallbackEffort = agent.fallbackEffort && agent.fallbackEffort !== 'default' ? agent.fallbackEffort : undefined;

  const toolsObj: Record<string, unknown> = {
    claude: {
      model:
        agent.engine === 'claude'
          ? agent.model
          : agent.fallbackEngine === 'claude'
          ? (agent.fallbackModel || MODEL_PRESETS.claude[0])
          : MODEL_PRESETS.claude[0],
      ...(agent.engine === 'claude' && effort ? { effort } : {}),
      ...(agent.fallbackEngine === 'claude' && fallbackEffort ? { effort: fallbackEffort } : {}),
    },
    codex: {
      model:
        agent.engine === 'codex'
          ? agent.model
          : agent.fallbackEngine === 'codex'
          ? (agent.fallbackModel || MODEL_PRESETS.codex[0])
          : MODEL_PRESETS.codex[0],
      ...(agent.engine === 'codex' && effort ? { effort } : {}),
      ...(agent.fallbackEngine === 'codex' && fallbackEffort ? { effort: fallbackEffort } : {}),
    },
    gemini: {
      model:
        agent.engine === 'gemini'
          ? agent.model
          : agent.fallbackEngine === 'gemini'
          ? (agent.fallbackModel || MODEL_PRESETS.gemini[0])
          : MODEL_PRESETS.gemini[0],
      ...(agent.engine === 'gemini' && effort ? { effort } : {}),
      ...(agent.fallbackEngine === 'gemini' && fallbackEffort ? { effort: fallbackEffort } : {}),
    },
    aisurada: {
      key: agent.key,
      role: agent.role,
      engine: agent.engine,
      model: agent.model,
      effort: agent.effort || 'default',
      fallbackEngine: agent.fallbackEngine || null,
      fallbackModel: agent.fallbackModel || null,
      fallbackEffort: agent.fallbackEffort || 'default',
      resources: agent.resources,
      order: agent.order,
      enabled: agent.enabled,
    },
  };

  const input: SaveInput = {
    kind: 'agent',
    scope: 'global',
    name: agent.name,
    description: agent.description,
    body: agent.prompt || '# 지침\n' + agent.description,
    permission: 'edit',
    skills: agent.skills,
    tools: JSON.stringify(toolsObj),
  };

  if (agent.id && !agent.id.startsWith('proc-')) {
    input.id = agent.id;
  }
  if (agent.f1) {
    input.f1 = agent.f1;
  }
  return input;
}

export function resourceRowToProcessAgent(row: ResourceRow, index = 0): ProcessAgent {
  const tools = (row.tools || {}) as Record<string, any>;
  const aisuradaMeta = tools.aisurada || {};

  const engine: AIEngine = (['gemini', 'claude', 'codex'].includes(aisuradaMeta.engine)
    ? aisuradaMeta.engine
    : 'gemini') as AIEngine;

  const model: string = aisuradaMeta.model || tools[engine]?.model || MODEL_PRESETS[engine][0];
  const role: string = aisuradaMeta.role || row.description || '';
  const key: string = aisuradaMeta.key || row.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const resources: string[] = Array.isArray(aisuradaMeta.resources) ? aisuradaMeta.resources : [];
  const order: number = typeof aisuradaMeta.order === 'number' ? aisuradaMeta.order : index + 1;
  const enabled: boolean = aisuradaMeta.enabled !== false;

  const VALID_EFFORTS = ['default', 'low', 'medium', 'high', 'xhigh', 'max'] as const;
  const rawEffort = aisuradaMeta.effort || tools[engine]?.effort;
  const effort: ReasoningEffort = VALID_EFFORTS.includes(rawEffort)
    ? (rawEffort as ReasoningEffort)
    : 'default';

  const rawFallbackEngine = aisuradaMeta.fallbackEngine;
  const fallbackEngine: AIEngine | undefined =
    ['gemini', 'claude', 'codex'].includes(rawFallbackEngine) && rawFallbackEngine !== engine
      ? (rawFallbackEngine as AIEngine)
      : undefined;

  const fallbackModel: string | undefined = fallbackEngine
    ? (typeof aisuradaMeta.fallbackModel === 'string' && aisuradaMeta.fallbackModel.trim().length > 0
        ? aisuradaMeta.fallbackModel.trim()
        : tools[fallbackEngine]?.model || MODEL_PRESETS[fallbackEngine][0])
    : undefined;

  const rawFallbackEffort = aisuradaMeta.fallbackEffort || (fallbackEngine ? tools[fallbackEngine]?.effort : undefined);
  const fallbackEffort: ReasoningEffort | undefined = fallbackEngine
    ? (VALID_EFFORTS.includes(rawFallbackEffort) ? (rawFallbackEffort as ReasoningEffort) : 'default')
    : undefined;

  return {
    id: row.id,
    key,
    name: row.name,
    role,
    description: row.description,
    skills: Array.isArray(row.skills) ? row.skills : [],
    resources,
    prompt: row.body || '',
    enabled,
    engine,
    model,
    effort,
    fallbackEngine,
    fallbackModel,
    fallbackEffort,
    order,
    f1: row.precondition,
    path: row.path,
    approval: row.approval,
  };
}

export function globalSkillToSaveInput(skill: GlobalSkill): SaveInput {
  const input: SaveInput = {
    kind: 'skill',
    scope: 'global',
    name: skill.name,
    description: skill.description,
    body: skill.body || '# ' + skill.name + '\n' + skill.description,
  };
  if (skill.id && !skill.id.startsWith('gskill-')) {
    input.id = skill.id;
  }
  if (skill.f1) {
    input.f1 = skill.f1;
  }
  return input;
}

export function resourceRowToGlobalSkill(row: ResourceRow): GlobalSkill {
  return {
    id: row.id,
    name: row.name,
    description: row.description,
    body: row.body || '',
    tags: [],
    createdAt: row.editedAt || new Date().toISOString(),
    updatedAt: row.editedAt || new Date().toISOString(),
    f1: row.precondition,
    path: row.path,
    approval: row.approval,
  };
}
