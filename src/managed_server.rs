use hbb_common::{anyhow, bail, config::Config, log, ResultType};
use serde_derive::Serialize;
use serde_json::Value;
use std::{
    fs,
    io::{self, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Mutex, Once},
    time::{Duration, Instant},
};

const OPT_ENABLED: &str = "managed-server-enabled";
const OPT_HBBS_PATH: &str = "managed-server-hbbs-path";
const OPT_HBBR_PATH: &str = "managed-server-hbbr-path";
const SERVER_HOST: &str = "127.0.0.1";
const ID_SERVER: &str = "127.0.0.1:21116";
const RELAY_SERVER: &str = "127.0.0.1:21117";
const API_SERVER: &str = "";
const RELEASE_API: &str = "https://api.github.com/repos/rustdesk/rustdesk-server/releases/latest";

lazy_static::lazy_static! {
    static ref MANAGED: Mutex<ManagedServer> = Default::default();
}

static SHUTDOWN_HOOK: Once = Once::new();

#[derive(Default)]
struct ManagedServer {
    hbbs: Option<Child>,
    hbbr: Option<Child>,
    last_error: String,
    installing: bool,
}

#[derive(Serialize)]
struct ManagedStatus {
    supported_install: bool,
    supported_run: bool,
    installed: bool,
    running: bool,
    enabled: bool,
    installing: bool,
    install_dir: String,
    hbbs_path: String,
    hbbr_path: String,
    id_server: &'static str,
    relay_server: &'static str,
    api_server: &'static str,
    key: String,
    message: String,
}

#[derive(serde_derive::Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

struct Asset {
    name: String,
    url: String,
}

pub fn status_json() -> String {
    serde_json::to_string(&status()).unwrap_or_default()
}

pub fn start_if_enabled() {
    if Config::get_option(OPT_ENABLED) != "Y" {
        return;
    }
    std::thread::spawn(|| {
        if let Err(err) = start_and_apply_config() {
            MANAGED.lock().unwrap().last_error = err.to_string();
            log::warn!("Failed to start managed self-hosted server: {}", err);
        }
    });
}

pub fn stop_on_shutdown() {
    let mut managed = MANAGED.lock().unwrap();
    stop_child(managed.hbbs.take());
    stop_child(managed.hbbr.take());
}

pub extern "C" fn stop_on_shutdown_hook() {
    stop_on_shutdown();
}

pub fn handle_command(key: &str, value: &str) {
    match key {
        "managed-server-enable" => {
            Config::set_option(
                OPT_ENABLED.to_owned(),
                if value == "Y" { "Y" } else { "" }.to_owned(),
            );
            if value == "Y" {
                start_if_enabled();
            } else {
                clear_last_error();
                stop_on_shutdown();
            }
        }
        "managed-server-install" => {
            if is_installing() {
                return;
            }
            Config::set_option(OPT_ENABLED.to_owned(), "Y".to_owned());
            clear_last_error();
            set_installing(true);
            std::thread::spawn(|| {
                let result = install_latest().and_then(|_| start_and_apply_config());
                set_installing(false);
                if let Err(err) = result {
                    MANAGED.lock().unwrap().last_error = err.to_string();
                    log::warn!("Managed self-hosted server install failed: {}", err);
                }
            });
        }
        "managed-server-start" => {
            Config::set_option(OPT_ENABLED.to_owned(), "Y".to_owned());
            clear_last_error();
            start_if_enabled();
        }
        "managed-server-stop" => {
            Config::set_option(OPT_ENABLED.to_owned(), "".to_owned());
            clear_last_error();
            stop_on_shutdown();
        }
        "managed-server-set-paths" => {
            if let Ok(v) = serde_json::from_str::<Value>(value) {
                Config::set_option(
                    OPT_HBBS_PATH.to_owned(),
                    v["hbbs"].as_str().unwrap_or_default().trim().to_owned(),
                );
                Config::set_option(
                    OPT_HBBR_PATH.to_owned(),
                    v["hbbr"].as_str().unwrap_or_default().trim().to_owned(),
                );
                clear_last_error();
            }
        }
        _ => {}
    }
}

fn status() -> ManagedStatus {
    let (hbbs_path, hbbr_path) = server_paths();
    let installed = hbbs_path.is_file() && hbbr_path.is_file();
    let running = is_running();
    let key = read_key().unwrap_or_default();
    let (installing, last_error) = {
        let managed = MANAGED.lock().unwrap();
        (managed.installing, managed.last_error.clone())
    };
    let message = if !is_run_supported() {
        "Managed server is available on desktop builds only.".to_owned()
    } else if installing {
        "Installing the open-source self-hosted server...".to_owned()
    } else if !last_error.is_empty() {
        last_error
    } else if !is_install_supported() {
        "Automatic download is not published for this platform. Choose local hbbs and hbbr binaries to let Beyond Remote manage them.".to_owned()
    } else if !installed {
        "Ready to install the open-source self-hosted server.".to_owned()
    } else if running {
        "Self-hosted server is running and this client is configured to use it.".to_owned()
    } else {
        "Self-hosted server is installed and ready to start.".to_owned()
    };

    ManagedStatus {
        supported_install: is_install_supported(),
        supported_run: is_run_supported(),
        installed,
        running,
        enabled: Config::get_option(OPT_ENABLED) == "Y",
        installing,
        install_dir: install_dir().to_string_lossy().to_string(),
        hbbs_path: hbbs_path.to_string_lossy().to_string(),
        hbbr_path: hbbr_path.to_string_lossy().to_string(),
        id_server: ID_SERVER,
        relay_server: RELAY_SERVER,
        api_server: API_SERVER,
        key,
        message,
    }
}

fn start_and_apply_config() -> ResultType<()> {
    start()?;
    apply_client_config();
    Ok(())
}

fn start() -> ResultType<()> {
    if !is_run_supported() {
        bail!("Managed server is not supported on this platform");
    }
    let (hbbs_path, hbbr_path) = server_paths();
    if !hbbs_path.is_file() || !hbbr_path.is_file() {
        bail!("hbbs and hbbr are not installed");
    }
    let work_dir = install_dir();
    fs::create_dir_all(&work_dir)?;
    SHUTDOWN_HOOK.call_once(|| {
        shutdown_hooks::add_shutdown_hook(stop_on_shutdown_hook);
    });
    let mut managed = MANAGED.lock().unwrap();
    if managed
        .hbbr
        .as_mut()
        .map_or(false, child_running)
        && managed.hbbs.as_mut().map_or(false, child_running)
    {
        return Ok(());
    }
    stop_child(managed.hbbs.take());
    stop_child(managed.hbbr.take());
    managed.hbbr = Some(spawn_server(&hbbr_path, &work_dir, &[])?);
    let relay_arg = format!("{SERVER_HOST}:21117");
    managed.hbbs = Some(spawn_server(&hbbs_path, &work_dir, &["-r", &relay_arg])?);
    managed.last_error.clear();
    drop(managed);
    wait_for_key(Duration::from_secs(8));
    Ok(())
}

fn apply_client_config() {
    Config::set_option("custom-rendezvous-server".to_owned(), ID_SERVER.to_owned());
    Config::set_option("relay-server".to_owned(), RELAY_SERVER.to_owned());
    Config::set_option("api-server".to_owned(), API_SERVER.to_owned());
    if let Some(key) = read_key() {
        Config::set_option("key".to_owned(), key);
    }
}

fn spawn_server(path: &Path, work_dir: &Path, args: &[&str]) -> io::Result<Child> {
    Command::new(path)
        .args(args)
        .current_dir(work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

fn child_running(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

fn is_running() -> bool {
    let mut managed = MANAGED.lock().unwrap();
    managed.hbbs.as_mut().map_or(false, child_running)
        && managed.hbbr.as_mut().map_or(false, child_running)
}

fn stop_child(child: Option<Child>) {
    if let Some(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn install_latest() -> ResultType<()> {
    if !is_install_supported() {
        bail!("Automatic server download is not available for this platform");
    }
    fs::create_dir_all(install_dir())?;
    let asset = latest_asset()?;
    let archive_path = install_dir().join(&asset.name);
    download_to(&asset.url, &archive_path)?;
    unzip_archive(&archive_path, &install_dir())?;
    fs::remove_file(archive_path).ok();
    let hbbs_path = find_extracted_binary("hbbs")?;
    let hbbr_path = find_extracted_binary("hbbr")?;
    Config::set_option(
        OPT_HBBS_PATH.to_owned(),
        hbbs_path.to_string_lossy().to_string(),
    );
    Config::set_option(
        OPT_HBBR_PATH.to_owned(),
        hbbr_path.to_string_lossy().to_string(),
    );
    make_unix_executable(&hbbs_path);
    make_unix_executable(&hbbr_path);
    Ok(())
}

fn latest_asset() -> ResultType<Asset> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("BeyondRemote-managed-server")
        .build()?;
    let release: Value = client.get(RELEASE_API).send()?.error_for_status()?.json()?;
    let assets: Vec<ReleaseAsset> = serde_json::from_value(release["assets"].clone())?;
    let wanted = asset_name_fragment()?;
    assets
        .into_iter()
        .find(|a| a.name.contains(wanted))
        .map(|a| Asset {
            name: a.name,
            url: a.browser_download_url,
        })
        .ok_or_else(|| anyhow::anyhow!("No rustdesk-server release asset found for {wanted}"))
}

fn download_to(url: &str, path: &Path) -> ResultType<()> {
    let mut response = reqwest::blocking::Client::builder()
        .user_agent("BeyondRemote-managed-server")
        .build()?
        .get(url)
        .send()?
        .error_for_status()?;
    let mut file = fs::File::create(path)?;
    io::copy(&mut response, &mut file)?;
    Ok(())
}

fn unzip_archive(archive_path: &Path, target_dir: &Path) -> ResultType<()> {
    let file = fs::File::open(archive_path)?;
    let mut zip = zip::ZipArchive::new(BufReader::new(file))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(path) = entry.enclosed_name().map(|p| target_dir.join(p)) else {
            continue;
        };
        if entry.is_dir() {
            fs::create_dir_all(&path)?;
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = fs::File::create(path)?;
            io::copy(&mut entry, &mut output)?;
        }
    }
    Ok(())
}

fn server_paths() -> (PathBuf, PathBuf) {
    let hbbs = Config::get_option(OPT_HBBS_PATH);
    let hbbr = Config::get_option(OPT_HBBR_PATH);
    if !hbbs.is_empty() && !hbbr.is_empty() {
        return (PathBuf::from(hbbs), PathBuf::from(hbbr));
    }
    let dir = install_dir();
    (dir.join(exe_name("hbbs")), dir.join(exe_name("hbbr")))
}

fn find_extracted_binary(stem: &str) -> ResultType<PathBuf> {
    let target = exe_name(stem);
    let mut stack = vec![install_dir()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .map_or(false, |name| name.eq_ignore_ascii_case(&target))
            {
                return Ok(path);
            }
        }
    }
    bail!("Installed server binary not found: {}", target);
}

fn install_dir() -> PathBuf {
    data_root().join("managed-server")
}

fn data_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(v) = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")) {
            return PathBuf::from(v).join("BeyondRemote");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("BeyondRemote");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(v) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(v).join("beyondremote");
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("beyondremote");
        }
    }
    std::env::temp_dir().join("beyondremote")
}

fn exe_name(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.exe")
    } else {
        stem.to_owned()
    }
}

fn is_run_supported() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux", target_os = "macos"))
}

fn is_install_supported() -> bool {
    asset_name_fragment().is_ok()
}

fn asset_name_fragment() -> ResultType<&'static str> {
    if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        Ok("windows-x86_64")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Ok("linux-amd64")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        Ok("linux-arm64v8")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "arm") {
        Ok("linux-armv7")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86") {
        Ok("linux-i386")
    } else {
        bail!("Unsupported server release platform");
    }
}

fn read_key() -> Option<String> {
    fs::read_to_string(install_dir().join("id_ed25519.pub"))
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn wait_for_key(timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if read_key().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn set_installing(value: bool) {
    MANAGED.lock().unwrap().installing = value;
}

fn is_installing() -> bool {
    MANAGED.lock().unwrap().installing
}

fn clear_last_error() {
    MANAGED.lock().unwrap().last_error.clear();
}

fn make_unix_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).ok();
        }
    }
}
