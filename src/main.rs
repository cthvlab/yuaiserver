use hyper::{service::service_fn, Request, Response, StatusCode, Error as HttpError};
use hyper_util::{rt::{TokioExecutor, TokioIo}, server::conn::auto::Builder as AutoBuilder};
use hyper::header;
use hyper_tungstenite::{HyperWebsocket, upgrade};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::ServerConfig;
use dashmap::DashMap;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::fs::File;
use std::io::BufReader as StdBufReader;
use tokio::sync::{Mutex as TokioMutex, RwLock};
use tokio::net::{TcpListener};
use tokio_rustls::TlsAcceptor;
use tokio::task::JoinHandle;
use tokio::runtime::Builder;
use tracing::{error, info, warn};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use quinn::{Endpoint, ServerConfig as QuinnServerConfig, Connection};
use futures::{StreamExt, SinkExt};
use rustls_pemfile::{certs, pkcs8_private_keys};
use webrtc::api::APIBuilder;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::ice_transport::ice_server::RTCIceServer;
use http_body_util::BodyExt;
use crate::console::run_console;

mod console;

const CONTENT_TYPE_UTF8: &str = "text/plain; charset=utf-8";

#[derive(Deserialize, Clone, PartialEq, Debug)]
struct Location {
    path: String,
    response: String,
}

#[derive(Deserialize, Clone, PartialEq)]
struct Config {
    http_port: u16,
    https_port: u16,
    quic_port: u16,
    cert_path: String,
    key_path: String,
    jwt_secret: String,
    worker_threads: usize,
    locations: Vec<Location>,
    ice_servers: Option<Vec<String>>
}

#[derive(Clone)]
struct CacheEntry {
    response_body: Vec<u8>,
    expiry: Instant,
}

#[derive(Clone)]
struct AuthCacheEntry {
    is_valid: bool,
    expiry: Instant,
}

struct ProxyState {
    cache: DashMap<String, CacheEntry>,
    auth_cache: DashMap<String, AuthCacheEntry>,
    whitelist: DashMap<String, ()>,
    blacklist: DashMap<String, ()>,
    sessions: DashMap<String, Instant>,
    auth_attempts: DashMap<String, (u32, Instant)>,
    rate_limits: DashMap<String, (Instant, u32)>,
    config: TokioMutex<Config>,
    webrtc_peers: DashMap<String, Arc<RTCPeerConnection>>,
    locations: Arc<RwLock<Vec<Location>>>,
    http_running: Arc<RwLock<bool>>,
    https_running: Arc<RwLock<bool>>,
    quic_running: Arc<RwLock<bool>>,
    http_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    https_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
    quic_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>,
}

async fn load_config() -> Result<Config, String> {
    let content = tokio::fs::read_to_string("config.toml").await
        .map_err(|e| format!("Ошибка чтения config.toml: {}", e))?;
    toml::from_str(&content)
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}

fn validate_config(config: &Config) -> Result<(), String> {
    if config.http_port == config.https_port || config.http_port == config.quic_port || config.https_port == config.quic_port {
        return Err(format!("Конфликт портов: HTTP={}, HTTPS={}, QUIC={}", config.http_port, config.https_port, config.quic_port));
    }
    if !std::path::Path::new(&config.cert_path).exists() || !std::path::Path::new(&config.key_path).exists() {
        return Err("Сертификат или ключ не найден".to_string());
    }
    if config.worker_threads == 0 || config.worker_threads > 1024 {
        return Err("Недопустимое количество потоков: должно быть от 1 до 1024".to_string());
    }
    load_tls_config(config).map_err(|e| format!("Ошибка TLS конфигурации: {}", e))?;

    // Проверка ice_servers
    match &config.ice_servers {
        Some(servers) if servers.is_empty() => {
            return Err("Список ICE-серверов не может быть пустым".to_string());
        }
        None => {
            warn!("ICE-серверы не указаны в конфигурации, будет использован стандартный STUN-сервер.");
        }
        Some(_) => {} // Всё в порядке, ICE-серверы есть и не пустые
    }

    Ok(())
}



fn load_tls_config(config: &Config) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let cert_file = File::open(&config.cert_path)?;
    let key_file = File::open(&config.key_path)?;

    // Load certificates
    let certs: Vec<CertificateDer> = certs(&mut StdBufReader::new(cert_file))?
        .into_iter()
        .map(|bytes| CertificateDer::from(bytes))
        .collect();

    // Load private keys
    let keys: Vec<PrivateKeyDer> = pkcs8_private_keys(&mut StdBufReader::new(key_file))?
        .into_iter()
        .map(|bytes| PrivateKeyDer::from(PrivatePkcs8KeyDer::from(bytes)))
        .collect();
    let key = keys.into_iter().next().ok_or("Приватный ключ не найден")?;

    // Configure ServerConfig
    let mut cfg = ServerConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(cfg)
}

fn load_quinn_config(config: &Config) -> Result<QuinnServerConfig, Box<dyn std::error::Error>> {
    let cert_file = File::open(&config.cert_path)?;
    let key_file = File::open(&config.key_path)?;

    // Load certificates
    let certs: Vec<CertificateDer> = certs(&mut StdBufReader::new(cert_file))?
        .into_iter()
        .map(|bytes| CertificateDer::from(bytes))
        .collect();

    // Load private keys
    let keys: Vec<PrivateKeyDer> = pkcs8_private_keys(&mut StdBufReader::new(key_file))?
        .into_iter()
        .map(|bytes| PrivateKeyDer::from(PrivatePkcs8KeyDer::from(bytes)))
        .collect();
    let key = keys.into_iter().next().ok_or("Приватный ключ не найден")?;

    // Configure QuinnServerConfig
    QuinnServerConfig::with_single_cert(certs, key)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}




async fn check_auth(req: &Request<hyper::body::Incoming>, state: &ProxyState, ip: &str) -> Result<(), Response<String>> {
    if let Some(entry) = state.auth_cache.get(ip) {
        if entry.expiry > Instant::now() {
            if entry.is_valid {
                return Ok(());
            } else {
                return Err(Response::builder()
                    .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                    .body("Неавторизован".to_string())
                    .unwrap());
            }
        }
    }

    let config = state.config.lock().await;
    let auth_header = req.headers().get("Authorization").and_then(|h| h.to_str().ok());

    let is_jwt_auth = auth_header.map(|auth| {
        auth.starts_with("Bearer ") && 
        decode::<HashMap<String, String>>(
            auth.trim_start_matches("Bearer "),
            &DecodingKey::from_secret(config.jwt_secret.as_ref()),
            &Validation::default()
        ).is_ok()
    }).unwrap_or(false);

    if !is_jwt_auth {
        let now = Instant::now();
        let mut entry = state.auth_attempts.entry(ip.to_string()).or_insert((0, now));
        if now.duration_since(entry.1) < Duration::from_secs(60) {
            entry.0 += 1;
            if entry.0 >= 5 {
                state.blacklist.insert(ip.to_string(), ());
                state.auth_attempts.remove(ip);
                warn!("IP {} добавлен в чёрный список", ip);
                state.auth_cache.insert(ip.to_string(), AuthCacheEntry { 
                    is_valid: false, 
                    expiry: Instant::now() + Duration::from_secs(60) 
                });
                return Err(Response::builder()
                    .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                    .body("Доступ запрещён".to_string())
                    .unwrap());
            }
        } else {
            *entry = (1, now);
        }
        state.auth_cache.insert(ip.to_string(), AuthCacheEntry { 
            is_valid: false, 
            expiry: Instant::now() + Duration::from_secs(60) 
        });
        return Err(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Неавторизован".to_string())
            .unwrap());
    }
    
    state.auth_cache.insert(ip.to_string(), AuthCacheEntry { 
        is_valid: true, 
        expiry: Instant::now() + Duration::from_secs(60) 
    });
    Ok(())
}

fn get_client_ip(req: &Request<hyper::body::Incoming>, client_ip: Option<IpAddr>) -> Option<String> {
    req.headers().get("X-Forwarded-For").and_then(|v| v.to_str().ok()).and_then(|s| s.split(',').next()).map(|s| s.trim().to_string()).or_else(|| client_ip.map(|ip| ip.to_string()))
}

async fn handle_websocket(websocket: HyperWebsocket) {
    let mut ws = websocket.await.unwrap(); // Преобразуем HyperWebsocket в WebSocketStream
    while let Some(msg) = ws.next().await {
        match msg {
            Ok(msg) if msg.is_text() || msg.is_binary() => {
                ws.send(msg).await.unwrap_or_else(|e| error!("Ошибка WebSocket: {}", e));
            }
            Err(e) => error!("Ошибка WebSocket: {}", e),
            _ => {}
        }
    }
}

async fn manage_session(state: &ProxyState, ip: &str) {
    state.sessions.insert(ip.to_string(), Instant::now());
}

async fn check_rate_limit(state: &ProxyState, ip: &str, max_requests: u32) -> bool {
    let now = Instant::now();
    let mut entry = state.rate_limits.entry(ip.to_string()).or_insert((now, 0));
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

async fn handle_webrtc_offer(offer_sdp: String, state: Arc<ProxyState>, client_ip: String) -> Result<String, String> {
    let api = APIBuilder::new().build();
    let config = state.config.lock().await; // Получаем доступ к конфигурации
    let ice_servers: Vec<RTCIceServer> = config.ice_servers.clone().unwrap_or_else(|| {
        warn!("ICE-серверы не указаны в конфигурации, используется стандартный STUN-сервер.");
        vec!["stun:stun.l.google.com:19302".to_string()]
    }).iter().map(|url| RTCIceServer {
        urls: vec![url.clone()],
        ..Default::default()
    }).collect();

    let config = webrtc::peer_connection::configuration::RTCConfiguration {
        ice_servers,
        ..Default::default()
    };
    let peer_connection = Arc::new(api.new_peer_connection(config).await.map_err(|e| e.to_string())?);
    let data_channel = peer_connection.create_data_channel("proxy", None).await.map_err(|e| e.to_string())?;
    let offer = RTCSessionDescription::offer(offer_sdp).map_err(|e| format!("Ошибка SDP: {}", e))?;
    peer_connection.set_remote_description(offer).await.map_err(|e| e.to_string())?;
    let answer = peer_connection.create_answer(None).await.map_err(|e| e.to_string())?;
    peer_connection.set_local_description(answer.clone()).await.map_err(|e| e.to_string())?;
    state.webrtc_peers.insert(client_ip.clone(), peer_connection.clone());
    data_channel.on_message(Box::new(move |msg| {
        info!("WebRTC данные от {}: {:?}", client_ip, msg.data.to_vec());
        Box::pin(async move {})
    }));
    Ok(answer.sdp)
}


async fn handle_http_request(req: Request<hyper::body::Incoming>, https_port: u16) -> Result<Response<String>, Infallible> {
    let redirect_url = format!(
        "https://{}:{}{}",
        req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1"),
        https_port,
        req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("")
    );
    Ok(Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY)
        .header(header::LOCATION, redirect_url)
        .body(String::new())
        .unwrap())
}

async fn handle_https_request(
    mut req: Request<hyper::body::Incoming>,
    state: Arc<ProxyState>,
    client_ip: Option<IpAddr>,
) -> Result<Response<String>, HttpError> {
    let ip = match get_client_ip(&req, client_ip) {
        Some(ip) => ip,
        None => return Ok(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("IP не определён".to_string())
            .unwrap()),
    };

    if state.blacklist.contains_key(&ip) {
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Доступ запрещён".to_string())
            .unwrap());
    }

    if let Err(resp) = check_auth(&req, &state, &ip).await {
        return Ok(resp);
    }

    manage_session(&state, &ip).await;

    let max_requests = if state.whitelist.contains_key(&ip) { 100 } else { 10 };
    if !check_rate_limit(&state, &ip, max_requests).await {
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Превышен лимит запросов".to_string())
            .unwrap());
    }

    if hyper_tungstenite::is_upgrade_request(&req) {
        let (response, websocket) = match upgrade(&mut req, None) {
            Ok(result) => result,
            Err(e) => return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(format!("Ошибка апгрейда WebSocket: {}", e))
                .unwrap()),
        };
        tokio::spawn(handle_websocket(websocket));
        return Ok(response.map(|_| String::new()));
    }

    if req.uri().path() == "/webrtc/offer" {
        let body = req.into_body();
        let collected = body.collect().await.map_err(|e| HttpError::from(e))?;
        let body_bytes = collected.to_bytes();
        let offer_sdp = String::from_utf8_lossy(&body_bytes).to_string();
        match handle_webrtc_offer(offer_sdp, state.clone(), ip).await {
            Ok(answer_sdp) => return Ok(Response::new(answer_sdp)),
            Err(e) => return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                .body(format!("Ошибка WebRTC: {}", e))
                .unwrap()),
        }
    }

    let url = req.uri().to_string();
    if let Some(entry) = state.cache.get(&url) {
        if entry.expiry > Instant::now() {
            return Ok(Response::builder()
                .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                .body(String::from_utf8_lossy(&entry.response_body).to_string())
                .unwrap());
        }
    }

    let locations = state.locations.read().await;
    let path = req.uri().path();
    let response_body = locations.iter()
        .find(|loc| path.starts_with(&loc.path))
        .map(|loc| loc.response.clone())
        .unwrap_or_else(|| "404 Not Found".to_string());

    state.cache.insert(url, CacheEntry { 
        response_body: response_body.as_bytes().to_vec(), 
        expiry: Instant::now() + Duration::from_secs(60) 
    });
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
        .body(response_body)
        .unwrap())
}

async fn reload_config(state: Arc<ProxyState>) {  
    let mut current_config = state.config.lock().await.clone();
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        if let Ok(new_config) = load_config().await {
            if current_config != new_config && validate_config(&new_config).is_ok() {
                info!("Обновление конфигурации: HTTP={}, HTTPS={}, QUIC={}", 
                      new_config.http_port, new_config.https_port, new_config.quic_port);
                *state.config.lock().await = new_config.clone();
                *state.locations.write().await = new_config.locations.clone();
                
                if !*state.http_running.read().await {
                    let handle = tokio::spawn(run_http_server(new_config.clone(), state.clone()));
                    *state.http_handle.lock().await = Some(handle);
                }
                if !*state.https_running.read().await {
                    let handle = tokio::spawn(run_https_server(state.clone(), new_config.clone()));
                    *state.https_handle.lock().await = Some(handle);
                }
                if !*state.quic_running.read().await {
                    let handle = tokio::spawn(run_quic_server(new_config.clone(), state.clone()));
                    *state.quic_handle.lock().await = Some(handle);
                }
                
                current_config = new_config;
            }
        }
    }
}

async fn handle_quic_connection(connection: Connection) {
    info!("QUIC соединение от {:?}", connection.remote_address());
    loop {
        if let Ok((mut send, mut recv)) = connection.accept_bi().await {
            tokio::spawn(async move {
                let mut buffer = vec![0; 4096];
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

async fn run_http_server(config: Config, state: Arc<ProxyState>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.http_port));
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("\x1b[92mHTTP сервер слушает на {} \x1b[0m", addr);
    *state.http_running.write().await = true;

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let stream = TokioIo::new(stream); // Оборачиваем в TokioIo
        let https_port = config.https_port;
        tokio::spawn(async move {
            if let Err(e) = AutoBuilder::new(TokioExecutor::new())
                .http1()
                .serve_connection(stream, service_fn(move |req| handle_http_request(req, https_port)))
                .await
            {
                error!("Ошибка HTTP: {}", e);
            }
        });
    }
}

async fn run_https_server(state: Arc<ProxyState>, config: Config) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.https_port));
    let listener = TcpListener::bind(addr).await.unwrap();
    info!("\x1b[92mHTTPS сервер слушает на {} \x1b[0m", listener.local_addr().unwrap());
    *state.https_running.write().await = true;
    let tls_acceptor = TlsAcceptor::from(Arc::new(load_tls_config(&config).unwrap()));

    loop {
        if let Ok((stream, client_ip)) = listener.accept().await {
            stream.set_nodelay(true).unwrap();
            let state = state.clone();
            let acceptor = tls_acceptor.clone();
            tokio::spawn(async move {
                let tls_stream = match acceptor.accept(stream).await {
                    Ok(s) => TokioIo::new(s), // Оборачиваем в TokioIo
                    Err(e) => {
                        error!("Ошибка TLS handshake: {}", e);
                        return;
                    }
                };
                let service = service_fn(move |req| handle_https_request(req, state.clone(), Some(client_ip.ip())));
                if let Err(e) = AutoBuilder::new(TokioExecutor::new())
                    .http1()
                    .http2()
                    .serve_connection(tls_stream, service)
                    .await
                {
                    error!("Ошибка HTTPS: {}", e);
                }
            });
        }
    }
}

async fn run_quic_server(config: Config, state: Arc<ProxyState>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.quic_port));
    match Endpoint::server(load_quinn_config(&config).unwrap(), addr) {
        Ok(endpoint) => {
            info!("\x1b[92mQUIC сервер слушает на {} \x1b[0m", endpoint.local_addr().unwrap());
            *state.quic_running.write().await = true;
            while let Some(conn) = endpoint.accept().await {
                tokio::spawn(handle_quic_connection(conn.await.unwrap()));
            }
        }
        Err(e) => {
            error!("Не удалось запустить QUIC сервер на порту {}: {}. Измените quic_port в config.toml.", config.quic_port, e);
            *state.quic_running.write().await = false;
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Ошибка CryptoProvider");

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let initial_config = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            match load_config().await {
                Ok(cfg) => match validate_config(&cfg) {
                    Ok(()) => Ok(cfg),
                    Err(e) => {
                        tracing::error!("Ошибка валидации конфигурации: {}", e);
                        Err("Некорректная конфигурация")
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "Ошибка загрузки конфигурации: {}. Используются настройки по умолчанию.",
                        e
                    );
                    let mut default_config = Config {
                        http_port: 80,
                        https_port: 443,
                        quic_port: 444,
                        cert_path: "cert.pem".to_string(),
                        key_path: "key.pem".to_string(),
                        jwt_secret: "your_jwt_secret".to_string(),
                        worker_threads: 16,
                        locations: vec![],
                        ice_servers: Some(vec!["stun:stun.l.google.com:19302".to_string()]),
                    };
                    if e.contains("missing field `ice_servers`") {
                        if let Ok(content) = tokio::fs::read_to_string("config.toml").await {
                            if let Ok(mut partial_config) = toml::from_str::<Config>(&content) {
                                partial_config.ice_servers =
                                    Some(vec!["stun:stun.l.google.com:19302".to_string()]);
                                default_config = partial_config;
                            }
                        }
                    }
                    match validate_config(&default_config) {
                        Ok(()) => Ok(default_config),
                        Err(e) => {
                            tracing::error!("Дефолтная конфигурация недействительна: {}", e);
                            Err("Некорректная конфигурация")
                        }
                    }
                }
            }
        })?;

    let runtime = Builder::new_multi_thread()
        .worker_threads(initial_config.worker_threads)
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        let state = Arc::new(ProxyState {
            cache: DashMap::new(),
            auth_cache: DashMap::new(),
            whitelist: DashMap::new(),
            blacklist: DashMap::new(),
            sessions: DashMap::new(),
            auth_attempts: DashMap::new(),
            rate_limits: DashMap::new(),
            config: TokioMutex::new(initial_config.clone()),
            webrtc_peers: DashMap::new(),
            locations: Arc::new(RwLock::new(initial_config.locations.clone())),
            http_running: Arc::new(RwLock::new(false)),
            https_running: Arc::new(RwLock::new(false)),
            quic_running: Arc::new(RwLock::new(false)),
            http_handle: Arc::new(TokioMutex::new(None)),
            https_handle: Arc::new(TokioMutex::new(None)),
            quic_handle: Arc::new(TokioMutex::new(None)),
        });

        let http_handle = tokio::spawn(run_http_server(initial_config.clone(), state.clone()));
        let https_handle = tokio::spawn(run_https_server(state.clone(), initial_config.clone()));
        let quic_handle = tokio::spawn(run_quic_server(initial_config.clone(), state.clone()));
        let reload_handle = tokio::spawn(reload_config(state.clone()));

        *state.http_handle.lock().await = Some(http_handle);
        *state.https_handle.lock().await = Some(https_handle);
        *state.quic_handle.lock().await = Some(quic_handle);

        let console_handle = tokio::spawn(run_console(state.clone()));

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        tokio::signal::ctrl_c().await?;
        tracing::info!("Получен сигнал Ctrl+C, завершаем работу сервера...");

        tracing::info!("Завершаем все задачи...");

        if let Some(handle) = state.http_handle.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = state.https_handle.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = state.quic_handle.lock().await.take() {
            handle.abort();
        }
        reload_handle.abort();
        console_handle.abort();

        tracing::info!("Сервер полностью завершён");

        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    Ok(())
}
