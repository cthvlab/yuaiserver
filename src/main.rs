use hyper::{Body, Request, Response, Server, header, service::{make_service_fn, service_fn}, StatusCode}; // Библиотеки для HTTP/HTTPS сервера
use rustls::pki_types::{CertificateDer, PrivateKeyDer}; // Типы данных для работы с сертификатами
use rustls::ServerConfig; // Конфигурация для защищённых соединений
use dashmap::DashMap; // Быстрая структура данных для многопоточной работы (как словарь)
use std::collections::HashMap; // Обычный словарь
use std::convert::Infallible; // Тип для обработки ошибок, которые не должны случаться
use std::net::{IpAddr, SocketAddr}; // Типы для работы с адресами в сети
use std::sync::Arc; // Инструмент для безопасного использования данных в разных потоках
use std::time::{Duration, Instant}; // Для работы со временем
use std::fs::File; // Для работы с файлами
use std::io::BufReader; // Помогает читать файлы
use tokio::sync::{Mutex as TokioMutex, RwLock}; // Замки для безопасной работы с данными в асинхронном коде
use tokio::net::TcpListener; // Слушатель для TCP соединений
use tokio::time::sleep; // Асинхронный "сон" (ждать какое-то время)
use tokio_tungstenite::WebSocketStream; // Для работы с WebSocket
use tungstenite::protocol::Role; // Роли для WebSocket (сервер или клиент)
use tracing::{error, info, warn}; // Логирование (запись сообщений о работе программы)
use tracing_subscriber; // Настройка логирования
use jsonwebtoken::{decode, DecodingKey, Validation}; // Проверка JWT токенов
use serde::Deserialize; // Преобразование текста (TOML) в структуры Rust
use base64::engine::general_purpose; // Кодирование и декодирование Base64
use base64::Engine; // Движок для Base64
use quinn::{Endpoint, ServerConfig as QuinnServerConfig, Connection}; // QUIC протокол
use futures::SinkExt; // Для отправки данных в потоках
use rustls_pemfile::{certs, pkcs8_private_keys}; // Чтение сертификатов и ключей из файлов
use webrtc::api::APIBuilder; // WebRTC для видеосвязи и данных
use webrtc::peer_connection::RTCPeerConnection; // Соединение WebRTC
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription; // Описание сессии WebRTC
use webrtc::ice_transport::ice_server::RTCIceServer; // ICE серверы для WebRTC
use tokio_rustls::TlsAcceptor; // Приём TLS соединений
use hyper::upgrade::Upgraded; // Обновление соединения для WebSocket

// Константа для типа ответа (текст в формате UTF-8)
const CONTENT_TYPE_UTF8: &str = "text/plain; charset=utf-8";

// Структура для одной локации
#[derive(Deserialize, Clone, PartialEq, Debug)]
struct Location {
    path: String,      // Путь, например, "/"
    response: String,  // Ответ для этого пути
}

// Структура для хранения настроек сервера из config.toml
#[derive(Deserialize, Clone, PartialEq)]
struct Config {
    http_port: u16,             // Порт для HTTP
    https_port: u16,            // Порт для HTTPS
    quic_port: u16,             // Порт для QUIC
    cert_path: String,          // Путь к сертификату
    key_path: String,           // Путь к ключу
    jwt_secret: String,         // Секрет для JWT
    basic_auth_login: String,   // Логин для авторизации
    basic_auth_password: String,// Пароль для авторизации
    worker_threads: usize,      // Количество потоков для обработки задач
    locations: Vec<Location>,   // Список локаций
}

// Структура для хранения записи в кэше (ответа сервера)
#[derive(Clone)]
struct CacheEntry {
    response_body: Vec<u8>, // Сами данные ответа
    expiry: Instant,        // Время, когда запись устареет
}

// Структура для кэширования авторизации
#[derive(Clone)]
struct AuthCacheEntry {
    is_valid: bool,   // Прошёл ли пользователь проверку
    expiry: Instant,  // Время, когда запись устареет
}

// Главная структура, которая хранит состояние сервера
struct ProxyState {
    cache: DashMap<String, CacheEntry>,                // Кэш для быстрых ответов
    auth_cache: DashMap<String, AuthCacheEntry>,       // Кэш авторизаций
    whitelist: DashMap<String, ()>,                    // Белый список IP
    blacklist: DashMap<String, ()>,                    // Чёрный список IP
    sessions: DashMap<String, Instant>,                // Сессии пользователей
    auth_attempts: DashMap<String, (u32, Instant)>,    // Попытки авторизации
    rate_limits: DashMap<String, (Instant, u32)>,      // Лимиты запросов
    config: TokioMutex<Config>,                        // Настройки сервера (с замком для безопасности)
    webrtc_peers: DashMap<String, Arc<RTCPeerConnection>>, // WebRTC соединения
    locations: Arc<RwLock<Vec<Location>>>,             // Динамически обновляемый список локаций
}

// Функция для загрузки настроек из файла config.toml
async fn load_config() -> Result<Config, String> {
    let content = tokio::fs::read_to_string("config.toml").await // Читаем содержимое файла напрямую по пути
        .map_err(|e| format!("Ошибка чтения config.toml: {}", e))?;
    toml::from_str(&content) // Преобразуем текст в структуру Config
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}

// Проверяем, что настройки корректны
fn validate_config(config: &Config) -> Result<(), String> {
    // Проверяем, что порты не совпадают
    if config.http_port == config.https_port || config.http_port == config.quic_port || config.https_port == config.quic_port {
        return Err(format!("Конфликт портов: HTTP={}, HTTPS={}, QUIC={}", config.http_port, config.https_port, config.quic_port));
    }
    // Проверяем, что файлы сертификата и ключа существуют
    if !std::path::Path::new(&config.cert_path).exists() || !std::path::Path::new(&config.key_path).exists() {
        return Err("Сертификат или ключ не найден".to_string());
    }
    // Проверяем, что количество потоков в пределах нормы (от 1 до 1024)
    if config.worker_threads == 0 || config.worker_threads > 1024 {
        return Err("Недопустимое количество потоков: должно быть от 1 до 1024".to_string());
    }
    // Проверяем TLS настройки
    load_tls_config(config).map_err(|e| format!("Ошибка TLS конфигурации: {}", e))?;
    Ok(())
}

// Загружаем настройки для защищённых соединений (TLS)
fn load_tls_config(config: &Config) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let certs = certs(&mut BufReader::new(File::open(&config.cert_path)?))?.into_iter().map(CertificateDer::from).collect(); // Читаем сертификаты
    let key = pkcs8_private_keys(&mut BufReader::new(File::open(&config.key_path)?))?.into_iter().next().ok_or("Приватный ключ не найден")?; // Читаем ключ
    let mut cfg = ServerConfig::builder().with_no_client_auth().with_single_cert(certs, PrivateKeyDer::Pkcs8(key.into()))?; // Создаём конфигурацию TLS
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()]; // Поддерживаем HTTP/2 и HTTP/1.1
    Ok(cfg)
}

// Настраиваем QUIC соединения
fn load_quinn_config(config: &Config) -> QuinnServerConfig {
    let certs = certs(&mut BufReader::new(File::open(&config.cert_path).unwrap())).unwrap().into_iter().map(CertificateDer::from).collect();
    let key = pkcs8_private_keys(&mut BufReader::new(File::open(&config.key_path).unwrap())).unwrap().into_iter().next().unwrap();
    QuinnServerConfig::with_single_cert(certs, PrivateKeyDer::Pkcs8(key.into())).unwrap()
}

// Проверяем авторизацию пользователя
async fn check_auth(req: &Request<Body>, state: &ProxyState, ip: &str) -> Result<(), Response<Body>> {
    // Сначала проверяем, есть ли данные в кэше авторизации
    if let Some(entry) = state.auth_cache.get(ip) {
        if entry.expiry > Instant::now() { // Если кэш ещё действителен
            if entry.is_valid {
                return Ok(()); // Пользователь уже проверен, всё ок
            } else {
                return Err(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Неавторизован")).unwrap());
            }
        }
    }

    let config = state.config.lock().await; // Блокируем конфигурацию для чтения
    let auth_header = req.headers().get("Authorization").and_then(|h| h.to_str().ok()); // Получаем заголовок авторизации

    // Проверяем базовую авторизацию (Basic Auth)
    let is_basic_auth = auth_header.map(|auth| {
        auth.starts_with("Basic ") && general_purpose::STANDARD.decode(auth.trim_start_matches("Basic ")).ok()
            .and_then(|d| String::from_utf8(d).ok())
            .map(|cred| cred == format!("{}:{}", config.basic_auth_login, config.basic_auth_password))
            .unwrap_or(false)
    }).unwrap_or(false);

    // Проверяем JWT токен
    let is_jwt_auth = auth_header.map(|auth| {
        auth.starts_with("Bearer ") && decode::<HashMap<String, String>>(auth.trim_start_matches("Bearer "), &DecodingKey::from_secret(config.jwt_secret.as_ref()), &Validation::default()).is_ok()
    }).unwrap_or(false);

    // Если ни одна проверка не прошла
    if !is_basic_auth && !is_jwt_auth {
        let now = Instant::now();
        let mut entry = state.auth_attempts.entry(ip.to_string()).or_insert((0, now)); // Записываем попытку авторизации
        if now.duration_since(entry.1) < Duration::from_secs(60) { // Если в течение минуты
            entry.0 += 1; // Увеличиваем счётчик попыток
            if entry.0 >= 5 { // Если слишком много попыток
                state.blacklist.insert(ip.to_string(), ()); // Добавляем в чёрный список
                state.auth_attempts.remove(ip);
                warn!("IP {} добавлен в чёрный список", ip);
                state.auth_cache.insert(ip.to_string(), AuthCacheEntry { is_valid: false, expiry: Instant::now() + Duration::from_secs(60) });
                return Err(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Доступ запрещён")).unwrap());
            }
        } else {
            *entry = (1, now); // Сбрасываем счётчик
        }
        state.auth_cache.insert(ip.to_string(), AuthCacheEntry { is_valid: false, expiry: Instant::now() + Duration::from_secs(60) });
        return Err(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Неавторизован")).unwrap());
    }
    // Если авторизация прошла, кэшируем результат
    state.auth_cache.insert(ip.to_string(), AuthCacheEntry { is_valid: true, expiry: Instant::now() + Duration::from_secs(60) });
    Ok(())
}

// Получаем IP клиента из заголовков или соединения
fn get_client_ip(req: &Request<Body>, client_ip: Option<IpAddr>) -> Option<String> {
    req.headers().get("X-Forwarded-For").and_then(|v| v.to_str().ok()).and_then(|s| s.split(',').next()).map(|s| s.trim().to_string()).or_else(|| client_ip.map(|ip| ip.to_string()))
}

// Обрабатываем WebSocket соединение
async fn handle_websocket(upgraded: Upgraded) {
    let mut ws = WebSocketStream::from_raw_socket(upgraded, Role::Server, None).await; // Преобразуем соединение в WebSocket
    while let Some(msg) = tokio_stream::StreamExt::next(&mut ws).await { // Ждём сообщений
        if let Ok(msg) = msg {
            if msg.is_text() || msg.is_binary() { // Если это текст или данные
                ws.send(msg).await.unwrap_or_else(|e| error!("Ошибка WebSocket: {}", e)); // Отправляем обратно
            }
        }
    }
}

// Управляем сессией пользователя
async fn manage_session(state: &ProxyState, ip: &str) {
    state.sessions.insert(ip.to_string(), Instant::now()); // Записываем время начала сессии
}

// Проверяем лимит запросов
async fn check_rate_limit(state: &ProxyState, ip: &str, max_requests: u32) -> bool {
    let now = Instant::now();
    let mut entry = state.rate_limits.entry(ip.to_string()).or_insert((now, 0)); // Получаем или создаём запись
    if now.duration_since(entry.0) > Duration::from_secs(60) { // Если прошла минута
        *entry = (now, 1); // Сбрасываем счётчик
        true
    } else if entry.1 < max_requests { // Если лимит не превышен
        entry.1 += 1; // Увеличиваем счётчик
        true
    } else {
        false // Лимит превышен
    }
}

// Обрабатываем WebRTC запрос
async fn handle_webrtc_offer(offer_sdp: String, state: Arc<ProxyState>, client_ip: String) -> Result<String, String> {
    let api = APIBuilder::new().build(); // Создаём API для WebRTC
    let config = webrtc::peer_connection::configuration::RTCConfiguration {
        ice_servers: vec![RTCIceServer { // Добавляем STUN сервер Google
            urls: vec!["stun:stun.l.google.com:19302".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    };
    let peer_connection = Arc::new(api.new_peer_connection(config).await.map_err(|e| e.to_string())?); // Создаём соединение
    let data_channel = peer_connection.create_data_channel("proxy", None).await.map_err(|e| e.to_string())?; // Создаём канал данных
    let offer = RTCSessionDescription::offer(offer_sdp).map_err(|e| format!("Ошибка SDP: {}", e))?; // Парсим запрос клиента
    peer_connection.set_remote_description(offer).await.map_err(|e| e.to_string())?; // Устанавливаем описание
    let answer = peer_connection.create_answer(None).await.map_err(|e| e.to_string())?; // Создаём ответ
    peer_connection.set_local_description(answer.clone()).await.map_err(|e| e.to_string())?; // Устанавливаем локальное описание
    state.webrtc_peers.insert(client_ip.clone(), peer_connection.clone()); // Сохраняем соединение
    data_channel.on_message(Box::new(move |msg| { // Обрабатываем сообщения
        info!("WebRTC данные от {}: {:?}", client_ip, msg.data.to_vec());
        Box::pin(async move {})
    }));
    Ok(answer.sdp) // Возвращаем SDP ответ
}

// Обрабатываем HTTP запросы (перенаправляем на HTTPS)
async fn handle_http_request(req: Request<Body>, https_port: u16) -> Result<Response<Body>, Infallible> {
    let redirect_url = format!("https://{}:{}{}", req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1"), https_port, req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or(""));
    Ok(Response::builder().status(StatusCode::MOVED_PERMANENTLY).header(header::LOCATION, redirect_url).body(Body::empty()).unwrap())
}

// Обрабатываем HTTPS запросы
async fn handle_https_request(req: Request<Body>, state: Arc<ProxyState>, client_ip: Option<IpAddr>) -> Result<Response<Body>, Infallible> {
    let ip = match get_client_ip(&req, client_ip) { // Получаем IP клиента
        Some(ip) => ip,
        None => return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("IP не определён")).unwrap()),
    };

    if state.blacklist.contains_key(&ip) { // Проверяем чёрный список
        return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Доступ запрещён")).unwrap());
    }

    if let Err(resp) = check_auth(&req, &state, &ip).await { // Проверяем авторизацию
        return Ok(resp);
    }

    manage_session(&state, &ip).await; // Управляем сессией

    let max_requests = if state.whitelist.contains_key(&ip) { 100 } else { 10 }; // Устанавливаем лимит запросов
    if !check_rate_limit(&state, &ip, max_requests).await { // Проверяем лимит
        return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Превышен лимит запросов")).unwrap());
    }

    // Если запрос на WebSocket
    if req.headers().get("Upgrade").map(|v| v == "websocket").unwrap_or(false) {
        if let Ok(upgraded) = hyper::upgrade::on(req).await {
            tokio::spawn(handle_websocket(upgraded));
            return Ok(Response::new(Body::empty()));
        }
        return Ok(Response::builder().status(500).header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from("Ошибка WebSocket")).unwrap());
    }

    // Если запрос на WebRTC
    if req.uri().path() == "/webrtc/offer" {
        let offer_sdp = String::from_utf8_lossy(&hyper::body::to_bytes(req.into_body()).await.unwrap()).to_string();
        match handle_webrtc_offer(offer_sdp, state.clone(), ip).await {
            Ok(answer_sdp) => return Ok(Response::new(Body::from(answer_sdp))),
            Err(e) => return Ok(Response::builder().status(500).header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from(format!("Ошибка WebRTC: {}", e))).unwrap()),
        }
    }

    // Проверяем кэш для обычного запроса
    let url = req.uri().to_string();
    if let Some(entry) = state.cache.get(&url) {
        if entry.expiry > Instant::now() {
            return Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from(entry.response_body.clone())).unwrap());
        }
    }

    // Обрабатываем локации
    let locations = state.locations.read().await; // Читаем текущие локации
    let path = req.uri().path();
    let response_body = locations.iter()
        .find(|loc| path.starts_with(&loc.path)) // Находим первую подходящую локацию
        .map(|loc| loc.response.as_bytes().to_vec()) // Берём её ответ
        .unwrap_or_else(|| "404 Not Found".as_bytes().to_vec()); // Если нет совпадений — 404

    // Кэшируем ответ
    state.cache.insert(url, CacheEntry { response_body: response_body.clone(), expiry: Instant::now() + Duration::from_secs(60) });
    Ok(Response::builder().header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8).body(Body::from(response_body)).unwrap())
}

// Перезагружаем конфигурацию каждые 60 секунд
async fn reload_config(state: Arc<ProxyState>) {  // Убираем tx, так как не перезапускаем сервер
    let mut current_config = state.config.lock().await.clone();
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await; // Ждём минуту
        if let Ok(new_config) = load_config().await {
            if current_config != new_config && validate_config(&new_config).is_ok() { // Если конфигурация изменилась
                info!("Обновление конфигурации: HTTP={}, HTTPS={}, QUIC={}", 
                      new_config.http_port, new_config.https_port, new_config.quic_port);
                *state.config.lock().await = new_config.clone();
                // Обновляем локации
                let mut locations = state.locations.write().await;
                *locations = new_config.locations.clone();
                info!("Локации обновлены: {:?}", locations);
                current_config = new_config;
            }
        }
    }
}

// Обрабатываем QUIC соединение
async fn handle_quic_connection(connection: Connection) {
    info!("QUIC соединение от {:?}", connection.remote_address());
    loop {
        if let Ok((mut send, mut recv)) = connection.accept_bi().await { // Принимаем двусторонний поток
            tokio::spawn(async move {
                let mut buffer = vec![0; 4096]; // Буфер для чтения данных
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

// Запускаем сервер с обработкой запросов
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

// Запускаем все серверы
async fn start_servers(state: Arc<ProxyState>, config: Config, http_tx: &mut tokio::sync::mpsc::Sender<()>, https_tx: &mut tokio::sync::mpsc::Sender<()>, quic_tx: &mut tokio::sync::mpsc::Sender<()>) {
    let (http_shutdown_tx, http_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1); // Канал для остановки HTTP
    let (https_shutdown_tx, https_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1); // Канал для остановки HTTPS
    let (quic_shutdown_tx, quic_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1); // Канал для остановки QUIC

    tokio::spawn(run_http_server(config.clone(), http_shutdown_rx));
    tokio::spawn(run_https_server(state.clone(), config.clone(), https_shutdown_rx));
    tokio::spawn(run_quic_server(config.clone(), quic_shutdown_rx));

    *http_tx = http_shutdown_tx;
    *https_tx = https_shutdown_tx;
    *quic_tx = quic_shutdown_tx;
}

// HTTP сервер
async fn run_http_server(config: Config, shutdown_rx: tokio::sync::mpsc::Receiver<()>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.http_port));
    let service = move |req: Request<Body>| handle_http_request(req, config.https_port);
    run_server(addr, service, shutdown_rx).await.unwrap_or_else(|e| error!("Ошибка HTTP: {}", e));
}

// HTTPS сервер
async fn run_https_server(state: Arc<ProxyState>, config: Config, mut shutdown_rx: tokio::sync::mpsc::Receiver<()>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.https_port));
    let listener = TcpListener::bind(addr).await.unwrap(); // Слушаем входящие соединения
    info!("HTTPS сервер слушает на {}", listener.local_addr().unwrap());

    let tls_acceptor = TlsAcceptor::from(Arc::new(load_tls_config(&config).unwrap())); // Настраиваем TLS
    loop {
        tokio::select! {
            Ok((stream, client_ip)) = listener.accept() => { // Принимаем соединение
                stream.set_nodelay(true).unwrap(); // Ускоряем соединение
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

// QUIC сервер
async fn run_quic_server(config: Config, mut shutdown_rx: tokio::sync::mpsc::Receiver<()>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.quic_port));
    let endpoint = Endpoint::server(load_quinn_config(&config), addr).unwrap(); // Запускаем QUIC
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

// Главная функция программы
fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider().install_default().expect("Ошибка CryptoProvider"); // Настраиваем шифрование
    tracing_subscriber::fmt::init(); // Включаем логирование

    // Загружаем начальную конфигурацию
    let initial_config = tokio::runtime::Runtime::new().unwrap().block_on(async {
        load_config().await.and_then(|cfg| validate_config(&cfg).map(|_| cfg)).map_err(|e| {
            error!("Ошибка начальной конфигурации: {}", e);
            "Некорректная конфигурация"
        })
    })?;

    // Создаём runtime с количеством потоков из конфигурации
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(initial_config.worker_threads) // Устанавливаем количество потоков
        .enable_all() // Включаем все возможности Tokio
        .build()
        .unwrap();

    // Запускаем сервер в нашем runtime
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
            locations: Arc::new(RwLock::new(initial_config.locations.clone())), // Инициализируем локации
        });

		let (_, mut config_rx) = tokio::sync::mpsc::channel::<Config>(10); // Игнорируем Sender, берём только Receiver
		let (mut http_shutdown_tx, http_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
		let (mut https_shutdown_tx, https_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);
		let (mut quic_shutdown_tx, quic_shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Запускаем все серверы
        tokio::spawn(run_http_server(initial_config.clone(), http_shutdown_rx));
        tokio::spawn(run_https_server(state.clone(), initial_config.clone(), https_shutdown_rx));
        tokio::spawn(run_quic_server(initial_config, quic_shutdown_rx));
        tokio::spawn(reload_config(state.clone())); // Передаём только state, без tx

        // Обрабатываем обновления конфигурации (оставляем для совместимости, но теперь не обязательно)
        while let Some(new_config) = config_rx.recv().await {
            let _ = http_shutdown_tx.send(()).await;
            let _ = https_shutdown_tx.send(()).await;
            let _ = quic_shutdown_tx.send(()).await;
            sleep(Duration::from_secs(1)).await;
            start_servers(state.clone(), new_config, &mut http_shutdown_tx, &mut https_shutdown_tx, &mut quic_shutdown_tx).await;
        }

        tokio::signal::ctrl_c().await?; // Ждём Ctrl+C для завершения
        info!("Сервер завершил работу");
        Ok::<(), Box<dyn std::error::Error>>(()) // Явно указываем тип ошибки
    })?;

    Ok(())
}
