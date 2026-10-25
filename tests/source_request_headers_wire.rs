//! 书源请求的首部写法回归。
//!
//! Cloudflare 的托管挑战会按首部写法识别客户端(实测站点上逐项二分确认):
//! `Host` 放在首位 + 其余首部 Title-Case → 200; hyper 默认的「首部全小写、
//! `Host` 最后」→ 403 `cf-mitigated: challenge`。桌面端书源请求因此必须：
//! 1. client 打开 `http1_title_case_headers`;
//! 2. fetcher 显式注入 `Host` 并让它排在最前。
//!
//! 这里用一个本地 TCP 回显服务抓真实发出的字节, 断言这两点。

use reader_rust::crawler::fetcher::{fetch_with_client, RequestSpec};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// 起一个只读一次请求的本地服务, 返回 (地址, 收到请求头的 sniff 结果)。
async fn capture_headers(spec_builder: impl FnOnce(String) -> RequestSpec) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buf = vec![0u8; 8192];
        let n = socket.read(&mut buf).await.unwrap_or(0);
        let _ = socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await;
        String::from_utf8_lossy(&buf[..n]).to_string()
    });
    let spec = spec_builder(format!("http://{addr}/probe?x=1"));
    let client = reader_rust::crawler::http_client::HttpClient::new(10, None)
        .expect("http client")
        .source_context("test", "https://source.example.test", None)
        .expect("context")
        .client;
    let response = fetch_with_client(&client, spec).await.expect("fetch");
    assert_eq!(response.status, 200);
    server.await.expect("server")
}

fn default_headers() -> Vec<(String, String)> {
    vec![
        (
            "User-Agent".to_string(),
            "Mozilla/5.0 (Linux; Android 12) Chrome/120.0.0.0 Mobile Safari/537.36".to_string(),
        ),
        ("Referer".to_string(), "http://source.example.test/".to_string()),
    ]
}

/// `Host` 必须是**第一个**首部, 且首部名是 Title-Case。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_requests_send_host_first_with_title_case_headers() {
    let request = capture_headers(|url| RequestSpec {
        url,
        headers: default_headers(),
        retry: 0,
        ..Default::default()
    })
    .await;

    let mut lines = request.lines();
    let request_line = lines.next().unwrap_or_default();
    assert!(request_line.starts_with("GET /probe?x=1 HTTP/1.1"), "{request}");
    let headers: Vec<&str> = lines.take_while(|line| !line.is_empty()).collect();
    assert!(
        headers
            .first()
            .is_some_and(|line| line.starts_with("Host: 127.0.0.1:")),
        "首个首部必须是 Host: {headers:#?}"
    );
    assert!(
        headers.iter().any(|line| line.starts_with("User-Agent:")),
        "User-Agent 要保留 Title-Case(全小写会被 Cloudflare 判成脚本): {headers:#?}"
    );
    assert!(
        headers.iter().any(|line| line.starts_with("Referer:")),
        "Referer 要保留 Title-Case: {headers:#?}"
    );
    assert!(
        !headers.iter().any(|line| line.starts_with("user-agent:")),
        "不能出现全小写首部: {headers:#?}"
    );
}

/// 书源里显式写了 `Host` 时不要被 URL 推导出的值覆盖。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_host_header_is_not_overridden() {
    let request = capture_headers(|url| RequestSpec {
        url,
        headers: {
            let mut headers = vec![("Host".to_string(), "custom.example.test".to_string())];
            headers.extend(default_headers());
            headers
        },
        retry: 0,
        ..Default::default()
    })
    .await;
    assert!(
        request.contains("Host: custom.example.test"),
        "显式 Host 要保留: {request}"
    );
    assert!(
        request.matches("Host:").count() == 1,
        "不能出现两个 Host: {request}"
    );
}

/// 带端口/带用户信息的地址也要给出正确的 `Host` 值。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_header_keeps_explicit_port() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("accept");
        let mut buf = vec![0u8; 4096];
        let n = socket.read(&mut buf).await.unwrap_or(0);
        let _ = socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await;
        String::from_utf8_lossy(&buf[..n]).to_string()
    });
    let spec = RequestSpec {
        url: format!("http://user:pass@{addr}/x"),
        headers: default_headers(),
        retry: 0,
        ..Default::default()
    };
    let client = reader_rust::crawler::http_client::HttpClient::new(10, None)
        .expect("http client")
        .source_context("test", "https://source.example.test", None)
        .expect("context")
        .client;
    let response = fetch_with_client(&client, spec).await.expect("fetch");
    assert_eq!(response.status, 200);
    let request = server.await.expect("server");
    assert!(
        request.contains(&format!("Host: 127.0.0.1:{}", addr.port())),
        "Host 要带端口且不含 userinfo: {request}"
    );
    assert!(!request.contains("user:pass"), "userinfo 不能进 Host: {request}");
}

