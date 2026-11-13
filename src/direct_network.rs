//! Per-device direct internet route. Local network observation only; no heartbeat.
use crate::managed_network::{network_identity, Mapping};
use hbb_common::{
    config::{Config, LocalConfig},
    log,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct State {
    context: String,
    network: Option<(std::net::Ipv4Addr, std::net::Ipv4Addr)>,
    mapping: Option<Mapping>,
    endpoint: Option<(String, u16)>,
    attempts: u8,
    attempted: Option<Instant>,
}
lazy_static::lazy_static! { static ref STATE: Mutex<State> = Mutex::new(State::default()); }
static BUSY: AtomicBool = AtomicBool::new(false);

pub fn endpoint() -> Option<(String, u16)> {
    STATE.lock().unwrap().endpoint.clone()
}

pub fn observe() {
    if !crate::common::STAGING_BUILD || BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        reconcile();
        BUSY.store(false, Ordering::SeqCst);
    });
}
fn reconcile() {
    let access = LocalConfig::get_option("access_token");
    let enabled = !access.is_empty()
        && !hbb_common::config::option2bool("stop-service", &Config::get_option("stop-service"));
    let network = network_identity().ok();
    let context = if enabled {
        format!("{:?}|{}|{}", network, Config::get_id(), access)
    } else {
        String::new()
    };
    let mut state = STATE.lock().unwrap();
    if state.context != context {
        // Clear publication immediately; cleanup/discovery never hold this lock.
        let network_changed = state.network.is_some() && state.network != network;
        let previous = state.mapping.take();
        *state = State {
            context: context.clone(),
            network,
            ..State::default()
        };
        drop(state);
        drop(previous);
        if network_changed {
            crate::managed_server::network_changed();
        }
        state = STATE.lock().unwrap();
    }
    if !enabled
        || network.is_none()
        || state.endpoint.is_some()
        || state.attempts >= 3
        || state
            .attempted
            .map_or(false, |t| t.elapsed() < Duration::from_secs(10))
    {
        return;
    }
    state.attempts += 1;
    state.attempted = Some(Instant::now());
    drop(state);
    match Mapping::establish_direct(&Config::get_id()) {
        Ok((mapping, port)) => {
            if network_identity().ok() != network
                || LocalConfig::get_option("access_token") != access
            {
                return;
            }
            let mut state = STATE.lock().unwrap();
            state.endpoint = Some((mapping.public_ip.to_string(), port));
            state.mapping = Some(mapping);
            log::info!("Account direct internet route verified through local router");
        }
        Err(e) => log::info!("Automatic direct route unavailable on this network: {}", e),
    }
}
