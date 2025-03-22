// АРРР! Это наш пиратский ящик с инструментами для космопорта!

use hyper::{service::service_fn, Request, Response, StatusCode, Error as HttpError}; // Двигатель для HTTP: запросы, ответы и ошибки
use hyper_util::{rt::{TokioExecutor, TokioIo}, server::conn::auto::Builder as AutoBuilder}; // Помощники для двигателя: юнги и провода
use hyper::header; // Заголовки — как печати на письмах
use hyper_tungstenite::{HyperWebsocket, upgrade}; // Телепорт WebSocket для быстрых прыжков
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer}; // Шифры: пропуска и ключи
use rustls::ServerConfig; // Настройки шифров для безопасных полетов
use dashmap::DashMap; // Быстрый сундук для хранения добычи
use std::collections::HashMap; // Обычный сундук для списков
use std::convert::Infallible; // Ошибка, которой не бывает — как честный пират!
use std::net::{IpAddr, SocketAddr}; // Координаты: адрес и порт
use std::sync::Arc; // Общий штурвал для юнг
use std::time::{Duration, Instant}; // Часы: сколько ждать и когда началось
use std::fs::File; // Файлы — как карты в сундуке
use std::io::BufReader as StdBufReader; // Юнга, читающий карты
use tokio::sync::{Mutex as TokioMutex, RwLock}; // Замки: один пишет, все читают
use tokio::net::{TcpListener}; // Ухо капитана для TCP-сигналов
use tokio_rustls::TlsAcceptor; // Приемник шифров для TLS
use tokio::task::JoinHandle; // Задачи для юнг в фоне
use tokio::runtime::Builder; // Строитель корабля для команды
use tracing::{error, info, warn}; // Рация: кричать о бедах и победах
use jsonwebtoken::{decode, DecodingKey, Validation}; // Проверка пропусков JWT
use serde::Deserialize; // Чтение карт из текста
use quinn::{Endpoint, ServerConfig as QuinnServerConfig, Connection}; // QUIC: причал и связь для звездолетов
use futures::{StreamExt, SinkExt}; // Потоки: принимать и отправлять посылки
use rustls_pemfile::{certs, pkcs8_private_keys}; // Достаем шифры из файлов
use webrtc::api::APIBuilder; // Строитель телепорта WebRTC
use webrtc::peer_connection::RTCPeerConnection; // Связь для телепортации
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription; // Сигналы для WebRTC
use webrtc::ice_transport::ice_server::RTCIceServer; // Маяки для телепортации
use http_body_util::BodyExt; // Собираем добычу из запросов
use crate::console::run_console; // Консоль капитана для приказов

mod console; // Отдельный отсек для консоли

const CONTENT_TYPE_UTF8: &str = "text/plain; charset=utf-8"; // Метка для текстовых посылок

// АРРР! Это как табличка на звездной карте: куда лететь и что отдавать пришельцам!
#[derive(Deserialize, Clone, PartialEq, Debug)]
struct Location {
    path: String,     // Путь — это как "поворот налево у третьей звезды"
    response: String, // Ответ — это сокровище, что мы даем гостям
}

// Это наш главный звездный атлас, где все настройки космопорта!
#[derive(Deserialize, Clone, PartialEq)]
struct Config {
    http_port: u16,        // Порт для старых шлюпок (HTTP), как номер причала
    https_port: u16,       // Порт для бронированных крейсеров (HTTPS)
    quic_port: u16,        // Порт для гиперскоростных звездолетов (QUIC)
    cert_path: String,     // Где лежит сундук с сертификатами (как пароль для входа)
    key_path: String,      // Где лежит ключ от сундука
    jwt_secret: String,    // Тайный пиратский код для проверки пропусков
    worker_threads: usize, // Сколько энергодвигателей задействуем для ускорения
    locations: Vec<Location>, // Список всех маршрутов для пришельцев
    ice_servers: Option<Vec<String>>, // Маяки для телепортации (WebRTC), как звездные фонари
}

// Это как сундук с добычей, которую мы уже нашли и храним!
#[derive(Clone)]
struct CacheEntry {
    response_body: Vec<u8>, // Добыча в виде байтов (цифровое золото!)
    expiry: Instant,        // Когда добыча "протухнет", как старый ром
}

// Это журнал пропусков: кто может войти, а кто нет!
#[derive(Clone)]
struct AuthCacheEntry {
    is_valid: bool,   // Пропуск настоящий или фальшивый?
    expiry: Instant,  // Когда пропуск сгорит, как бумага в костре
}

// Главный штурвал космопорта! Тут все, что нужно для управления!
struct ProxyState {
    cache: DashMap<String, CacheEntry>,          // Склад с добычей (кэш)
    auth_cache: DashMap<String, AuthCacheEntry>, // Журнал пропусков
    whitelist: DashMap<String, ()>,              // Список друзей капитана
    blacklist: DashMap<String, ()>,              // Список врагов и шпионов
    sessions: DashMap<String, Instant>,          // Кто сейчас в порту (сессии)
    auth_attempts: DashMap<String, (u32, Instant)>, // Сколько раз ломились без пропуска
    rate_limits: DashMap<String, (Instant, u32)>, // Лимит запросов, чтобы трюм не лопнул
    config: TokioMutex<Config>,                  // Звездный атлас под замком
    webrtc_peers: DashMap<String, Arc<RTCPeerConnection>>, // Телепортационные связи с гостями
    locations: Arc<RwLock<Vec<Location>>>,       // Карта маршрутов, где что искать
    http_running: Arc<RwLock<bool>>,             // Порт HTTP работает или спит?
    https_running: Arc<RwLock<bool>>,            // Порт HTTPS на ходу?
    quic_running: Arc<RwLock<bool>>,             // Порт QUIC открыт?
    http_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>, // Рычаг для управления HTTP
    https_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>, // Рычаг для HTTPS
    quic_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>, // Рычаг для QUIC
}

// АРРР! Читаем карту сокровищ (config.toml), чтобы знать, куда лететь!
async fn load_config() -> Result<Config, String> {
    // Пробуем открыть сундук с картой
    let content = tokio::fs::read_to_string("config.toml").await
        .map_err(|e| format!("Йо-хо-хо, карта пропала: {}", e))?;
    // Превращаем карту из текста в понятные нам указания
    toml::from_str(&content)
        .map_err(|e| format!("Шторм и гром, карта порвана: {}", e))
}

// Проверяем, не кривая ли наша карта, чтобы не врезаться в астероид!
fn validate_config(config: &Config) -> Result<(), String> {
    // Если порты одинаковые, это как два корабля на одном причале — катастрофа!
    if config.http_port == config.https_port || config.http_port == config.quic_port || config.https_port == config.quic_port {
        return Err(format!("Порты дерутся, капитан! HTTP={}, HTTPS={}, QUIC={}", config.http_port, config.https_port, config.quic_port));
    }
    // Проверяем, есть ли сундуки с сертификатами и ключами на месте
    if !std::path::Path::new(&config.cert_path).exists() || !std::path::Path::new(&config.key_path).exists() {
        return Err("Сундук с шифрами потерян, арр!".to_string());
    }
    // Юнги нужны, но не целая армия!
    if config.worker_threads == 0 || config.worker_threads > 1024 {
        return Err("Юнг должно быть от 1 до 1024, иначе бардак!".to_string());
    }
    // Проверяем шифры, чтобы нас не подслушали космические шпионы
    load_tls_config(config).map_err(|e| format!("Шифры сломаны: {}", e))?;

    // Смотрим, есть ли маяки для телепортации
    match &config.ice_servers {
        Some(servers) if servers.is_empty() => {
            return Err("Маяки пусты, как бутылка рома после пира!".to_string());
        }
        None => {
            warn!("Маяков нет, берем старый маяк STUN!");
        }
        Some(_) => {} // Все готово, юнга!
    }

    Ok(())
}

// Грузим шифры для бронированных крейсеров (TLS), чтобы летать безопасно!
fn load_tls_config(config: &Config) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    let cert_file = File::open(&config.cert_path)?; // Открываем сундук с сертификатами
    let key_file = File::open(&config.key_path)?;   // Достаем ключ от шифров

    // Загружаем сертификаты, как добычу в трюм
    let certs: Vec<CertificateDer> = certs(&mut StdBufReader::new(cert_file))?
        .into_iter()
        .map(|bytes| CertificateDer::from(bytes))
        .collect();

    // Достаем ключи, чтобы шифровать наши послания
    let keys: Vec<PrivateKeyDer> = pkcs8_private_keys(&mut StdBufReader::new(key_file))?
        .into_iter()
        .map(|bytes| PrivateKeyDer::from(PrivatePkcs8KeyDer::from(bytes)))
        .collect();
    let key = keys.into_iter().next().ok_or("Ключ пропал, как ром перед боем!")?;

    // Настраиваем шифры для безопасных полетов
    let mut cfg = ServerConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
        .with_protocol_versions(&[&rustls::version::TLS13])? // Только самые новые шифры!
        .with_no_client_auth() // Пришельцы без пропусков тоже могут зайти
        .with_single_cert(certs, key)?; // Ставим сертификат и ключ
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()]; // Говорим, какие языки понимаем
    Ok(cfg)
}

// Шифры для гиперскоростных звездолетов (QUIC), чтобы летать быстрее света!
fn load_quinn_config(config: &Config) -> Result<QuinnServerConfig, Box<dyn std::error::Error>> {
    let cert_file = File::open(&config.cert_path)?; // Сундук с сертификатами
    let key_file = File::open(&config.key_path)?;   // Ключ от шифров

    let certs: Vec<CertificateDer> = certs(&mut StdBufReader::new(cert_file))?
        .into_iter()
        .map(|bytes| CertificateDer::from(bytes))
        .collect();

    let keys: Vec<PrivateKeyDer> = pkcs8_private_keys(&mut StdBufReader::new(key_file))?
        .into_iter()
        .map(|bytes| PrivateKeyDer::from(PrivatePkcs8KeyDer::from(bytes)))
        .collect();
    let key = keys.into_iter().next().ok_or("Ключ улетел в черную дыру!")?;

    // Настраиваем QUIC-шифры
    QuinnServerConfig::with_single_cert(certs, key)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

// Проверяем, настоящий ли пропуск у пришельца!
async fn check_auth(req: &Request<hyper::body::Incoming>, state: &ProxyState, ip: &str) -> Result<(), Response<String>> {
    // Смотрим, есть ли пропуск в журнале
    if let Some(entry) = state.auth_cache.get(ip) {
        if entry.expiry > Instant::now() { // Пропуск еще свежий?
            if entry.is_valid {
                return Ok(()); // Добро пожаловать, гость!
            } else {
                return Err(Response::builder()
                    .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                    .body("Фальшивый пропуск, шельма!".to_string())
                    .unwrap());
            }
        }
    }

    let config = state.config.lock().await; // Берем звездный атлас под замок
    let auth_header = req.headers().get("Authorization").and_then(|h| h.to_str().ok()); // Ищем пропуск в запросе

    // Проверяем, настоящий ли это JWT-пропуск (секретный код)
    let is_jwt_auth = auth_header.map(|auth| {
        auth.starts_with("Bearer ") && // Пропуск должен начинаться с "Bearer "
        decode::<HashMap<String, String>>(
            auth.trim_start_matches("Bearer "), // Убираем "Bearer " и проверяем код
            &DecodingKey::from_secret(config.jwt_secret.as_ref()), // Наш тайный ключ
            &Validation::default() // Правила проверки
        ).is_ok()
    }).unwrap_or(false);

    if !is_jwt_auth { // Если пропуска нет или он фальшивый...
        let now = Instant::now();
        let mut entry = state.auth_attempts.entry(ip.to_string()).or_insert((0, now)); // Записываем попытку
        if now.duration_since(entry.1) < Duration::from_secs(60) { // Если ломились недавно...
            entry.0 += 1; // Добавляем еще одну попытку
            if entry.0 >= 5 { // 5 раз — и ты шпион!
                state.blacklist.insert(ip.to_string(), ()); // В черный список!
                state.auth_attempts.remove(ip); // Убираем из журнала попыток
                warn!("IP {} — шпион, в черный список!", ip);
                state.auth_cache.insert(ip.to_string(), AuthCacheEntry { 
                    is_valid: false, 
                    expiry: Instant::now() + Duration::from_secs(60) // Запрет на час
                });
                return Err(Response::builder()
                    .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                    .body("Ты в черном списке, убирайся!".to_string())
                    .unwrap());
            }
        } else {
            *entry = (1, now); // Начинаем считать заново
        }
        state.auth_cache.insert(ip.to_string(), AuthCacheEntry { 
            is_valid: false, 
            expiry: Instant::now() + Duration::from_secs(60) // Записываем, что пропуск фальшивый
        });
        return Err(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Покажи настоящий пропуск, юнга!".to_string())
            .unwrap());
    }
    
    // Пропуск настоящий, записываем в журнал!
    state.auth_cache.insert(ip.to_string(), AuthCacheEntry { 
        is_valid: true, 
        expiry: Instant::now() + Duration::from_secs(60) // Пропуск действует час
    });
    Ok(())
}

// Узнаем, откуда прилетел гость (IP-адрес)!
fn get_client_ip(req: &Request<hyper::body::Incoming>, client_ip: Option<IpAddr>) -> Option<String> {
    // Ищем в заголовках или берем напрямую, как координаты на карте
    req.headers().get("X-Forwarded-For").and_then(|v| v.to_str().ok()).and_then(|s| s.split(',').next()).map(|s| s.trim().to_string()).or_else(|| client_ip.map(|ip| ip.to_string()))
}

// Телепортация через WebSocket — как мгновенный прыжок между звездами!
async fn handle_websocket(websocket: HyperWebsocket) {
    let mut ws = websocket.await.unwrap(); // Открываем телепорт
    while let Some(msg) = ws.next().await { // Ждем сообщений от пришельца
        match msg {
            Ok(msg) if msg.is_text() || msg.is_binary() => { // Если это текст или данные...
                ws.send(msg).await.unwrap_or_else(|e| error!("Телепорт барахлит: {}", e)); // Отправляем обратно
            }
            Err(e) => error!("Телепорт сломался: {}", e), // Ошибка? Кричим в рацию!
            _ => {}
        }
    }
}

// Отмечаем, кто сейчас в порту, как в журнале капитана!
async fn manage_session(state: &ProxyState, ip: &str) {
    state.sessions.insert(ip.to_string(), Instant::now()); // Пишем: "Гость прилетел вот сейчас!"
}

// Проверяем, не слишком ли много запросов от одного гостя!
async fn check_rate_limit(state: &ProxyState, ip: &str, max_requests: u32) -> bool {
    let now = Instant::now(); // Текущее время
    let mut entry = state.rate_limits.entry(ip.to_string()).or_insert((now, 0)); // Берем запись о госте
    if now.duration_since(entry.0) > Duration::from_secs(60) { // Прошла минута?
        *entry = (now, 1); // Начинаем считать заново, один запрос
        true // Можно лететь!
    } else if entry.1 < max_requests { // Еще есть место в трюме?
        entry.1 += 1; // Добавляем запрос
        true // Лети дальше!
    } else {
        false // Трюм полон, жди!
    }
}

// Телепортация через WebRTC — как магия для связи между кораблями!
async fn handle_webrtc_offer(offer_sdp: String, state: Arc<ProxyState>, client_ip: String) -> Result<String, String> {
    let api = APIBuilder::new().build(); // Создаем телепортационную машину
    let config = state.config.lock().await; // Берем настройки
    let ice_servers: Vec<RTCIceServer> = config.ice_servers.clone().unwrap_or_else(|| {
        warn!("Маяков нет, берем старый STUN-маяк!");
        vec!["stun:stun.l.google.com:19302".to_string()] // Запасной маяк
    }).iter().map(|url| RTCIceServer {
        urls: vec![url.clone()], // Список маяков
        ..Default::default()
    }).collect();

    let config = webrtc::peer_connection::configuration::RTCConfiguration {
        ice_servers, // Маяки для телепортации
        ..Default::default()
    };
    let peer_connection = Arc::new(api.new_peer_connection(config).await.map_err(|e| e.to_string())?); // Создаем связь
    let data_channel = peer_connection.create_data_channel("proxy", None).await.map_err(|e| e.to_string())?; // Канал для данных
    let offer = RTCSessionDescription::offer(offer_sdp).map_err(|e| format!("Сигнал кривой: {}", e))?; // Принимаем сигнал от гостя
    peer_connection.set_remote_description(offer).await.map_err(|e| e.to_string())?; // Устанавливаем его сигнал
    let answer = peer_connection.create_answer(None).await.map_err(|e| e.to_string())?; // Создаем ответный сигнал
    peer_connection.set_local_description(answer.clone()).await.map_err(|e| e.to_string())?; // Ставим свой сигнал
    state.webrtc_peers.insert(client_ip.clone(), peer_connection.clone()); // Записываем гостя в журнал
    data_channel.on_message(Box::new(move |msg| { // Если гость что-то шлет...
        info!("Гость {} прислал: {:?}", client_ip, msg.data.to_vec());
        Box::pin(async move {})
    }));
    Ok(answer.sdp) // Отправляем ответный сигнал
}

// Старые шлюпки (HTTP) отправляем к бронированным крейсерам (HTTPS)!
async fn handle_http_request(req: Request<hyper::body::Incoming>, https_port: u16) -> Result<Response<String>, Infallible> {
    // Создаем новый курс на HTTPS
    let redirect_url = format!(
        "https://{}:{}{}",
        req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1"), // Имя корабля
        https_port, // Новый порт
        req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("") // Путь, куда лететь
    );
    Ok(Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY) // "Лети туда, юнга!"
        .header(header::LOCATION, redirect_url) // Новый курс
        .body(String::new())
        .unwrap())
}

// Главная палуба для бронированных крейсеров (HTTPS)!
async fn handle_https_request(
    mut req: Request<hyper::body::Incoming>,
    state: Arc<ProxyState>,
    client_ip: Option<IpAddr>,
) -> Result<Response<String>, HttpError> {
    // Узнаем, откуда гость
    let ip = match get_client_ip(&req, client_ip) {
        Some(ip) => ip,
        None => return Ok(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Гость без координат, кто ты?".to_string())
            .unwrap()),
    };

    // Если гость в черном списке...
    if state.blacklist.contains_key(&ip) {
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Ты шпион, вон из порта!".to_string())
            .unwrap());
    }

    // Проверяем пропуск
    if let Err(resp) = check_auth(&req, &state, &ip).await {
        return Ok(resp); // Пропуск не прошел, до свидания!
    }

    manage_session(&state, &ip).await; // Отмечаем гостя в журнале

    // Сколько запросов можно? Друзьям больше!
    let max_requests = if state.whitelist.contains_key(&ip) { 100 } else { 10 };
    if !check_rate_limit(&state, &ip, max_requests).await {
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Слишком много запросов, трюм полон!".to_string())
            .unwrap());
    }

    // Телепортация через WebSocket?
    if hyper_tungstenite::is_upgrade_request(&req) {
        let (response, websocket) = match upgrade(&mut req, None) {
            Ok(result) => result,
            Err(e) => return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(format!("Телепорт сломался: {}", e))
                .unwrap()),
        };
        tokio::spawn(handle_websocket(websocket)); // Запускаем телепорт
        return Ok(response.map(|_| String::new()));
    }

    // Телепортация через WebRTC?
    if req.uri().path() == "/webrtc/offer" {
        let body = req.into_body(); // Берем сигнал от гостя
        let collected = body.collect().await.map_err(|e| HttpError::from(e))?;
        let body_bytes = collected.to_bytes();
        let offer_sdp = String::from_utf8_lossy(&body_bytes).to_string();
        match handle_webrtc_offer(offer_sdp, state.clone(), ip).await {
            Ok(answer_sdp) => return Ok(Response::new(answer_sdp)), // Отправляем ответ
            Err(e) => return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                .body(format!("Телепорт сломан: {}", e))
                .unwrap()),
        }
    }

    let url = req.uri().to_string(); // Куда летит гость?
    if let Some(entry) = state.cache.get(&url) { // Есть ли добыча в складе?
        if entry.expiry > Instant::now() { // Добыча еще свежая?
            return Ok(Response::builder()
                .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                .body(String::from_utf8_lossy(&entry.response_body).to_string())
                .unwrap());
        }
    }

    let locations = state.locations.read().await; // Открываем карту маршрутов
    let path = req.uri().path(); // Куда гость хочет?
    let response_body = locations.iter()
        .find(|loc| path.starts_with(&loc.path)) // Ищем нужный путь
        .map(|loc| loc.response.clone()) // Берем сокровище
        .unwrap_or_else(|| "404 — звезда не найдена!".to_string()); // Или "нет такого!"

    // Кладем добычу в склад
    state.cache.insert(url, CacheEntry { 
        response_body: response_body.as_bytes().to_vec(), 
        expiry: Instant::now() + Duration::from_secs(60) // Добыча свежая час
    });
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
        .body(response_body)
        .unwrap())
}

// Обновляем карту каждые 60 секунд, вдруг новые звезды появились!
async fn reload_config(state: Arc<ProxyState>) {  
    let mut current_config = state.config.lock().await.clone(); // Берем текущую карту
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await; // Ждем минуту
        if let Ok(new_config) = load_config().await { // Пробуем новую карту
            if current_config != new_config && validate_config(&new_config).is_ok() { // Новая и годная?
                info!("Новая карта, юнга! HTTP={}, HTTPS={}, QUIC={}", 
                      new_config.http_port, new_config.https_port, new_config.quic_port);
                *state.config.lock().await = new_config.clone(); // Меняем карту
                *state.locations.write().await = new_config.locations.clone(); // Обновляем маршруты
                
                // Если порт не работает, запускаем его!
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
                
                current_config = new_config; // Теперь это наша карта
            }
        }
    }
}

// Обрабатываем гиперскоростные звездолеты (QUIC)!
async fn handle_quic_connection(connection: Connection) {
    info!("Гость на гиперскорости из {:?}", connection.remote_address());
    loop {
        if let Ok((mut send, mut recv)) = connection.accept_bi().await { // Принимаем связь
            tokio::spawn(async move {
                let mut buffer = vec![0; 4096]; // Трюм для данных
                if let Ok(Some(n)) = recv.read(&mut buffer).await { // Читаем послание
                    let response = format!("QUIC добыча: {}", String::from_utf8_lossy(&buffer[..n]));
                    send.write_all(response.as_bytes()).await.unwrap_or_else(|e| error!("Ошибка QUIC: {}", e)); // Отправляем ответ
                }
            });
        } else {
            break; // Связь пропала
        }
    }
}

// Запускаем порт для старых шлюпок (HTTP)!
async fn run_http_server(config: Config, state: Arc<ProxyState>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.http_port)); // Причал для шлюпок
    let listener = TcpListener::bind(addr).await.unwrap(); // Открываем причал
    info!("\x1b[92mHTTP порт открыт на {} \x1b[0m", addr);
    *state.http_running.write().await = true; // Отмечаем, что порт работает

    loop {
        let (stream, _) = listener.accept().await.unwrap(); // Ждем шлюпку
        let stream = TokioIo::new(stream); // Готовим шлюпку к работе
        let https_port = config.https_port; // Порт для перенаправления
        tokio::spawn(async move {
            if let Err(e) = AutoBuilder::new(TokioExecutor::new())
                .http1() // Только старый протокол
                .serve_connection(stream, service_fn(move |req| handle_http_request(req, https_port))) // Перенаправляем
                .await
            {
                error!("Шлюпка утонула: {}", e);
            }
        });
    }
}

// Запускаем порт для бронированных крейсеров (HTTPS)!
async fn run_https_server(state: Arc<ProxyState>, config: Config) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.https_port)); // Причал для крейсеров
    let listener = TcpListener::bind(addr).await.unwrap(); // Открываем причал
    info!("\x1b[92mHTTPS порт открыт на {} \x1b[0m", listener.local_addr().unwrap());
    *state.https_running.write().await = true; // Отмечаем, что порт работает
    let tls_acceptor = TlsAcceptor::from(Arc::new(load_tls_config(&config).unwrap())); // Готовим шифры

    loop {
        if let Ok((stream, client_ip)) = listener.accept().await { // Ждем крейсер
            stream.set_nodelay(true).unwrap(); // Ускоряем связь
            let state = state.clone();
            let acceptor = tls_acceptor.clone();
            tokio::spawn(async move {
                let tls_stream = match acceptor.accept(stream).await { // Шифруем связь
                    Ok(s) => TokioIo::new(s),
                    Err(e) => {
                        error!("Шифры подвели: {}", e);
                        return;
                    }
                };
                let service = service_fn(move |req| handle_https_request(req, state.clone(), Some(client_ip.ip())));
                if let Err(e) = AutoBuilder::new(TokioExecutor::new())
                    .http1() // Старый протокол
                    .http2() // Новый протокол
                    .serve_connection(tls_stream, service) // Обрабатываем запросы
                    .await
                {
                    error!("Крейсер подбит: {}", e);
                }
            });
        }
    }
}

// Запускаем порт для гиперскоростных звездолетов (QUIC)!
async fn run_quic_server(config: Config, state: Arc<ProxyState>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.quic_port)); // Причал для QUIC
    match Endpoint::server(load_quinn_config(&config).unwrap(), addr) { // Открываем причал
        Ok(endpoint) => {
            info!("\x1b[92mQUIC порт открыт на {} \x1b[0m", endpoint.local_addr().unwrap());
            *state.quic_running.write().await = true; // Отмечаем, что порт работает
            while let Some(conn) = endpoint.accept().await { // Ждем звездолет
                tokio::spawn(handle_quic_connection(conn.await.unwrap())); // Обрабатываем
            }
        }
        Err(e) => {
            error!("QUIC порт не открылся, меняй карту: {}", e);
            *state.quic_running.write().await = false;
        }
    }
}

// Главная палуба космопорта! Здесь все начинается!
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // АРРР! Устанавливаем шифры, чтобы нас не подслушали космические шпионы!
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Шифры сломаны, юнга!");

    // Включаем рацию, чтобы кричать о проблемах и победах!
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr) // Кричим в космос
        .init();

    // Создаем машину времени (runtime), чтобы все работало быстро
    let initial_config = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            // Пробуем взять карту из сундука
            match load_config().await {
                Ok(cfg) => match validate_config(&cfg) { // Проверяем карту
                    Ok(()) => Ok(cfg), // Карта годная!
                    Err(e) => {
                        tracing::error!("Карта кривая: {}", e);
                        Err("Карта не годится")
                    }
                },
                Err(e) => { // Карта пропала? Берем запасную!
                    tracing::warn!("Карта потеряна: {}. Берем старую!", e);
                    let mut default_config = Config {
                        http_port: 80, // Причал для шлюпок
                        https_port: 443, // Причал для крейсеров
                        quic_port: 444, // Причал для звездолетов
                        cert_path: "cert.pem".to_string(), // Сундук с шифрами
                        key_path: "key.pem".to_string(), // Ключ от сундука
                        jwt_secret: "your_jwt_secret".to_string(), // Тайный код
                        worker_threads: 16, // 16 юнг на палубе
                        locations: vec![], // Пока маршрутов нет
                        ice_servers: Some(vec!["stun:stun.l.google.com:19302".to_string()]), // Запасной маяк
                    };
                    if e.contains("missing field `ice_servers`") { // Если маяков нет в старой карте...
                        if let Ok(content) = tokio::fs::read_to_string("config.toml").await {
                            if let Ok(mut partial_config) = toml::from_str::<Config>(&content) {
                                partial_config.ice_servers = Some(vec!["stun:stun.l.google.com:19302".to_string()]);
                                default_config = partial_config; // Чиним карту
                            }
                        }
                    }
                    match validate_config(&default_config) {
                        Ok(()) => Ok(default_config), // Запасная карта годится!
                        Err(e) => {
                            tracing::error!("Запасная карта кривая: {}", e);
                            Err("Карта не годится")
                        }
                    }
                }
            }
        })?;

    // Создаем корабль с юнгами, чтобы все работало одновременно!
    let runtime = Builder::new_multi_thread()
        .worker_threads(initial_config.worker_threads) // Сколько юнг бегает
        .enable_all() // Включаем все пушки
        .build()
        .unwrap();

    // Запускаем корабль в полет!
    runtime.block_on(async {
        // Собираем штурвал космопорта
        let state = Arc::new(ProxyState {
            cache: DashMap::new(), // Пустой склад для добычи
            auth_cache: DashMap::new(), // Пустой журнал пропусков
            whitelist: DashMap::new(), // Список друзей пуст
            blacklist: DashMap::new(), // Список врагов пуст
            sessions: DashMap::new(), // Журнал гостей пуст
            auth_attempts: DashMap::new(), // Журнал попыток пуст
            rate_limits: DashMap::new(), // Лимиты пусты
            config: TokioMutex::new(initial_config.clone()), // Карта под замком
            webrtc_peers: DashMap::new(), // Телепортационных связей пока нет
            locations: Arc::new(RwLock::new(initial_config.locations.clone())), // Маршруты с карты
            http_running: Arc::new(RwLock::new(false)), // HTTP еще не работает
            https_running: Arc::new(RwLock::new(false)), // HTTPS спит
            quic_running: Arc::new(RwLock::new(false)), // QUIC отдыхает
            http_handle: Arc::new(TokioMutex::new(None)), // Рычаг для HTTP пуст
            https_handle: Arc::new(TokioMutex::new(None)), // Рычаг для HTTPS пуст
            quic_handle: Arc::new(TokioMutex::new(None)), // Рычаг для QUIC пуст
        });

        // Запускаем все порты и помощников!
        let http_handle = tokio::spawn(run_http_server(initial_config.clone(), state.clone())); // Шлюпки
        let https_handle = tokio::spawn(run_https_server(state.clone(), initial_config.clone())); // Крейсеры
        let quic_handle = tokio::spawn(run_quic_server(initial_config.clone(), state.clone())); // Звездолеты
        let reload_handle = tokio::spawn(reload_config(state.clone())); // Обновление карты

        // Ставим рычаги на место
        *state.http_handle.lock().await = Some(http_handle);
        *state.https_handle.lock().await = Some(https_handle);
        *state.quic_handle.lock().await = Some(quic_handle);

        // Запускаем капитанскую консоль
        let console_handle = tokio::spawn(run_console(state.clone()));

        // Ждем чуть-чуть, чтобы все заработало
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Ждем сигнала "Шторм!" (Ctrl+C) от капитана
        tokio::signal::ctrl_c().await?;
        tracing::info!("Шторм зовет, юнга! Завершаем рейд...");

        // Сворачиваем паруса, закрываем порты!
        tracing::info!("Сворачиваем паруса...");
        if let Some(handle) = state.http_handle.lock().await.take() { // Выключаем HTTP
            handle.abort();
        }
        if let Some(handle) = state.https_handle.lock().await.take() { // Выключаем HTTPS
            handle.abort();
        }
        if let Some(handle) = state.quic_handle.lock().await.take() { // Выключаем QUIC
            handle.abort();
        }
        reload_handle.abort(); // Перестаем обновлять карту
        console_handle.abort(); // Выключаем консоль

        tracing::info!("Космопорт закрыт, до новых приключений, юнга!");

        Ok::<(), Box<dyn std::error::Error>>(()) // Все готово, капитан!
    })?;

    Ok(()) // Корабль успешно завершил рейд!
}
