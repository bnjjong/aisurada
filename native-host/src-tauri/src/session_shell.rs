use crate::lifecycle::HostState;
use crate::terminal::Started;
use portable_pty::{CommandBuilder, PtySize};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::State;

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

#[derive(Deserialize, Debug)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum SessionAction {
    #[serde(rename = "resume")]
    Resume {
        #[serde(rename = "sessionId")]
        session_id: String,
        cwd: Option<String>,
        /// 세션을 연 에이전트(claude, codex, gemini). 없으면 claude.
        #[serde(default)]
        agent: Option<String>,
    },
    #[serde(rename = "attach")]
    Attach {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
}

impl SessionAction {
    pub fn session_id(&self) -> &str {
        match self {
            Self::Resume { session_id, .. } => session_id,
            Self::Attach { session_id } => session_id,
        }
    }
}

/// 에이전트별로 세션을 이어 여는 명령을 만든다. 아는 에이전트가 아니면 None이다.
/// 세션 id는 호출하기 전에 영문자, 숫자, - 로만 이뤄졌는지 검사하므로 셸에 그대로 넣어도 된다.
/// Gemini는 터미널 안에서 만든 노트가 이 세션을 가리키도록 세션 변수를 함께 넣는다(Gemini 자신은 주지 않는다).
fn resume_command(agent: Option<&str>, session_id: &str) -> Option<String> {
    match agent.unwrap_or("claude") {
        "claude" => Some(format!("claude --resume {}", session_id)),
        "codex" => Some(format!("codex resume {}", session_id)),
        "gemini" => Some(format!(
            "AISURADA_SESSION_ID={id} AISURADA_SESSION_AGENT=gemini gemini --resume {id}",
            id = session_id
        )),
        _ => None,
    }
}

/// 로그인 셸 `-c` 에 넘기는 첫 명령(10.10). `;` 둘레에 빈칸을 둔다.
/// 알림 글에는 작은따옴표와 경로를 넣지 않는다. 셸 주입 자리가 되기 때문이다.
fn first_command(action: &SessionAction, fell_back: bool) -> String {
    const NOTICE: &str = "원래 작업 폴더가 없어서 홈 폴더에서 이어 가요. 이어 간 대화의 파일 명령은 홈을 기준으로 돌아요.";
    match action {
        SessionAction::Resume { session_id, agent, .. } => {
            // 에이전트 이름은 pty_start_session이 먼저 걸러서 여기서는 항상 아는 이름이다
            let resume = resume_command(agent.as_deref(), session_id)
                .unwrap_or_else(|| format!("claude --resume {}", session_id));
            if fell_back {
                format!("printf '%s\\n' '{}' ; {} ; exec zsh -l -i", NOTICE, resume)
            } else {
                format!("{} ; exec zsh -l -i", resume)
            }
        }
        SessionAction::Attach { session_id } => {
            format!("claude attach {} ; exec zsh -l -i", session_id)
        }
    }
}

#[tauri::command]
pub fn pty_start_session(
    state: State<'_, HostState>,
    key: String,
    cols: u16,
    rows: u16,
    session_action: SessionAction,
    on_data: Channel<String>,
) -> Result<Started, String> {
    let sink = Arc::new(move |b: &[u8]| crate::ipc::send_bytes(&on_data, b).map_err(|e| e.to_string()));

    // 1. 이미 존재하는 셸인 경우 다시 붙음 (재접속 시 세션 검사 건너뜀)
    if state.terminals.is_alive(&key) {
        let fell_back = state.terminals.start_shell(
            key,
            size(cols, rows),
            sink,
            CommandBuilder::new("/bin/zsh"), // 더미 커맨드 (start_shell 내부에서 기존 엔트리 재사용)
            false,
        )?;
        return Ok(Started { fell_back });
    }

    // 2. 새로 띄울 때
    // 2-1. 세션 id 검사: 비었거나 영문자, 숫자, - 밖의 글자가 있으면 거절
    let sid = session_action.session_id();
    if sid.is_empty() || !sid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("세션 id 모양이 아니다: {}", sid));
    }

    // 2-1-1. 에이전트 검사: 아는 이름(claude, codex, gemini)이 아니면 거절. 셸 명령에 들어가기 때문이다
    if let SessionAction::Resume { agent, session_id, .. } = &session_action {
        if resume_command(agent.as_deref(), session_id).is_none() {
            return Err(format!("아는 에이전트가 아니다: {}", agent.as_deref().unwrap_or("")));
        }
    }

    // 2-2. 셸을 띄울 폴더 및 fell_back 계산
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let (target_cwd, fell_back) = match &session_action {
        SessionAction::Resume { cwd, .. } => match cwd {
            Some(p_str) if !p_str.is_empty() => {
                let p = Path::new(p_str);
                if p.is_absolute() && p.is_dir() {
                    (Some(p.to_path_buf()), false)
                } else {
                    (home.clone(), true)
                }
            }
            _ => (home.clone(), false),
        },
        SessionAction::Attach { .. } => (home.clone(), false),
    };

    // 2-3. 첫 명령 구성
    let first_cmd = first_command(&session_action, fell_back);

    // 2-4. 로그인 셸 /bin/zsh -l -i -c '<첫명령>'
    let mut c = CommandBuilder::new("/bin/zsh");
    c.args(["-l", "-i", "-c", &first_cmd]);
    c.env("TERM", "xterm-256color");
    if let Some(path) = &target_cwd {
        c.cwd(path);
    }

    let fell_back = state.terminals.start_shell(
        key,
        size(cols, rows),
        sink,
        c,
        fell_back,
    )?;

    Ok(Started { fell_back })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_command_has_spaces_around_semicolons() {
        let resume = SessionAction::Resume { session_id: "3f2a9c4e-1b2d".into(), cwd: None, agent: None };
        assert_eq!(first_command(&resume, false), "claude --resume 3f2a9c4e-1b2d ; exec zsh -l -i");
        assert_eq!(
            first_command(&resume, true),
            "printf '%s\\n' '원래 작업 폴더가 없어서 홈 폴더에서 이어 가요. 이어 간 대화의 파일 명령은 홈을 기준으로 돌아요.' ; claude --resume 3f2a9c4e-1b2d ; exec zsh -l -i"
        );
        let attach = SessionAction::Attach { session_id: "753ecd82".into() };
        assert_eq!(first_command(&attach, false), "claude attach 753ecd82 ; exec zsh -l -i");
    }

    #[test]
    fn resume_picks_the_command_of_the_agent_that_opened_the_session() {
        let codex = SessionAction::Resume { session_id: "019e-aa".into(), cwd: None, agent: Some("codex".into()) };
        assert_eq!(first_command(&codex, false), "codex resume 019e-aa ; exec zsh -l -i");
        let gemini = SessionAction::Resume { session_id: "b1481fa3-9be2".into(), cwd: None, agent: Some("gemini".into()) };
        assert_eq!(
            first_command(&gemini, false),
            "AISURADA_SESSION_ID=b1481fa3-9be2 AISURADA_SESSION_AGENT=gemini gemini --resume b1481fa3-9be2 ; exec zsh -l -i"
        );
        let claude = SessionAction::Resume { session_id: "abc".into(), cwd: None, agent: Some("claude".into()) };
        assert_eq!(first_command(&claude, false), "claude --resume abc ; exec zsh -l -i");
    }

    #[test]
    fn an_unknown_agent_never_reaches_the_shell() {
        assert!(resume_command(Some("codex; rm -rf ~"), "abc").is_none());
        assert!(resume_command(Some(""), "abc").is_none());
        assert_eq!(resume_command(None, "abc").as_deref(), Some("claude --resume abc"));
    }
}
