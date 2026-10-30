#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod environment;
mod lifecycle;
mod menu;
use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;

fn main() {
    environment::sanitize_startup();
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_deep_link::init())
        .register_asynchronous_uri_scheme_protocol("wlframe", |_ctx, _request, responder| {
            // Chapter 10 will supply scoped HTML. Until then no filesystem path is served.
            std::thread::spawn(move || {
                responder.respond(
                    tauri::http::Response::builder()
                        .status(404)
                        .body(Vec::<u8>::new())
                        .unwrap(),
                )
            });
        })
        .manage(lifecycle::HostState::default())
        .manage(lifecycle::SecretTable::default())
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
