#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod arduino;
mod configs;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::{
    extract::{State, WebSocketUpgrade},
    // http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIconBuilder,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::WindowId,
};

use arduino::{
    arduino_check_and_install_core, arduino_check_and_install_library, arduino_dir_init,
    board_list, board_listall, compile, sketch_create, sketch_delete, sketch_list, sketch_read,
    sketch_write, upload, version, OnData,
};
use configs::{config_file, get_configs, load_configs};

// ── Shared state ───────────────────────────────────────────────────────────

struct AppState {
    port_writers: Mutex<HashMap<String, Box<dyn serialport::SerialPort + Send>>>,
    port_stops: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

// ── WebSocket sender type ──────────────────────────────────────────────────

type WsTx = Arc<Mutex<futures_util::stream::SplitSink<WebSocket, Message>>>;

// ── WebSocket messaging ────────────────────────────────────────────────────

async fn ws_send(ws: &WsTx, id: Option<&str>, msg_type: &str, payload: Value) {
    let msg = json!({ "id": id, "type": msg_type, "payload": payload });
    let _ = ws.lock().await.send(Message::Text(msg.to_string().into())).await;
}

async fn ws_error(ws: &WsTx, id: Option<&str>, message: &str) {
    ws_send(ws, id, "error", json!(message)).await;
}

fn make_on_data(ws: WsTx, id: Option<String>) -> OnData {
    let rt = tokio::runtime::Handle::current();
    Arc::new(move |stream: &str, data: &str| {
        let ws = ws.clone();
        let id = id.clone();
        let stream = stream.to_string();
        let data = data.to_string();
        rt.spawn(async move {
            ws_send(&ws, id.as_deref(), "stream", json!({ "stream": stream, "data": data })).await;
        });
    })
}

// ── WebSocket upgrade handler ──────────────────────────────────────────────

async fn ws_handler(
    ws_upgrade: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws_upgrade.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (sink, mut stream) = socket.split();
    let ws = Arc::new(Mutex::new(sink));

    println!("[ws] client connected");

    while let Some(Ok(Message::Text(text))) = stream.next().await {
        #[derive(Deserialize)]
        struct WsMsg {
            id: Option<String>,
            action: String,
            params: Option<Value>,
        }

        let msg: WsMsg = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => {
                ws_error(&ws, None, "invalid JSON").await;
                continue;
            }
        };

        let id = msg.id.clone();
        let action = msg.action.clone();
        let params = msg.params.unwrap_or(json!({}));
        let ws = ws.clone();
        let state = state.clone();

        tokio::spawn(async move {
            match dispatch(&action, &params, &ws, &state, id.clone()).await {
                Ok(payload) => ws_send(&ws, id.as_deref(), "result", payload).await,
                Err(e) => ws_error(&ws, id.as_deref(), &e).await,
            }
        });
    }

    println!("[ws] client disconnected");
}

// ── Action dispatcher ──────────────────────────────────────────────────────

async fn dispatch(
    action: &str,
    params: &Value,
    ws: &WsTx,
    state: &Arc<AppState>,
    id: Option<String>,
) -> Result<Value, String> {
    let on_data: Option<OnData> = Some(make_on_data(ws.clone(), id.clone()));

    macro_rules! str_param {
        ($k:literal) => {
            params.get($k).and_then(|v| v.as_str()).unwrap_or_default()
        };
        ($k:literal, opt) => {
            params.get($k).and_then(|v| v.as_str())
        };
    }

    match action {
        // ── board ──────────────────────────────────────────────────────
        "board.list" => board_list().await,

        "board.listall" => board_listall(str_param!("fqbn", opt)).await,

        // ── install ────────────────────────────────────────────────────
        "core.install" => {
            arduino_check_and_install_core(
                str_param!("id"),
                str_param!("version"),
                str_param!("package_index", opt),
                on_data,
            )
            .await?;
            Ok(json!({ "ok": true }))
        }

        "lib.install" => {
            let depends: Vec<String> = params
                .get("depends")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            arduino_check_and_install_library(depends, on_data).await?;
            Ok(json!(null))
        }

        // ── sketch ─────────────────────────────────────────────────────
        "sketch.list" => Ok(json!(sketch_list())),

        "sketch.create" => {
            let path = sketch_create(str_param!("name"), str_param!("code"))?;
            Ok(json!({ "path": path }))
        }

        "sketch.read" => {
            let code = sketch_read(str_param!("name"))?;
            Ok(json!({ "code": code }))
        }

        "sketch.write" => {
            sketch_write(str_param!("name"), str_param!("code"))?;
            Ok(json!({ "ok": true }))
        }

        "sketch.delete" => {
            sketch_delete(str_param!("name"))?;
            Ok(json!({ "ok": true }))
        }

        // ── compile / upload ───────────────────────────────────────────
        "compile" => {
            compile(
                str_param!("sketch"),
                str_param!("fqbn"),
                str_param!("boardOption", opt),
                on_data,
            )
            .await?;
            Ok(json!({ "ok": true }))
        }

        "upload" => {
            upload(
                str_param!("sketch"),
                str_param!("fqbn"),
                str_param!("port"),
                str_param!("boardOption", opt),
                on_data,
            )
            .await?;
            Ok(json!({ "ok": true }))
        }

        // ── serial port ────────────────────────────────────────────────
        "port.list" => {
            let ports = serialport::available_ports().map_err(|e| e.to_string())?;
            let list: Vec<Value> = ports
                .iter()
                .map(|p| {
                    let manufacturer = match &p.port_type {
                        serialport::SerialPortType::UsbPort(info) => {
                            info.manufacturer.clone().unwrap_or_default()
                        }
                        _ => String::new(),
                    };
                    json!({ "path": p.port_name, "manufacturer": manufacturer })
                })
                .collect();
            Ok(json!(list))
        }

        "port.connect" => {
            let port_path = str_param!("port").to_string();
            let baud_rate = params
                .get("baudRate")
                .and_then(|v| v.as_u64())
                .unwrap_or(9600) as u32;
            connect_serial(state, ws, port_path, baud_rate, id).await?;
            Ok(json!({ "ok": true }))
        }

        "port.disconnect" => {
            let port_path = str_param!("port");
            disconnect_serial(state, port_path).await?;
            Ok(json!({ "ok": true }))
        }

        "port.write" => {
            let port_path = str_param!("port");
            let data = str_param!("data");
            write_serial(state, port_path, data).await?;
            Ok(json!({ "ok": true }))
        }

        // ── misc ───────────────────────────────────────────────────────
        "version" => version().await,

        "config.init" => {
            let urls: Vec<String> = params
                .get("additional_urls")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            arduino_dir_init(urls);
            Ok(json!({ "ok": true }))
        }

        _ => Err(format!("unknown action: {}", action)),
    }
}

// ── Serial port management ─────────────────────────────────────────────────

async fn connect_serial(
    state: &Arc<AppState>,
    ws: &WsTx,
    port_path: String,
    baud_rate: u32,
    id: Option<String>,
) -> Result<(), String> {
    let sp = serialport::new(&port_path, baud_rate)
        .timeout(std::time::Duration::from_millis(100))
        .open()
        .map_err(|e| e.to_string())?;

    let reader = sp.try_clone().map_err(|e| e.to_string())?;

    // Enable DTR/RTS
    {
        let mut dtr = sp.try_clone().map_err(|e| e.to_string())?;
        dtr.write_data_terminal_ready(true).ok();
        dtr.write_request_to_send(true).ok();
    }

    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_clone = stop_flag.clone();

    state
        .port_writers
        .lock()
        .await
        .insert(port_path.clone(), sp);
    state
        .port_stops
        .lock()
        .await
        .insert(port_path.clone(), stop_flag);

    let ws_clone = ws.clone();
    let port_clone = port_path.clone();
    let id_clone = id.clone();
    let rt = tokio::runtime::Handle::current();

    tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; 1024];
        loop {
            if stop_clone.load(Ordering::Relaxed) {
                break;
            }
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let ws = ws_clone.clone();
                    let port = port_clone.clone();
                    let id = id_clone.clone();
                    rt.spawn(async move {
                        ws_send(&ws, id.as_deref(), "port.data", json!({ "port": port, "data": data })).await;
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_) => break,
            }
        }
        let ws = ws_clone.clone();
        let port = port_clone.clone();
        let id = id_clone.clone();
        rt.spawn(async move {
            ws_send(&ws, id.as_deref(), "port.close", json!({ "port": port })).await;
        });
    });

    Ok(())
}

async fn disconnect_serial(state: &Arc<AppState>, port_path: &str) -> Result<(), String> {
    if let Some(flag) = state.port_stops.lock().await.remove(port_path) {
        flag.store(true, Ordering::Relaxed);
    }
    state.port_writers.lock().await.remove(port_path);
    Ok(())
}

async fn write_serial(
    state: &Arc<AppState>,
    port_path: &str,
    data: &str,
) -> Result<(), String> {
    let mut writers = state.port_writers.lock().await;
    let sp = writers
        .get_mut(port_path)
        .ok_or_else(|| format!("Port {} not open", port_path))?;
    sp.write_all(data.as_bytes()).map_err(|e| e.to_string())
}

// ── HTTP handler ───────────────────────────────────────────────────────────
/* async fn http_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("Content-Type", "application/json")],
        json!({ "status": "flowcode-agent running" }).to_string(),
    )
} */

// ── Auto-start ─────────────────────────────────────────────────────────────

fn apply_auto_start() {
    let configs = get_configs();
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(exe_str) = exe.to_str() else { return };

    if configs.auto_start {
        std::process::Command::new("reg")
            .args([
                "add",
                "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "FlowcodeAgent",
                "/t",
                "REG_SZ",
                "/d",
                exe_str,
                "/f",
            ])
            .output()
            .ok();
    } else {
        std::process::Command::new("reg")
            .args([
                "delete",
                "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "FlowcodeAgent",
                "/f",
            ])
            .output()
            .ok();
    }
}

// ── Settings ───────────────────────────────────────────────────────────────

fn open_settings_and_reload() {
    let cfg = config_file();
    std::thread::spawn(move || {
        std::process::Command::new("notepad.exe")
            .arg(&cfg)
            .status()
            .ok();
        load_configs();
        println!("Configs reloaded");
        apply_auto_start();
    });
}

// ── Tray icon ──────────────────────────────────────────────────────────────

fn load_tray_icon() -> tray_icon::Icon {
    let bytes = include_bytes!("../asset/logo.png");
    let img = image::load_from_memory(bytes)
        .expect("failed to load logo.png")
        .into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).expect("failed to create icon")
}

struct TrayApp {
    tray: Option<tray_icon::TrayIcon>,
    menu: Option<Menu>,
    settings_id: tray_icon::menu::MenuId,
    exit_id: tray_icon::menu::MenuId,
}

impl ApplicationHandler for TrayApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_some() {
            return;
        }
        let menu = self.menu.take().unwrap();
        let icon = load_tray_icon();
        self.tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("FlowCode Agent")
            .with_icon(icon)
            .build()
            .ok();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if ev.id == self.settings_id {
                open_settings_and_reload();
            } else if ev.id == self.exit_id {
                event_loop.exit();
                std::process::exit(0);
            }
        }
    }
}

// ── async server ───────────────────────────────────────────────────────────

async fn run_server(state: Arc<AppState>) {
    use tower_http::cors::CorsLayer;

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        // .route("/", get(http_handler))
        .route("/", get(ws_handler))
        .layer(cors)
        .with_state(state);

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8080);

    let addr = format!("0.0.0.0:{}", port);
    println!("FlowCode Agent listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ── Console attach (release GUI build launched from terminal) ──────────────
//
// windows_subsystem = "windows" makes Rust treat stdout as NULL and silently
// drop all println! output.  If the parent is a console (cmd / PowerShell),
// AttachConsole lets us reconnect and route output there.

#[cfg(windows)]
extern "system" {
    fn AttachConsole(dwProcessId: u32) -> i32;
    fn CreateFileW(
        name: *const u16, access: u32, share: u32,
        sa: *mut u8, disposition: u32, flags: u32, template: isize,
    ) -> isize;
    fn SetStdHandle(which: u32, handle: isize) -> i32;
    fn CreateMutexW(attrs: *mut core::ffi::c_void, owner: i32, name: *const u16) -> isize;
    fn GetLastError() -> u32;
}

#[cfg(windows)]
fn setup_console() {
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    const OPEN_EXISTING: u32 = 3;
    const STD_OUTPUT_HANDLE: u32 = (-11_i32) as u32;
    const STD_ERROR_HANDLE: u32 = (-12_i32) as u32;

    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return; // not launched from a terminal — no console to attach
        }
        let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        let h = CreateFileW(
            name.as_ptr(), GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null_mut(), OPEN_EXISTING, 0, 0,
        );
        if h != -1 {
            SetStdHandle(STD_OUTPUT_HANDLE, h);
            SetStdHandle(STD_ERROR_HANDLE, h);
        }
    }
}

// ── Single instance guard (Windows named mutex) ────────────────────────────

#[cfg(windows)]
fn ensure_single_instance() {
    use std::os::windows::ffi::OsStrExt;
    let name: Vec<u16> = std::ffi::OsStr::new("Local\\FlowcodeAgentMutex")
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr());
        if GetLastError() == 183 {
            // ERROR_ALREADY_EXISTS — another instance is already running
            std::process::exit(0);
        }
    }
}

// ── main ───────────────────────────────────────────────────────────────────

fn main() {
    #[cfg(windows)]
    setup_console();

    #[cfg(windows)]
    ensure_single_instance();

    load_configs();
    apply_auto_start();

    let state = Arc::new(AppState {
        port_writers: Mutex::new(HashMap::new()),
        port_stops: Mutex::new(HashMap::new()),
    });

    // Run async server on a background thread
    let state_clone = state.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(run_server(state_clone));
    });

    // Build tray menu
    let settings_item = MenuItem::new("Settings...", true, None);
    let exit_item = MenuItem::new("Exit", true, None);
    let settings_id = settings_item.id().clone();
    let exit_id = exit_item.id().clone();

    let menu = Menu::new();
    menu.append(&settings_item).ok();
    menu.append(&exit_item).ok();

    // Run winit event loop on main thread (required for tray on Windows)
    let event_loop = EventLoop::new().unwrap();
    let mut app = TrayApp {
        tray: None,
        menu: Some(menu),
        settings_id,
        exit_id,
    };
    event_loop.run_app(&mut app).ok();
}
