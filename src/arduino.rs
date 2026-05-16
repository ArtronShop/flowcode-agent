use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;
use tokio::io::AsyncBufReadExt;

use crate::configs::{get_additional_urls_from_preferences, get_configs};

// ── Path helpers ───────────────────────────────────────────────────────────

fn get_data_path() -> String {
    get_configs().arduino_data_path
}
fn get_sketch_path() -> String {
    get_configs().arduino_sketch_path
}
fn get_download_path() -> String {
    get_configs().arduino_downloads_path
}
fn get_cli_path() -> String {
    get_configs().arduino_cli_path
}
fn get_config_file() -> String {
    PathBuf::from(get_data_path())
        .join("settings.yaml")
        .to_string_lossy()
        .to_string()
}
fn cli() -> String {
    format!(
        "\"{}\" --config-file \"{}\"",
        get_cli_path(),
        get_config_file()
    )
}

// ── arduino_dir_init ───────────────────────────────────────────────────────

pub fn arduino_dir_init(add_urls: Vec<String>) {
    let sketch = get_sketch_path();
    let data = get_data_path();
    let dl = get_download_path();

    for dir in [&data, &dl, &sketch] {
        fs::create_dir_all(dir).ok();
    }

    let prefs_urls = get_additional_urls_from_preferences();
    let mut all_urls = prefs_urls.clone();
    for u in &add_urls {
        if !prefs_urls.contains(u) {
            all_urls.push(u.clone());
        }
    }

    let urls_yaml = if all_urls.is_empty() {
        " []".to_string()
    } else {
        "\n".to_string()
            + &all_urls
                .iter()
                .map(|u| format!("    - {}", u))
                .collect::<Vec<_>>()
                .join("\n")
    };

    let content = format!(
        "directories:\n  data: {}\n  downloads: {}\n  user: {}\nboard_manager:\n  additional_urls:{}\nupdater:\n  enable_notification: false\n",
        serde_json::to_string(&data).unwrap(),
        serde_json::to_string(&dl).unwrap(),
        serde_json::to_string(&sketch).unwrap(),
        urls_yaml
    );

    if let Err(e) = fs::write(get_config_file(), content) {
        eprintln!("write arduino config fail: {}", e);
    }
}

// ── run ────────────────────────────────────────────────────────────────────

pub type OnData = Arc<dyn Fn(&str, &str) + Send + Sync>;

// Parse a Windows-style command string into (exe_path, args).
// Handles quoted exe paths that may contain spaces, e.g.:
//   "C:\Program Files\arduino-cli.exe" --flag "quoted value"
fn split_cmd(cmd_str: &str) -> (String, Vec<String>) {
    let s = cmd_str.trim();
    let (exe, rest) = if s.starts_with('"') {
        let end = s[1..].find('"').map(|i| i + 2).unwrap_or(s.len());
        (s[1..end - 1].to_string(), s[end..].trim().to_string())
    } else {
        match s.split_once(' ') {
            Some((e, r)) => (e.to_string(), r.trim().to_string()),
            None => (s.to_string(), String::new()),
        }
    };

    let mut args: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    for ch in rest.chars() {
        match ch {
            '"' => in_q = !in_q,
            ' ' if !in_q => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    (exe, args)
}

pub async fn run(cmd: &str, on_data: Option<OnData>) -> Result<Option<Value>, String> {
    println!("{}", cmd);

    let (exe, args) = split_cmd(cmd);
    let mut proc = tokio::process::Command::new(&exe);
    proc.args(&args).stdout(Stdio::piped()).stderr(Stdio::piped());

    // Hide the console window spawned for the subprocess on Windows
    #[cfg(windows)]
    {
        // use std::os::windows::process::CommandExt;
        proc.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let mut child = proc.spawn().map_err(|e| e.to_string())?;

    let stdout_pipe = child.stdout.take().unwrap();
    let stderr_pipe = child.stderr.take().unwrap();

    let od_a = on_data.clone();
    let od_b = on_data.clone();

    let stdout_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout_pipe).lines();
        let mut out = String::new();
        while let Ok(Some(line)) = reader.next_line().await {
            let data = format!("{}\n", line);
            if let Some(ref f) = od_a {
                f("stdout", &data);
            }
            out.push_str(&data);
        }
        out
    });

    let stderr_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr_pipe).lines();
        let mut out = String::new();
        while let Ok(Some(line)) = reader.next_line().await {
            let data = format!("{}\n", line);
            if let Some(ref f) = od_b {
                f("stderr", &data);
            }
            out.push_str(&data);
        }
        out
    });

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let status = child.wait().await.map_err(|e| e.to_string())?;

    if status.success() {
        let json = if cmd.contains("--format jsonmini") {
            serde_json::from_str(&stdout).ok()
        } else {
            None
        };
        Ok(json)
    } else {
        Err(stderr)
    }
}

// ── board ──────────────────────────────────────────────────────────────────

pub async fn board_list() -> Result<Value, String> {
    run(&format!("{} board list --format jsonmini", cli()), None)
        .await
        .map(|j| j.unwrap_or(Value::Null))
}

pub async fn board_listall(fqbn: Option<&str>) -> Result<Value, String> {
    run(
        &format!("{} board listall {} --format jsonmini", cli(), fqbn.unwrap_or("")),
        None,
    )
    .await
    .map(|j| j.unwrap_or(Value::Null))
}

// ── sketch helpers ─────────────────────────────────────────────────────────

pub fn sketch_dir(name: &str) -> PathBuf {
    PathBuf::from(get_sketch_path()).join(name)
}

fn sketch_ino_path(name: &str) -> PathBuf {
    sketch_dir(name).join(format!("{}.ino", name))
}

pub fn sketch_create(name: &str, code: &str) -> Result<String, String> {
    let dir = sketch_dir(name);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ino = sketch_ino_path(name);
    if !ino.exists() {
        fs::write(&ino, code).map_err(|e| e.to_string())?;
    }
    Ok(ino.to_string_lossy().to_string())
}

pub fn sketch_read(name: &str) -> Result<String, String> {
    fs::read_to_string(sketch_ino_path(name)).map_err(|e| e.to_string())
}

pub fn sketch_write(name: &str, code: &str) -> Result<(), String> {
    let dir = sketch_dir(name);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(sketch_ino_path(name), code).map_err(|e| e.to_string())
}

pub fn sketch_delete(name: &str) -> Result<(), String> {
    let dir = sketch_dir(name);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn sketch_list() -> Vec<String> {
    let path = get_sketch_path();
    let Ok(entries) = fs::read_dir(&path) else {
        return vec![];
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            e.path().join(format!("{}.ino", name)).exists()
        })
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

// ── compile / upload ───────────────────────────────────────────────────────

pub async fn compile(
    sketch: &str,
    fqbn: &str,
    board_option: Option<&str>,
    on_data: Option<OnData>,
) -> Result<(), String> {
    let opt = board_option
        .map(|o| format!(" --board-options \"{}\"", o))
        .unwrap_or_default();
    run(
        &format!(
            "{} compile -b {}{} \"{}\" -v",
            cli(),
            fqbn,
            opt,
            sketch_dir(sketch).to_string_lossy()
        ),
        on_data,
    )
    .await
    .map(|_| ())
}

pub async fn upload(
    sketch: &str,
    fqbn: &str,
    port: &str,
    board_option: Option<&str>,
    on_data: Option<OnData>,
) -> Result<(), String> {
    let opt = board_option
        .map(|o| format!(" --board-options \"{}\"", o))
        .unwrap_or_default();
    run(
        &format!(
            "{} upload -b {} -p {}{} \"{}\" -v",
            cli(),
            fqbn,
            port,
            opt,
            sketch_dir(sketch).to_string_lossy()
        ),
        on_data,
    )
    .await
    .map(|_| ())
}

// ── version ────────────────────────────────────────────────────────────────

pub async fn version() -> Result<Value, String> {
    run(&format!("{} version --format jsonmini", cli()), None)
        .await
        .map(|j| j.unwrap_or(Value::Null))
}

// ── core ───────────────────────────────────────────────────────────────────

async fn core_list() -> Result<Value, String> {
    run(&format!("{} core list --format jsonmini", cli()), None)
        .await
        .map(|j| j.unwrap_or(Value::Null))
}

async fn core_update_index(on_data: Option<OnData>) -> Result<(), String> {
    run(&format!("{} core update-index", cli()), on_data)
        .await
        .map(|_| ())
}

async fn core_install(id: &str, on_data: Option<OnData>) -> Result<(), String> {
    run(&format!("{} core install {}", cli(), id), on_data)
        .await
        .map(|_| ())
}

// ── lib ────────────────────────────────────────────────────────────────────

async fn lib_list() -> Result<Value, String> {
    run(&format!("{} lib list --format jsonmini", cli()), None)
        .await
        .map(|j| j.unwrap_or(Value::Null))
}

async fn lib_update_index(on_data: Option<OnData>) -> Result<(), String> {
    run(&format!("{} lib update-index", cli()), on_data)
        .await
        .map(|_| ())
}

async fn lib_install(name: &str, on_data: Option<OnData>) -> Result<(), String> {
    run(&format!("{} lib install \"{}\"", cli(), name), on_data)
        .await
        .map(|_| ())
}

// ── high-level helpers ─────────────────────────────────────────────────────

pub async fn arduino_check_and_install_library(
    depends: Vec<String>,
    on_data: Option<OnData>,
) -> Result<(), String> {
    let list = lib_list().await?;
    let installed: Vec<String> = list
        .get("installed_libraries")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let lib = e.get("library")?;
                    let name = lib.get("name")?.as_str()?;
                    let ver = lib.get("version")?.as_str()?;
                    Some(format!("{}@{}", name, ver))
                })
                .collect()
        })
        .unwrap_or_default();

    let to_install: Vec<_> = depends
        .into_iter()
        .filter(|d| !installed.contains(d))
        .collect();

    static LIB_INDEX_UPDATED: AtomicBool = AtomicBool::new(false);

    if !to_install.is_empty() && !LIB_INDEX_UPDATED.load(Ordering::Relaxed) {
        lib_update_index(on_data.clone()).await?;
        LIB_INDEX_UPDATED.store(true, Ordering::Relaxed);
    }

    for name in to_install {
        lib_install(&name, on_data.clone()).await?;
    }

    Ok(())
}

pub async fn arduino_check_and_install_core(
    id: &str,
    version_str: &str,
    package_index: Option<&str>,
    on_data: Option<OnData>,
) -> Result<(), String> {
    arduino_dir_init(
        package_index
            .map(|s| vec![s.to_string()])
            .unwrap_or_default(),
    );

    let list = core_list().await?;
    let installed = list
        .get("platforms")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().any(|p| {
                p.get("id").and_then(|v| v.as_str()) == Some(id)
                    && p.get("installed_version").and_then(|v| v.as_str())
                        == Some(version_str)
            })
        })
        .unwrap_or(false);

    if !installed {
        core_update_index(on_data.clone()).await?;
        core_install(&format!("{}@{}", id, version_str), on_data).await?;
    }

    Ok(())
}
