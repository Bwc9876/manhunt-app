use reqwest::StatusCode;

use manhunt_logic::prelude::*;

const fn server_host() -> &'static str {
    if let Some(host) = option_env!("SIGNAL_SERVER_HOST") {
        host
    } else {
        "localhost"
    }
}

const fn server_port() -> u16 {
    if let Some(port) = option_env!("SIGNAL_SERVER_PORT") {
        const_str::parse!(port, u16)
    } else {
        3536
    }
}

const fn server_secure() -> bool {
    if let Some(secure) = option_env!("SIGNAL_SERVER_SECURE") {
        const_str::eq_ignore_ascii_case!(secure, "true") || const_str::equal!(secure, "1")
    } else {
        false
    }
}

const fn server_ws_proto() -> &'static str {
    if server_secure() { "wss" } else { "ws" }
}

const fn server_http_proto() -> &'static str {
    if server_secure() { "https" } else { "http" }
}

const SERVER_HOST: &str = server_host();
const SERVER_PORT: u16 = server_port();
const SERVER_WS_PROTO: &str = server_ws_proto();
const SERVER_HTTP_PROTO: &str = server_http_proto();

const SERVER_SOCKET: &str = const_str::concat!(SERVER_HOST, ":", SERVER_PORT);

const SERVER_WEBSOCKET_URL: &str = const_str::concat!(SERVER_WS_PROTO, "://", SERVER_SOCKET);
const SERVER_HTTP_URL: &str = const_str::concat!(SERVER_HTTP_PROTO, "://", SERVER_SOCKET);

pub fn room_url(code: &str, host: bool) -> String {
    let query_param = if host { "?create" } else { "" };
    format!("{SERVER_WEBSOCKET_URL}/{code}{query_param}")
}

pub async fn room_exists(code: &str) -> Result<bool> {
    let url = format!("{SERVER_HTTP_URL}/room_exists/{code}");
    reqwest::get(url)
        .await
        .map(|resp| resp.status() == StatusCode::OK)
        .context("Failed to make request")
}

pub async fn mark_room_started(code: &str) -> Result {
    let url = format!("{SERVER_HTTP_URL}/mark_started/{code}");
    let client = reqwest::Client::builder().build()?;
    client
        .post(url)
        .send()
        .await
        .context("Could not send request")?
        .error_for_status()
        .context("Server returned error")?;
    Ok(())
}

pub fn generate_join_code() -> String {
    // 5 character sequence of A-Z
    (0..5)
        .map(|_| (b'A' + rand::random_range(0..26)) as char)
        .collect::<String>()
}
