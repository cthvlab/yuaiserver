use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use hyper::server::conn::{AddrStream, AddrIncoming};
use hyper_rustls::{TlsAcceptor, acceptor::TlsStream};
use rustls::{Certificate, PrivateKey, ServerConfig};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use tracing_subscriber;
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;
use base64::engine::general_purpose;
use base64::Engine;
use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Role;
use hyper::upgrade::Upgraded;
use quinn::{Endpoint, ServerConfig as QuinnServerConfig};
use tokio::time::sleep;
use futures::join;
use futures::{StreamExt, SinkExt};
use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio::net::TcpListener;
use webrtc::api::APIBuilder;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

// Структура конфигурации
#[derive(Deserialize, Clone, PartialEq)]
struct Config {
    http_port: u16,
    https_port: u16,
    quic_port: u16,
    cert_path: String,
    key_path: String,
    jwt_secret: String,
}

// Структура для кэширования
#[derive(Clone)]
struct CacheEntry {
    response_body: Vec<u8>,
    expiry: Instant,
}

// Состояние сервера
struct ProxyState {
    cache: Mutex<HashMap<String, CacheEntry>>,
    whitelist: Mutex<HashSet<String>>,
    blacklist: Mutex<HashSet<String>>,
    sessions: Mutex<HashMap<String, Instant>>,
    auth_attempts: Mutex<HashMap<String, (u32, Instant)>>,
    rate_limits: Mutex<HashMap<String, (Instant, u32)>>,
    config: Mutex<Config>,
    webrtc_peers: Mutex<HashMap<String, Arc<RTCPeerConnection>>>,
}

// Загрузка конфигурации из файла
fn load_config() -> Config {
    let file = File::open("config.toml").expect("Не удалось открыть config.toml");
    let reader = BufReader::new(file);
    toml::from_str(&std::io::read_to_string(reader).unwrap()).expect("Ошибка чтения конфигурации")
}

// Настройка TLS для HTTPS
fn load_tls_config(config: &Config) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let cert_file = File::open(&config.cert_path)?;
    let key_file = File::open(&config.key_path)?;

    let certs: Vec<Certificate> = certs(&mut BufReader::new(cert_file))?
        .into_iter()
        .map(|der| Certificate(der.to_vec()))
        .collect();

    let key = pkcs8_private_keys(&mut BufReader::new(key_file))?
        .into_iter()
        .next()
        .map(|der| PrivateKey(der.to_vec()))
        .ok_or("Приватный ключ не найден")?;

    Ok(ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)?)
}

// Настройка TLS для HTTP/3 (QUIC)
fn load_quinn_server_config(config: &Config) -> QuinnServerConfig {
    let cert_file = File::open(&config.cert_path).expect("Не удалось открыть сертификат");
    let key_file = File::open(&config.key_path).expect("Не удалось открыть ключ");

    let certs: Vec<Certificate> = certs(&mut BufReader::new(cert_file))
        .expect("Ошибка чтения сертификатов")
        .into_iter()
        .map(|der| Certificate(der.to_vec()))
        .collect();

    let key = pkcs8_private_keys(&mut BufReader::new(key_file))
        .expect("Ошибка чтения ключей")
        .into_iter()
        .next()
        .map(|der| PrivateKey(der.to_vec()))
        .expect("Приватный ключ не найден");

    QuinnServerConfig::with_single_cert(certs, key).expect("Ошибка создания QUIC конфигурации")
}

// Проверка Basic Auth
fn check_basic_auth(req: &Request<Body>) -> bool {
    if let Some(auth) = req.headers().get("Authorization") {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Basic ") {
                let encoded = auth_str.trim_start_matches("Basic ");
                if let Ok(decoded) = general_purpose::STANDARD.decode(encoded) {
                    if let Ok(credentials) = String::from_utf8(decoded) {
                        return credentials == "user:password";
                    }
                }
            }
        }
    }
    false
}

// Проверка JWT
fn check_jwt(req: &Request<Body>, secret: &str) -> bool {
    if let Some(auth) = req.headers().get("Authorization") {
        if let Ok(token) = auth.to_str() {
            if token.starts_with("Bearer ") {
                let token = token.trim_start_matches("Bearer ");
                let validation = Validation::default();
                let decoding_key = DecodingKey::from_secret(secret.as_ref());
                return decode::<HashMap<String, String>>(token, &decoding_key, &validation).is_ok();
            }
        }
    }
    false
}

// Извлечение реального IP из запроса
fn get_client_ip(req: &Request<Body>, client_ip: IpAddr) -> String {
    req.headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| client_ip.to_string())
}

// Обработка WebSocket
async fn handle_websocket(upgraded: Upgraded) {
    let ws_stream = WebSocketStream::from_raw_socket(upgraded, Role::Server, None).await;
    let mut ws = ws_stream;
    while let Some(msg) = ws.next().await {
        if let Ok(msg) = msg {
            if msg.is_text() || msg.is_binary() {
                ws.send(msg).await.unwrap_or_else(|e| error!("Ошибка отправки WebSocket: {}", e));
            }
        }
    }
}

// Управление сессиями
async fn manage_session(state: &ProxyState, ip: &str) {
    let mut sessions = state.sessions.lock().await;
    let now = Instant::now();
    sessions.insert(ip.to_string(), now);
}

// Проверка ограничения скорости
async fn check_rate_limit(state: &ProxyState, ip: &str, max_requests: u32, window: Duration) -> bool {
    let mut limits = state.rate_limits.lock().await;
    let now = Instant::now();
    let entry = limits.entry(ip.to_string()).or_insert((now, 0));
    if now.duration_since(entry.0) > window {
        *entry = (now, 1);
        true
    } else if entry.1 < max_requests {
        entry.1 += 1;
        true
    } else {
        false
    }
}

// Обработка WebRTC offer
async fn handle_webrtc_offer(
    offer_sdp: String,
    state: Arc<ProxyState>,
    client_ip: String,
) -> Result<String, String> {
    let api = APIBuilder::new().build();
    let peer_connection = api
        .new_peer_connection(Default::default())
        .await
        .map_err(|e| e.to_string())?;
    let peer_connection = Arc::new(peer_connection);

    // Создаём канал данных
    let data_channel = peer_connection
        .create_data_channel("proxy", None)
        .await
        .map_err(|e| e.to_string())?;

    // Устанавливаем удалённое описание (offer от клиента)
    let offer = RTCSessionDescription::offer(offer_sdp).map_err(|e| e.to_string())?;
    peer_connection
        .set_remote_description(offer)
        .await
        .map_err(|e| e.to_string())?;

    // Создаём ответ (answer)
    let answer = peer_connection
        .create_answer(None)
        .await
        .map_err(|e| e.to_string())?;
    peer_connection
        .set_local_description(answer.clone())
        .await
        .map_err(|e| e.to_string())?;

    // Сохраняем соединение
    state
        .webrtc_peers
        .lock()
        .await
        .insert(client_ip.clone(), peer_connection.clone());

    // Обработка входящих данных
    data_channel.on_message(Box::new(move |msg| {
        let data = msg.data.to_vec();
        info!("Получены данные через WebRTC от {}: {:?}", client_ip, data);
        Box::pin(async move {})
    }));

    Ok(answer.sdp)
}

// Обработка HTTP-запросов
async fn handle_http_request(
    req: Request<Body>,
    state: Arc<ProxyState>,
    client_ip: IpAddr,
) -> Result<Response<Body>, Infallible> {
    let ip = get_client_ip(&req, client_ip);
    let url = req.uri().to_string();

    // Проверка чёрного списка
    {
        let blacklist = state.blacklist.lock().await;
        if blacklist.contains(&ip) {
            return Ok(Response::new(Body::from("Доступ запрещён")));
        }
    }
    let is_whitelisted = state.whitelist.lock().await.contains(&ip);

    // Проверка авторизации
    let config = state.config.lock().await;
    if !check_basic_auth(&req) && !check_jwt(&req, &config.jwt_secret) {
        let mut attempts = state.auth_attempts.lock().await;
        let now = Instant::now();
        let entry = attempts.entry(ip.clone()).or_insert((0, now));
        if now.duration_since(entry.1) < Duration::from_secs(60) {
            entry.0 += 1;
            if entry.0 >= 5 {
                state.blacklist.lock().await.insert(ip.clone());
                attempts.remove(&ip);
                warn!("IP {} добавлен в чёрный список", ip);
                return Ok(Response::new(Body::from("Доступ запрещён")));
            }
        } else {
            *entry = (1, now);
        }
        return Ok(Response::new(Body::from("Неавторизован")));
    }

    // Управление сессиями
    manage_session(&state, &ip).await;

    // Ограничение скорости
    let max_requests = if is_whitelisted { 100 } else { 10 };
    if !check_rate_limit(&state, &ip, max_requests, Duration::from_secs(60)).await {
        return Ok(Response::new(Body::from("Превышен лимит запросов")));
    }

    // Обработка WebSocket
    if req.headers().contains_key("Upgrade") && req.headers().get("Upgrade").unwrap() == "websocket" {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                tokio::spawn(handle_websocket(upgraded));
                return Ok(Response::new(Body::empty()));
            }
            Err(e) => {
                error!("Ошибка обновления соединения: {}", e);
                return Ok(Response::builder()
                    .status(500)
                    .body(Body::from("Внутренняя ошибка сервера"))
                    .unwrap());
            }
        }
    }

    // Обработка WebRTC
    if req.uri().path() == "/webrtc/offer" {
        let body = hyper::body::to_bytes(req.into_body()).await.unwrap();
        let offer_sdp = String::from_utf8_lossy(&body).to_string();
        match handle_webrtc_offer(offer_sdp, state.clone(), ip.clone()).await {
            Ok(answer_sdp) => {
                return Ok(Response::new(Body::from(answer_sdp)));
            }
            Err(e) => {
                error!("Ошибка обработки WebRTC offer: {}", e);
                return Ok(Response::builder()
                    .status(500)
                    .body(Body::from("Ошибка WebRTC"))
                    .unwrap());
            }
        }
    }

    // Проверка кэша
    let mut cache = state.cache.lock().await;
    if let Some(entry) = cache.get(&url) {
        if entry.expiry > Instant::now() {
            info!("Кэшированный ответ для {}", url);
            return Ok(Response::new(Body::from(entry.response_body.clone())));
        }
    }

    // Обработка запроса
    let response_body = "Добро пожаловать".as_bytes().to_vec();
    let response = Response::new(Body::from(response_body.clone()));
    cache.insert(url.clone(), CacheEntry {
        response_body,
        expiry: Instant::now() + Duration::from_secs(60),
    });

    Ok(response)
}

// Обработка HTTPS-запросов
async fn handle_https_request(
    req: Request<Body>,
    state: Arc<ProxyState>,
    client_ip: IpAddr,
) -> Result<Response<Body>, Infallible> {
    let ip = get_client_ip(&req, client_ip);
    let url = req.uri().to_string();

    // Проверка чёрного списка
    {
        let blacklist = state.blacklist.lock().await;
        if blacklist.contains(&ip) {
            return Ok(Response::new(Body::from("Доступ запрещён")));
        }
    }
    let is_whitelisted = state.whitelist.lock().await.contains(&ip);

    // Проверка авторизации
   /*  let config = state.config.lock().await;
    if !check_basic_auth(&req) && !check_jwt(&req, &config.jwt_secret) {
        let mut attempts = state.auth_attempts.lock().await;
        let now = Instant::now();
        let entry = attempts.entry(ip.clone()).or_insert((0, now));
        if now.duration_since(entry.1) < Duration::from_secs(60) {
            entry.0 += 1;
            if entry.0 >= 5 {
                state.blacklist.lock().await.insert(ip.clone());
                attempts.remove(&ip);
                warn!("IP {} добавлен в чёрный список", ip);
                return Ok(Response::new(Body::from("Доступ запрещён")));
            }
        } else {
            *entry = (1, now);
        }
        return Ok(Response::new(Body::from("Неавторизован")));
    } */

    // Управление сессиями
    manage_session(&state, &ip).await;

    // Ограничение скорости
    let max_requests = if is_whitelisted { 100 } else { 10 };
    if !check_rate_limit(&state, &ip, max_requests, Duration::from_secs(60)).await {
        return Ok(Response::new(Body::from("Превышен лимит запросов")));
    }

    // Обработка WebSocket
    if req.headers().contains_key("Upgrade") && req.headers().get("Upgrade").unwrap() == "websocket" {
        match hyper::upgrade::on(req).await {
            Ok(upgraded) => {
                tokio::spawn(handle_websocket(upgraded));
                return Ok(Response::new(Body::empty()));
            }
            Err(e) => {
                error!("Ошибка обновления соединения: {}", e);
                return Ok(Response::builder()
                    .status(500)
                    .body(Body::from("Внутренняя ошибка сервера"))
                    .unwrap());
            }
        }
    }

    // Обработка WebRTC
    if req.uri().path() == "/webrtc/offer" {
        let body = hyper::body::to_bytes(req.into_body()).await.unwrap();
        let offer_sdp = String::from_utf8_lossy(&body).to_string();
        match handle_webrtc_offer(offer_sdp, state.clone(), ip.clone()).await {
            Ok(answer_sdp) => {
                return Ok(Response::new(Body::from(answer_sdp)));
            }
            Err(e) => {
                error!("Ошибка обработки WebRTC offer: {}", e);
                return Ok(Response::builder()
                    .status(500)
                    .body(Body::from("Ошибка WebRTC"))
                    .unwrap());
            }
        }
    }

    // Проверка кэша
    let mut cache = state.cache.lock().await;
    if let Some(entry) = cache.get(&url) {
        if entry.expiry > Instant::now() {
            info!("Кэшированный ответ для {}", url);
            return Ok(Response::new(Body::from(entry.response_body.clone())));
        }
    }

    // Обработка запроса
    let response_body = "Добро пожаловать (HTTPS)".as_bytes().to_vec();
    let response = Response::new(Body::from(response_body.clone()));
    cache.insert(url.clone(), CacheEntry {
        response_body,
        expiry: Instant::now() + Duration::from_secs(60),
    });

    Ok(response)
}

// Динамическая перезагрузка конфигурации
async fn reload_config(state: Arc<ProxyState>) {
    loop {
        sleep(Duration::from_secs(60)).await;
        let new_config = load_config();
        let mut config = state.config.lock().await;
        if *config != new_config {
            *config = new_config;
            info!("Конфигурация обновлена");
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = load_config();
    let state = Arc::new(ProxyState {
        cache: Mutex::new(HashMap::new()),
        whitelist: Mutex::new(HashSet::new()),
        blacklist: Mutex::new(HashSet::new()),
        sessions: Mutex::new(HashMap::new()),
        auth_attempts: Mutex::new(HashMap::new()),
        rate_limits: Mutex::new(HashMap::new()),
        config: Mutex::new(config.clone()),
        webrtc_peers: Mutex::new(HashMap::new()),
    });

    let tls_config = Arc::new(load_tls_config(&config).expect("Ошибка загрузки TLS"));
    let https_listener = TcpListener::bind(("127.0.0.1", config.https_port)).await.unwrap();
    let incoming = AddrIncoming::from_listener(https_listener).unwrap();
    let acceptor = TlsAcceptor::new(tls_config, incoming);

    let http_addr = SocketAddr::from(([127, 0, 0, 1], config.http_port));
    let state_http = state.clone();
    let http_service = make_service_fn(move |conn: &AddrStream| {
        let state = state_http.clone();
        let client_ip = conn.remote_addr().ip();
        async move {
            let service = service_fn(move |req| handle_http_request(req, state.clone(), client_ip));
            Ok::<_, Infallible>(service)
        }
    });
    let http_server = Server::bind(&http_addr).serve(http_service);

    let state_https = state.clone();
    let https_service = make_service_fn(move |conn: &TlsStream<AddrStream>| {
        let state = state_https.clone();
        let client_ip = conn.io().expect("TLS stream should have an AddrStream").remote_addr().ip();
        async move {
            let service = service_fn(move |req| handle_https_request(req, state.clone(), client_ip));
            Ok::<_, Infallible>(service)
        }
    });
    let https_server = Server::builder(acceptor).serve(https_service);

    let quic_config = load_quinn_server_config(&config);
    let quic_endpoint = Endpoint::server(quic_config, SocketAddr::from(([127, 0, 0, 1], config.quic_port))).unwrap();
    tokio::spawn(async move {
        while let Some(conn) = quic_endpoint.accept().await {
            let connection = conn.await.unwrap();
            tokio::spawn(async move {
                if let Ok((mut send, mut recv)) = connection.accept_bi().await {
                    let mut buffer = vec![0; 1024];
                    if let Ok(n) = recv.read(&mut buffer).await {
                        let response = format!("QUIC response: {}", String::from_utf8_lossy(&buffer[..n.unwrap()]));
                        send.write_all(response.as_bytes()).await.unwrap();
                    }
                }
            });
        }
    });

    tokio::spawn(reload_config(state.clone()));

    info!(
        "Сервер запущен: HTTP ({}), HTTPS ({}), HTTP/3 ({})",
        config.http_port, config.https_port, config.quic_port
    );

    let (res1, res2) = join!(http_server, https_server);
if let Err(e) = res1 {
    error!("Ошибка HTTP сервера: {:?}", e);
}
if let Err(e) = res2 {
    error!("Ошибка HTTPS сервера: {:?}", e);
}
}