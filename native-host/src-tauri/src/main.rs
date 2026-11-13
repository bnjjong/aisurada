#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod desktop;
#[cfg(feature = "claude")]
pub mod ask;
#[cfg(feature = "claude")]
pub mod agents;
mod environment;
mod files;
mod frame;
mod github;
mod ipc;
mod layout;
mod lifecycle;
mod markdown;
mod menu;
mod notes;
mod notifications;
mod process;
#[cfg(feature = "claude")]
mod runner;
mod secrets;
mod self_write;
mod sidecar;
mod terminal;
mod workspace;
#[cfg(feature = "claude")]
mod session_image;
#[cfg(feature = "claude")]
mod dictation;
#[cfg(feature = "claude")]
mod session_shell;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

fn main() {
    environment::sanitize_startup();
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_notification::init())
        .register_asynchronous_uri_scheme_protocol("aisframe", |ctx, request, responder| {
            let vault = ctx
                .app_handle()
                .state::<lifecycle::HostState>()
                .vault
                .lock()
                .unwrap()
                .clone();
            std::thread::spawn(move || {
                responder.respond(frame::serve(vault.as_deref(), request.uri().path()))
            });
        })
        .manage(lifecycle::HostState::default())
        .manage(secrets::SecretState::default())
        .manage(github::GithubState::default())
        .manage(notifications::NotifyState::default())
        .invoke_handler(tauri::generate_handler![
            lifecycle::exit_app,
            workspace::allow_workspace,
            workspace::vault_root,
            workspace::is_workspace,
            workspace::create_workspace_dir,
            sidecar::sidecar_call,
            files::write_attachment_bytes,
            files::copy_dropped_attachment,
            files::open_in_default_app,
            files::open_external_path,
            self_write::announce_self_write,
            markdown::resolve_md_target,
            markdown::read_external_md,
            markdown::allow_external_images,
            notes::open_note_window,
            notes::note_windows,
            notes::close_note_windows,
            notes::begin_quit,
            notes::cancel_quit,
            notes::destroy_current_window,
            notes::show_in_main,
            notes::saved_note_windows,
            notes::restore_note_windows,
            notes::remember_note_window,
            terminal::pty_start,
            terminal::pty_write,
            terminal::pty_resize,
            terminal::pty_detach,
            terminal::pty_alive,
            terminal::pty_stats,
            github::github_status,
            github::github_connect,
            github::github_disconnect,
            github::github_repositories,
            github::github_clone,
            notifications::notifications_list,
            notifications::notifications_push,
            notifications::notifications_mark_read,
            notifications::notifications_settings_get,
            notifications::notifications_settings_set,
            secrets::secret_table_probe,
            secrets::secret_table_status,
            secrets::mask_scrollback,
            secrets::scan_text_shapes,
            #[cfg(feature = "claude")]
            ask::ask_claude,
            #[cfg(feature = "claude")]
            ask::ask_runtime,
            #[cfg(feature = "claude")]
            ask::cancel_ask,
            #[cfg(feature = "claude")]
            runner::runner_status,
            #[cfg(feature = "claude")]
            runner::runner_settings_set,
            #[cfg(feature = "claude")]
            runner::runner_run,
            #[cfg(feature = "claude")]
            runner::runner_stop,
            #[cfg(feature = "claude")]
            runner::runner_answer,
            #[cfg(feature = "claude")]
            runner::runner_runs,
            #[cfg(feature = "claude")]
            agents::list_claude_agents,
            #[cfg(feature = "claude")]
            agents::alive_pids,
            #[cfg(feature = "claude")]
            agents::held_agent_sessions,
            #[cfg(feature = "claude")]
            agents::release_agent_session,
            #[cfg(feature = "claude")]
            session_image::write_session_image,
            #[cfg(feature = "claude")]
            dictation::start_dictation,
            #[cfg(feature = "claude")]
            session_shell::pty_start_session
        ])
        .setup(|app| {
            // 앱에 든 ais-cli 스크립트 폴더를 claude의 PATH에 얹는다(ask.rs)
            #[cfg(feature = "claude")]
            if let Ok(resources) = app.path().resource_dir() {
                ask::set_tools_dir(resources.join("bin"));
            }
            #[cfg(feature = "claude")]
            runner::install(app.handle());
            lifecycle::create_main(app.handle())?;
            menu::install(app.handle())?;
            let handle = app.handle().clone();
            app.deep_link()
                .on_open_url(move |_| lifecycle::bring_main_forward(&handle));
            Ok(())
        })
        .on_menu_event(menu::clicked)
        .on_page_load(|webview, _payload| {
            webview
                .state::<lifecycle::HostState>()
                .page_loaded(webview.label());
        })
        .on_window_event(lifecycle::window_event)
        .build(tauri::generate_context!())
        .expect("aisurada native host failed to build")
        .run(lifecycle::run_event);
}

#[cfg(test)]
mod sidecar_integration_tests;

#[cfg(test)]
mod file_ipc_tests;
