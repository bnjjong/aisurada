use serde::Deserialize;
use tauri::webview::{NewWindowResponse, WebviewBuilder};
use tauri::{LogicalPosition, LogicalSize, Manager, Rect, Runtime, Url, Webview, WebviewUrl};

const VIEW: &str = "archyl-original";

#[derive(Deserialize)]
pub struct Bounds {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Bounds {
    fn rect(self, size: LogicalSize<f64>) -> Option<Rect> {
        let Self {
            left,
            top,
            right,
            bottom,
        } = self;
        if ![left, top, right, bottom]
            .iter()
            .all(|v| (0.0..=1.0).contains(v))
            || left >= right
            || top >= bottom
        {
            return None;
        }
        Some(Rect {
            position: LogicalPosition::new(left * size.width, top * size.height).into(),
            size: LogicalSize::new((right - left) * size.width, (bottom - top) * size.height)
                .into(),
        })
    }
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Action {
    Show {
        project_id: String,
        team_id: String,
        bounds: Bounds,
        reload: bool,
    },
    Hide,
    Close,
}

#[tauri::command]
pub fn archyl_view<R: Runtime>(host: Webview<R>, action: Action) -> Result<(), String> {
    if host.label() != "main" {
        return Err("Archyl 화면은 Aisurada 기본 창에서 열 수 있어요.".into());
    }
    update(&host, action).map_err(|error| format!("Archyl 화면을 열지 못했어요. {error}"))
}

fn update<R: Runtime>(host: &Webview<R>, action: Action) -> Result<(), Box<dyn std::error::Error>> {
    let view = host.get_webview(VIEW);
    match action {
        Action::Hide => {
            if let Some(view) = view {
                view.hide()?;
            }
        }
        Action::Close => {
            if let Some(view) = view {
                view.close()?;
            }
        }
        Action::Show {
            project_id,
            team_id,
            bounds,
            reload,
        } => {
            if !crate::archyl::valid_id(&project_id) || !crate::archyl::valid_id(&team_id) {
                return Err("Archyl 프로젝트 주소를 확인해 주세요.".into());
            }
            let url = Url::parse(&format!(
                "https://app.archyl.com/team/{team_id}/project/{project_id}"
            ))?;
            let window = host.window();
            let size = window.inner_size()?.to_logical(window.scale_factor()?);
            let rect = bounds
                .rect(size)
                .ok_or("Archyl 화면 영역을 확인해 주세요.")?;
            if let Some(view) = view {
                view.set_bounds(rect)?;
                if reload {
                    view.navigate(url)?;
                }
                view.show()?;
            } else {
                let builder = WebviewBuilder::new(VIEW, WebviewUrl::External(url))
                    // 로그인 서비스의 이동을 허용하되, 앱의 로컬 주소는 열지 않는다.
                    .on_navigation(|url| {
                        url.scheme() == "https"
                            && url.domain().is_some_and(|host| {
                                let host = host.trim_end_matches('.');
                                host.contains('.') && !host.ends_with(".localhost")
                            })
                    })
                    .on_new_window(|_, _| NewWindowResponse::Deny);
                window.add_child(builder, rect.position, rect.size)?;
            }
        }
    }
    Ok(())
}
