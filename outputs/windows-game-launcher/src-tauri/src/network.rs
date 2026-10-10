use crate::system_proxy::Snapshot;
use crate::{db, lock_db, webview_proxy, AppState};
use reqwest::{Client, ClientBuilder, Proxy, Url};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tauri::Emitter;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ProxyMode {
    #[default]
    System,
    Direct,
    Custom,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxySettings {
    pub mode: ProxyMode,
    #[serde(default)]
    pub address: String,
}

impl ProxySettings {
    pub(crate) fn validated(mut self) -> Result<Self, String> {
        self.address = self.address.trim().to_string();
        if self.mode != ProxyMode::Custom {
            return Ok(self);
        }
        let url = Url::parse(&self.address).map_err(|_| "代理地址无效")?;
        if !matches!(url.scheme(), "http" | "socks5")
            || url.host_str().is_none()
            || !url.port_or_known_default().is_some_and(|port| port > 0)
            || !url.username().is_empty()
            || url.password().is_some()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("请填写有效的 HTTP 或 SOCKS5 代理地址".into());
        }
        self.address = url.as_str().trim_end_matches('/').to_string();
        Ok(self)
    }

    fn configure(&self, builder: ClientBuilder) -> Result<ClientBuilder, String> {
        Ok(match self.mode {
            ProxyMode::System => builder,
            ProxyMode::Direct => builder.no_proxy(),
            ProxyMode::Custom => builder
                .no_proxy()
                .proxy(Proxy::all(&self.address).map_err(|_| "代理地址无效")?),
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Service {
    Steam,
    Epic,
    Public,
    Image,
}

struct Clients {
    steam: Client,
    epic: Client,
    public: Client,
    image: Client,
}

impl Clients {
    fn new(settings: &ProxySettings) -> Result<Self, String> {
        Ok(Self {
            steam: build_client(settings, Service::Steam)?,
            epic: build_client(settings, Service::Epic)?,
            public: build_client(settings, Service::Public)?,
            image: build_client(settings, Service::Image)?,
        })
    }
    fn get(&self, service: Service) -> Client {
        match service {
            Service::Steam => &self.steam,
            Service::Epic => &self.epic,
            Service::Public => &self.public,
            Service::Image => &self.image,
        }
        .clone()
    }
}

pub(crate) fn build_client(settings: &ProxySettings, service: Service) -> Result<Client, String> {
    let (agent, connect, timeout, redirects) = match service {
        Service::Steam => ("LocalAchievementLauncher/0.1", 8, 15, true),
        Service::Epic => (
            "EpicGamesLauncher/14.0.8-22004686+++Portal+Release-Live",
            8,
            20,
            false,
        ),
        Service::Public => ("GameCollection/0.2.1", 5, 12, false),
        Service::Image => ("GameCollection/0.2.1", 8, 20, true),
    };
    settings
        .configure(Client::builder())?
        .user_agent(agent)
        .connect_timeout(Duration::from_secs(connect))
        .timeout(Duration::from_secs(timeout))
        .redirect(if redirects {
            reqwest::redirect::Policy::limited(5)
        } else {
            reqwest::redirect::Policy::none()
        })
        .build()
        .map_err(|_| "无法初始化网络连接".into())
}

struct ClientCache {
    settings: ProxySettings,
    clients: Clients,
    system_proxy: Option<Snapshot>,
    app: Option<tauri::AppHandle>,
}

impl ClientCache {
    fn new(settings: &ProxySettings) -> Result<Self, String> {
        let before = (settings.mode == ProxyMode::System).then(Snapshot::read);
        let clients = Clients::new(settings)?;
        // 构建期间如果配置变化，不把这些客户端标记为最新；下一次请求重新检查。
        let system_proxy = before.filter(|snapshot| *snapshot == Snapshot::read());
        Ok(Self {
            settings: settings.clone(),
            clients,
            system_proxy,
            app: None,
        })
    }

    fn refresh_system_proxy(&mut self) -> Result<bool, String> {
        self.refresh_system_proxy_with(Snapshot::read)
    }

    fn refresh_system_proxy_with(
        &mut self,
        mut read: impl FnMut() -> Snapshot,
    ) -> Result<bool, String> {
        if self.settings.mode != ProxyMode::System {
            return Ok(false);
        }
        let current = read();
        if self.system_proxy.as_ref() == Some(&current) {
            return Ok(false);
        }
        let next = Clients::new(&self.settings)?;
        self.system_proxy = (read() == current).then_some(current);
        self.clients = next;
        Ok(true)
    }
}

static CLIENTS: OnceLock<Mutex<Option<ClientCache>>> = OnceLock::new();
fn clients() -> &'static Mutex<Option<ClientCache>> {
    CLIENTS.get_or_init(|| Mutex::new(None))
}

pub(crate) fn client(service: Service) -> Result<Client, String> {
    let mut slot = clients().lock().map_err(|_| "网络暂时不可用")?;
    if slot.is_none() {
        *slot = Some(ClientCache::new(&ProxySettings::default())?);
    }
    let cache = slot.as_mut().unwrap();
    let changed = cache.refresh_system_proxy()?;
    let client = cache.clients.get(service);
    let app = if changed { cache.app.clone() } else { None };
    drop(slot);
    if let Some(app) = app {
        let _ = app.emit("network-changed", ());
    }
    Ok(client)
}

fn read(conn: &rusqlite::Connection) -> Result<ProxySettings, String> {
    let value = db::setting(conn, "network_proxy")?;
    if value.is_empty() {
        return Ok(ProxySettings::default());
    }
    serde_json::from_str::<ProxySettings>(&value)
        .map_err(|_| "代理配置无法读取".to_string())?
        .validated()
}

pub(crate) fn initialize(
    conn: &rusqlite::Connection,
    app: &tauri::AppHandle,
) -> Result<ProxySettings, String> {
    let settings = read(conn)?;
    let mut next = ClientCache::new(&settings)?;
    next.app = Some(app.clone());
    *clients().lock().map_err(|_| "网络暂时不可用")? = Some(next);
    Ok(settings)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetworkSettings {
    #[serde(flatten)]
    settings: ProxySettings,
    webview_restart_required: bool,
}

impl NetworkSettings {
    fn new(app: &tauri::AppHandle, settings: ProxySettings) -> Self {
        Self {
            webview_restart_required: webview_proxy::restart_required(app, &settings),
            settings,
        }
    }
}

#[tauri::command]
pub(crate) fn get_network_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<NetworkSettings, String> {
    Ok(NetworkSettings::new(&app, read(&*lock_db(&state)?)?))
}

#[tauri::command]
pub(crate) fn save_network_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: ProxySettings,
) -> Result<NetworkSettings, String> {
    let mut slot = clients().lock().map_err(|_| "网络暂时不可用")?;
    let settings = store_and_replace(&*lock_db(&state)?, &mut slot, settings)?;
    drop(slot);
    let _ = app.emit("network-changed", ());
    Ok(NetworkSettings::new(&app, settings))
}

fn store_and_replace(
    conn: &rusqlite::Connection,
    slot: &mut Option<ClientCache>,
    settings: ProxySettings,
) -> Result<ProxySettings, String> {
    let settings = settings.validated()?;
    let mut next = ClientCache::new(&settings)?;
    next.app = slot.as_ref().and_then(|cache| cache.app.clone());
    let value = serde_json::to_string(&settings).map_err(|_| "代理配置无法保存")?;
    db::set_setting(conn, "network_proxy", &value)?;
    *slot = Some(next);
    Ok(settings)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConnectionTest {
    api: String,
    image: String,
}

fn test_message(result: Result<(), String>) -> String {
    result
        .map(|_| "连接成功".into())
        .unwrap_or_else(|error| error)
}

fn connection_test_clients(settings: &ProxySettings) -> Result<(Client, Client), String> {
    let mut slot = clients().lock().map_err(|_| "网络暂时不可用")?;
    let (pair, app) = connection_test_clients_from(&mut slot, settings)?;
    drop(slot);
    if let Some(app) = app {
        let _ = app.emit("network-changed", ());
    }
    Ok(pair)
}

type TestClients = ((Client, Client), Option<tauri::AppHandle>);
fn connection_test_clients_from(
    slot: &mut Option<ClientCache>,
    settings: &ProxySettings,
) -> Result<TestClients, String> {
    if let Some(cache) = slot.as_mut().filter(|cache| {
        cache.settings.mode == settings.mode
            && (settings.mode != ProxyMode::Custom || cache.settings.address == settings.address)
    }) {
        let changed = cache.refresh_system_proxy()?;
        let pair = (
            cache.clients.get(Service::Steam),
            cache.clients.get(Service::Image),
        );
        let app = if changed { cache.app.clone() } else { None };
        return Ok((pair, app));
    }
    // 未保存的草稿使用独立客户端，不改变当前模式或已保存的代理配置。
    let draft = ClientCache::new(settings)?;
    Ok((
        (
            draft.clients.get(Service::Steam),
            draft.clients.get(Service::Image),
        ),
        None,
    ))
}

#[tauri::command]
pub(crate) async fn test_network_connection(
    settings: ProxySettings,
) -> Result<ConnectionTest, String> {
    let settings = settings.validated()?;
    let (api_client, image_client) = connection_test_clients(&settings)?;
    let api = async {
        let input = serde_json::json!({"ids":[{"appid":367520}],"context":{"language":"schinese","country_code":"US"}}).to_string();
        let response = api_client
            .get("https://api.steampowered.com/IStoreBrowseService/GetItems/v1/")
            .query(&[("input_json", input)])
            .send()
            .await
            .map_err(|_| "连接失败".to_string())?;
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status().as_u16()));
        }
        let data = response
            .json::<serde_json::Value>()
            .await
            .map_err(|_| "响应无效".to_string())?;
        if !data["response"]["store_items"]
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item["appid"] == 367520))
        {
            return Err("响应无效".into());
        }
        Ok(())
    };
    let image = async {
        // 仅测试资料代理到图片域名的连通性，页面图片由 WebView 直接请求。
        let mut response = image_client
            .get("https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/367520/header.jpg")
            .send().await.map_err(|_| "连接失败".to_string())?;
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status().as_u16()));
        }
        if !response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("image/"))
        {
            return Err("响应不是图片".into());
        }
        if !response
            .chunk()
            .await
            .map_err(|_| "读取失败".to_string())?
            .is_some_and(|chunk| !chunk.is_empty())
        {
            return Err("图片内容为空".into());
        }
        Ok(())
    };
    let (api, image) = tokio::join!(api, image);
    Ok(ConnectionTest {
        api: test_message(api),
        image: test_message(image),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_requests_reuse_clients_until_configuration_changes() {
        let baseline = Snapshot::default();
        let mut cache = ClientCache::new(&ProxySettings::default()).unwrap();
        cache.system_proxy = Some(baseline.clone());
        let mut reads = 0;
        assert!(!cache
            .refresh_system_proxy_with(|| {
                reads += 1;
                baseline.clone()
            })
            .unwrap());
        assert_eq!(reads, 1);
        let mut updated = baseline.clone();
        updated.environment[0] = Some("http://127.0.0.1:7890".into());
        reads = 0;
        assert!(cache
            .refresh_system_proxy_with(|| {
                reads += 1;
                updated.clone()
            })
            .unwrap());
        assert_eq!(reads, 2);
        assert!(cache.system_proxy.as_ref() == Some(&updated));
        assert!(!cache.refresh_system_proxy_with(|| updated.clone()).unwrap());
        assert_eq!(cache.settings.mode, ProxyMode::System);
    }

    #[cfg(windows)]
    #[test]
    fn system_switches_detect_enable_address_and_bypass_changes() {
        let mut snapshot = Snapshot::default();
        let mut cache = ClientCache::new(&ProxySettings::default()).unwrap();
        cache.system_proxy = Some(snapshot.clone());
        snapshot.windows.enabled = true;
        snapshot.windows.server = Some("127.0.0.1:7890".into());
        assert!(cache
            .refresh_system_proxy_with(|| snapshot.clone())
            .unwrap());
        snapshot.windows.server = Some("127.0.0.1:7891".into());
        assert!(cache
            .refresh_system_proxy_with(|| snapshot.clone())
            .unwrap());
        snapshot.windows.bypass = Some("<local>;*.example.com".into());
        assert!(cache
            .refresh_system_proxy_with(|| snapshot.clone())
            .unwrap());
        snapshot.windows = crate::system_proxy::WindowsProxy::default();
        assert!(cache
            .refresh_system_proxy_with(|| snapshot.clone())
            .unwrap());
        assert!(!cache
            .refresh_system_proxy_with(|| snapshot.clone())
            .unwrap());
    }

    #[test]
    fn direct_and_custom_modes_do_not_read_system_configuration() {
        for mode in [ProxyMode::Direct, ProxyMode::Custom] {
            let settings = ProxySettings {
                mode,
                address: "http://127.0.0.1:7890".into(),
            };
            let mut cache = ClientCache::new(&settings).unwrap();
            assert!(!cache
                .refresh_system_proxy_with(|| panic!("系统配置不应读取"))
                .unwrap());
            assert!(cache.system_proxy.is_none());
        }
    }

    #[test]
    fn configuration_changing_during_build_is_checked_again_next_request() {
        let before = Snapshot::default();
        let mut after = before.clone();
        after.environment[0] = Some("http://127.0.0.1:7890".into());
        let mut cache = ClientCache::new(&ProxySettings::default()).unwrap();
        cache.system_proxy = None;
        let mut readings = [before, after.clone()].into_iter();
        assert!(cache
            .refresh_system_proxy_with(|| readings.next().unwrap())
            .unwrap());
        assert!(cache.system_proxy.is_none());
        assert!(cache.refresh_system_proxy_with(|| after.clone()).unwrap());
        assert!(cache.system_proxy.as_ref() == Some(&after));
        assert!(!cache.refresh_system_proxy_with(|| after.clone()).unwrap());
    }

    #[test]
    fn testing_saved_system_mode_refreshes_cache_but_drafts_do_not_replace_it() {
        let settings = ProxySettings::default();
        let mut cache = ClientCache::new(&settings).unwrap();
        cache.system_proxy = None;
        let mut slot = Some(cache);
        let draft = ProxySettings {
            mode: ProxyMode::Custom,
            address: "http://127.0.0.1:7890".into(),
        };
        connection_test_clients_from(&mut slot, &draft).unwrap();
        assert_eq!(slot.as_ref().unwrap().settings.mode, ProxyMode::System);
        assert!(slot.as_ref().unwrap().system_proxy.is_none());
        connection_test_clients_from(&mut slot, &settings).unwrap();
        assert!(slot.as_ref().unwrap().system_proxy.as_ref() == Some(&Snapshot::read()));
        slot.as_mut().unwrap().system_proxy = None;
        let system_draft = ProxySettings {
            mode: ProxyMode::System,
            address: "http://127.0.0.1:7891".into(),
        };
        connection_test_clients_from(&mut slot, &system_draft).unwrap();
        assert!(slot.as_ref().unwrap().system_proxy.as_ref() == Some(&Snapshot::read()));
        assert!(slot.as_ref().unwrap().settings.address.is_empty());
    }

    #[test]
    fn persists_configuration_and_keeps_previous_clients_on_invalid_save() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL)")
            .unwrap();
        let mut slot = None;
        assert_eq!(read(&conn).unwrap().mode, ProxyMode::System);
        let settings = ProxySettings {
            mode: ProxyMode::Custom,
            address: " http://127.0.0.1:7890/ ".into(),
        };
        store_and_replace(&conn, &mut slot, settings).unwrap();
        assert_eq!(read(&conn).unwrap().address, "http://127.0.0.1:7890");
        assert!(slot.is_some());
        assert!(store_and_replace(
            &conn,
            &mut slot,
            ProxySettings {
                mode: ProxyMode::Custom,
                address: "invalid".into()
            }
        )
        .is_err());
        assert_eq!(read(&conn).unwrap().mode, ProxyMode::Custom);
        store_and_replace(
            &conn,
            &mut slot,
            ProxySettings {
                mode: ProxyMode::Direct,
                address: "http://127.0.0.1:7890".into(),
            },
        )
        .unwrap();
        assert_eq!(read(&conn).unwrap().mode, ProxyMode::Direct);
    }

    #[test]
    fn validates_proxy_without_accepting_credentials_or_other_schemes() {
        for address in [
            "http://127.0.0.1:7890",
            "socks5://127.0.0.1:1080",
            "http://[::1]:7890",
        ] {
            assert!(ProxySettings {
                mode: ProxyMode::Custom,
                address: address.into()
            }
            .validated()
            .is_ok());
        }
        for address in [
            "",
            "127.0.0.1:7890",
            "https://localhost:7890",
            "file:///x",
            "http://user:pass@localhost:7890",
            "http://localhost:0",
            "http://localhost:7890/path",
            "http://localhost:7890?x=1",
        ] {
            assert!(
                ProxySettings {
                    mode: ProxyMode::Custom,
                    address: address.into()
                }
                .validated()
                .is_err(),
                "{address}"
            );
        }
        assert!(ProxySettings {
            mode: ProxyMode::Direct,
            address: "invalid".into()
        }
        .validated()
        .is_ok());
    }
}
