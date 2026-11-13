//! Account-authorized, on-demand wake. No hosted queue, heartbeat or desktop permission.
use hbb_common::{
    anyhow, bail,
    config::{Config, LocalConfig},
    message_proto::{message, LoginRequest, Message},
    protobuf::Message as _,
    socket_client, timeout, tokio, ResultType,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    net::{Ipv4Addr, UdpSocket},
    sync::Mutex,
    time::Duration,
};

pub const PREFIX: &str = "br-wake-v1:";
pub const SENT: &str = "BR_WAKE_SENT";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub mac: String,
    pub ip: Ipv4Addr,
    pub prefix: u8,
}

lazy_static::lazy_static! {
    static ref JOB: Mutex<Value> = Mutex::new(json!({"state":"idle"}));
    static ref RECEIVED: Mutex<std::collections::VecDeque<std::time::Instant>> = Default::default();
    static ref PENDING: Mutex<Option<(String,String)>> = Mutex::new(None);
}

fn mac_bytes(mac: &str) -> ResultType<[u8; 6]> {
    let mut bytes = [0; 6];
    let parts: Vec<_> = mac.split(':').collect();
    if parts.len() != 6 {
        bail!("Invalid wake address");
    }
    for (n, p) in parts.iter().enumerate() {
        if p.len() != 2 {
            bail!("Invalid wake address");
        }
        bytes[n] = u8::from_str_radix(p, 16)?;
    }
    if bytes == [0; 6] || bytes[0] & 1 != 0 {
        bail!("Invalid wake address");
    }
    Ok(bytes)
}
fn packet(mac: &str) -> ResultType<[u8; 102]> {
    let mac = mac_bytes(mac)?;
    let mut p = [255; 102];
    for chunk in p[6..].chunks_exact_mut(6) {
        chunk.copy_from_slice(&mac);
    }
    Ok(p)
}
fn broadcast(profile: &Profile, local: Ipv4Addr, prefix: u8) -> Option<Ipv4Addr> {
    if !profile.ip.is_private()
        || !local.is_private()
        || !(16..=30).contains(&prefix)
        || profile.prefix != prefix
    {
        return None;
    }
    let mask = u32::MAX << (32 - prefix);
    let net = u32::from(local) & mask;
    if u32::from(profile.ip) & mask != net {
        return None;
    }
    Some(Ipv4Addr::from(net | !mask))
}

fn physical(iface: &default_net::Interface) -> bool {
    use default_net::interface::InterfaceType;
    matches!(
        iface.if_type,
        InterfaceType::Ethernet
            | InterfaceType::Wireless80211
            | InterfaceType::FastEthernetT
            | InterfaceType::FastEthernetFx
            | InterfaceType::GigabitEthernet
    )
}

#[cfg(target_os = "macos")]
lazy_static::lazy_static! {
    static ref MAC_CACHE: Mutex<std::collections::HashMap<String,String>> = Default::default();
}
fn hardware_mac(iface: &default_net::Interface) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        // default-net 0.14 misreads modern Darwin sockaddr_dl offsets. Read the
        // OS adapter address, once per interface/address set, never every presence tick.
        let key = format!("{}:{:?}", iface.name, iface.ipv4);
        if let Some(mac) = MAC_CACHE.lock().unwrap().get(&key).cloned() {
            return Some(mac);
        }
        if !iface.name.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        let output = std::process::Command::new("/sbin/ifconfig")
            .arg(&iface.name)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mac = text.lines().find_map(|line| {
            let mut fields = line.split_whitespace();
            (fields.next() == Some("ether"))
                .then(|| fields.next().map(str::to_owned))
                .flatten()
        })?;
        if mac_bytes(&mac).is_err() {
            return None;
        }
        let mut cache = MAC_CACHE.lock().unwrap();
        if cache.len() >= 16 {
            cache.clear();
        }
        cache.insert(key, mac.clone());
        return Some(mac);
    }
    #[cfg(not(target_os = "macos"))]
    iface.mac_addr.as_ref().map(|m| m.to_string())
}

pub fn profiles() -> Vec<Profile> {
    if !crate::common::STAGING_BUILD || Config::get_option("beyondremote-wake-enabled") != "Y" {
        return vec![];
    }
    let mut out = Vec::new();
    for iface in default_net::get_interfaces() {
        // Physical adapters only; exclude VPN/loopback/virtual links.
        if !physical(&iface) || !iface.ipv4.iter().any(|n| n.addr.is_private()) {
            continue;
        }
        let Some(mac) = hardware_mac(&iface) else {
            continue;
        };
        if mac_bytes(&mac).is_err() {
            continue;
        }
        for net in iface.ipv4 {
            if net.addr.is_private() && (16..=30).contains(&net.prefix_len) {
                out.push(Profile {
                    mac: mac.clone(),
                    ip: net.addr,
                    prefix: net.prefix_len,
                });
            }
        }
    }
    out.sort_by_key(|p| (p.mac.clone(), p.ip));
    out.truncate(4);
    out
}

fn send_profiles(profiles: &[Profile]) -> ResultType<()> {
    if profiles.is_empty() || profiles.len() > 4 {
        bail!("No valid wake addresses");
    }
    let interfaces = default_net::get_interfaces();
    let mut sent = 0;
    for profile in profiles {
        let packet = packet(&profile.mac)?;
        for iface in &interfaces {
            if !physical(&iface) {
                continue;
            }
            for local in &iface.ipv4 {
                if let Some(dest) = broadcast(profile, local.addr, local.prefix_len) {
                    let socket = UdpSocket::bind((local.addr, 0))?;
                    socket.set_broadcast(true)?;
                    socket.set_write_timeout(Some(Duration::from_secs(1)))?;
                    for _ in 0..3 {
                        socket.send_to(&packet, (dest, 9))?;
                    }
                    sent += 1;
                    break;
                }
            }
            if sent > 0 {
                break;
            }
        }
        if sent > 0 {
            break;
        } // one adapter, three fixed packets, no arbitrary UDP proxy
    }
    if sent == 0 {
        bail!("Home helper is not on the saved local network");
    }
    Ok(())
}

async fn api(path: &str, body: Value, access: &str) -> ResultType<Value> {
    if !crate::common::STAGING_BUILD || access.is_empty() {
        bail!("Sign in to BeyondRemote Staging");
    }
    let base = crate::common::get_api_server(
        Config::get_option("api-server"),
        Config::get_option("custom-rendezvous-server"),
    );
    if !base.starts_with("https://") {
        bail!("Account API must use HTTPS");
    }
    let url = format!("{}/api/v1/wake/{}", base.trim_end_matches('/'), path);
    let r = crate::hbbs_http::create_http_client_async_with_url(&url)
        .await
        .post(&url)
        .bearer_auth(access)
        .json(&body)
        .timeout(Duration::from_secs(8))
        .send()
        .await?;
    let ok = r.status().is_success();
    let v: Value = r.json().await?;
    if !ok {
        bail!(
            "{}",
            v["error"].as_str().unwrap_or("Wake request was refused")
        );
    }
    Ok(v)
}

pub async fn receive(lr: &LoginRequest) -> ResultType<()> {
    if !crate::common::STAGING_BUILD {
        bail!("Wake unavailable");
    }
    let grant = lr.avatar.strip_prefix(PREFIX).unwrap_or_default();
    if grant.len() != 64 || !grant.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("Invalid wake request");
    }
    // Bound unauthenticated attempts before they can spend hosted operations.
    {
        let now = std::time::Instant::now();
        let mut received = RECEIVED.lock().unwrap();
        while received
            .front()
            .map(|t| now.duration_since(*t) >= Duration::from_secs(60))
            .unwrap_or(false)
        {
            received.pop_front();
        }
        if received.len() >= 6 {
            bail!("Too many wake attempts");
        }
        received.push_back(now);
    }
    let access = Config::get_option("beyondremote-account-access-token");
    let v=api("consume",json!({"grant":grant,"source_id":lr.my_id,"target_id":lr.username,"helper_id":Config::get_id()}),&access).await?;
    let profiles: Vec<Profile> = serde_json::from_value(v["profiles"].clone())?;
    tokio::task::spawn_blocking(move || send_profiles(&profiles)).await??;
    Ok(())
}

async fn request(target: &str, access: &str) -> ResultType<()> {
    if LocalConfig::get_option("access_token") != access {
        bail!("Sign in again");
    }
    let source = Config::get_id();
    let v = api(
        "prepare",
        json!({"source_id":source,"target_id":target}),
        access,
    )
    .await?;
    let helper = v["helper_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing home helper"))?;
    let pk = hbb_common::sodiumoxide::base64::decode(
        v["public_key"].as_str().unwrap_or_default(),
        hbb_common::sodiumoxide::base64::Variant::Original,
    )
    .map_err(|_| anyhow::anyhow!("Invalid helper key"))?;
    let grant = v["grant"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing wake permission"))?;
    let addresses = v["addresses"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No helper address"))?;
    for address in addresses.iter().take(5).filter_map(Value::as_str) {
        if LocalConfig::get_option("access_token") != access {
            bail!("Sign in again");
        }
        let Ok(mut stream) =
            socket_client::connect_tcp_local(format!("{}:21128", address), None, 900).await
        else {
            continue;
        };
        crate::client::Client::secure_account_connection(helper, &pk, &mut stream).await?;
        // The server's ordinary challenge precedes login. Wake has a different, one-use purpose.
        let bytes = timeout(3000, stream.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("Helper disconnected"))??;
        if !matches!(
            Message::parse_from_bytes(&bytes)?.union,
            Some(message::Union::Hash(_))
        ) {
            bail!("Unexpected helper response");
        }
        let mut m = Message::new();
        m.set_login_request(LoginRequest {
            my_id: source.clone(),
            username: target.to_owned(),
            avatar: format!("{}{}", PREFIX, grant),
            ..Default::default()
        });
        stream.send(&m).await?;
        let bytes = timeout(12000, stream.next())
            .await?
            .ok_or_else(|| anyhow::anyhow!("Helper disconnected"))??;
        match Message::parse_from_bytes(&bytes)?.union {
            Some(message::Union::LoginResponse(r)) if r.error() == SENT => return Ok(()),
            _ => bail!("Home helper could not send the wake request"),
        }
    }
    bail!("Home helper is unreachable. Keep a home computer awake and connected.")
}

pub fn enqueue(target: &str) {
    if !crate::common::STAGING_BUILD || target.is_empty() || target.len() > 128 {
        return;
    }
    let mut job = JOB.lock().unwrap();
    if job["state"] == "sending" {
        return;
    }
    let access = LocalConfig::get_option("access_token");
    *job = json!({"state":"sending","target":target,"message":"Contacting your home computer…"});
    *PENDING.lock().unwrap() = Some((target.to_owned(), access));
    crate::hbbs_http::sync::start(); // existing process-owned runtime; never nest a runtime
}

pub fn dispatch_pending() {
    let pending = PENDING.lock().unwrap().take();
    if let Some((target, access)) = pending {
        tokio::spawn(async move {
            let result =
                tokio::time::timeout(Duration::from_secs(30), request(&target, &access)).await;
            let (state, message) = match result {
                Ok(Ok(())) => (
                    "sent",
                    "Wake signal sent. Connecting may take a moment.".to_owned(),
                ),
                Ok(Err(e)) => ("error", e.to_string()),
                Err(_) => (
                    "error",
                    "Wake timed out. Check your home computer and try again.".to_owned(),
                ),
            };
            *JOB.lock().unwrap() = json!({"state":state,"target":target,"message":message});
        });
    }
}
pub fn job_json() -> String {
    JOB.lock().unwrap().to_string()
}

lazy_static::lazy_static! {
    static ref SETUP: Mutex<Value> = Mutex::new(json!({"busy":false,"message":""}));
}
fn wake_exit_message(code: Option<i32>, details: &str) -> String {
    let reason = match code {
        Some(10) => "No active physical network adapter was found. Connect Ethernet or Wi-Fi, then retry.",
        Some(11) => "Your active adapter driver reports that Wake on Magic Packet is unsupported. Try a wake-capable Ethernet adapter; Wi-Fi and USB adapters often cannot wake a sleeping computer.",
        Some(12) => "The adapter supports wake, but its driver did not retain Wake on Magic Packet. Open Device Manager, expand Network adapters, then check the adapter's Advanced and Power Management tabs.",
        Some(13) => "Wake on Magic Packet was enabled, but Windows did not arm the adapter to wake the computer. Check Device Manager > Network adapters > Properties > Power Management. Firmware or Modern Standby policy may prevent arming.",
        Some(16) => "Windows could not read or change the adapter power-management settings. Check administrator approval and the network driver in Device Manager.",
        Some(20) => "Administrator approval was cancelled. No successful wake setup was confirmed. Retry and approve the system prompt, or open power settings for manual setup.",
        Some(21) => "The elevated setup process could not start. Your Windows administrator or security policy may block elevation.",
        _ if details.contains("(-128)") => "Administrator approval was cancelled. Retry and approve the macOS prompt, or use Battery > Options / Energy settings.",
        _ => "The operating system rejected wake setup. Review the diagnostic below, then open power or adapter settings.",
    };
    if details.trim().is_empty() {
        reason.to_owned()
    } else {
        format!(
            "{}\nSystem diagnostic: {}",
            reason,
            details.trim().chars().take(1200).collect::<String>()
        )
    }
}

pub(crate) fn bounded_setup_command(
    mut command: std::process::Command,
) -> ResultType<(std::process::ExitStatus, String, String)> {
    use std::{io::Read, process::Stdio, time::Instant};
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Setup output unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("Setup diagnostics unavailable"))?;
    let read = |mut stream: Box<dyn Read + Send>| {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut data = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(count) => {
                        let keep = count.min(16384usize.saturating_sub(data.len()));
                        data.extend_from_slice(&chunk[..keep]);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        hbb_common::log::warn!("Setup diagnostic pipe failed: {}", error);
                        break;
                    }
                }
            }
            let _ = send.send(String::from_utf8_lossy(&data).into_owned());
        });
        receive
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((
                status,
                out.recv_timeout(Duration::from_secs(2)).unwrap_or_default(),
                err.recv_timeout(Duration::from_secs(2)).unwrap_or_default(),
            ));
        }
        if start.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("System setup timed out waiting for approval or the OS helper. An elevated prompt may still be open; close it before retrying. No success was confirmed.");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn elevated_powershell(script: &str) -> std::process::Command {
    let encoded = hbb_common::base64::encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let outer = format!("$ErrorActionPreference='Stop'; try {{ $p=Start-Process powershell.exe -Verb RunAs -PassThru -ArgumentList '-NoProfile -NonInteractive -EncodedCommand {}'; $p.WaitForExit(); exit $p.ExitCode }} catch {{ if($_.Exception.NativeErrorCode -eq 1223) {{ exit 20 }}; exit 21 }}", encoded);
    let mut command = std::process::Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &outer]);
    command
}

fn prepare_os() -> ResultType<String> {
    #[cfg(target_os = "macos")]
    {
        let capabilities = std::process::Command::new("/usr/bin/pmset")
            .args(["-g", "cap"])
            .output()?;
        if !capabilities.status.success() {
            bail!("macOS could not report power capabilities. Open Battery > Options or Energy settings.");
        }
        let caps = String::from_utf8_lossy(&capabilities.stdout);
        if !caps.lines().any(|line| line.trim() == "womp") {
            bail!("This Mac does not advertise Wake for network access (womp). Automatic wake cannot be enabled on this hardware/power configuration. Check Battery > Options or Energy; keep a supported home helper awake.");
        }
        let mut command = std::process::Command::new("/usr/bin/osascript");
        command.args([
            "-e",
            r#"do shell script "/usr/bin/pmset -c womp 1" with administrator privileges"#,
        ]);
        let (status, _, error) = bounded_setup_command(command)?;
        if !status.success() {
            bail!("{}", wake_exit_message(status.code(), &error));
        }
        let readback = std::process::Command::new("/usr/bin/pmset")
            .args(["-g", "custom"])
            .output()?;
        let mut ac = false;
        let verified = String::from_utf8_lossy(&readback.stdout)
            .lines()
            .any(|line| {
                if line.trim().ends_with("Power:") {
                    ac = line.trim() == "AC Power:";
                }
                let fields: Vec<_> = line.split_whitespace().collect();
                ac && fields == ["womp", "1"]
            });
        if !readback.status.success() || !verified {
            bail!("macOS did not retain Wake for network access for AC power. Open Battery > Options or Energy and check the setting. Battery sleep policy was not changed.");
        }
        return Ok("macOS confirmed Wake for network access on AC power. Battery policy is unchanged. Keep power connected and another home computer awake; a real sleep-and-wake test is still required.".to_owned());
    }
    #[cfg(target_os = "windows")]
    {
        let command = elevated_powershell(include_str!("../scripts/windows/prepare-wake.ps1"));
        let (status, _, error) = bounded_setup_command(command)?;
        let diagnostic_script = r#"$ErrorActionPreference='Stop'; try { Get-NetAdapter -Physical | Where-Object Status -eq 'Up' | Select-Object -First 8 | ForEach-Object { $a=$_; try { $p=Get-NetAdapterPowerManagement -Name $a.Name; Write-Output ($a.Name + ' [' + $a.InterfaceDescription + ']: Wake on Magic Packet = ' + $p.WakeOnMagicPacket) } catch { Write-Output ($a.Name + ': power-management query failed: ' + $_.Exception.Message) } }; Write-Output 'Windows sleep support:'; powercfg.exe /a } catch { Write-Output ('Adapter query failed: '+$_.Exception.Message) }"#;
        let encoded = hbb_common::base64::encode(
            diagnostic_script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let mut diagnostic = std::process::Command::new("powershell.exe");
        diagnostic.args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded]);
        let details = bounded_setup_command(diagnostic)
            .map(|(_, out, _)| out.chars().take(2400).collect::<String>())
            .unwrap_or_else(|e| format!("Adapter diagnostics unavailable: {}", e));
        if !status.success() {
            bail!("{}\n{}", wake_exit_message(status.code(), &error), details);
        }
        return Ok("Windows verified Wake on Magic Packet and a wake-armed adapter. No adapter was restarted. Keep power connected and another home computer awake; firmware and real sleep/wake still need testing.\n".to_owned() + &details);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    bail!(
        "Enable Wake on LAN in your network adapter and firmware settings, then allow wake here."
    );
}

pub fn open_settings(target: &str) {
    // Fixed destinations only; never execute a UI-supplied command or URL.
    #[cfg(target_os = "macos")]
    {
        let destination = match target {
            "power" => "x-apple.systempreferences:com.apple.preference.battery",
            "energy" => "x-apple.systempreferences:com.apple.preference.energysaver",
            "firewall" => "x-apple.systempreferences:com.apple.preference.security?Firewall",
            "network" => "x-apple.systempreferences:com.apple.preference.network",
            "local-network" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"
            }
            _ => return,
        };
        if let Err(e) = std::process::Command::new("/usr/bin/open")
            .arg(destination)
            .spawn()
        {
            hbb_common::log::warn!("Could not open system settings: {}", e);
        }
    }
    #[cfg(target_os = "windows")]
    {
        let destination = match target {
            "power" => "powercfg.cpl",
            "network" => "ncpa.cpl",
            "firewall" => "firewall.cpl",
            "adapters" => "devmgmt.msc",
            _ => return,
        };
        let mut command = if target == "adapters" {
            std::process::Command::new("mmc.exe")
        } else {
            std::process::Command::new("control.exe")
        };
        if let Err(e) = command.arg(destination).spawn() {
            hbb_common::log::warn!("Could not open system settings: {}", e);
        }
    }
}
pub fn prepare_settings() {
    if !crate::common::STAGING_BUILD {
        return;
    }
    let mut setup = SETUP.lock().unwrap();
    if setup["busy"] == true {
        return;
    }
    *setup = json!({"busy":true,"message":"Approve wake setup in your system permission dialog."});
    std::thread::spawn(|| {
        let message = match prepare_os() {
            Ok(message) => {
                set_enabled(true);
                message
            }
            Err(e) => e.to_string(),
        };
        *SETUP.lock().unwrap() = json!({"busy":false,"message":message});
    });
}

pub fn settings_json() -> String {
    #[cfg(target_os = "macos")]
    MAC_CACHE.lock().unwrap().clear();
    json!({"setup":SETUP.lock().unwrap().clone(),"enabled":Config::get_option("beyondremote-wake-enabled")=="Y",
        "profiles":profiles().len(),
        "message":"Enable Wake for network access in macOS, or Wake on Magic Packet in your Windows network adapter and firmware. Keep another home computer awake. Hardware support varies; verify with a sleep-and-wake test."}).to_string()
}
pub fn set_enabled(enabled: bool) {
    if !crate::common::STAGING_BUILD {
        return;
    }
    let value = if enabled { "Y" } else { "" };
    // Update both app and background service; presence publishes only changed metadata.
    Config::set_option("beyondremote-wake-enabled".into(), value.into());
    crate::ipc::set_option("beyondremote-wake-enabled", value);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "explicit physical adapter check on the development Mac"]
    fn mac_adapter_uses_valid_os_address() {
        let found = default_net::get_interfaces()
            .into_iter()
            .filter(|i| physical(i) && i.ipv4.iter().any(|n| n.addr.is_private()))
            .filter_map(|i| hardware_mac(&i))
            .collect::<Vec<_>>();
        assert!(!found.is_empty());
        assert!(found.iter().all(|m| mac_bytes(m).is_ok()));
    }
    #[test]
    fn wake_failures_explain_specific_next_steps() {
        assert!(wake_exit_message(Some(20), "").contains("cancelled"));
        assert!(wake_exit_message(Some(11), "").contains("unsupported"));
        assert!(wake_exit_message(Some(13), "").contains("did not arm"));
        assert!(wake_exit_message(Some(1), "User canceled. (-128)").contains("cancelled"));
        assert!(wake_exit_message(Some(1), "driver diagnostic").contains("driver diagnostic"));
    }
    #[test]
    fn only_fixed_unicast_magic_packets() {
        let p = packet("02:12:34:56:78:9a").unwrap();
        assert_eq!(&p[..6], &[255; 6]);
        assert!(p[6..]
            .chunks_exact(6)
            .all(|c| c == [2, 18, 52, 86, 120, 154]));
        for bad in [
            "ff:ff:ff:ff:ff:ff",
            "01:00:00:00:00:01",
            "00:00:00:00:00:00",
            "02:00:00:00:00",
            "hello",
        ] {
            assert!(packet(bad).is_err());
        }
    }
    #[test]
    fn broadcast_is_limited_to_same_local_subnet() {
        let p = Profile {
            mac: "02:12:34:56:78:9a".into(),
            ip: Ipv4Addr::new(192, 168, 4, 54),
            prefix: 24,
        };
        assert_eq!(
            broadcast(&p, Ipv4Addr::new(192, 168, 4, 247), 24),
            Some(Ipv4Addr::new(192, 168, 4, 255))
        );
        assert!(broadcast(&p, Ipv4Addr::new(192, 168, 5, 2), 24).is_none());
        assert!(broadcast(&p, Ipv4Addr::new(192, 168, 4, 2), 16).is_none());
        assert!(broadcast(&p, Ipv4Addr::new(8, 8, 8, 8), 24).is_none());
    }
}
