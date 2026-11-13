use hbb_common::{anyhow, bail, config::Config, log, ResultType};
use serde_derive::Serialize;
use serde_json::Value;
use std::{
    fs,
    io::{self, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, Once,
    },
    time::{Duration, Instant},
};

const OPT_ENABLED: &str = "managed-server-enabled";
const OPT_HBBS_PATH: &str = "managed-server-hbbs-path";
const OPT_HBBR_PATH: &str = "managed-server-hbbr-path";
const OPT_PUBLIC_HOST: &str = "managed-server-public-host";
const LOCAL_HOST: &str = "127.0.0.1";
const RELEASE_API: &str = "https://api.github.com/repos/rustdesk/rustdesk-server/releases/latest";

lazy_static::lazy_static! {
    static ref MANAGED: Mutex<ManagedServer> = Default::default();
    static ref FIREWALL_SETUP: Mutex<Value> = Mutex::new(serde_json::json!({"busy":false,"verified":false,"message":"Firewall access has not been checked by setup."}));
}

static INSPECTING: AtomicBool = AtomicBool::new(false);
lazy_static::lazy_static! { static ref NETWORK_DETAILS: Mutex<Value> = Mutex::new(serde_json::json!({})); }

static SHUTDOWN_HOOK: Once = Once::new();
static STARTING: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct ManagedServer {
    hbbs: Option<Child>,
    hbbr: Option<Child>,
    last_error: String,
    installing: bool,
    mapping: Option<crate::managed_network::Mapping>,
    mapping_verified: bool,
}

#[derive(Serialize)]
struct ManagedStatus {
    starting: bool,
    router_mapped: bool,
    inspecting: bool,
    network: Value,
    firewall_setup: Value,
    supported_install: bool,
    supported_run: bool,
    installed: bool,
    running: bool,
    enabled: bool,
    installing: bool,
    install_dir: String,
    hbbs_path: String,
    hbbr_path: String,
    public_host: String,
    id_server: String,
    relay_server: String,
    api_server: String,
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

pub fn public_key() -> String {
    read_key().unwrap_or_default()
}

pub fn account_server_info() -> Option<(String, String)> {
    if Config::get_option(OPT_ENABLED) != "Y" || !is_running() {
        return None;
    }
    let host = public_host();
    let key = public_key();
    if host.is_empty() || key.is_empty() {
        None
    } else {
        Some((host, key))
    }
}

pub fn start_if_enabled() {
    if Config::get_option(OPT_ENABLED) != "Y"
        || Config::get_option("managed-server-paused") == "Y"
        || is_running()
    {
        return;
    }
    if crate::common::STAGING_BUILD
        && (Config::get_option("beyondremote-selected-server-id") != Config::get_id()
            || hbb_common::config::LocalConfig::get_option("access_token").is_empty())
    {
        return;
    }
    if STARTING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        if let Err(err) = start_and_apply_config() {
            MANAGED.lock().unwrap().last_error = err.to_string();
            log::warn!("Failed to start managed self-hosted server: {}", err);
        }
        STARTING.store(false, Ordering::SeqCst);
    });
}

/// Called only on an OS-observed route change, on a dedicated blocking thread.
pub fn network_changed() {
    if !crate::common::STAGING_BUILD || Config::get_option(OPT_ENABLED) != "Y" {
        return;
    }
    Config::set_option(OPT_PUBLIC_HOST.to_owned(), String::new());
    stop_on_shutdown();
    start_if_enabled();
}

pub fn stop_on_shutdown() {
    let mut managed = MANAGED.lock().unwrap();
    stop_child(managed.hbbs.take());
    stop_child(managed.hbbr.take());
    managed.mapping_verified = false;
    let mapping = managed.mapping.take();
    drop(managed);
    if let Some(mut mapping) = mapping {
        mapping.remove();
    }
    stop_existing_managed_processes(&install_dir());
}

pub extern "C" fn stop_on_shutdown_hook() {
    // macOS invokes atexit after main-thread TLS teardown. reqwest and OS
    // discovery need live TLS; run bounded cleanup on a fresh owned thread.
    if let Ok(cleanup) = std::thread::Builder::new()
        .name("connection-server-shutdown".into())
        .spawn(stop_on_shutdown)
    {
        let _ = cleanup.join();
    }
}

pub fn handle_command(key: &str, value: &str) {
    match key {
        "managed-server-inspect" if crate::common::STAGING_BUILD => inspect_setup(),
        "managed-server-prepare-firewall" if crate::common::STAGING_BUILD => {
            prepare_firewall();
        }
        "managed-server-open-router" if crate::common::STAGING_BUILD => {
            if let Err(e) = crate::managed_network::open_router_settings() {
                MANAGED.lock().unwrap().last_error = e.to_string();
            }
        }
        "managed-server-reconcile" if crate::common::STAGING_BUILD => {
            if value == "selected" {
                Config::set_option("managed-server-paused".to_owned(), String::new());
            }
            let selected = Config::get_option("beyondremote-selected-server-id");
            if selected == Config::get_id()
                && !hbb_common::config::LocalConfig::get_option("access_token").is_empty()
            {
                Config::set_option(OPT_ENABLED.to_owned(), "Y".to_owned());
                start_if_enabled();
            } else if Config::get_option(OPT_ENABLED) == "Y" {
                Config::set_option(OPT_ENABLED.to_owned(), String::new());
                std::thread::spawn(stop_on_shutdown);
            }
        }
        "managed-server-manual-start" if crate::common::STAGING_BUILD => {
            let host = value.trim();
            if Config::get_option("beyondremote-selected-server-id") != Config::get_id()
                || host.is_empty()
                || host.len() > 253
                || !host.contains('.')
                || !host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
            {
                MANAGED.lock().unwrap().last_error =
                    "Enter a valid public IPv4 address or DNS name for this selected computer."
                        .to_owned();
                return;
            }
            Config::set_option("managed-server-manual-network".to_owned(), "Y".to_owned());
            Config::set_option(OPT_PUBLIC_HOST.to_owned(), host.to_owned());
            Config::set_option("managed-server-paused".to_owned(), String::new());
            Config::set_option(OPT_ENABLED.to_owned(), "Y".to_owned());
            clear_last_error();
            start_if_enabled();
        }
        "managed-server-refresh" if crate::common::STAGING_BUILD => {
            if Config::get_option("beyondremote-selected-server-id") != Config::get_id() {
                return;
            }
            Config::set_option("managed-server-paused".to_owned(), String::new());
            Config::set_option("managed-server-manual-network".to_owned(), String::new());
            Config::set_option(OPT_ENABLED.to_owned(), "Y".to_owned());
            recheck_setup();
        }
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
            save_public_host(value);
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
            save_public_host(value);
            Config::set_option(OPT_ENABLED.to_owned(), "Y".to_owned());
            clear_last_error();
            start_if_enabled();
        }
        "managed-server-restart" => {
            save_public_host(value);
            Config::set_option(OPT_ENABLED.to_owned(), "Y".to_owned());
            clear_last_error();
            std::thread::spawn(|| {
                stop_on_shutdown();
                if let Err(err) = start_and_apply_config() {
                    MANAGED.lock().unwrap().last_error = err.to_string();
                    log::warn!("Managed self-hosted server restart failed: {}", err);
                }
            });
        }
        "managed-server-stop" => {
            if crate::common::STAGING_BUILD {
                Config::set_option("managed-server-paused".to_owned(), "Y".to_owned());
            }
            Config::set_option(OPT_ENABLED.to_owned(), "".to_owned());
            clear_last_error();
            if crate::common::STAGING_BUILD {
                std::thread::spawn(stop_on_shutdown);
            } else {
                stop_on_shutdown();
            }
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
        "managed-server-set-public-host" => {
            save_public_host(value);
            apply_client_config();
            clear_last_error();
        }
        _ => {}
    }
}

/// Local facts only; run on explicit wizard opening, not on each status poll.
fn inspect_setup() {
    if INSPECTING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        *NETWORK_DETAILS.lock().unwrap() = match crate::managed_network::network_identity() {
            Ok((gateway, local)) => {
                serde_json::json!({"router":gateway.to_string(),"local_address":local.to_string()})
            }
            Err(error) => serde_json::json!({"error":error.to_string()}),
        };
        if let Some(state) = inspect_firewall() {
            let mut current = FIREWALL_SETUP.lock().unwrap();
            if current["busy"] != true {
                *current = state;
            }
        }
        INSPECTING.store(false, Ordering::SeqCst);
    });
}

fn recheck_setup() {
    if hbb_common::config::LocalConfig::get_option("access_token").is_empty()
        || Config::get_option("beyondremote-selected-server-id") != Config::get_id()
        || STARTING.swap(true, Ordering::SeqCst)
    {
        return;
    }
    inspect_setup();
    std::thread::spawn(|| {
        let mut mapping = MANAGED.lock().unwrap().mapping.take();
        let result = if is_running() {
            match mapping.as_mut() {
                Some(mapping) => mapping.recheck(),
                None => {
                    crate::managed_network::Mapping::establish(&Config::get_id()).map(|value| {
                        mapping = Some(value);
                    })
                }
            }
        } else {
            start_and_apply_config()
        };
        let mut state = MANAGED.lock().unwrap();
        // Never re-publish after logout, deselection or Stop during discovery.
        let still_selected = Config::get_option(OPT_ENABLED) == "Y"
            && Config::get_option("beyondremote-selected-server-id") == Config::get_id()
            && !hbb_common::config::LocalConfig::get_option("access_token").is_empty();
        if still_selected {
            if let Some(mapping) = mapping {
                Config::set_option(OPT_PUBLIC_HOST.to_owned(), mapping.public_ip.to_string());
                state.mapping = Some(mapping);
            }
            state.mapping_verified = result.is_ok() && state.mapping.is_some();
            state.last_error = result.err().map(|e| e.to_string()).unwrap_or_default();
        }
        drop(state);
        STARTING.store(false, Ordering::SeqCst);
    });
}

fn inspect_firewall() -> Option<Value> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
            .arg("--getglobalstate")
            .env("LC_ALL", "C")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        if text.contains("State = 0") {
            return Some(
                serde_json::json!({"busy":false,"verified":true,"mode":"off","message":"macOS Firewall is off, so it is not blocking this connection. Setup did not change that setting."}),
            );
        }
        let (hbbs, hbbr) = server_paths();
        let app = std::env::current_exe().ok()?;
        let allowed = [&app, &hbbs, &hbbr].iter().all(|path| {
            Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
                .arg("--getappblocked")
                .arg(path)
                .env("LC_ALL", "C")
                .output()
                .map(|o| {
                    o.status.success()
                        && String::from_utf8_lossy(&o.stdout)
                            .to_lowercase()
                            .contains("allow")
                })
                .unwrap_or(false)
        });
        Some(
            serde_json::json!({"busy":false,"verified":allowed,"mode":"on","message":if allowed {"macOS allows BeyondRemote and its bundled server."} else {"Allow BeyondRemote and its bundled server in macOS Firewall."}}),
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

fn prepare_firewall() {
    if hbb_common::config::LocalConfig::get_option("access_token").is_empty() {
        return;
    }
    let mut state = FIREWALL_SETUP.lock().unwrap();
    if state["busy"] == true {
        return;
    }
    *state = serde_json::json!({"busy":true,"verified":false,"message":"Approve the system prompt to allow only BeyondRemote and its bundled server. The firewall stays enabled."});
    std::thread::spawn(|| {
        let result = configure_firewall();
        *FIREWALL_SETUP.lock().unwrap() = match result {
            Ok(message) => serde_json::json!({"busy":false,"verified":true,"message":message}),
            Err(error) => {
                serde_json::json!({"busy":false,"verified":false,"message":error.to_string()})
            }
        };
    });
}

fn configure_firewall() -> ResultType<String> {
    if let Some(state) = inspect_firewall() {
        if state["verified"] == true {
            return Ok(state["message"].as_str().unwrap_or_default().to_owned());
        }
    }
    install_bundled()?;
    let (hbbs, hbbr) = server_paths();
    let app = std::env::current_exe()?;
    #[cfg(target_os = "windows")]
    {
        let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "''"));
        let specs = [
            ("Account", &app, "TCP", "21128"),
            ("Rendezvous", &hbbs, "TCP", "21115,21116"),
            ("Discovery", &hbbs, "UDP", "21116"),
            ("Relay", &hbbr, "TCP", "21117"),
        ];
        let mut script = "$ErrorActionPreference='Stop'; try {".to_owned();
        for (label, path, protocol, ports) in specs {
            let name = format!("BeyondRemote-Staging-{}", label);
            script.push_str(&format!("$path={}; if(!(Test-Path -LiteralPath $path -PathType Leaf)){{exit 30}}; Get-NetFirewallRule -Name '{}' -ErrorAction SilentlyContinue | Remove-NetFirewallRule; New-NetFirewallRule -Name '{}' -DisplayName 'BeyondRemote Staging {}' -Direction Inbound -Action Allow -Enabled True -Profile Private,Public -Program $path -Protocol {} -LocalPort {} | Out-Null; $rule=Get-NetFirewallRule -Name '{}'; if($rule.Enabled -ne 'True' -or $rule.Action -ne 'Allow'){{exit 31}};", quote(path),name,name,label,protocol,ports,name));
        }
        script.push_str("exit 0 } catch { exit 31 }");
        let (status, _, error) =
            crate::wake::bounded_setup_command(crate::wake::elevated_powershell(&script))?;
        if !status.success() {
            bail!("{} {}", match status.code() { Some(20)=>"Administrator approval was cancelled. No completed firewall setup was confirmed.", Some(30)=>"A bundled server file is missing. Reinstall this build before changing firewall access.", _=>"Windows refused or did not retain the app-specific firewall rules. Open Windows Firewall; an administrator or organization policy may be required." }, error.trim());
        }
        return Ok("Windows confirmed app-specific inbound rules for the account connection and bundled server ports. The firewall remains enabled. A connection from another network still needs testing.".to_owned());
    }
    #[cfg(target_os = "macos")]
    {
        let quote = |p: &Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"));
        let mut shell = String::new();
        for path in [&app, &hbbs, &hbbr] {
            shell.push_str(&format!("/usr/libexec/ApplicationFirewall/socketfilterfw --add {} && /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp {} && ", quote(path), quote(path)));
        }
        shell.push_str("true");
        let apple = format!(
            "do shell script \"{}\" with administrator privileges",
            shell.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let mut command = Command::new("/usr/bin/osascript");
        command.args(["-e", &apple]);
        let (status, _, error) = crate::wake::bounded_setup_command(command)?;
        if !status.success() {
            bail!("macOS firewall setup was not completed: {}. Approve administrator access or open Firewall Options and allow BeyondRemote, hbbs and hbbr.",error.trim());
        }
        for path in [&app, &hbbs, &hbbr] {
            let output = Command::new("/usr/libexec/ApplicationFirewall/socketfilterfw")
                .arg("--getappblocked")
                .arg(path)
                .output()?;
            let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
            if !output.status.success() || !text.contains("allow") {
                bail!("macOS did not confirm an allowed entry for {}. Open Firewall Options and review it; third-party security tools must be checked separately.",path.file_name().unwrap_or_default().to_string_lossy());
            }
        }
        return Ok("macOS confirmed allowed firewall entries for BeyondRemote and its bundled server. The firewall remains enabled. Test from another network before relying on remote access.".to_owned());
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    bail!("Open your system firewall and allow the app and server ports listed in setup.");
}

fn status() -> ManagedStatus {
    let (hbbs_path, hbbr_path) = server_paths();
    let installed = (hbbs_path.is_file() && hbbr_path.is_file()) || bundled_paths().is_some();
    let running = is_running();
    let key = read_key().unwrap_or_default();
    let (installing, last_error) = {
        let managed = MANAGED.lock().unwrap();
        (managed.installing, managed.last_error.clone())
    };
    let message = if STARTING.load(Ordering::SeqCst) {
        "Preparing this computer and checking the router…".to_owned()
    } else if !is_run_supported() {
        "Managed server is available on desktop builds only.".to_owned()
    } else if installing {
        "Installing the open-source self-hosted server...".to_owned()
    } else if !last_error.is_empty() {
        last_error
    } else if crate::common::STAGING_BUILD && installed {
        if running {
            "Server running. Internet reachability still depends on the network.".to_owned()
        } else {
            "Bundled server ready. Select this computer to start it automatically.".to_owned()
        }
    } else if !is_install_supported() {
        "Automatic download is not published for this platform. Choose local hbbs and hbbr binaries to let Beyond Remote manage them.".to_owned()
    } else if !installed {
        "Ready to install the open-source self-hosted server.".to_owned()
    } else if running {
        if public_host().is_empty() {
            "Self-hosted server is running for this computer. Add a public DNS name or IP to connect from other networks.".to_owned()
        } else {
            "Self-hosted server is running. Other devices use the public address; this computer connects locally to avoid router hairpin resets."
                .to_owned()
        }
    } else {
        "Self-hosted server is installed and ready to start.".to_owned()
    };

    ManagedStatus {
        starting: STARTING.load(Ordering::SeqCst),
        router_mapped: MANAGED.lock().unwrap().mapping_verified,
        inspecting: INSPECTING.load(Ordering::SeqCst),
        network: NETWORK_DETAILS.lock().unwrap().clone(),
        firewall_setup: FIREWALL_SETUP.lock().unwrap().clone(),
        supported_install: is_install_supported(),
        supported_run: is_run_supported(),
        installed,
        running,
        enabled: Config::get_option(OPT_ENABLED) == "Y",
        installing,
        install_dir: install_dir().to_string_lossy().to_string(),
        hbbs_path: hbbs_path.to_string_lossy().to_string(),
        hbbr_path: hbbr_path.to_string_lossy().to_string(),
        public_host: public_host(),
        id_server: id_server(),
        relay_server: relay_server(),
        api_server: account_api_server(),
        key,
        message,
    }
}

fn start_and_apply_config() -> ResultType<()> {
    if let Err(err) = start() {
        stop_on_shutdown();
        return Err(err);
    }
    if read_key().is_none() || !is_running() {
        stop_on_shutdown();
        bail!("Connection server did not become ready");
    }
    if !crate::common::STAGING_BUILD {
        if let Err(err) = crate::local_api::start() {
            log::warn!("Failed to start Beyond Remote account API: {}", err);
        }
    }
    apply_client_config();
    Ok(())
}

fn start() -> ResultType<()> {
    if !is_run_supported() {
        bail!("Managed server is not supported on this platform");
    }
    if crate::common::STAGING_BUILD || bundled_paths().is_some() {
        install_bundled()?;
    }
    let (hbbs_path, hbbr_path) = server_paths();
    if !hbbs_path.is_file() || !hbbr_path.is_file() {
        bail!("hbbs and hbbr are not installed");
    }
    let work_dir = install_dir();
    fs::create_dir_all(&work_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&work_dir, fs::Permissions::from_mode(0o700))?;
    }
    SHUTDOWN_HOOK.call_once(|| {
        shutdown_hooks::add_shutdown_hook(stop_on_shutdown_hook);
    });
    let mut managed = MANAGED.lock().unwrap();
    if managed.hbbr.as_mut().map_or(false, child_running)
        && managed.hbbs.as_mut().map_or(false, child_running)
    {
        return Ok(());
    }
    if existing_managed_processes(&work_dir) == (true, true) {
        log::info!(
            "Using existing managed self-hosted server processes from {}",
            work_dir.display()
        );
        managed.last_error.clear();
        drop(managed);
        wait_for_key(Duration::from_secs(8));
        return Ok(());
    }
    stop_child(managed.hbbs.take());
    stop_child(managed.hbbr.take());
    drop(managed);
    stop_existing_managed_processes(&work_dir);
    let mapping = if crate::common::STAGING_BUILD
        && Config::get_option("managed-server-manual-network") != "Y"
    {
        Some(crate::managed_network::Mapping::establish(
            &Config::get_id(),
        )?)
    } else {
        None
    };
    // Selection/logout may change while router discovery is in flight.
    if crate::common::STAGING_BUILD
        && (Config::get_option(OPT_ENABLED) != "Y"
            || Config::get_option("beyondremote-selected-server-id") != Config::get_id()
            || hbb_common::config::LocalConfig::get_option("access_token").is_empty())
    {
        bail!("Server setup was cancelled");
    }
    let mut managed = MANAGED.lock().unwrap();
    if let Some(mapping) = mapping {
        Config::set_option(OPT_PUBLIC_HOST.to_owned(), mapping.public_ip.to_string());
        managed.mapping = Some(mapping);
        managed.mapping_verified = true;
    }
    managed.hbbr = Some(spawn_server(&hbbr_path, &work_dir, &["-k", "_"])?);
    drop(managed);
    wait_for_key(Duration::from_secs(8));
    if crate::common::STAGING_BUILD && Config::get_option(OPT_ENABLED) != "Y" {
        bail!("Server setup was cancelled");
    }
    let mut managed = MANAGED.lock().unwrap();
    let relay_arg = relay_server();
    managed.hbbs = Some(spawn_server(
        &hbbs_path,
        &work_dir,
        &["-r", &relay_arg, "-k", "_"],
    )?);
    managed.last_error.clear();
    drop(managed);
    wait_for_key(Duration::from_secs(8));
    Ok(())
}

fn apply_client_config() {
    Config::set_option("custom-rendezvous-server".to_owned(), local_id_server());
    Config::set_option("relay-server".to_owned(), local_relay_server());
    let configured_api = Config::get_option("api-server");
    if should_reset_account_api(&configured_api) {
        Config::set_option(
            "api-server".to_owned(),
            crate::common::DEFAULT_ACCOUNT_API_SERVER.to_owned(),
        );
    }
    if let Some(key) = read_key() {
        Config::set_option("key".to_owned(), key);
    }
}

fn account_api_server() -> String {
    let configured = Config::get_option("api-server").trim().to_owned();
    if configured.is_empty() {
        crate::common::DEFAULT_ACCOUNT_API_SERVER.to_owned()
    } else {
        configured
    }
}

fn should_reset_account_api(configured: &str) -> bool {
    let configured = configured.trim();
    if configured.is_empty() {
        return true;
    }
    let lower = configured.to_ascii_lowercase();
    lower == "https://admin.rustdesk.com"
        || lower == "https://rustdesk.com"
        || lower == "https://api.rustdesk.com"
        || lower == crate::local_api::local_url()
        || lower.ends_with(":21114")
}

fn spawn_server(path: &Path, work_dir: &Path, args: &[&str]) -> io::Result<Child> {
    let mut command = Command::new(path);
    command
        .args(args)
        .current_dir(work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn()
}

fn child_running(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

fn is_running() -> bool {
    let work_dir = install_dir();
    let mut managed = MANAGED.lock().unwrap();
    (managed.hbbs.as_mut().map_or(false, child_running)
        && managed.hbbr.as_mut().map_or(false, child_running))
        || existing_managed_processes(&work_dir) == (true, true)
}

fn stop_child(child: Option<Child>) {
    if let Some(mut child) = child {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn stop_existing_managed_processes(work_dir: &Path) {
    let Ok(work_dir) = fs::canonicalize(work_dir) else {
        return;
    };
    let current_pid = hbb_common::sysinfo::Pid::from_u32(std::process::id());
    let mut sys = hbb_common::sysinfo::System::new();
    sys.refresh_processes();
    for process in sys.processes().values() {
        if process.pid() == current_pid {
            continue;
        }
        let name = process.name().trim_end_matches(".exe");
        if !name.eq_ignore_ascii_case("hbbs") && !name.eq_ignore_ascii_case("hbbr") {
            continue;
        }
        let Ok(exe) = fs::canonicalize(process.exe()) else {
            continue;
        };
        if exe.starts_with(&work_dir) {
            log::info!(
                "Stopping orphaned managed server process: {} ({})",
                process.name(),
                process.pid()
            );
            let _ = process.kill();
        }
    }
}

fn existing_managed_processes(work_dir: &Path) -> (bool, bool) {
    let Ok(work_dir) = fs::canonicalize(work_dir) else {
        return (false, false);
    };
    let current_pid = hbb_common::sysinfo::Pid::from_u32(std::process::id());
    let mut found_hbbs = false;
    let mut found_hbbr = false;
    let mut sys = hbb_common::sysinfo::System::new();
    sys.refresh_processes();
    for process in sys.processes().values() {
        if process.pid() == current_pid {
            continue;
        }
        let name = process.name().trim_end_matches(".exe");
        if !name.eq_ignore_ascii_case("hbbs") && !name.eq_ignore_ascii_case("hbbr") {
            continue;
        }
        let Ok(exe) = fs::canonicalize(process.exe()) else {
            continue;
        };
        if exe.starts_with(&work_dir) {
            found_hbbs |= name.eq_ignore_ascii_case("hbbs");
            found_hbbr |= name.eq_ignore_ascii_case("hbbr");
        }
    }
    (found_hbbs, found_hbbr)
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

fn bundled_paths() -> Option<(PathBuf, PathBuf)> {
    let executable = std::env::current_exe().ok()?;
    let parent = executable.parent()?;
    let root = if cfg!(target_os = "macos") {
        parent.parent()?.join("Resources").join("connection-server")
    } else {
        parent.join("connection-server")
    };
    let paths = (root.join(exe_name("hbbs")), root.join(exe_name("hbbr")));
    if paths.0.is_file() && paths.1.is_file() {
        Some(paths)
    } else {
        None
    }
}

fn install_bundled() -> ResultType<()> {
    let (hbbs,hbbr) = bundled_paths().ok_or_else(|| anyhow::anyhow!("This build does not contain its connection server; install the complete staging package"))?;
    let dir = install_dir();
    fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    }
    for (source, stem) in [(hbbs, "hbbs"), (hbbr, "hbbr")] {
        let target = dir.join(exe_name(stem));
        if !target.is_file() || fs::read(&source)? != fs::read(&target)? {
            fs::copy(&source, &target)?;
            make_unix_executable(&target);
        }
    }
    Config::set_option(
        OPT_HBBS_PATH.to_owned(),
        dir.join(exe_name("hbbs")).to_string_lossy().to_string(),
    );
    Config::set_option(
        OPT_HBBR_PATH.to_owned(),
        dir.join(exe_name("hbbr")).to_string_lossy().to_string(),
    );
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

fn save_public_host(value: &str) {
    let host = normalize_public_host(value);
    Config::set_option(OPT_PUBLIC_HOST.to_owned(), host);
}

fn public_host() -> String {
    normalize_public_host(&Config::get_option(OPT_PUBLIC_HOST))
}

fn server_host() -> String {
    let host = public_host();
    if host.is_empty() {
        LOCAL_HOST.to_owned()
    } else {
        host
    }
}

fn id_server() -> String {
    host_with_port(&server_host(), hbb_common::config::RENDEZVOUS_PORT)
}

fn relay_server() -> String {
    host_with_port(&server_host(), hbb_common::config::RELAY_PORT)
}

fn local_id_server() -> String {
    host_with_port(LOCAL_HOST, hbb_common::config::RENDEZVOUS_PORT)
}

fn local_relay_server() -> String {
    host_with_port(LOCAL_HOST, hbb_common::config::RELAY_PORT)
}

fn normalize_public_host(value: &str) -> String {
    let mut host = value.trim();
    if let Some(rest) = host.strip_prefix("https://") {
        host = rest;
    } else if let Some(rest) = host.strip_prefix("http://") {
        host = rest;
    }
    if let Some((before_path, _)) = host.split_once('/') {
        host = before_path;
    }
    strip_default_server_port(host.trim()).to_owned()
}

fn host_with_port(host: &str, port: i32) -> String {
    if has_explicit_port(host) {
        host.to_owned()
    } else {
        format!("{host}:{port}")
    }
}

fn strip_default_server_port(host: &str) -> &str {
    let Some((base, port)) = host.rsplit_once(':') else {
        return host;
    };
    if base.ends_with(']') {
        return host;
    }
    if let Ok(port) = port.parse::<i32>() {
        if port == hbb_common::config::RENDEZVOUS_PORT || port == hbb_common::config::RELAY_PORT {
            return base;
        }
    }
    host
}

fn has_explicit_port(host: &str) -> bool {
    if host.starts_with('[') {
        return host
            .rsplit_once("]:")
            .and_then(|(_, port)| port.parse::<u16>().ok())
            .is_some();
    }
    host.rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .is_some()
}

fn is_local_server(value: &str) -> bool {
    let Some((host, _)) = value.trim().rsplit_once(':') else {
        return false;
    };
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

fn data_root() -> PathBuf {
    let app_dir = if crate::common::STAGING_BUILD {
        "BeyondRemote Staging"
    } else if cfg!(all(unix, not(target_os = "macos"))) {
        "beyondremote"
    } else {
        "BeyondRemote"
    };
    #[cfg(target_os = "windows")]
    {
        if let Some(v) = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("APPDATA")) {
            return PathBuf::from(v).join(app_dir);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(app_dir);
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(v) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(v).join(app_dir);
        }
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(app_dir);
        }
    }
    std::env::temp_dir().join(app_dir)
}

fn exe_name(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.exe")
    } else {
        stem.to_owned()
    }
}

fn is_run_supported() -> bool {
    cfg!(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "macos"
    ))
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

fn make_unix_executable(_path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(_path) {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(_path, permissions).ok();
        }
    }
}
