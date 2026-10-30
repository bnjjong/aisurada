use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem as P, Submenu},
    Manager,
};

pub fn install(app: &tauri::AppHandle) -> tauri::Result<()> {
    let application = Submenu::with_items(
        app,
        "withlog",
        true,
        &[
            &P::about(app, None, None)?,
            &P::separator(app)?,
            &P::services(app, None)?,
            &P::separator(app)?,
            &P::hide(app, None)?,
            &P::hide_others(app, None)?,
            &P::separator(app)?,
            &P::quit(app, None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "파일",
        true,
        &[
            &MenuItem::with_id(
                app,
                "wl_new_workspace",
                "새 워크스페이스…",
                true,
                Some("CmdOrCtrl+Shift+N"),
            )?,
            &MenuItem::with_id(
                app,
                "wl_open_workspace",
                "워크스페이스 열기…",
                true,
                Some("CmdOrCtrl+O"),
            )?,
            &P::separator(app)?,
            &MenuItem::with_id(
                app,
                "wl_welcome",
                "시작 화면으로 돌아가기",
                true,
                None::<&str>,
            )?,
            &P::separator(app)?,
            &MenuItem::with_id(
                app,
                "wl_open_terminal",
                "터미널 열기",
                true,
                Some("CmdOrCtrl+Shift+T"),
            )?,
            &MenuItem::with_id(app, "wl_close_tab", "탭 닫기", true, Some("CmdOrCtrl+W"))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "편집",
        true,
        &[
            &P::undo(app, None)?,
            &P::redo(app, None)?,
            &P::separator(app)?,
            &P::cut(app, None)?,
            &P::copy(app, None)?,
            &P::paste(app, None)?,
            &P::select_all(app, None)?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "창",
        true,
        &[&P::minimize(app, None)?, &P::maximize(app, None)?],
    )?;
    app.set_menu(Menu::with_items(
        app,
        &[&application, &file, &edit, &window],
    )?)?;
    Ok(())
}

pub fn clicked(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();
    if !id.starts_with("wl_") {
        return;
    }
    let focused = app
        .webview_windows()
        .into_values()
        .find(|w| w.is_focused().unwrap_or(false));
    if id == "wl_close_tab" {
        if let Some(window) = focused {
            let _ = crate::ipc::emit_window(app, window.label(), crate::ipc::MENU, id);
        }
    } else {
        if focused
            .as_ref()
            .is_some_and(|w| w.label().starts_with("note-"))
        {
            crate::lifecycle::bring_main_forward(app);
        }
        if let Some(main) = app.get_webview_window("main") {
            let _ = crate::ipc::emit_window(app, main.label(), crate::ipc::MENU, id);
        }
    }
}
