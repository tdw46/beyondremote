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

pub fn profiles() -> Vec<Profile> {
    if !crate::common::STAGING_BUILD || Config::get_option("beyondremote-wake-enabled") != "Y" {
        return vec![];
    }
    let mut out = Vec::new();
    for iface in default_net::get_interfaces() {
        // Only the physical interface with a gateway; exclude VPN/loopback/virtual links.
        if !physical(&iface) {
            continue;
        }
        let Some(mac) = iface.mac_addr.map(|m| m.to_string()) else {
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
fn bounded_command(mut command: std::process::Command) -> ResultType<()> {
    use std::{process::Stdio, time::Instant};
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            bail!("Wake setup was not completed. Check system permissions and adapter support.");
        }
        if start.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Wake setup timed out. You can retry from network settings.");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn prepare_os() -> ResultType<()> {
    #[cfg(target_os = "macos")]
    {
        // AC power only; preserve the user's battery policy and normal sleep settings.
        let mut command = std::process::Command::new("/usr/bin/osascript");
        command.args([
            "-e",
            r#"do shell script "/usr/bin/pmset -c womp 1" with administrator privileges"#,
        ]);
        return bounded_command(command);
    }
    #[cfg(target_os = "windows")]
    {
        // Fixed program, no user-controlled shell text. Never restart an adapter.
        let script = r#"$ErrorActionPreference='Stop'; $count=0; Get-NetAdapter -Physical | Where-Object Status -eq 'Up' | ForEach-Object { $a=$_; $p=Get-NetAdapterPowerManagement -Name $a.Name; if($p.WakeOnMagicPacket -ne 'Unsupported'){ Set-NetAdapterPowerManagement -Name $a.Name -WakeOnMagicPacket Enabled -NoRestart; & powercfg.exe /deviceenablewake $a.InterfaceDescription; if($LASTEXITCODE -eq 0){$count++} } }; if($count -eq 0){exit 1}"#;
        let encoded = hbb_common::base64::encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let outer=format!("$p=Start-Process powershell.exe -Verb RunAs -PassThru -ArgumentList '-NoProfile -NonInteractive -EncodedCommand {}'; $p.WaitForExit(); exit $p.ExitCode",encoded);
        let mut command = std::process::Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", &outer]);
        return bounded_command(command);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    bail!(
        "Enable Wake on LAN in your network adapter and firmware settings, then allow wake here."
    );
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
            Ok(()) => {
                set_enabled(true);
                "Wake settings applied. Keep power connected and test sleep and wake before relying on it.".to_owned()
            }
            Err(e) => e.to_string(),
        };
        *SETUP.lock().unwrap() = json!({"busy":false,"message":message});
    });
}

pub fn settings_json() -> String {
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
