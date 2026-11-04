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

/// 로그인 셸 `-c` 에 넘기는 첫 명령(10.10). `;` 둘레에 빈칸을 둔다.
/// 알림 글에는 작은따옴표와 경로를 넣지 않는다. 셸 주입 자리가 되기 때문이다.
fn first_command(action: &SessionAction, fell_back: bool) -> String {
    const NOTICE: &str = "원래 작업 폴더가 없어서 홈 폴더에서 이어 가요. 이어 간 대화의 파일 명령은 홈을 기준으로 돌아요.";
    match action {
        SessionAction::Resume { session_id, .. } => {
            if fell_back {
                format!("printf '%s\\n' '{}' ; claude --resume {} ; exec zsh -l -i", NOTICE, session_id)
            } else {
                format!("claude --resume {} ; exec zsh -l -i", session_id)
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
        let resume = SessionAction::Resume { session_id: "3f2a9c4e-1b2d".into(), cwd: None };
        assert_eq!(first_command(&resume, false), "claude --resume 3f2a9c4e-1b2d ; exec zsh -l -i");
        assert_eq!(
            first_command(&resume, true),
            "printf '%s\\n' '원래 작업 폴더가 없어서 홈 폴더에서 이어 가요. 이어 간 대화의 파일 명령은 홈을 기준으로 돌아요.' ; claude --resume 3f2a9c4e-1b2d ; exec zsh -l -i"
        );
        let attach = SessionAction::Attach { session_id: "753ecd82".into() };
        assert_eq!(first_command(&attach, false), "claude attach 753ecd82 ; exec zsh -l -i");
    }
}
