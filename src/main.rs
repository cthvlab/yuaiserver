use hyper::{service::service_fn, Request, Response, StatusCode, Error as HttpError}; // Двигатель для HTTP: запросы, ответы и ошибки
use hyper_util::{rt::{TokioExecutor, TokioIo}, server::conn::auto::Builder as AutoBuilder}; // Помощники для двигателя: капитаны и провода
use hyper::header; // Заголовки — как печати на письмах
use hyper_tungstenite::{HyperWebsocket, upgrade}; // Телепорт WebSocket для быстрых прыжков
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer}; // Шифры: пропуска и ключи
use rustls::ServerConfig; // Настройки шифров для безопасных полетов
use dashmap::DashMap; // Быстрый сундук для хранения добычи
use std::collections::HashMap; // Обычный сундук для списков
use std::convert::Infallible; // Ошибка, которой не бывает — как честный пират!
use std::net::{IpAddr, SocketAddr}; // Координаты: адрес и порт
use std::sync::Arc; // Общий штурвал для капитанов
use std::time::{Duration, Instant}; // Часы: сколько ждать и когда началось
use std::fs::File; // Файлы — как карты в сундуке
use std::io::BufReader as StdBufReader; // Капитан, читающий карты
use tokio::sync::{Mutex as TokioMutex, RwLock}; // Замки: один пишет, все читают
use tokio::net::{TcpListener}; // Ухо капитана для TCP-сигналов
use tokio_rustls::TlsAcceptor; // Приемник шифров для TLS
use tokio::task::JoinHandle; // Задачи для капитанов в фоне
use tokio::runtime::Builder; // Строитель корабля для команды
use tracing::{error, info, warn}; // Рация: кричать о бедах и победах
use jsonwebtoken::{decode, DecodingKey, Validation}; // Проверка пропусков JWT
use serde::{Deserialize, Serialize}; // Чтение карт из текста
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
mod setup;   // Модуль для подготовки космопорта к запуску

const CONTENT_TYPE_UTF8: &str = "text/plain; charset=utf-8"; // Метка для текстовых посылок

// Это как табличка на звездной карте: куда лететь и что отдавать пришельцам!
#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
struct Location {
    path: String,     // Путь — это как "поворот налево у третьей звезды"
    response: String, // Ответ — это сокровище, что мы даем гостям
}

// Это наш главный план, где все настройки космопорта!
#[derive(Serialize, Deserialize, Clone, PartialEq)]
struct Config {
    http_port: u16,              // Порт для старых шлюпок (HTTP), как номер причала
    https_port: u16,             // Порт для бронированных крейсеров (HTTPS)
    quic_port: u16,              // Порт для гиперскоростных звездолетов (QUIC)
    cert_path: String,           // Где лежит сундук с сертификатами (как пароль для входа)
    key_path: String,            // Где лежит ключ от сундука
    jwt_secret: String,          // Тайный пиратский код для проверки пропусков
    worker_threads: usize,       // Сколько капитанов бегает по палубе
    locations: Vec<Location>,    // Список всех маршрутов для пришельцев
    ice_servers: Option<Vec<String>>, // Маяки для телепортации, как звездные фонари
    guest_rate_limit: u32,       // Лимит для шатающихся гостей, не в списке
    whitelist_rate_limit: u32,   // Лимит для друзей капитана из белого списка
    blacklist_rate_limit: u32,   // Лимит для шпионов из черного списка, чтоб не шныряли!
    rate_limit_window: u64,      // Сколько секунд ждать, пока трюм снова откроется
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
    auth_cache: DashMap<String, AuthCacheEntry>, // Журнал пропусков (пока спит, но пригодится для секретных миссий!)
    whitelist: DashMap<String, ()>,              // Список друзей капитана
    blacklist: DashMap<String, ()>,              // Список врагов и шпионов
    sessions: DashMap<String, Instant>,          // Кто сейчас в порту (сессии)
    auth_attempts: DashMap<String, (u32, Instant)>, // Сколько раз ломились без пропуска
    rate_limits: DashMap<String, (Instant, u32)>, // Лимит запросов, чтобы трюм не лопнул
    privileged_clients: DashMap<String, Instant>, // Список гостей с золотыми пропусками для особых дел!
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

// Читаем карту сокровищ (config.toml), чтобы знать, куда лететь!
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
    // Если порты дерутся, это как два капитана на одном корабле!
    if config.http_port == config.https_port || config.http_port == config.quic_port || config.https_port == config.quic_port {
        return Err(format!("Порты дерутся, капитан! HTTP={}, HTTPS={}, QUIC={}", config.http_port, config.https_port, config.quic_port));
    }
    // Сундук с шифрами на месте?
    if !std::path::Path::new(&config.cert_path).exists() || !std::path::Path::new(&config.key_path).exists() {
        return Err("Сундук с шифрами потерян!".to_string());
    }
    // Капитаны есть, но не толпа ли?
    if config.worker_threads == 0 || config.worker_threads > 1024 {
        return Err("Капитанов должно быть от 1 до 1024, иначе бардак на палубе!".to_string());
    }
    load_tls_config(config).map_err(|e| format!("Шифры сломаны, шторм и гром: {}", e))?;

    // Лимиты скорости — чтоб трюм не лопнул!
    if config.guest_rate_limit == 0 || config.whitelist_rate_limit == 0 || config.blacklist_rate_limit == 0 {
        return Err("Лимиты скорости не могут быть 0, капитан! Как жить без добычи?".to_string());
    }
    if config.rate_limit_window == 0 {
        return Err("Окно лимита 0 секунд? Это как ром без бочки, капитан!".to_string());
    }

    // Маяки для телепортации на месте?
    match &config.ice_servers {
        Some(servers) if servers.is_empty() => {
            return Err("Маяки пусты, как трюм после шторма!".to_string());
        }
        None => {
            warn!("Маяков нет, берем старый маяк STUN, йо-хо-хо!");
        }
        Some(_) => {}
    }

    Ok(()) // Карта готова, капитан!
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

// Проверяем, есть ли у пришельца золотой пропуск для особых дел!
async fn check_auth(req: &Request<hyper::body::Incoming>, state: &ProxyState, ip: &str) -> bool {
    // Смотрим, есть ли уже запись в журнале привилегий
    if let Some(entry) = state.privileged_clients.get(ip) {
        if *entry > Instant::now() {
            info!("Гость {} уже с золотым пропуском, добро пожаловать в каюту капитана!", ip);
            return true; // Золотой пропуск свежий, как ром из бочки!
        }
    }

    // Берем звездный атлас под замок
    let config = state.config.lock().await;
    let auth_header = req.headers().get("Authorization").and_then(|h| h.to_str().ok());

    // Проверяем, есть ли у гостя JWT — тайный код для привилегий
    let is_jwt_auth = auth_header.map(|auth| {
        auth.starts_with("Bearer ") && // Пропуск должен начинаться с "Bearer "
        decode::<HashMap<String, String>>(
            auth.trim_start_matches("Bearer "), // Убираем "Bearer " и проверяем код
            &DecodingKey::from_secret(config.jwt_secret.as_ref()), // Наш тайный ключ
            &Validation::default() // Правила проверки
        ).is_ok()
    }).unwrap_or(false);

    // Записываем в журнал авторизации для будущих секретных миссий!
    state.auth_cache.insert(ip.to_string(), AuthCacheEntry {
        is_valid: is_jwt_auth, // Пропуск настоящий или фальшивый?
        expiry: Instant::now() + Duration::from_secs(3600), // Свежий на час, как ром в трюме!
    });

    if is_jwt_auth {
        // Золотой пропуск в кармане, записываем в журнал привилегий!
        state.privileged_clients.insert(ip.to_string(), Instant::now() + Duration::from_secs(3600)); // Действует час, как звезда на небе!
        info!("Гость {} получил золотой пропуск, йо-хо-хо! Теперь он в команде!", ip);
        true // Добро пожаловать в элиту, капитан!
    } else {
        // Нет пропуска? Ну и ладно, гуляй как простой гость!
        info!("Гость {} без золотого пропуска, но пусть заходит, трюм открыт для всех!", ip);
        false
    }
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

// Проверяем, не слишком ли много добычи тащит этот гость, друг или шпион!
async fn check_rate_limit(state: &ProxyState, ip: &str) -> bool {
    let config = state.config.lock().await; // Хватаем звездный атлас под замок
    let now = Instant::now(); // Сколько сейчас на часах капитана?
    let mut entry = state.rate_limits.entry(ip.to_string()).or_insert((now, 0)); // Смотрим в трюм гостя
    let window = Duration::from_secs(config.rate_limit_window); // Сколько ждать, пока ром снова нальют
    let max_requests = if state.blacklist.contains_key(ip) {
        config.blacklist_rate_limit // Шпионы получают меньше всех, чтоб не наглели!
    } else if state.whitelist.contains_key(ip) {
        config.whitelist_rate_limit // Друзья капитана могут тащить больше добычи!
    } else {
        config.guest_rate_limit // Обычным гостям — по чуть-чуть, не разгуляешься!
    };

    if now.duration_since(entry.0) > window { // Прошло время, трюм снова открыт?
        *entry = (now, 1); // Новый рейд, начинаем считать заново!
        true // Лети, капитан, добыча твоя!
    } else if entry.1 < max_requests { // Еще есть место в трюме?
        entry.1 += 1; // Бросай еще один сундук!
        true // Тащи дальше, капитан доволен!
    } else {
        false // Трюм полон, жди, пока ром не выпьют!
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

// Старые шлюпки (HTTP) отправляем к бронированным крейсерам (HTTPS) с правильным курсом!
async fn handle_http_request(req: Request<hyper::body::Incoming>, https_port: u16) -> Result<Response<String>, Infallible> {
    // Берем имя корабля из запроса, но убираем старый порт, чтоб не запутаться в звездах!
    let host = req.headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("127.0.0.1");
    let host_without_port = host.split(':').next().unwrap_or("127.0.0.1"); // Вырезаем порт, как лишний груз с палубы!

    // Прокладываем новый курс на HTTPS — чистый, без космического мусора!
    let redirect_url = format!(
        "https://{}:{}{}",
        host_without_port, // Только имя корабля, никаких старых портов!
        https_port,        // Новый порт — наша цель в гиперпространстве!
        req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("") // Путь, куда лететь, как звезда на карте!
    );

    // Отправляем шлюпку в безопасные воды с приказом "Лети туда, капитан!"
    let response = Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY) // "Полный вперед на новый курс!"
        .header(header::LOCATION, &redirect_url) // Карта с новым маршрутом
        .body(String::new()) // Без лишнего груза в трюме!
        .unwrap();

    Ok(response)
}

// Главная палуба для бронированных крейсеров (HTTPS)!
async fn handle_https_request(
    mut req: Request<hyper::body::Incoming>,
    state: Arc<ProxyState>,
    client_ip: Option<IpAddr>,
) -> Result<Response<String>, HttpError> {
    // Кто этот гость, откуда приплыл?
    let ip = match get_client_ip(&req, client_ip) {
        Some(ip) => ip,
        None => return Ok(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Гость без координат, кто ты, шельма?".to_string())
            .unwrap()),
    };

    // Шпион в черном списке? Пусть заходит, но с лимитами!
    if state.blacklist.contains_key(&ip) && !check_rate_limit(&state, &ip).await {
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Ты шпион, и трюм полон! Пушки на тебя, жди своей очереди!".to_string())
            .unwrap());
    }

    // Проверяем, есть ли золотой пропуск для особых дел
    let is_privileged = check_auth(&req, &state, &ip).await;
    manage_session(&state, &ip).await; // Отмечаем в журнале: "Этот гость здесь!"

    // Трюм не лопнул от запросов?
    if !check_rate_limit(&state, &ip).await {
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
            .body("Слишком много добычи, трюм трещит! Жди, капитан!".to_string())
            .unwrap());
    }

    // Заглядываем в журнал авторизации, вдруг там что интересное!
    if let Some(auth_entry) = state.auth_cache.get(&ip) {
        if auth_entry.is_valid && auth_entry.expiry > Instant::now() {
            info!("Журнал подтверждает: гость {} — проверенный капитан!", ip);
        }
    }

    // Телепортация через WebSocket? Йо-хо-хо, прыгай!
    if hyper_tungstenite::is_upgrade_request(&req) {
        let (response, websocket) = match upgrade(&mut req, None) {
            Ok(result) => result,
            Err(e) => return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(format!("Телепорт сломался, шторм и гром: {}", e))
                .unwrap()),
        };
        tokio::spawn(handle_websocket(websocket)); // Прыжок в гиперпространство!
        return Ok(response.map(|_| String::new()));
    }

    // Телепортация через WebRTC? Магия звезд зовет!
    if req.uri().path() == "/webrtc/offer" {
        let body = req.into_body(); // Хватаем сигнал от гостя
        let collected = body.collect().await.map_err(|e| HttpError::from(e))?;
        let body_bytes = collected.to_bytes();
        let offer_sdp = String::from_utf8_lossy(&body_bytes).to_string();
        match handle_webrtc_offer(offer_sdp, state.clone(), ip).await {
            Ok(answer_sdp) => return Ok(Response::new(answer_sdp)), // "Лови ответ, капитан!"
            Err(e) => return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                .body(format!("Телепорт барахлит, шторм его побери: {}", e))
                .unwrap()),
        }
    }

    let url = req.uri().to_string(); // Куда гость плывет?
    if let Some(entry) = state.cache.get(&url) { // Есть добыча в сундуке?
        if entry.expiry > Instant::now() { // Ром еще свежий?
            return Ok(Response::builder()
                .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                .body(String::from_utf8_lossy(&entry.response_body).to_string())
                .unwrap());
        }
    }

    let locations = state.locations.read().await; // Открываем космический атлас
    let path = req.uri().path(); // Куда курс, капитан?
    let mut response_body = locations.iter()
        .find(|loc| path.starts_with(&loc.path)) // Ищем сокровище по карте
        .map(|loc| loc.response.clone()) // Вот твоя добыча!
        .unwrap_or_else(|| "404 — звезда не найдена, шторм тебя побери!".to_string()); // Или пусто!

    // Если гость с золотым пропуском, даем ему привет от капитана (пример будущей функции)
    if is_privileged {
        response_body = format!("Привет от капитана, привилегированный гость! {}\n(Скоро тут будет админка, йо-хо-хо!)", response_body);
    }

    // Бросаем добычу в сундук
    state.cache.insert(url, CacheEntry { 
        response_body: response_body.as_bytes().to_vec(), 
        expiry: Instant::now() + Duration::from_secs(60) // Свежо на час, как ром из бочки!
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
        tokio::time::sleep(Duration::from_secs(60)).await; // Ждем минуту, как штиль перед бурей!
        if let Ok(new_config) = load_config().await { // Пробуем новую карту
            if current_config != new_config && validate_config(&new_config).is_ok() { // Новая и годная?
                info!("\x1b[32mНовая карта, капитан! HTTP={}, HTTPS={}, QUIC={}\x1b[0m", 
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
            break; // Связь пропала, как корабль в тумане!
        }
    }
}

// Запускаем порт для старых шлюпок (HTTP)
async fn run_http_server(config: Config, state: Arc<ProxyState>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.http_port));
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            info!("\x1b[92mHTTP порт открыт на {} \x1b[0m", addr);
            *state.http_running.write().await = true;
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let stream = TokioIo::new(stream);
                        let https_port = config.https_port;
                        tokio::spawn(async move {
                            if let Err(e) = AutoBuilder::new(TokioExecutor::new())
                                .http1()
                                .serve_connection(stream, service_fn(move |req| handle_http_request(req, https_port)))
                                .await
                            {
                                error!("Ошибка обработки HTTP-соединения: {}", e);
                            }
                        });
                    }
                    Err(e) => error!("Ошибка принятия HTTP-соединения: {}", e),
                }
            }
        }
        Err(e) => error!("Не удалось открыть HTTP-порт на {}: {}", addr, e),
    }
}

// Запускаем причал для бронированных крейсеров (HTTPS)
async fn run_https_server(state: Arc<ProxyState>, config: Config) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.https_port));
    match TcpListener::bind(addr).await {
        Ok(listener) => {
            info!("\x1b[92mHTTPS порт открыт на {} \x1b[0m", listener.local_addr().unwrap());
            *state.https_running.write().await = true;
            let tls_acceptor = TlsAcceptor::from(Arc::new(load_tls_config(&config).unwrap()));
            loop {
                if let Ok((stream, client_ip)) = listener.accept().await {
                    stream.set_nodelay(true).unwrap();
                    let state = state.clone();
                    let acceptor = tls_acceptor.clone();
                    tokio::spawn(async move {
                        match acceptor.accept(stream).await {
                            Ok(tls_stream) => {
                                let tls_stream = TokioIo::new(tls_stream);
                                let service = service_fn(move |req| handle_https_request(req, state.clone(), Some(client_ip.ip())));
                                if let Err(e) = AutoBuilder::new(TokioExecutor::new())
                                    .http1()
                                    .http2()
                                    .serve_connection(tls_stream, service)
                                    .await
                                {
                                    error!("Ошибка обработки HTTPS-соединения: {}", e);
                                }
                            }
                            Err(e) => error!("Ошибка принятия TLS: {}", e),
                        }
                    });
                }
            }
        }
        Err(e) => error!("Не удалось открыть HTTPS-порт на {}: {}", addr, e),
    }
}

// Запускаем причал для гиперскоростных звездолётов (QUIC)
async fn run_quic_server(config: Config, state: Arc<ProxyState>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.quic_port));
    match Endpoint::server(load_quinn_config(&config).unwrap(), addr) {
        Ok(endpoint) => {
            info!("\x1b[92mQUIC порт открыт на {} \x1b[0m", endpoint.local_addr().unwrap());
            *state.quic_running.write().await = true;
            while let Some(conn) = endpoint.accept().await {
                tokio::spawn(handle_quic_connection(conn.await.unwrap()));
            }
        }
        Err(e) => error!("Не удалось открыть QUIC-порт на {}: {}", addr, e),
    }
}

// Главная палуба космопорта! Здесь всё начинается!
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Устанавливаем криптопровайдер для TLS
    match rustls::crypto::ring::default_provider().install_default() {
        Ok(()) => info!("Шифры на борту, капитан доволен, йо-хо-хо!"),
        Err(e) => {
            warn!("Внимание! Шифры сломаны: {:?}", e);
            info!("Летим дальше, но безопасность не гарантируется!");
        }
    }

    // Настраиваем рацию для криков о победах и бедах
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time() // Убираем звездное время, чтоб не путаться в часах!
        .with_target(false) // Без названий отсеков, только чистые вести!
        .with_level(false) // Уровень шума не нужен, кричим как есть!
        .init();

    info!("\x1b[32mЗапускаем космопорт, проверяем системы...\x1b[0m");

    // Загружаем начальную карту сокровищ
    let initial_config = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            match setup::setup_config().await {
                Ok(config) => Ok(config),
                Err(e) => {
                    error!("Ошибка загрузки конфигурации: {}", e);
                    Err("Запуск отменён, шторм нас побери!")
                }
            }
        })?;

    info!("\x1b[32mКонфигурация загружена, готовим корабль к взлету...\x1b[0m");

    // Строим многопалубный звездолет с командой капитанов
    let runtime = Builder::new_multi_thread()
        .worker_threads(initial_config.worker_threads)
        .enable_all()
        .build()
        .unwrap();

    // Взлетаем в гиперпространство!
    runtime.block_on(async {
        // Собираем штурвал и сундуки для управления космопортом
        let state = Arc::new(ProxyState {
            cache: DashMap::new(),
            auth_cache: DashMap::new(),
            whitelist: DashMap::new(),
            blacklist: DashMap::new(),
            sessions: DashMap::new(),
            auth_attempts: DashMap::new(),
            rate_limits: DashMap::new(),
            privileged_clients: DashMap::new(),
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

        // Запускаем шлюпки HTTP в космос
        let http_handle = tokio::spawn({
            let state = state.clone();
            let config = initial_config.clone();
            async move {
                run_http_server(config, state).await;
            }
        });

        // Отправляем бронированные крейсеры HTTPS на орбиту
        let https_handle = tokio::spawn({
            let state = state.clone();
            let config = initial_config.clone();
            async move {
                run_https_server(state, config).await;
            }
        });

        // Выпускаем гиперскоростные звездолеты QUIC в галактику
        let quic_handle = tokio::spawn({
            let state = state.clone();
            let config = initial_config.clone();
            async move {
                run_quic_server(config, state).await;
            }
        });

        // Задаем капитану обновлять карту каждые 60 секунд
        let reload_handle = tokio::spawn(reload_config(state.clone()));

        // Закрепляем рычаги управления
        *state.http_handle.lock().await = Some(http_handle);
        *state.https_handle.lock().await = Some(https_handle);
        *state.quic_handle.lock().await = Some(quic_handle);

        // Ждем, пока все паруса поднимутся и двигатели загудят!
        while !(*state.http_running.read().await && *state.https_running.read().await && *state.quic_running.read().await) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        info!("\x1b[32mКосмопорт запущен, все системы в порядке! Полный вперед, йо-хо-хо!\x1b[0m");
        // Показываем карту и статус на мостике
        let initial_status = console::get_server_status(&state, true).await;
        info!("{}", initial_status);

        // Врубаем консоль для приказов с капитанского мостика
        let console_handle = tokio::spawn(run_console(state.clone()));

        // Ждем сигнала с мостика (Ctrl+C) для посадки
        tokio::signal::ctrl_c().await?;
        info!("\x1b[32mПора уходить в гиперпространство, закрываем шлюзы и гасим огни\x1b[0m");
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

        info!("\x1b[32mКосмопорт ушел в другую галактику, до новых приключений, капитан!\x1b[0m");
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;   

    Ok(())
}
