use hyper::{Body, Request, Response, Server, header, service::{make_service_fn, service_fn}, StatusCode};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::ServerConfig;
use dashmap::DashMap;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::fs::File;
use std::io::{BufReader as StdBufReader};
use tokio::sync::{Mutex as TokioMutex, RwLock};
use tokio::net::TcpListener;
use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Role;
use tracing::{error, info, warn};
use tracing_subscriber;
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use quinn::{Endpoint, ServerConfig as QuinnServerConfig, Connection};
use futures::SinkExt;
use rustls_pemfile::{certs, pkcs8_private_keys};
use webrtc::api::APIBuilder;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::ice_transport::ice_server::RTCIceServer;
use tokio_rustls::TlsAcceptor;
use hyper::upgrade::Upgraded;
use tokio::task::JoinHandle;
use tokio::io::{self as aio, AsyncBufReadExt, BufReader as AsyncBufReader, AsyncWriteExt};

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
    Ok(())
}

fn load_tls_config(config: &Config) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let certs = certs(&mut StdBufReader::new(File::open(&config.cert_path)?))?.into_iter().map(CertificateDer::from).collect();
    let key = pkcs8_private_keys(&mut StdBufReader::new(File::open(&config.key_path)?))?.into_iter().next().ok_or("Приватный ключ не найден")?;
    let mut cfg = ServerConfig::builder().with_no_client_auth().with_single_cert(certs, PrivateKeyDer::Pkcs8(key.into()))?;
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(cfg)
}

fn load_quinn_config(config: &Config) -> QuinnServerConfig {
    let certs = certs(&mut StdBufReader::new(File::open(&config.cert_path).unwrap())).unwrap().into_iter().map(CertificateDer::from).collect();
    let key = pkcs8_private_keys(&mut StdBufReader::new(File::open(&config.key_path).unwrap())).unwrap().into_iter().next().unwrap();
    QuinnServerConfig::with_single_cert(certs, PrivateKeyDer::Pkcs8(key.into())).unwrap()
}

async fn check_auth(req: &Request<Body>, state: &ProxyState, ip: &str) -> Result<(), Response<Body>> {
    if let Some(entry) = state.auth_cache.get(ip) {
        if entry.expiry > Instant::now() {
            if entry.is_valid {
                return Ok(());
            } else {
                return Err(Response::builder()
                    .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                    .body(Body::from("Неавторизован"))
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
                    .body(Body::from("Доступ запрещён"))
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
            .body(Body::from("Неавторизован"))
            .unwrap());
    }
    
    state.auth_cache.insert(ip.to_string(), AuthCacheEntry { 
        is_valid: true, 
        expiry: Instant::now() + Duration::from_secs(60) 
    });
    Ok(())
}

fn get_client_ip(req: &Request<Body>, client_ip: Option<IpAddr>) -> Option<String> {
    req.headers().get("X-Forwarded-For").and_then(|v| v.to_str().ok()).and_then(|s| s.split(',').next()).map(|s| s.trim().to_string()).or_else(|| client_ip.map(|ip| ip.to_string()))
}

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
    let config = webrtc::peer_connection::configuration::RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }],
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

async fn handle_http_request(req: Request<Body>, https_port: u16) -> Result<Response<Body>, Infallible> {
    let redirect_url = format!("https://{}:{}{}", req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1"), https_port, req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or(""));
    Ok(Response::builder().status(StatusCode::MOVED_PERMANENTLY).header(header::LOCATION, redirect_url).body(Body::empty()).unwrap())
}

async fn handle_https_request(req: Request<Body>, state: Arc<ProxyState>, client_ip: Option<IpAddr>) -> Result<Response<Body>, Infallible> {
    let ip = match get_client_ip(&req, client_ip) {
        Some(ip) => ip,
        None => return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("IP не определён")).unwrap()),
    };

    if state.blacklist.contains_key(&ip) {
        return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Доступ запрещён")).unwrap());
    }

    if let Err(resp) = check_auth(&req, &state, &ip).await {
        return Ok(resp);
    }

    manage_session(&state, &ip).await;

    let max_requests = if state.whitelist.contains_key(&ip) { 100 } else { 10 };
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

    let locations = state.locations.read().await;
    let path = req.uri().path();
    let response_body = locations.iter()
        .find(|loc| path.starts_with(&loc.path))
        .map(|loc| loc.response.as_bytes().to_vec())
        .unwrap_or_else(|| "404 Not Found".as_bytes().to_vec());

    state.cache.insert(url, CacheEntry { response_body: response_body.clone(), expiry: Instant::now() + Duration::from_secs(60) });
    Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from(response_body)).unwrap())
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
    match Server::try_bind(&addr) {
        Ok(builder) => {
            let service = move |req: Request<Body>| handle_http_request(req, config.https_port);
            let server = builder.serve(make_service_fn(move |_| {
                let service = service.clone();
                async move { Ok::<_, Infallible>(service_fn(service)) }
            }));
            info!("HTTP сервер слушает на {}", addr);
            *state.http_running.write().await = true;
            if let Err(e) = server.await {
                error!("Ошибка HTTP: {}", e);
                *state.http_running.write().await = false;
            }
        }
        Err(e) => {
            error!("Не удалось запустить HTTP сервер на порту {}: {}. Измените http_port в config.toml.", config.http_port, e);
            *state.http_running.write().await = false;
        }
    }
}

async fn run_https_server(state: Arc<ProxyState>, config: Config) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.https_port));
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            info!("HTTPS сервер слушает на {}", listener.local_addr().unwrap());
            *state.https_running.write().await = true;
            let tls_acceptor = TlsAcceptor::from(Arc::new(load_tls_config(&config).unwrap()));
            loop {
                if let Ok((stream, client_ip)) = listener.accept().await {
                    stream.set_nodelay(true).unwrap();
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
            }
        }
        Err(e) => {
            error!("Не удалось запустить HTTPS сервер на порту {}: {}. Измените https_port в config.toml.", config.https_port, e);
            *state.https_running.write().await = false;
        }
    }
}

async fn run_quic_server(config: Config, state: Arc<ProxyState>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.quic_port));
    match Endpoint::server(load_quinn_config(&config), addr) {
        Ok(endpoint) => {
            info!("QUIC сервер слушает на {}", endpoint.local_addr().unwrap());
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

async fn print_help(stdout: &mut tokio::io::Stdout) {
    stdout.write_all(
        "\n=== Список команд ===\n\
         1 - Показать статус\n\
         2 - Перезагрузить конфигурацию\n\
         3 - Добавить локацию\n\
         4 - Показать активные сессии\n\
         5 - Выход\n\
         help - Показать этот список\n".as_bytes()
    ).await.unwrap();
    stdout.flush().await.unwrap();
}

async fn prompt_new_location(stdout: &mut tokio::io::Stdout, reader: &mut AsyncBufReader<tokio::io::Stdin>) -> Option<(String, String)> {
    let mut path = String::new();
    let mut response = String::new();

    stdout.write_all("\nВведите путь (например, /test): ".as_bytes()).await.unwrap();
    stdout.flush().await.unwrap();
    if reader.read_line(&mut path).await.unwrap_or(0) == 0 {
        return None;
    }

    stdout.write_all("Введите ответ: ".as_bytes()).await.unwrap();
    stdout.flush().await.unwrap();
    if reader.read_line(&mut response).await.unwrap_or(0) == 0 {
        return None;
    }

    let path = path.trim().to_string();
    let response = response.trim().to_string();
    if path.is_empty() || response.is_empty() {
        None
    } else {
        Some((path, response))
    }
}

async fn run_console(state: Arc<ProxyState>) {
    let stdin = aio::stdin();
    let mut reader = AsyncBufReader::new(stdin);
    let mut stdout = aio::stdout();

    stdout.write_all(
        format!(
            "\nСтатус сервера:\nСессий: {}\nРазмер кэша: {}\nПопыток авторизации: {}\n",
            state.sessions.len(),
            state.cache.len(),
            state.auth_attempts.len()
        ).as_bytes()
    ).await.unwrap();
    print_help(&mut stdout).await;
    stdout.write_all(b"\n> ").await.unwrap();
    stdout.flush().await.unwrap();

    let mut input = String::new();
    loop {
        input.clear();

        if reader.read_line(&mut input).await.unwrap_or(0) == 0 {
            stdout.write_all("\nЗавершение консоли...\n".as_bytes()).await.unwrap();
            break;
        }

        let command = input.trim();

        match command {
            "1" => {
                stdout.write_all(
                    format!(
                        "\nСтатус сервера:\nСессий: {}\nРазмер кэша: {}\nПопыток авторизации: {}\nHTTP: {}\nHTTPS: {}\nQUIC: {}\n",
                        state.sessions.len(),
                        state.cache.len(),
                        state.auth_attempts.len(),
                        if *state.http_running.read().await { "работает" } else { "не работает" },
                        if *state.https_running.read().await { "работает" } else { "не работает" },
                        if *state.quic_running.read().await { "работает" } else { "не работает" }
                    ).as_bytes()
                ).await.unwrap();
            }
            "2" => {
                match load_config().await {
                    Ok(new_config) if validate_config(&new_config).is_ok() => {
                        *state.config.lock().await = new_config.clone();
                        *state.locations.write().await = new_config.locations.clone();
                        stdout.write_all("\nКонфигурация перезагружена\n".as_bytes()).await.unwrap();
                    }
                    _ => {
                        stdout.write_all("\nОшибка валидации конфигурации\n".as_bytes()).await.unwrap();
                    }
                }
            }
            "3" => {
                if let Some((path, response)) = prompt_new_location(&mut stdout, &mut reader).await {
                    state.locations.write().await.push(Location { path, response });
                    stdout.write_all("\nЛокация добавлена\n".as_bytes()).await.unwrap();
                }
            }
            "4" => {
                let sessions: Vec<_> = state.sessions.iter().map(|e| e.key().clone()).collect();
                stdout.write_all(format!("\nАктивные сессии: {:?}\n", sessions).as_bytes()).await.unwrap();
            }
            "5" => {
                stdout.write_all("\nЗавершение консоли...\n".as_bytes()).await.unwrap();
                break;
            }
            "help" => {
                print_help(&mut stdout).await;
            }
            "" => {}
            _ => {
                stdout.write_all("\nНеверная команда, введите 'help' для списка команд\n".as_bytes()).await.unwrap();
            }
        }

        stdout.write_all("\nВведите команду: ".as_bytes()).await.unwrap();
        stdout.flush().await.unwrap();
    }

    stdout.flush().await.unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider().install_default().expect("Ошибка CryptoProvider");
    tracing_subscriber::fmt::init();

    let initial_config = tokio::runtime::Runtime::new().unwrap().block_on(async {
        load_config().await.and_then(|cfg| validate_config(&cfg).map(|_| cfg)).map_err(|e| {
            error!("Ошибка начальной конфигурации: {}", e);
            "Некорректная конфигурация"
        })
    })?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
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

        tokio::time::sleep(Duration::from_millis(100)).await;

        run_console(state.clone()).await;

        tokio::signal::ctrl_c().await?;
        info!("Сервер завершил работу");

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

        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    Ok(())
}
