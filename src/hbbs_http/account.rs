use super::HbbHttpResponse;
use crate::hbbs_http::{create_http_client_async_with_url, create_http_client_with_url};
use hbb_common::{
    config::{Config, LocalConfig},
    log, ResultType,
};
use serde_derive::{Deserialize, Serialize};
use serde_json::json;
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use url::Url;

lazy_static::lazy_static! {
    static ref OIDC_SESSION: Arc<RwLock<OidcSession>> = Arc::new(RwLock::new(OidcSession::new()));
}

const QUERY_INTERVAL_SECS: f32 = 1.0;
const QUERY_TIMEOUT_SECS: u64 = 60 * 3;

const REQUESTING_ACCOUNT_AUTH: &str = "Requesting account auth";
const WAITING_ACCOUNT_AUTH: &str = "Waiting account auth";
const LOGIN_ACCOUNT_AUTH: &str = "Login account auth";

#[derive(Deserialize, Clone, Debug)]
pub struct OidcAuthUrl {
    code: String,
    url: Url,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct DeviceInfo {
    /// Linux , Windows , Android ...
    #[serde(default)]
    pub os: String,

    /// `browser` or `client`
    #[serde(default)]
    pub r#type: String,

    /// device name from rustdesk client,
    /// browser info(name + version) from browser
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhitelistItem {
    data: String, // ip / device uuid
    info: DeviceInfo,
    exp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserInfo {
    #[serde(default, flatten)]
    pub settings: UserSettings,
    #[serde(default)]
    pub login_device_whitelist: Vec<WhitelistItem>,
    #[serde(default, deserialize_with = "deserialize_other_map")]
    pub other: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserSettings {
    #[serde(default)]
    pub email_verification: bool,
    #[serde(default)]
    pub email_alarm_notification: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize_repr, Deserialize_repr)]
#[repr(i64)]
pub enum UserStatus {
    Disabled = 0,
    Normal = 1,
    Unverified = -1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPayload {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub status: UserStatus,
    #[serde(default)]
    pub info: UserInfo,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub third_auth_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthBody {
    pub access_token: String,
    pub r#type: String,
    #[serde(default)]
    pub tfa_type: String,
    #[serde(default)]
    pub secret: String,
    pub user: UserPayload,
}

pub struct OidcSession {
    warmed_api_server: Option<String>,
    state_msg: &'static str,
    failed_msg: String,
    code_url: Option<OidcAuthUrl>,
    auth_body: Option<AuthBody>,
    keep_querying: bool,
    running: bool,
    query_timeout: Duration,
}

#[derive(Serialize)]
pub struct AuthResult {
    pub state_msg: String,
    pub failed_msg: String,
    pub url: Option<String>,
    pub auth_body: Option<AuthBody>,
}

pub async fn verify_same_account_device(
    caller_access_token: &str,
    target_access_token: &str,
    source_id: &str,
    target_id: &str,
) -> bool {
    let caller_access_token = caller_access_token.trim();
    let target_access_token = target_access_token.trim();
    let source_id = source_id.trim();
    let target_id = target_id.trim();
    if caller_access_token.is_empty()
        || target_access_token.is_empty()
        || source_id.is_empty()
        || target_id.is_empty()
    {
        return false;
    }

    let api_server = crate::common::get_api_server(
        Config::get_option("api-server"),
        Config::get_option("custom-rendezvous-server"),
    );
    if api_server.trim().is_empty() {
        return false;
    }

    let url = format!("{}/api/beyondremote/device-auth", api_server.trim_end_matches('/'));
    let client = create_http_client_async_with_url(&url).await;
    let resp = client
        .post(&url)
        .bearer_auth(caller_access_token)
        .json(&json!({
            "source_id": source_id,
            "target_id": target_id,
            "target_access_token": target_access_token,
        }))
        .send()
        .await;
    let Ok(resp) = resp else {
        log::warn!("Same-account device auth request failed for {} -> {}", source_id, target_id);
        return false;
    };
    if !resp.status().is_success() {
        log::info!(
            "Same-account device auth denied for {} -> {}: HTTP {}",
            source_id,
            target_id,
            resp.status()
        );
        return false;
    }
    true
}

impl Default for UserStatus {
    fn default() -> Self {
        UserStatus::Normal
    }
}

fn deserialize_other_map<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
    let serde_json::Value::Object(map) = value else {
        return Ok(HashMap::new());
    };
    Ok(map
        .into_iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key, value.to_owned())))
        .collect())
}

impl OidcSession {
    fn new() -> Self {
        Self {
            warmed_api_server: None,
            state_msg: REQUESTING_ACCOUNT_AUTH,
            failed_msg: "".to_owned(),
            code_url: None,
            auth_body: None,
            keep_querying: false,
            running: false,
            query_timeout: Duration::from_secs(QUERY_TIMEOUT_SECS),
        }
    }

    fn ensure_client(api_server: &str) {
        let mut write_guard = OIDC_SESSION.write().unwrap();
        if write_guard.warmed_api_server.as_deref() == Some(api_server) {
            return;
        }
        // This URL is used to detect the appropriate TLS implementation for the server.
        let login_option_url = format!("{}/api/login-options", api_server);
        let _ = create_http_client_with_url(&login_option_url);
        write_guard.warmed_api_server = Some(api_server.to_owned());
    }

    fn auth(
        api_server: &str,
        op: &str,
        id: &str,
        uuid: &str,
    ) -> ResultType<HbbHttpResponse<OidcAuthUrl>> {
        Self::ensure_client(api_server);
        let body = serde_json::json!({
            "op": op,
            "id": id,
            "uuid": uuid,
            "deviceInfo": crate::ui_interface::get_login_device_info(),
        })
        .to_string();
        let resp = crate::post_request_sync(format!("{}/api/oidc/auth", api_server), body, "")?;
        HbbHttpResponse::parse(&resp)
    }

    fn query(
        api_server: &str,
        code: &str,
        id: &str,
        uuid: &str,
    ) -> ResultType<HbbHttpResponse<AuthBody>> {
        let url = Url::parse_with_params(
            &format!("{}/api/oidc/auth-query", api_server),
            &[("code", code), ("id", id), ("uuid", uuid)],
        )?;
        Self::ensure_client(api_server);
        #[derive(Deserialize)]
        struct HttpResponseBody {
            body: String,
        }

        let resp = crate::http_request_sync(
            url.to_string(),
            "GET".to_owned(),
            None,
            "{}".to_owned(),
        )?;
        let resp = serde_json::from_str::<HttpResponseBody>(&resp)?;
        HbbHttpResponse::parse(&resp.body)
    }

    fn reset(&mut self) {
        self.state_msg = REQUESTING_ACCOUNT_AUTH;
        self.failed_msg = "".to_owned();
        self.keep_querying = true;
        self.running = false;
        self.code_url = None;
        self.auth_body = None;
    }

    fn before_task(&mut self) {
        self.reset();
        self.running = true;
    }

    fn after_task(&mut self) {
        self.running = false;
    }

    fn sleep(secs: f32) {
        std::thread::sleep(std::time::Duration::from_secs_f32(secs));
    }

    fn auth_task(api_server: String, op: String, id: String, uuid: String, remember_me: bool) {
        let auth_request_res = Self::auth(&api_server, &op, &id, &uuid);
        log::info!("Request oidc auth result: {:?}", &auth_request_res);
        let code_url = match auth_request_res {
            Ok(HbbHttpResponse::<_>::Data(code_url)) => code_url,
            Ok(HbbHttpResponse::<_>::Error(err)) => {
                OIDC_SESSION
                    .write()
                    .unwrap()
                    .set_state(REQUESTING_ACCOUNT_AUTH, err);
                return;
            }
            Ok(_) => {
                OIDC_SESSION
                    .write()
                    .unwrap()
                    .set_state(REQUESTING_ACCOUNT_AUTH, "Invalid auth response".to_owned());
                return;
            }
            Err(err) => {
                OIDC_SESSION
                    .write()
                    .unwrap()
                    .set_state(REQUESTING_ACCOUNT_AUTH, err.to_string());
                return;
            }
        };

        OIDC_SESSION
            .write()
            .unwrap()
            .set_state(WAITING_ACCOUNT_AUTH, "".to_owned());
        OIDC_SESSION.write().unwrap().code_url = Some(code_url.clone());

        let begin = Instant::now();
        let query_timeout = OIDC_SESSION.read().unwrap().query_timeout;
        while OIDC_SESSION.read().unwrap().keep_querying && begin.elapsed() < query_timeout {
            match Self::query(&api_server, &code_url.code, &id, &uuid) {
                Ok(HbbHttpResponse::<_>::Data(auth_body)) => {
                    if auth_body.r#type == "access_token" {
                        if remember_me {
                            LocalConfig::set_option(
                                "access_token".to_owned(),
                                auth_body.access_token.clone(),
                            );
                            LocalConfig::set_option(
                                "user_info".to_owned(),
                                serde_json::json!({
                                    "name": auth_body.user.name,
                                    "display_name": auth_body.user.display_name,
                                    "avatar": auth_body.user.avatar,
                                    "status": auth_body.user.status
                                })
                                .to_string(),
                            );
                        }
                    }
                    OIDC_SESSION
                        .write()
                        .unwrap()
                        .set_state(LOGIN_ACCOUNT_AUTH, "".to_owned());
                    OIDC_SESSION.write().unwrap().auth_body = Some(auth_body);
                    return;
                }
                Ok(HbbHttpResponse::<_>::Error(err)) => {
                    if err.contains("No authed oidc is found") {
                        // ignore, keep querying
                    } else {
                        OIDC_SESSION
                            .write()
                            .unwrap()
                            .set_state(WAITING_ACCOUNT_AUTH, err);
                        return;
                    }
                }
                Ok(unexpected) => {
                    let msg = format!("Invalid auth-query response: {:?}", unexpected);
                    log::warn!("{}", msg);
                    OIDC_SESSION
                        .write()
                        .unwrap()
                        .set_state(WAITING_ACCOUNT_AUTH, msg);
                    return;
                }
                Err(err) => {
                    log::warn!("Failed query oidc {}", err);
                    // ignore
                }
            }
            Self::sleep(QUERY_INTERVAL_SECS);
        }

        if begin.elapsed() >= query_timeout {
            OIDC_SESSION
                .write()
                .unwrap()
                .set_state(WAITING_ACCOUNT_AUTH, "timeout".to_owned());
        }

        // no need to handle "keep_querying == false"
    }

    fn set_state(&mut self, state_msg: &'static str, failed_msg: String) {
        self.state_msg = state_msg;
        self.failed_msg = failed_msg;
    }

    fn wait_stop_querying() {
        let wait_secs = 0.3;
        while OIDC_SESSION.read().unwrap().running {
            Self::sleep(wait_secs);
        }
    }

    pub fn account_auth(
        api_server: String,
        op: String,
        id: String,
        uuid: String,
        remember_me: bool,
    ) {
        Self::auth_cancel();
        Self::wait_stop_querying();
        OIDC_SESSION.write().unwrap().before_task();
        std::thread::spawn(move || {
            Self::auth_task(api_server, op, id, uuid, remember_me);
            OIDC_SESSION.write().unwrap().after_task();
        });
    }

    fn get_result_(&self) -> AuthResult {
        AuthResult {
            state_msg: self.state_msg.to_string(),
            failed_msg: self.failed_msg.clone(),
            url: self.code_url.as_ref().map(|x| x.url.to_string()),
            auth_body: self.auth_body.clone(),
        }
    }

    pub fn auth_cancel() {
        OIDC_SESSION.write().unwrap().keep_querying = false;
    }

    pub fn get_result() -> AuthResult {
        OIDC_SESSION.read().unwrap().get_result_()
    }
}
