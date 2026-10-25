use reqwest::{Client, NoProxy, Proxy};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use url::Url;
use std::{collections::HashMap, path::PathBuf};
use super::source_session::{SourceHttpContext, SourceSession};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    System,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyStatus {
    pub active: bool,
    pub address: Option<String>,
}

#[derive(Clone)]
pub struct HttpClient {
    client: Arc<RwLock<Client>>,
    active_proxy: Arc<RwLock<Option<String>>>,
    timeout_secs: u64,
    source_sessions: Arc<RwLock<HashMap<String, Arc<SourceSession>>>>,
    source_clients: Arc<RwLock<HashMap<String, Client>>>,
    source_storage_dir: Arc<RwLock<Option<PathBuf>>>,
}

impl HttpClient {
    pub fn new(timeout_secs: u64, proxy: Option<String>) -> anyhow::Result<Self> {
        let configured_proxy = match proxy {
            Some(value) => resolve_manual_proxy(&value)?,
            None => resolve_system_proxy(),
        };
        let client = build_client(timeout_secs, configured_proxy.as_deref())?;
        Ok(Self {
            client: Arc::new(RwLock::new(client)),
            active_proxy: Arc::new(RwLock::new(configured_proxy)),
            timeout_secs,
            source_sessions: Arc::default(),
            source_clients: Arc::default(),
            source_storage_dir: Arc::default(),
        })
    }

    pub fn client(&self) -> Client {
        self.client
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_source_storage_dir(&self, path: PathBuf) {
        *self.source_storage_dir.write().unwrap_or_else(|e| e.into_inner()) = Some(path);
    }

    pub fn source_context(&self, user_ns: &str, source_url: &str, proxy: Option<&str>) -> anyhow::Result<SourceHttpContext> {
        let proxy = match proxy.map(str::trim).filter(|v| !v.is_empty()) {
            Some(value) => resolve_manual_proxy(value)?,
            None => self.active_proxy(),
        };
        self.source_context_with_proxy(user_ns, source_url, proxy)
    }

    /// 与 [`Self::source_context`] 相同, 但**强制直连**(忽略系统/手动代理)。
    ///
    /// 代理出口是共享 IP, 容易被站点 WAF 整个拦掉(返回几百字节的 403 拒绝页);
    /// 同一台机器的直连出口常常正常。书源请求在这种情况下用它重试一次,
    /// 避免像 shuhaoxs.com 那样「手机端能看、桌面端整页空白」。
    pub fn source_context_direct(
        &self,
        user_ns: &str,
        source_url: &str,
    ) -> anyhow::Result<SourceHttpContext> {
        self.source_context_with_proxy(user_ns, source_url, None)
    }

    fn source_context_with_proxy(
        &self,
        user_ns: &str,
        source_url: &str,
        proxy: Option<String>,
    ) -> anyhow::Result<SourceHttpContext> {
        let source_url = crate::util::text::normalize_source_url(source_url);
        let key = format!("{user_ns}::{source_url}");
        let session = {
            let mut sessions = self.source_sessions.write().unwrap_or_else(|e| e.into_inner());
            sessions.entry(key.clone()).or_insert_with(|| {
                let root = self.source_storage_dir.read().unwrap_or_else(|e| e.into_inner());
                Arc::new(SourceSession::new(user_ns, &source_url, root.as_deref()))
            }).clone()
        };
        let client_key = format!("{key}::proxy={}", proxy.as_deref().unwrap_or(""));
        let mut clients = self.source_clients.write().unwrap_or_else(|e| e.into_inner());
        let client = match clients.get(&client_key) {
            Some(client) => client.clone(),
            None => {
                let client = build_source_client(self.timeout_secs, proxy.as_deref(), session.cookies.clone())?;
                clients.insert(client_key.clone(), client.clone());
                client
            }
        };
        // 不跟随重定向的孪生 client: legado 的 `java.get/head/post` 对应
        // `Jsoup.connect(...).followRedirects(false)`, 书源靠读 302 的 `Location`
        // 做两步搜索(先拿 searchid 再拼结果地址)。与上面的 client 共用同一个
        // cookie jar, 登录态不受影响。按需构建, 只有真正用到才付这份开销。
        let no_redirect_key = format!("{client_key}::no-redirect");
        let client_no_redirect = match clients.get(&no_redirect_key) {
            Some(client) => client.clone(),
            None => {
                let client = build_source_client_no_redirect(self.timeout_secs, proxy.as_deref(), session.cookies.clone())?;
                clients.insert(no_redirect_key, client.clone());
                client
            }
        };
        Ok(SourceHttpContext {
            session,
            client,
            client_no_redirect,
            proxy,
            timeout_secs: self.timeout_secs,
        })
    }

    /// 当前是否有代理生效。
    pub fn active_proxy(&self) -> Option<String> {
        self.active_proxy
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// 完全不走代理的直连客户端。用于代理出口 IP 被目标站点限流时的回退。
    pub fn client_direct(&self) -> anyhow::Result<Client> {
        build_client(self.timeout_secs, None)
    }

    pub fn client_with_proxy(&self, proxy: Option<&str>) -> anyhow::Result<Client> {
        match proxy.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => {
                let proxy = resolve_manual_proxy(value)?;
                build_client(self.timeout_secs, proxy.as_deref())
            }
            None => Ok(self.client()),
        }
    }

    /// Build a fresh client with its own cookie jar. The shared client is kept
    /// for unrelated requests (RSS, updates, etc.), while book-source traffic
    /// uses one client per source so sessions cannot cross-contaminate.
    pub fn new_client_with_proxy(&self, proxy: Option<&str>) -> anyhow::Result<Client> {
        let configured_proxy = match proxy.map(str::trim).filter(|value| !value.is_empty()) {
            Some(value) => resolve_manual_proxy(value)?,
            None => self
                .active_proxy
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone(),
        };
        build_client(self.timeout_secs, configured_proxy.as_deref())
    }

    pub fn configure_proxy(
        &self,
        mode: ProxyMode,
        proxy: Option<&str>,
    ) -> anyhow::Result<ProxyStatus> {
        let configured_proxy = match mode {
            ProxyMode::System => resolve_system_proxy(),
            ProxyMode::Manual => {
                let value = proxy
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| anyhow::anyhow!("手动代理地址不能为空"))?;
                resolve_manual_proxy(value)?
            }
        };

        let current_proxy = self
            .active_proxy
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if current_proxy != configured_proxy {
            let client = build_client(self.timeout_secs, configured_proxy.as_deref())?;
            *self
                .client
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = client;
            *self
                .active_proxy
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = configured_proxy.clone();
            self.source_clients.write().unwrap_or_else(|e| e.into_inner()).clear();
        }

        Ok(ProxyStatus {
            active: configured_proxy.is_some(),
            address: configured_proxy,
        })
    }

    /// Discard the reqwest cookie jar by rebuilding the underlying Client with
    /// the same proxy. Used when a source cookie is changed/cleared so stale
    /// session cookies accumulated in the jar (e.g. an expired login session
    /// written by an antivirus-degraded page) stop being attached to requests.
    pub fn reset_cookie_jar(&self) -> anyhow::Result<()> {
        let configured_proxy = self
            .active_proxy
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let client = build_client(self.timeout_secs, configured_proxy.as_deref())?;
        *self
            .client
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = client;
        Ok(())
    }
}

fn build_client(timeout_secs: u64, proxy: Option<&str>) -> anyhow::Result<Client> {
    build_client_builder(timeout_secs, proxy)?.cookie_store(true).build().map_err(Into::into)
}

pub(crate) fn build_source_client(timeout_secs: u64, proxy: Option<&str>, jar: Arc<super::source_session::SourceCookieJar>) -> anyhow::Result<Client> {
    build_source_builder(timeout_secs, proxy)?.cookie_provider(jar).build().map_err(Into::into)
}

/// 与 [`build_source_client`] 相同, 但不跟随 3xx。
///
/// 对应 legado 的 `Jsoup.connect(...).followRedirects(false)`(`java.get`/`java.head`/
/// `java.post`)。书源用这种写法**读 302 响应的 `Location` 头**完成两步搜索:
/// 先用搜索词换一个 `searchid`, 再拿它拼结果页地址。跟随后 `Location` 消失,
/// 脚本在 `location.match(...)` 处抛空引用, 整个搜索直接失败。
pub(crate) fn build_source_client_no_redirect(timeout_secs: u64, proxy: Option<&str>, jar: Arc<super::source_session::SourceCookieJar>) -> anyhow::Result<Client> {
    build_source_builder(timeout_secs, proxy)?
        .redirect(reqwest::redirect::Policy::none())
        .cookie_provider(jar)
        .build()
        .map_err(Into::into)
}

/// 与 [`build_source_client`] 相同, 但关闭响应的自动解压。
///
/// `java.net.URL(...).openConnection()` 形态的脚本按 Java 语义自行处理
/// `Content-Encoding`(例如源脚本自己 new GZIPInputStream), 自动解压会把已解压
/// 的正文再解一次并丢掉原始头部, 因此这条路径必须拿到未解码的字节。
pub(crate) fn build_raw_source_client(timeout_secs: u64, proxy: Option<&str>, jar: Arc<super::source_session::SourceCookieJar>) -> anyhow::Result<Client> {
    build_source_builder(timeout_secs, proxy)?
        .gzip(false)
        .brotli(false)
        .deflate(false)
        .cookie_provider(jar)
        .build()
        .map_err(Into::into)
}

fn build_client_builder(timeout_secs: u64, proxy: Option<&str>) -> anyhow::Result<reqwest::ClientBuilder> {
    let builder = build_builder_base(timeout_secs, proxy)?
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");
    Ok(builder)
}

/// 书源请求用的 builder: 在通用配置之上打开 **浏览器风格的首部写法**。
///
/// Cloudflare 的托管挑战(managed challenge)对某些站点不只看 UA, 还看首部的
/// **大小写与顺序**: 同一份 `User-Agent` 用 `user-agent`(全小写)发出去直接
/// `403 cf-mitigated: challenge`, 改成 `User-Agent` 并把 `Host` 放在第一个首部
/// 就返回 200(实测站点上逐项二分确认)。reqwest/hyper 默认把首部全小写化 ——
/// 与真实浏览器(Chrome 发的是 Title-Case)不一致, 于是「手机端能读、桌面端
/// 整页被拦」。
///
/// `http1_title_case_headers` 只影响 HTTP/1.x(h2 要求小写, hyper 会自行处理),
/// 显式 `Host` 由 [`crate::crawler::fetcher`] 注入到请求首位。
fn build_source_builder(timeout_secs: u64, proxy: Option<&str>) -> anyhow::Result<reqwest::ClientBuilder> {
    Ok(build_builder_base(timeout_secs, proxy)?
        .http1_title_case_headers()
        .http1_only())
}

fn build_builder_base(timeout_secs: u64, proxy: Option<&str>) -> anyhow::Result<reqwest::ClientBuilder> {
    let mut builder = Client::builder().timeout(Duration::from_secs(timeout_secs));
    if let Some(proxy) = proxy {
        let bypass = NoProxy::from_string("localhost,127.0.0.1,::1");
        builder = builder.proxy(Proxy::all(proxy)?.no_proxy(bypass));
    } else {
        builder = builder.no_proxy();
    }
    Ok(builder)
}

fn resolve_manual_proxy(value: &str) -> anyhow::Result<Option<String>> {
    let normalized = if value.contains("://") {
        value.trim().to_string()
    } else {
        format!("http://{}", value.trim())
    };
    let parsed = Url::parse(&normalized).map_err(|_| anyhow::anyhow!("代理地址格式无效"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(anyhow::anyhow!("代理地址仅支持 HTTP 或 HTTPS"));
    }
    Ok(Some(normalized))
}

fn resolve_system_proxy() -> Option<String> {
    platform_system_proxy()
        .or_else(environment_proxy)
        .and_then(|value| resolve_manual_proxy(&value).ok().flatten())
}

fn environment_proxy() -> Option<String> {
    ["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"]
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "windows")]
fn platform_system_proxy() -> Option<String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    let settings = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
        .ok()?;
    let enabled = settings.get_value::<u32, _>("ProxyEnable").unwrap_or(0) == 1;
    if !enabled {
        return None;
    }
    let server = settings.get_value::<String, _>("ProxyServer").ok()?;
    select_proxy_server(&server)
}

#[cfg(target_os = "macos")]
fn platform_system_proxy() -> Option<String> {
    use std::process::Command;

    let output = Command::new("networksetup")
        .arg("-listallnetworkservices")
        .output()
        .ok()?;
    let services = String::from_utf8_lossy(&output.stdout);
    for service in services.lines().skip(1).map(str::trim).filter(|line| !line.is_empty()) {
        let service = service.trim_start_matches('*').trim();
        for proxy_type in ["-getsecurewebproxy", "-getwebproxy"] {
            let proxy = Command::new("networksetup")
                .args([proxy_type, service])
                .output()
                .ok()?;
            let text = String::from_utf8_lossy(&proxy.stdout);
            if !text.lines().any(|line| line.trim() == "Enabled: Yes") {
                continue;
            }
            let host = setting_value(&text, "Server:")?;
            let port = setting_value(&text, "Port:")?;
            return Some(format!("http://{host}:{port}"));
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn platform_system_proxy() -> Option<String> {
    use std::process::Command;

    let mode = Command::new("gsettings")
        .args(["get", "org.gnome.system.proxy", "mode"])
        .output()
        .ok()?;
    if String::from_utf8_lossy(&mode.stdout).trim() != "'manual'" {
        return None;
    }
    for scheme in ["https", "http"] {
        let schema = format!("org.gnome.system.proxy.{scheme}");
        let host = Command::new("gsettings")
            .args(["get", &schema, "host"])
            .output()
            .ok()?;
        let host = String::from_utf8_lossy(&host.stdout)
            .trim()
            .trim_matches('\'')
            .to_string();
        let port = Command::new("gsettings")
            .args(["get", &schema, "port"])
            .output()
            .ok()?;
        let port = String::from_utf8_lossy(&port.stdout).trim().to_string();
        if !host.is_empty() && port.parse::<u16>().ok().filter(|port| *port > 0).is_some() {
            return Some(format!("http://{host}:{port}"));
        }
    }
    None
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn platform_system_proxy() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn select_proxy_server(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if !value.contains('=') {
        return Some(value.to_string());
    }
    let entries = value.split(';').filter_map(|entry| entry.split_once('='));
    let mut http = None;
    for (scheme, address) in entries {
        let address = address.trim();
        if address.is_empty() {
            continue;
        }
        match scheme.trim().to_ascii_lowercase().as_str() {
            "https" => return Some(address.to_string()),
            "http" => http = Some(address.to_string()),
            _ => {}
        }
    }
    http
}

#[cfg(target_os = "macos")]
fn setting_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix(key).map(str::trim))
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_proxy_accepts_local_address_without_scheme() {
        assert_eq!(
            resolve_manual_proxy("127.0.0.1:7890").unwrap(),
            Some("http://127.0.0.1:7890".to_string())
        );
    }

    #[test]
    fn manual_proxy_rejects_unsupported_scheme() {
        assert!(resolve_manual_proxy("socks5://127.0.0.1:7890").is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn selects_https_then_http_from_windows_proxy_list() {
        assert_eq!(
            select_proxy_server("http=127.0.0.1:7890;https=127.0.0.1:7891"),
            Some("127.0.0.1:7891".to_string())
        );
        assert_eq!(
            select_proxy_server("http=127.0.0.1:7890"),
            Some("127.0.0.1:7890".to_string())
        );
    }
}
