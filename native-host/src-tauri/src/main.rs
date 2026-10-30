#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod desktop;
mod environment;
mod files;
mod frame;
mod ipc;
mod layout;
mod lifecycle;
mod markdown;
mod menu;
mod notes;
mod process;
mod secrets;
mod self_write;
mod sidecar;
mod terminal;
mod workspace;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

fn main() {
    environment::sanitize_startup();
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .register_asynchronous_uri_scheme_protocol("wlframe", |ctx, request, responder| {
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
        .invoke_handler(tauri::generate_handler![
            workspace::allow_workspace,
            workspace::vault_root,
            workspace::is_workspace,
            workspace::create_workspace_dir,
            sidecar::sidecar_call,
            files::write_attachment_bytes,
            files::open_in_default_app,
            self_write::announce_self_write,
            markdown::resolve_md_target,
            markdown::read_external_md,
            markdown::allow_external_images,
            notes::open_note_window,
            notes::note_windows,
            notes::close_note_windows,
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
            secrets::secret_table_probe,
            secrets::secret_table_status,
            secrets::mask_scrollback,
            secrets::scan_text_shapes
        ])
        .setup(|app| {
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
        .run(tauri::generate_context!())
        .expect("withlog native host failed to start");
}

#[cfg(test)]
mod sidecar_integration_tests;

#[cfg(test)]
mod file_ipc_tests;
