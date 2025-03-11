use hyper::{Body, Request, Response, Server, header, service::{make_service_fn, service_fn}, StatusCode};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use dashmap::DashMap;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::fs::File;
use std::io::BufReader;
use tokio::sync::{Mutex as TokioMutex, RwLock};
use tokio::net::TcpListener;
use tokio::time::sleep;
use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Role;
use tracing::{error, info, warn};
use tracing_subscriber;
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use base64::engine::general_purpose;
use base64::Engine;
use quinn::{Endpoint, ServerConfig as QuinnServerConfig, Connection};
use futures::SinkExt;
use rustls_pemfile::{certs, pkcs8_private_keys};
use webrtc::api::APIBuilder;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use tokio_rustls::TlsAcceptor;
use hyper::upgrade::Upgraded;

const CONTENT_TYPE_UTF8: &str = "text/plain; charset=utf-8";

#[derive(Deserialize, Clone, PartialEq)]
struct Config {
    http_port: u16,
    https_port: u16,
    quic_port: u16,
    cert_path: String,
    key_path: String,
    jwt_secret: String,
    basic_auth_login: String,
    basic_auth_password: String,
}

#[derive(Clone)]
struct CacheEntry {
    response_body: Vec<u8>,
    expiry: Instant,
}

struct ProxyState {
    cache: DashMap<String, CacheEntry>,
    whitelist: RwLock<HashSet<String>>,
    blacklist: RwLock<HashSet<String>>,
    sessions: TokioMutex<HashMap<String, Instant>>,
    auth_attempts: TokioMutex<HashMap<String, (u32, Instant)>>,
    rate_limits: TokioMutex<HashMap<String, (Instant, u32)>>,
    config: TokioMutex<Config>,
    webrtc_peers: TokioMutex<HashMap<String, Arc<RTCPeerConnection>>>,
}

// Загружает конфигурацию из файла config.toml
fn load_config() -> Result<Config, String> {
    let file = File::open("config.toml").map_err(|e| format!("Не удалось открыть config.toml: {}", e))?;
    let content = std::io::read_to_string(BufReader::new(file)).map_err(|e| format!("Ошибка чтения: {}", e))?;
    toml::from_str(&content).map_err(|e| format!("Ошибка парсинга: {}", e))
}

// Проверяет корректность конфигурации (порты и наличие сертификатов)
fn validate_config(config: &Config) -> Result<(), String> {
    if config.http_port == config.https_port || config.http_port == config.quic_port || config.https_port == config.quic_port {
        return Err(format!("Конфликт портов: HTTP={}, HTTPS={}, QUIC={}", config.http_port, config.https_port, config.quic_port));
    }
    if !std::path::Path::new(&config.cert_path).exists() || !std::path::Path::new(&config.key_path).exists() {
        return Err("Сертификат или ключ не найден".to_string());
    }
    load_tls_config(config).map_err(|e| format!("Ошибка TLS конфигурации: {}", e))?;
    Ok(())
}

// Загружает TLS конфигурацию для HTTPS сервера из файлов сертификата и ключа
fn load_tls_config(config: &Config) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let certs = certs(&mut BufReader::new(File::open(&config.cert_path)?))?.into_iter().map(CertificateDer::from).collect();
    let key = pkcs8_private_keys(&mut BufReader::new(File::open(&config.key_path)?))?.into_iter().next().ok_or("Приватный ключ не найден")?;
    let mut cfg = ServerConfig::builder().with_no_client_auth().with_single_cert(certs, PrivateKeyDer::Pkcs8(key.into()))?;
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(cfg)
}

// Загружает конфигурацию для QUIC сервера из файлов сертификата и ключа
fn load_quinn_config(config: &Config) -> QuinnServerConfig {
    let certs = certs(&mut BufReader::new(File::open(&config.cert_path).unwrap())).unwrap().into_iter().map(CertificateDer::from).collect();
    let key = pkcs8_private_keys(&mut BufReader::new(File::open(&config.key_path).unwrap())).unwrap().into_iter().next().unwrap();
    QuinnServerConfig::with_single_cert(certs, PrivateKeyDer::Pkcs8(key.into())).unwrap()
}

// Проверяет авторизацию клиента (Basic Auth или JWT), добавляет IP в чёрный список при превышении попыток
async fn check_auth(req: &Request<Body>, state: &ProxyState, ip: &str) -> Result<(), Response<Body>> {
    let config = state.config.lock().await;
    let auth_header = req.headers().get("Authorization").and_then(|h| h.to_str().ok());
    let is_basic_auth = auth_header.map(|auth| {
        auth.starts_with("Basic ") && general_purpose::STANDARD.decode(auth.trim_start_matches("Basic ")).ok()
            .and_then(|d| String::from_utf8(d).ok())
            .map(|cred| cred == format!("{}:{}", config.basic_auth_login, config.basic_auth_password))
            .unwrap_or(false)
    }).unwrap_or(false);
    let is_jwt_auth = auth_header.map(|auth| {
        auth.starts_with("Bearer ") && decode::<HashMap<String, String>>(auth.trim_start_matches("Bearer "), &DecodingKey::from_secret(config.jwt_secret.as_ref()), &Validation::default()).is_ok()
    }).unwrap_or(false);

    if !is_basic_auth && !is_jwt_auth {
        let mut attempts = state.auth_attempts.lock().await;
        let now = Instant::now();
        let entry = attempts.entry(ip.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) < Duration::from_secs(60) {
            entry.0 += 1;
            if entry.0 >= 5 {
                state.blacklist.write().await.insert(ip.to_string());
                attempts.remove(ip);
                warn!("IP {} добавлен в чёрный список", ip);
                return Err(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Доступ запрещён")).unwrap());
            }
        } else {
            *entry = (1, now);
        }
        return Err(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Неавторизован")).unwrap());
    }
    Ok(())
}

// Получает IP клиента из заголовка X-Forwarded-For или адреса соединения
fn get_client_ip(req: &Request<Body>, client_ip: Option<IpAddr>) -> Option<String> {
    req.headers().get("X-Forwarded-For").and_then(|v| v.to_str().ok()).and_then(|s| s.split(',').next()).map(|s| s.trim().to_string()).or_else(|| client_ip.map(|ip| ip.to_string()))
}

// Обрабатывает WebSocket соединение, отправляя и получая сообщения
async fn handle_websocket(upgraded: Upgraded) {
    let mut ws = WebSocketStream::from_raw_socket(upgraded, Role::Server, None).await;
    while let Some(msg) = tokio_stream::StreamExt::next(&mut ws).await {
        if let Ok(msg) = msg {
            if msg.is_text() || msg.is_binary() {
                ws.send(msg).await.unwrap_or_else(|e| error!("Ошибка WebSocket: {}", e));
            }
        }
    }
}

// Обновляет или создаёт сессию для указанного IP
async fn manage_session(state: &ProxyState, ip: &str) {
    let mut sessions = state.sessions.lock().await;
    sessions.insert(ip.to_string(), Instant::now());
}

// Проверяет лимит запросов для IP, возвращает true, если запрос разрешён
async fn check_rate_limit(state: &ProxyState, ip: &str, max_requests: u32) -> bool {
    let mut limits = state.rate_limits.lock().await;
    let now = Instant::now();
    let entry = limits.entry(ip.to_string()).or_insert((now, 0));
    if now.duration_since(entry.0) > Duration::from_secs(60) {
        *entry = (now, 1);
        true
    } else if entry.1 < max_requests {
        entry.1 += 1;
        true
    } else {
        false
    }
}

// Обрабатывает WebRTC предложение, создаёт соединение и возвращает ответное SDP
async fn handle_webrtc_offer(offer_sdp: String, state: Arc<ProxyState>, client_ip: String) -> Result<String, String> {
    let api = APIBuilder::new().build();
    let peer_connection = Arc::new(api.new_peer_connection(Default::default()).await.map_err(|e| e.to_string())?);
    let data_channel = peer_connection.create_data_channel("proxy", None).await.map_err(|e| e.to_string())?;
    let offer = RTCSessionDescription::offer(offer_sdp).map_err(|e| format!("Ошибка SDP: {}", e))?;
    peer_connection.set_remote_description(offer).await.map_err(|e| e.to_string())?;
    let answer = peer_connection.create_answer(None).await.map_err(|e| e.to_string())?;
    peer_connection.set_local_description(answer.clone()).await.map_err(|e| e.to_string())?;
    state.webrtc_peers.lock().await.insert(client_ip.clone(), peer_connection.clone());
    data_channel.on_message(Box::new(move |msg| {
        info!("WebRTC данные от {}: {:?}", client_ip, msg.data.to_vec());
        Box::pin(async move {})
    }));
    Ok(answer.sdp)
}

// Обрабатывает HTTP запрос, перенаправляя на HTTPS
async fn handle_http_request(req: Request<Body>, https_port: u16) -> Result<Response<Body>, Infallible> {
    let redirect_url = format!("https://{}:{}{}", req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1"), https_port, req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or(""));
    Ok(Response::builder().status(StatusCode::MOVED_PERMANENTLY).header(header::LOCATION, redirect_url).body(Body::empty()).unwrap())
}

// Обрабатывает HTTPS запросы, включая авторизацию, лимиты, WebSocket и WebRTC
async fn handle_https_request(req: Request<Body>, state: Arc<ProxyState>, client_ip: Option<IpAddr>) -> Result<Response<Body>, Infallible> {
    let ip = match get_client_ip(&req, client_ip) {
        Some(ip) => ip,
        None => return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("IP не определён")).unwrap()),
    };

    if state.blacklist.read().await.contains(&ip) {
        return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Доступ запрещён")).unwrap());
    }

    if let Err(resp) = check_auth(&req, &state, &ip).await {
        return Ok(resp);
    }

    manage_session(&state, &ip).await;

    let max_requests = if state.whitelist.read().await.contains(&ip) { 100 } else { 10 };
    if !check_rate_limit(&state, &ip, max_requests).await {
        return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Превышен лимит запросов")).unwrap());
    }

    if req.headers().get("Upgrade").map(|v| v == "websocket").unwrap_or(false) {
        if let Ok(upgraded) = hyper::upgrade::on(req).await {
            tokio::spawn(handle_websocket(upgraded));
            return Ok(Response::new(Body::empty()));
        }
        return Ok(Response::builder().status(500).header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Ошибка WebSocket")).unwrap());
    }

    if req.uri().path() == "/webrtc/offer" {
        let offer_sdp = String::from_utf8_lossy(&hyper::body::to_bytes(req.into_body()).await.unwrap()).to_string();
        match handle_webrtc_offer(offer_sdp, state.clone(), ip).await {
            Ok(answer_sdp) => return Ok(Response::new(Body::from(answer_sdp))),
            Err(e) => return Ok(Response::builder().status(500).header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from(format!("Ошибка WebRTC: {}", e))).unwrap()),
        }
    }

    let url = req.uri().to_string();
    if let Some(entry) = state.cache.get(&url) {
        if entry.expiry > Instant::now() {
            return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from(entry.response_body.clone())).unwrap());
        }
    }

    let response_body = "Добро пожаловать (HTTPS)".as_bytes().to_vec();
    state.cache.insert(url, CacheEntry { response_body: response_body.clone(), expiry: Instant::now() + Duration::from_secs(60) });
    Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from(response_body)).unwrap())
}

// Периодически проверяет и обновляет конфигурацию из файла
async fn reload_config(state: Arc<ProxyState>, tx: tokio::sync::mpsc::Sender<Config>) {
    let mut current_config = state.config.lock().await.clone();
    loop {
        sleep(Duration::from_secs(60)).await;
        if let Ok(new_config) = load_config() {
            if current_config != new_config && validate_config(&new_config).is_ok() {
                info!("Обновление конфигурации: HTTP={}, HTTPS={}, QUIC={}", new_config.http_port, new_config.https_port, new_config.quic_port);
                *state.config.lock().await = new_config.clone();
                if tx.send(new_config.clone()).await.is_err() {
                    error!("Ошибка отправки конфигурации");
                    break;
                }
                current_config = new_config;
            }
        }
    }
}

// Обрабатывает QUIC соединение, принимая и отправляя данные
async fn handle_quic_connection(connection: Connection) {
    info!("QUIC соединение от {:?}", connection.remote_address());
    loop {
        if let Ok((mut send, mut recv)) = connection.accept_bi().await {
            tokio::spawn(async move {
                let mut buffer = vec![0; 1024];
                if let Ok(Some(n)) = recv.read(&mut buffer).await {
                    let response = format!("QUIC ответ: {}", String::from_utf8_lossy(&buffer[..n]));
                    send.write_all(response.as_bytes()).await.unwrap_or_else(|e| error!("Ошибка QUIC: {}", e));
                }
            });
        } else {
            break;
        }
    }
}

// Запускает сервер (HTTP или HTTPS) с graceful shutdown
async fn run_server<F, Fut>(addr: SocketAddr, service: F, mut shutdown_rx: tokio::sync::mpsc::Receiver<()>) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(Request<Body>) -> Fut + Send + Clone + 'static,
    Fut: futures::Future<Output = Result<Response<Body>, Infallible>> + Send + 'static,
{
    let server = Server::bind(&addr).serve(make_service_fn(move |_| {
        let service = service.clone();
        async move { Ok::<_, Infallible>(service_fn(service)) }
    }));
    info!("Сервер слушает на {}", addr);
    tokio::select! {
        result = server => result.map_err(|e| e.into()),
        _ = shutdown_rx.recv() => {
            info!("Сервер на {} завершил работу", addr);
            Ok(())
        }
    }
}

// Запускает все серверы (HTTP, HTTPS, QUIC) с новыми каналами завершения
async fn start_servers(state: Arc<ProxyState>, config: Config, http_tx: &mut tokio::sync::mpsc::Sender<()>, https_tx: &mut tokio::sync::mpsc::Sender<()>, quic_tx: &mut tokio::sync::mpsc::Sender<()>) {
    let (http_shutdown_tx, http_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    let (https_shutdown_tx, https_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    let (quic_shutdown_tx, quic_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    tokio::spawn(run_http_server(config.clone(), http_shutdown_rx));
    tokio::spawn(run_https_server(state.clone(), config.clone(), https_shutdown_rx));
    tokio::spawn(run_quic_server(config.clone(), quic_shutdown_rx));

    *http_tx = http_shutdown_tx;
    *https_tx = https_shutdown_tx;
    *quic_tx = quic_shutdown_tx;
}

// Запускает HTTP сервер с перенаправлением на HTTPS
async fn run_http_server(config: Config, shutdown_rx: tokio::sync::mpsc::Receiver<()>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.http_port));
    let service = move |req: Request<Body>| handle_http_request(req, config.https_port);
    run_server(addr, service, shutdown_rx).await.unwrap_or_else(|e| error!("Ошибка HTTP: {}", e));
}

// Запускает HTTPS сервер с обработкой TLS соединений
async fn run_https_server(state: Arc<ProxyState>, config: Config, mut shutdown_rx: tokio::sync::mpsc::Receiver<()>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.https_port));
    let tls_acceptor = TlsAcceptor::from(Arc::new(load_tls_config(&config).unwrap()));
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("HTTPS сервер слушает на {}", listener.local_addr().unwrap());

    loop {
        tokio::select! {
            Ok((stream, client_ip)) = listener.accept() => {
                let state = state.clone();
                let acceptor = tls_acceptor.clone();
                tokio::spawn(async move {
                    let tls_stream = match acceptor.accept(stream).await {
                        Ok(s) => s,
                        Err(e) => {
                            error!("Ошибка TLS handshake: {}", e);
                            return;
                        }
                    };
                    let service = move |req: Request<Body>| handle_https_request(req, state.clone(), Some(client_ip.ip()));
                    if let Err(e) = Server::builder(hyper::server::accept::from_stream(
                        tokio_stream::once(Ok::<_, std::io::Error>(tls_stream))
                    ))
                    .serve(make_service_fn(move |_| {
                        let service = service.clone();
                        async move { Ok::<_, Infallible>(service_fn(service)) }
                    }))
                    .await
                    {
                        error!("Ошибка HTTPS: {}", e);
                    }
                });
            }
            _ = shutdown_rx.recv() => {
                info!("HTTPS сервер завершил работу");
                break;
            }
        }
    }
}

// Запускает QUIC сервер с обработкой соединений
async fn run_quic_server(config: Config, mut shutdown_rx: tokio::sync::mpsc::Receiver<()>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.quic_port));
    let endpoint = Endpoint::server(load_quinn_config(&config), addr).unwrap();
    info!("QUIC сервер слушает на {}", endpoint.local_addr().unwrap());
    loop {
        tokio::select! {
            Some(conn) = endpoint.accept() => {
                tokio::spawn(handle_quic_connection(conn.await.unwrap()));
            }
            _ = shutdown_rx.recv() => {
                endpoint.close(0u32.into(), b"Shutting down");
                info!("QUIC сервер завершил работу");
                break;
            }
        }
    }
}

// Главная функция, инициализирует и запускает серверы
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider().install_default().expect("Ошибка CryptoProvider");
    tracing_subscriber::fmt::init();

    let initial_config = load_config().and_then(|cfg| validate_config(&cfg).map(|_| cfg)).map_err(|e| {
        error!("Ошибка начальной конфигурации: {}", e);
        "Некорректная конфигурация"
    })?;

    let state = Arc::new(ProxyState {
        cache: DashMap::new(),
        whitelist: RwLock::new(HashSet::new()),
        blacklist: RwLock::new(HashSet::new()),
        sessions: TokioMutex::new(HashMap::new()),
        auth_attempts: TokioMutex::new(HashMap::new()),
        rate_limits: TokioMutex::new(HashMap::new()),
        config: TokioMutex::new(initial_config.clone()),
        webrtc_peers: TokioMutex::new(HashMap::new()),
    });

    let (config_tx, mut config_rx) = tokio::sync::mpsc::channel::<Config>(10);
    let (mut http_shutdown_tx, http_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    let (mut https_shutdown_tx, https_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
    let (mut quic_shutdown_tx, quic_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    tokio::spawn(run_http_server(initial_config.clone(), http_shutdown_rx));
    tokio::spawn(run_https_server(state.clone(), initial_config.clone(), https_shutdown_rx));
    tokio::spawn(run_quic_server(initial_config, quic_shutdown_rx));
    tokio::spawn(reload_config(state.clone(), config_tx));

    while let Some(new_config) = config_rx.recv().await {
        let _ = http_shutdown_tx.send(()).await;
        let _ = https_shutdown_tx.send(()).await;
        let _ = quic_shutdown_tx.send(()).await;
        sleep(Duration::from_secs(1)).await;
        start_servers(state.clone(), new_config, &mut http_shutdown_tx, &mut https_shutdown_tx, &mut quic_shutdown_tx).await;
    }

    tokio::signal::ctrl_c().await?;
    info!("Сервер завершил работу");
    Ok(())
}
