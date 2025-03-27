use hyper::{service::service_fn, Request, Response, StatusCode, header, Error as HttpError}; // Двигатель для HTTP: запросы, ответы и шторма — как паруса для шлюпок!
use hyper_util::{rt::{TokioExecutor, TokioIo}, server::conn::auto::Builder as AutoBuilder}; // Помощники для двигателя: юнги крутят штурвал, а провода тянут паруса!
use hyper_tungstenite::{HyperWebsocket, upgrade, is_upgrade_request}; // Телепорт WebSocket для прыжков через гиперпространство, быстрей ветра!
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer}; // Шифры: пропуска и ключи, как сундуки с золотом и тайными печатями!
use rustls_pemfile::{certs, pkcs8_private_keys}; // Достаем шифры из трюма, как сокровища с глубин!
use rustls::ServerConfig; // Настройки шифров для полетов через звездные шторма, броня крепка!
use http_body_util::{BodyExt, Full}; // Собираем добычу из запросов и шлём ответы — как ром в бочки!
use bytes::{Bytes, Buf}; // Байты — цифровое золото в трюме, звенит при каждом шаге!
use dashmap::DashMap; // Быстрый сундук для добычи, открывается одним взглядом!
use std::collections::HashMap; // Обычный сундук для списков, потяжелее, но верный!
use std::convert::Infallible; // Ошибка, которой не бывает — как честный пират в легендах!
use std::net::{IpAddr, SocketAddr}; // Координаты планет, адрес и порт!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use std::time::{Duration, Instant}; // Часы: сколько ждать и когда началась буря!
use std::fs::File; // Файлы — карты в сундуке, указывают путь к сокровищам!
use std::io::BufReader as StdBufReader; // Капитан с лупой читает карты, йо-хо-хо!
use std::str::FromStr; // Парсим IP, как карту в руки капитана!
use tokio::sync::{Mutex as TokioMutex, RwLock}; // Замки: один пишет, все читают — как приказы на мостике!
use tokio::net::TcpListener; // Ухо капитана для TCP-сигналов, ловит шорох в эфире!
use tokio_rustls::TlsAcceptor; // Приемник шифров для TLS, страж у ворот космопорта!
use tokio::task::JoinHandle; // Задачи для юнг в фоне, палуба блестит!
use tokio::runtime::Builder; // Строитель звездолета для команды, возводит с нуля!
use tracing::{error, info, warn}; // Рация: кричим о бедах и победах через космос!
use tracing_subscriber::fmt; // Громкоговоритель для всей команды, шторм нас не заглушит!
use tracing_subscriber::prelude::*; // Каналы рации, чтобы орать на разных частотах!
use tracing_subscriber::filter::{LevelFilter, Targets}; // Куда в рацию или в сундук со свитками
use jsonwebtoken::{decode, DecodingKey, Validation}; // Проверка пропусков JWT, как печати на тайных письмах!
use serde::{Deserialize, Serialize}; // Чтение карт из текста — расшифровка древних свитков!
use quinn::{Endpoint, ServerConfig as QuinnServerConfig, Connection}; // QUIC: причал и связь для гиперскоростных звездолетов!
use h3_quinn::Connection as H3QuinnConnection; // HTTP/3 и QUIC — двигатель для скорости света!
use h3::server::Connection as H3Connection; // HTTP/3 палуба для самых шустрых гостей!
use futures::{StreamExt, SinkExt}; // Потоки: грузы через шлюзы, как посылки в трюм!
use webrtc::api::APIBuilder; // Строитель телепорта WebRTC, магия меж звёзд!
use webrtc::peer_connection::RTCPeerConnection; // Связь для телепортации, мост между кораблями!
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription; // Сигналы WebRTC — звездные маяки в эфире!
use webrtc::ice_transport::ice_server::RTCIceServer; // Маяки для телепортации, фонари в галактическом тумане!
use colored::Colorize; // Красота в консоли

mod setup;   // Подготовка космопорта к запуску — ремонт парусов перед рейдом!
mod console; // Отсек консоли, где капитан кричит команды!

const CONTENT_TYPE_UTF8: &str = "text/plain; charset=utf-8"; // Метка для текстовых посылок, как ром в бочке!

// Маршрут на звездной карте: куда лететь и что отдавать!
#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
struct Location {
    path: String,                       // Путь — "налево у третьей звезды"!
    response: String,                   // Ответ — сокровище для гостей!
    headers: Option<Vec<(String, String)>>, // Заголовки, паруса на заказ!
}

// Главный план космопорта, все настройки здесь!
#[derive(Serialize, Deserialize, Clone, PartialEq)]
struct Config {
    http_port: u16,               // Порт для шлюпок (HTTP), номер причала!
    https_port: u16,              // Порт для крейсеров (HTTPS), броня крепка!
    quic_port: u16,               // Порт для звездолетов (QUIC), быстрее света!
    cert_path: String,            // Сундук с сертификатами, пароль в тайную каюту!
    key_path: String,             // Ключ от сундука, без него — пустой трюм!
    jwt_secret: String,           // Пиратский код для пропусков, штамп капитана!
    locations: Vec<Location>,     // Маршруты для гостей, звездная карта!
    ice_servers: Option<Vec<String>>, // Маяки для телепортации, фонари в ночи!
    guest_rate_limit: u32,        // Лимит для гостей, не наглей, матрос!
    whitelist_rate_limit: u32,    // Лимит для друзей, им больше рома!
    blacklist_rate_limit: u32,    // Лимит для шпионов, чтоб не шныряли!
    rate_limit_window: u64,       // Сколько ждать, пока трюм откроется!
    trusted_host: String,         // Надёжный порт для перенаправления!
    max_request_body_size: usize, // Ограничение трюма, шпионы не затопят!
	log_path: String,         	// Путь к сундуку логов, где храним свитки рейда!
}

// Добыча в сундуке, хранится недолго!
#[derive(Clone)]
struct CacheEntry {
    response_body: Vec<u8>, // Байты — цифровое золото в трюме!
    expiry: Instant,        // Когда "протухнет", как старый ром!
}

// Пропуск в журнале: настоящий или фальшивый?
#[derive(Clone)]
struct AuthCacheEntry {
    is_valid: bool,   // Пропуск годен или шпионский?
    expiry: Instant,  // Когда сгорит, как бумага в костре!
}

// Штурвал космопорта, всё управление здесь!
struct ProxyState {
    cache: DashMap<String, CacheEntry>,          // Склад добычи, быстрый сундук!
    auth_cache: DashMap<String, AuthCacheEntry>, // Журнал пропусков, кто свой?
    whitelist: DashMap<String, ()>,              // Друзья капитана, ром без очереди!
    blacklist: DashMap<String, ()>,              // Шпионы, пушки наготове!
    sessions: DashMap<String, Instant>,          // Кто в порту, журнал мостика!
    auth_attempts: DashMap<String, (u32, Instant)>, // Сколько ломились без пропуска!
    rate_limits: DashMap<String, (Instant, u32)>, // Лимит запросов, трюм не лопнет!
    privileged_clients: DashMap<String, Instant>, // Гости с золотыми пропусками!
    config: TokioMutex<Config>,                  // Атлас под замком, только для капитана!
    webrtc_peers: DashMap<String, Arc<RTCPeerConnection>>, // Телепортационные мосты!
    locations: Arc<RwLock<Vec<Location>>>,       // Карта маршрутов, сокровища на звездах!
    http_running: Arc<RwLock<bool>>,             // HTTP порт спит или работает?
    https_running: Arc<RwLock<bool>>,            // HTTPS порт на ходу?
    quic_running: Arc<RwLock<bool>>,             // QUIC порт на гиперскорости?
    http_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>, // Рычаг для HTTP шлюпок!
    https_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>, // Рычаг для HTTPS крейсеров!
    quic_handle: Arc<TokioMutex<Option<JoinHandle<()>>>>, // Рычаг для QUIC звездолетов!
}

// Чистим трюм от старого хлама, как юнги после шторма!
async fn clean_cache(state: Arc<ProxyState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(300)); // Каждые 5 минут, смена вахты!
    loop {
        interval.tick().await; // Ждём сигнала юнги!
        let now = Instant::now(); // Сколько на часах капитана?
        state.cache.retain(|_, entry| entry.expiry > now); // Выкидываем протухший ром за борт!
        state.auth_cache.retain(|_, entry| entry.expiry > now); // Пропуска тоже чистим, шпионы не пройдут!
        info!("Трюм очищен, старый хлам за бортом, йо-хо-хо!");
    }
}

// Читаем карту сокровищ, чтоб знать, куда лететь!
async fn load_config() -> Result<Config, String> {
    let content = match tokio::fs::read_to_string("config.toml").await {
        Ok(content) => content,
        Err(e) => {
            let err = format!("Карта пропала в шторме: {}", e);
            error!("{}", err);
            return Err(err);
        }
    };
    match toml::from_str(&content) {
        Ok(config) => Ok(config),
        Err(e) => {
            let err = format!("Шторм и гром, карта порвана: {}", e);
            error!("{}", err);
            Err(err)
        }
    }
}

// Проверяем карту, чтоб не врезаться в астероид!
fn validate_config(config: &Config) -> Result<(), String> {
    info!("Проверяем карту, капитан! Все ли звезды на месте?");
    
    if config.http_port == config.https_port || config.http_port == config.quic_port || config.https_port == config.quic_port {
        let err = format!("Порты дерутся, как пираты за ром! HTTP={}, HTTPS={}, QUIC={}", config.http_port, config.https_port, config.quic_port);
        error!("{}", err);
        return Err(err);
    }
    
    if !std::path::Path::new(&config.cert_path).exists() || !std::path::Path::new(&config.key_path).exists() {
        let err = "Сундук с шифрами потерян в черной дыре!".to_string();
        error!("{}", err);
        return Err(err);
    }
    
    if let Err(e) = load_tls_config(config) {
        let err = format!("Шифры сломаны, шторм их побери: {}", e);
        error!("{}", err);
        return Err(err);
    }
    
    if config.guest_rate_limit == 0 || config.whitelist_rate_limit == 0 || config.blacklist_rate_limit == 0 {
        let err = "Лимиты скорости не могут быть 0, капитан! Как жить без добычи?".to_string();
        error!("{}", err);
        return Err(err);
    }
    
    if config.rate_limit_window == 0 {
        let err = "Окно лимита 0 секунд? Это как ром без бочки!".to_string();
        error!("{}", err);
        return Err(err);
    }
    
    if config.max_request_body_size == 0 || config.max_request_body_size > 1024 * 1024 * 100 {
        let err = "Лимит трюма должен быть от 1 байта до 100 МБ, иначе корабль потонет!".to_string();
        error!("{}", err);
        return Err(err);
    }
    
    match &config.ice_servers {
        Some(servers) if servers.is_empty() => {
            let err = "Маяки пусты, как трюм после шторма!".to_string();
            error!("{}", err);
            Err(err)
        }
        None => {
            warn!("Маяков нет, берем старый STUN!");
            Ok(())
        }
        Some(_) => Ok(())
    }
}


// Собираем посылку с добычей для HTTP/1.1 и HTTP/2, полный вперед!
fn build_response(
    status: StatusCode,
    body: Bytes,
    custom_headers: Option<Vec<(String, String)>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> { // Ошибка Hyper, как шторм в эфире!
    let mut builder = Response::builder()
        .status(status) // Флаг состояния — "Всё в порядке" или "Шторм на горизонте"!
        .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8) // Ром в бочке, текст по умолчанию!
        .header(header::SERVER, "YUAI CosmoPort") // Флаг нашего корабля, гордо реет!
        .header(header::CACHE_CONTROL, "no-cache") // Не храним добычу, если не сказано иное!
        .header(header::CONTENT_LENGTH, body.len().to_string()) // Сколько золота в трюме!
        .header("X-Content-Type-Options", "nosniff") // Не нюхай наш ром, шпион!
        .header("X-Frame-Options", "DENY") // Никаких рамок, это не твой трюм!
        .header("Content-Security-Policy", "default-src 'none'") // Только наш ром, никаких чужих сокровищ!
        .header("Strict-Transport-Security", "max-age=31536000; includeSubDomains"); // Броня на год, шторм не пробьёт!

    if let Some(headers) = custom_headers { // Особые паруса для маршрута?
        for (name, value) in headers {
            builder = builder.header(name.as_str(), value.as_str()); // Поднимаем их на мачты!
        }
    }

    match builder.body(Full::new(body)) {
        Ok(resp) => Ok(resp),
        Err(e) => {
            error!("[build_response] Failed to build response: {}", e);
            // Возвращаем безопасную заглушку вместо hyper::Error
            let fallback = Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::from_static(b"Internal Server Error")))
                .unwrap();
            Ok(fallback)
        }
    }
}

// Шапка для HTTP/3, добыча летит отдельно, как звездолёт на гиперскорости!
fn build_h3_response(
    status: StatusCode,
    custom_headers: Option<Vec<(String, String)>>,
) -> Result<Response<()>, String> {
    let mut builder = Response::builder()
        .status(status) // Флаг состояния, быстрый и четкий!
        .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8) // Текст по умолчанию, как ром в бочке!
        .header(header::SERVER, "YUAI CosmoPort") // Флаг корабля на гиперскорости!
        .header(header::CACHE_CONTROL, "no-cache"); // Не кэшируем, летим налегке!

    if let Some(headers) = custom_headers { // Особые паруса для гиперскорости?
        for (name, value) in headers {
            builder = builder.header(name.as_str(), value.as_str()); // Поднимаем их на мачты звездолета!
        }
    }

    match builder.body(()) {
		Ok(resp) => Ok(resp), // Шапка готова, добыча полетит следом!
		Err(e) => {
			let err = format!("Не удалось собрать шапку для HTTP/3: {}", e);
			error!("{}", err);
			Err(err)
		}
	}
}

// Грузим шифры для TLS, как броню на крейсер!
fn load_tls_config(config: &Config) -> Result<ServerConfig, Box<dyn std::error::Error>> {
    info!("Грузим шифры из сундука, капитан!");
    let cert_file = File::open(&config.cert_path)?;
    let key_file = File::open(&config.key_path)?;
    let certs: Vec<CertificateDer> = certs(&mut StdBufReader::new(cert_file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            error!("Ошибка чтения сертификатов: {}", e);
            Box::new(e) as Box<dyn std::error::Error>
        })?
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    let keys: Vec<PrivateKeyDer> = pkcs8_private_keys(&mut StdBufReader::new(key_file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            error!("Ошибка чтения ключей: {}", e);
            Box::new(e) as Box<dyn std::error::Error>
        })?
        .into_iter()
        .map(|bytes| PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(bytes)))
        .collect();
    let key = keys.into_iter().next().ok_or_else(|| {
		let err = "Ключ пропал, как ром перед боем!".to_string();
		error!("{}", err);
		err
	})?;
    let mut cfg = ServerConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    cfg.alpn_protocols = vec![b"h3".to_vec(), b"h2".to_vec(), b"http/1.1".to_vec()];
    info!("ALPN настроен: HTTP/3, HTTP/2, HTTP/1.1 — все паруса на месте!");
    Ok(cfg)
}

// Шифры для QUIC, полный вперед на гиперскорости!
fn load_quinn_config(config: &Config) -> Result<QuinnServerConfig, Box<dyn std::error::Error>> {
    info!("Готовим шифры для QUIC, полный вперед на гиперскорости!");
    let cert_file = File::open(&config.cert_path)?;
    let key_file = File::open(&config.key_path)?;
    let certs: Vec<CertificateDer> = certs(&mut StdBufReader::new(cert_file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            error!("Ошибка чтения QUIC сертификатов: {}", e);
            Box::new(e) as Box<dyn std::error::Error>
        })?
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    let keys: Vec<PrivateKeyDer> = pkcs8_private_keys(&mut StdBufReader::new(key_file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            error!("Ошибка чтения QUIC ключей: {}", e);
            Box::new(e) as Box<dyn std::error::Error>
        })?
        .into_iter()
        .map(|bytes| PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(bytes)))
        .collect();
    let key = keys.into_iter().next().ok_or_else(|| {
		let err = "Ключ улетел в черную дыру!".to_string();
		error!("{}", err);
		err
	})?;
    let mut quinn_config = QuinnServerConfig::with_single_cert(certs, key)?;
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_concurrent_bidi_streams(100u32.into());
    quinn_config.transport_config(Arc::new(transport_config));
    Ok(quinn_config)
}

// Проверяем пропуск, настоящий ли гость!
async fn check_auth(headers: &hyper::HeaderMap, state: &ProxyState, ip: &str) -> bool {
    if let Some(entry) = state.auth_cache.get(ip) {
        if entry.expiry > Instant::now() {
            info!("Пропуск для {} свежий, как ром из бочки!", ip);
            return entry.is_valid;
        } else {
            info!("Пропуск для {} протух, выкидываем за борт!", ip);
            state.auth_cache.remove(ip);
        }
    }
    if let Some(entry) = state.privileged_clients.get(ip) {
        if *entry > Instant::now() {
            info!("Гость {} с золотым пропуском, добро пожаловать на мостик!", ip);
            return true;
        }
    }
    let config = state.config.lock().await;
    let auth_header = headers.get("Authorization").and_then(|h| h.to_str().ok());
    let is_jwt_auth = auth_header.map(|auth| {
        auth.starts_with("Bearer ") &&
        decode::<HashMap<String, String>>(
            auth.trim_start_matches("Bearer "),
            &DecodingKey::from_secret(config.jwt_secret.as_ref()),
            &Validation::default()
        ).is_ok()
    }).unwrap_or(false);
    state.auth_cache.insert(ip.to_string(), AuthCacheEntry {
        is_valid: is_jwt_auth,
        expiry: Instant::now() + Duration::from_secs(3600),
    });
    if is_jwt_auth {
        state.privileged_clients.insert(ip.to_string(), Instant::now() + Duration::from_secs(3600));
        info!("Гость {} получил золотой пропуск, йо-хо-хо!", ip);
        true
    } else {
        info!("Гость {} без пропуска, заходи как простой матрос!", ip);
        false
    }
}

// Пушки на изготовку! Проверяем гостя и шпионов — за борт!
async fn restrict_access<B>(
    req: &Request<B>,
    client_ip: Option<IpAddr>,
    state: Arc<ProxyState>,
) -> Result<String, Response<Full<Bytes>>> {
    let config = state.config.lock().await;
    let trusted_proxy = Some(config.trusted_host.as_str());

    match get_client_ip(req, client_ip, trusted_proxy) {
        Ok(ip_str) => {
            // Проверяем, что запрос от доверенного маяка, если он указан!
            if let Some(proxy) = trusted_proxy {
                let proxy_ip = IpAddr::from_str(proxy).unwrap_or_else(|_| {
                    warn!("Маяк {} кривой, ставим запасной якорь 127.0.0.1!", proxy);
                    IpAddr::from([127, 0, 0, 1])
                });
                if client_ip != Some(proxy_ip) {
                    let err = format!("Запрос не от нашего маяка {}! Шпион, пушки на тебя!", proxy);
                    warn!("{}", err);
                    let response = Response::builder()
                        .status(StatusCode::FORBIDDEN)
                        .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                        .header(header::SERVER, "YUAI CosmoPort")
                        .body(Full::new(Bytes::from(err)))
                        .unwrap_or_else(|_| {
                            Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Full::new(Bytes::from("Шторм в эфире, шпион утонул!")))
                                .unwrap()
                        });
                    return Err(response);
                }
            }
            info!("Гость {} прошёл досмотр, добро пожаловать на борт!", ip_str);
            Ok(ip_str)
        }
        Err(e) => {
            warn!("Шпион пойман: {}", e);
            let response = Response::builder()
                .status(StatusCode::FORBIDDEN)
                .header(header::CONTENT_TYPE, CONTENT_TYPE_UTF8)
                .header(header::SERVER, "YUAI CosmoPort")
                .body(Full::new(Bytes::from(e)))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Full::new(Bytes::from("Шторм в эфире, шпион утонул!")))
                        .unwrap()
                });
            Err(response)
        }
    }
}

fn get_client_ip<B>(
    req: &Request<B>,
    client_ip: Option<IpAddr>,
    trusted_proxy: Option<&str>,
) -> Result<String, String> {
    // Проверяем сначала, локальный ли это клиент — свой с мостика!
    if let Some(ip) = client_ip {
        if ip.is_loopback() { // 127.0.0.1 или ::1
            let ip_str = ip.to_string();
            info!("Гость {} — свой, с локального мостика, пропускаем без досмотра!", ip_str);
            return Ok(ip_str);
        }
    }

    // Если есть доверенный прокси, роемся в трюме X-Forwarded-For за последним IP!
    if let Some(proxy) = trusted_proxy {
        let forwarded = req.headers()
            .get("X-Forwarded-For")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').last()) // Берём последний IP, так как он добавляется самим прокси!
            .map(|s| s.trim().to_string());

        if let Some(ip_str) = forwarded {
            match IpAddr::from_str(&ip_str) {
                Ok(_) => {
                    info!("Гость {} приплыл через доверенный маяк {}, пропускаем с последней меткой!", ip_str, proxy);
                    return Ok(ip_str);
                }
                Err(_) => {
                    let err = format!("Фальшивая карта IP {} через маяк {}, за борт!", ip_str, proxy);
                    warn!("{}", err);
                    return Err(err);
                }
            }
        } else {
            let err = format!("Маяк {} молчит, записки нет, шторм тебя побери!", proxy);
            warn!("{}", err);
            return Err(err);
        }
    }

    // Если нет прокси, но есть IP, берём его — компас TCP не врёт!
    match client_ip {
        Some(ip) => {
            let ip_str = ip.to_string();
            info!("Гость {} найден по компасу TCP, верный курс!", ip_str);
            Ok(ip_str)
        }
        None => {
            let err = "Ни маяка, ни якоря — шпион без IP, шторм его утащил!".to_string();
            warn!("{}", err);
            Err(err)
        }
    }
}

// Телепорт WebSocket — прыжок в гиперпространство!
async fn handle_websocket(websocket: HyperWebsocket, state: Arc<ProxyState>, ip: String) {
    info!("Открываем телепорт WebSocket, прыжок в гиперпространство для {}!", ip);
    let mut ws = match websocket.await {
        Ok(ws) => ws,
        Err(e) => {
            error!("Телепорт сломался в шторме для {}: {}", ip, e);
            return;
        }
    };
    if std::net::IpAddr::from_str(&ip).is_err() {
        warn!("Гость {} с фальшивой картой IP, телепорт закрыт!", ip);
        if let Err(e) = ws.send(hyper_tungstenite::tungstenite::Message::Text(
            "Фальшивый IP, шторм тебя побери!".to_string()
        )).await {
            error!("Не удалось предупредить шпиона {}: {}", ip, e);
        }
        return;
    }
    while let Some(msg) = tokio::time::timeout(Duration::from_secs(30), ws.next()).await.ok().flatten() {
        if !check_rate_limit(&state, &ip).await {
            warn!("Гость {} слишком шустрый, телепорт трещит!", ip);
            if let Err(e) = ws.send(hyper_tungstenite::tungstenite::Message::Text(
                "Трюм трещит, жди своей очереди, шельмец!".to_string()
            )).await {
                error!("Не удалось остановить шустрого гостя {}: {}", ip, e);
            }
            break;
        }
        match msg {
            Ok(msg) if msg.is_text() || msg.is_binary() => {
                if msg.len() > 64 * 1024 {
                    warn!("Гость {} тащит сундук больше 64 КБ через телепорт!", ip);
                    if let Err(e) = ws.send(hyper_tungstenite::tungstenite::Message::Text(
                        "Сундук слишком тяжёлый, телепорт не тянет!".to_string()
                    )).await {
                        error!("Не удалось отогнать жадного гостя {}: {}", ip, e);
                    }
                    break;
                }
                if let Err(e) = ws.send(msg).await {
                    error!("Телепорт барахлит для {}: {}", ip, e);
                    if let Err(e2) = ws.send(hyper_tungstenite::tungstenite::Message::Text(
                        "Ошибка на сервере, шторм в эфире!".to_string()
                    )).await {
                        error!("Не удалось сообщить о шторме {}: {}", ip, e2);
                    }
                    break;
                }
            }
            Err(e) => {
                error!("Телепорт рухнул для {}: {}", ip, e);
                if let Err(e2) = ws.send(hyper_tungstenite::tungstenite::Message::Text(
                    "Ошибка на сервере, шторм в эфире!".to_string()
                )).await {
                    error!("Не удалось сообщить о крахе {}: {}", ip, e2);
                }
                break;
            }
            _ => {}
        }
    }
}

// Телепорт WebRTC — магия звёзд для связи!
async fn handle_webrtc_offer(offer_sdp: String, state: Arc<ProxyState>, client_ip: String) -> Result<String, String> {
    info!("Гость {} вызывает телепорт WebRTC, зажигаем звёзды!", client_ip);
    if std::net::IpAddr::from_str(&client_ip).is_err() {
        let err = "Фальшивый IP, шторм тебя побери!".to_string();
        warn!("Гость {} с фальшивой картой IP, телепорт закрыт!", client_ip);
        return Err(err);
    }
    if !check_rate_limit(&state, &client_ip).await {
        let err = "Трюм трещит, жди своей очереди, шельмец!".to_string();
        warn!("Гость {} слишком шустрый, звёзды гаснут!", client_ip);
        return Err(err);
    }
    if offer_sdp.len() > 10 * 1024 {
        let err = "Слишком большой сигнал, звёзды не выдержат!".to_string();
        warn!("Гость {} тащит сигнал больше 10 КБ через WebRTC!", client_ip);
        return Err(err);
    }
    let api = APIBuilder::new().build();
    let config = state.config.lock().await;
    let ice_servers: Vec<RTCIceServer> = config.ice_servers.clone().unwrap_or_else(|| {
        warn!("Маяков нет, берём старый STUN-маяк, как запасной фонарь!");
        vec!["stun:stun.l.google.com:19302".to_string()]
    }).iter().map(|url| RTCIceServer {
        urls: vec![url.clone()],
        ..Default::default()
    }).collect();
    let peer_config = webrtc::peer_connection::configuration::RTCConfiguration {
        ice_servers,
        ..Default::default()
    };
    let peer_connection = match api.new_peer_connection(peer_config).await {
        Ok(pc) => Arc::new(pc),
        Err(e) => {
            let err = format!("Ошибка создания WebRTC моста для {}: {}", client_ip, e);
            error!("{}", err);
            return Err(err);
        }
    };
    let data_channel = match peer_connection.create_data_channel("proxy", None).await {
        Ok(dc) => dc,
        Err(e) => {
            let err = format!("Ошибка трубы в трюм для {}: {}", client_ip, e);
            error!("{}", err);
            return Err(err);
        }
    };
    let offer = match RTCSessionDescription::offer(offer_sdp) {
        Ok(offer) => offer,
        Err(e) => {
            let err = format!("Сигнал кривой для {}: {}", client_ip, e);
            error!("{}", err);
            return Err(err);
        }
    };
    if let Err(e) = peer_connection.set_remote_description(offer).await {
        let err = format!("Ошибка метки на карте для {}: {}", client_ip, e);
        error!("{}", err);
        return Err(err);
    }
    let answer = match peer_connection.create_answer(None).await {
        Ok(answer) => answer,
        Err(e) => {
            let err = format!("Ошибка маяка в ответ для {}: {}", client_ip, e);
            error!("{}", err);
            return Err(err);
        }
    };
    if let Err(e) = peer_connection.set_local_description(answer.clone()).await {
        let err = format!("Ошибка флага на мачте для {}: {}", client_ip, e);
        error!("{}", err);
        return Err(err);
    }
    state.webrtc_peers.insert(client_ip.clone(), peer_connection.clone());
    data_channel.on_message(Box::new(move |msg| {
        info!("Гость {} прислал через WebRTC: {:?}", client_ip, msg.data.to_vec());
        Box::pin(async move {})
    }));
    Ok(answer.sdp)
}

// Отмечаем гостя в журнале, как вахтёр на мостике!
async fn manage_session(state: &ProxyState, ip: &str, is_privileged: bool) {
    state.sessions.insert(ip.to_string(), Instant::now());
    if is_privileged {
        state.privileged_clients.insert(ip.to_string(), Instant::now() + Duration::from_secs(3600));
        info!("Гость {} отмечен как привилегированный в журнале, золотой пропуск выдан!", ip);
    } else {
        info!("Гость {} в порту, простой матрос в журнале!", ip);
    }
}

// Проверяем, не перегрузил ли гость трюм!
async fn check_rate_limit(state: &ProxyState, ip: &str) -> bool {
    let config = state.config.lock().await;
    let now = Instant::now();
    let mut entry = state.rate_limits.entry(ip.to_string()).or_insert((now, 0));
    let window = Duration::from_secs(config.rate_limit_window);
    let max_requests = if state.blacklist.contains_key(ip) {
        config.blacklist_rate_limit
    } else if state.whitelist.contains_key(ip) {
        config.whitelist_rate_limit
    } else {
        config.guest_rate_limit
    };
    if now.duration_since(entry.0) > window {
        *entry = (now, 1);
        info!("Трюм {} очищен, начинаем заново!", ip);
        true
    } else if entry.1 < max_requests {
        entry.1 += 1;
        info!("Гость {} добавил добычу, осталось места: {}", ip, max_requests - entry.1);
        true
    } else {
        warn!("Трюм {} полон, жди своей очереди!", ip);
        false
    }
}

// Отправляем все Шлюпки на HTTPS, полный вперёд!
async fn handle_http_request(
    req: Request<hyper::body::Incoming>,
    https_port: u16,
    trusted_host: String,
	state: Arc<ProxyState>,
) -> Result<Response<Full<Bytes>>, Infallible> {
	
	// Пушки на входе! Если шпион — сразу за борт!
    if let Err(response) = restrict_access(&req, None, state.clone()).await {
        return Ok(response);
    }
	
    let redirect_url = format!(
        "https://{}:{}{}",
        trusted_host,
        https_port,
        req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("")
    );
    info!("Шлюпка летит на HTTPS: {}, полный вперёд!", redirect_url);
    match Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY)
        .header(header::LOCATION, redirect_url)
        .header(header::SERVER, "YUAI CosmoPort")
        .body(Full::new(Bytes::new()))
    {
        Ok(resp) => Ok(resp),
        Err(e) => {
            error!("Не удалось собрать шлюпку для редиректа: {}", e);
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::from("Ошибка на сервере, шторм в эфире!")))
                .expect("Резервный ответ для шлюпки не может сломаться, йо-хо-хо!"))
        }
    }
}

// Главная палуба HTTPS для крейсеров!
async fn handle_https_request(
    mut req: Request<hyper::body::Incoming>,
    state: Arc<ProxyState>,
    client_ip: Option<IpAddr>,
) -> Result<Response<Full<Bytes>>, HttpError> {
	// Пушки на входе!
    let ip = match restrict_access(&req, client_ip, state.clone()).await {
        Ok(ip) => ip,
        Err(response) => return Ok(response),
    };
    
    info!("Гость {} ломится в HTTPS, проверяем сундуки!", ip);
    let url = req.uri().to_string();

    match tokio::time::timeout(Duration::from_secs(30), async {
        if let Some(entry) = state.cache.get(&url) {
            if entry.expiry > Instant::now() {
                info!("Добыча для {} найдена в сундуке, выдаём без парусов!", ip);
                return build_response(StatusCode::OK, Bytes::from(entry.response_body.clone()), None);
            }
        }
        if state.blacklist.contains_key(&ip) && !check_rate_limit(&state, &ip).await {
            warn!("Шпион {} в черном списке, трюм полон!", ip);
            return build_response(
                StatusCode::FORBIDDEN,
                Bytes::from("Ты шпион, и трюм полон! Пушки на тебя!"),
                None,
            );
        }
        if !check_rate_limit(&state, &ip).await {
            warn!("Гость {} тащит слишком много, трюм трещит!", ip);
            return build_response(
                StatusCode::TOO_MANY_REQUESTS,
                Bytes::from("Трюм трещит, жди своей очереди, шельмец!"),
                None,
            );
        }
		
        let is_privileged = check_auth(req.headers(), &state, &ip).await;
        manage_session(&state, &ip, is_privileged).await;
        
        if is_upgrade_request(&req) {
            info!("Гость {} прыгает через WebSocket, готовим телепорт!", ip);
            match upgrade(&mut req, None) {
                Ok((response, websocket)) => {
                    tokio::spawn(handle_websocket(websocket, state.clone(), ip.clone()));
                    return Ok(response.map(|_| Full::new(Bytes::new())));
                }
                Err(e) => {
                    error!("Телепорт сломался, шторм и гром: {}", e);
                    return build_response(
                        StatusCode::BAD_REQUEST,
                        Bytes::from("Ошибка на сервере, шторм в эфире!"),
                        None,
                    );
                }
            }
        }
        if req.uri().path() == "/webrtc/offer" {
            info!("Гость {} вызывает WebRTC, зажигаем звёзды!", ip);
            let config = state.config.lock().await;
            let max_body_size = config.max_request_body_size;
            let limited_body = http_body_util::Limited::new(req.into_body(), max_body_size);
            let body = match limited_body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(e) => {
                    warn!("Гость {} закинул слишком большую добычу: {}", ip, e);
                    return build_response(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Bytes::from("Трюм переполнен, шпион! Слишком большая добыча!"),
                        None,
                    );
                }
            };
            let offer_sdp = String::from_utf8_lossy(&body).to_string();
            match handle_webrtc_offer(offer_sdp, state.clone(), ip.clone()).await {
                Ok(answer_sdp) => {
                    return build_response(
                        StatusCode::OK,
                        Bytes::from(answer_sdp),
                        Some(vec![("Content-Type".to_string(), "application/sdp".to_string())]),
                    );
                }
                Err(e) => {
                    error!("Телепорт WebRTC барахлит: {}", e);
                    return build_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Bytes::from("Ошибка на сервере, шторм в эфире!"),
                        None,
                    );
                }
            }
        }
        let locations = state.locations.read().await;
        let path = req.uri().path();
        let location = locations.iter().filter(|loc| path.starts_with(&loc.path)).max_by_key(|loc| loc.path.len());
        
        // Определяем базовый ответ из конфига или 404
        let base_response = location
            .map(|loc| loc.response.clone())
            .unwrap_or_else(|| "404 — звезда не найдена, шторм тебя побери!".to_string());
        
        // Разделяем ответы для обычных и привилегированных пользователей
        let response_body = if is_privileged {
            format!(
                "Привет от капитана, привилегированный гость! Вот твоя добыча: {}",
                base_response
            )
        } else {
            base_response // Обычный ответ без дополнительных приветствий
        };
        
        let response_bytes = Bytes::from(response_body);
        state.cache.insert(url, CacheEntry {
            response_body: response_bytes.to_vec(),
            expiry: Instant::now() + Duration::from_secs(60),
        });
        build_response(
            StatusCode::OK,
            response_bytes,
            location.and_then(|loc| loc.headers.clone()),
        )
    }).await {
        Ok(result) => result,
        Err(_) => {
            warn!("Гость {} слишком долго копался, шторм утащил!", ip);
            build_response(
                StatusCode::REQUEST_TIMEOUT,
                Bytes::from("Запрос утонул в шторме, слишком долго ждали!"),
                None,
            )
        }
    }
}

// Главная палуба HTTP/3 для гиперскоростных звездолетов!
async fn handle_http3_request(
    mut conn: H3Connection<H3QuinnConnection, Bytes>,
    state: Arc<ProxyState>,
    client_ip: SocketAddr,
) {
	// Пушки на входе! 
	// Создаём фиктивный запрос, ведь в HTTP/3 мы работаем с соединением!
    let dummy_req = hyper::Request::new(());
	// Проверяем IP перед тем, как пустить в трюм!
    if let Err(_) = restrict_access(&dummy_req, Some(client_ip.ip()), state.clone()).await {
        return; // Шпион — за борт, ответа не будет!
    }
    info!("Гость прилетел на HTTP/3, полный вперед на гиперскорости!");

    while let Ok(Some((req, mut stream))) = conn.accept().await { // Спасательный круг Ok уже на месте!
        let state_clone = state.clone();
        let ip = client_ip.ip().to_string(); // IP уже проверен
        tokio::spawn(async move {
            let url = req.uri().to_string();
            if let Some(entry) = state_clone.cache.get(&url) {
                if entry.expiry > Instant::now() {
                    info!("Добыча для {} найдена в сундуке, выдаём на гиперскорости!", ip);
                    let resp = match build_h3_response(StatusCode::OK, None) {
                        Ok(resp) => resp,
                        Err(e) => {
                            error!("Не удалось собрать шапку для {}: {}", ip, e);
                            return;
                        }
                    };
                    if let Err(e) = stream.send_response(resp).await {
                        error!("Ошибка отправки шапки HTTP/3 для {}: {}", ip, e);
                        return;
                    }
                    if let Err(e) = stream.send_data(Bytes::from(entry.response_body.clone())).await {
                        error!("Ошибка отправки добычи HTTP/3 для {}: {}", ip, e);
                        return;
                    }
                    if let Err(e) = stream.finish().await {
                        error!("Ошибка завершения шлюза HTTP/3 для {}: {}", ip, e);
                        return;
                    }
                    info!("Гость {} улетел с добычей, гиперскорость на высоте!", ip);
                    return;
                }
            }
            let mut body_bytes: Vec<u8> = Vec::new();
            while let Ok(Some(mut chunk)) = stream.recv_data().await { // Окей, юнга кинул верёвку Result!
                let bytes = chunk.copy_to_bytes(chunk.remaining());
                body_bytes.extend_from_slice(&bytes);
                info!("Гость {} закинул кусок добычи, собираем трюм!", ip);
            }
            if state_clone.blacklist.contains_key(&ip) && !check_rate_limit(&state_clone, &ip).await {
                warn!("Шпион {} в черном списке, трюм полон!", ip);
                let resp = match build_h3_response(StatusCode::FORBIDDEN, None) {
                    Ok(resp) => resp,
                    Err(e) => {
                        error!("Не удалось собрать шапку для шпиона {}: {}", ip, e);
                        return;
                    }
                };
                if let Err(e) = stream.send_response(resp).await {
                    error!("Ошибка отправки шапки шпиону {}: {}", ip, e);
                    return;
                }
                if let Err(e) = stream.send_data(Bytes::from("Шпион, трюм полон!")).await {
                    error!("Ошибка отправки данных шпиону {}: {}", ip, e);
                    return;
                }
                if let Err(e) = stream.finish().await {
                    error!("Ошибка завершения шлюза для шпиона {}: {}", ip, e);
                    return;
                }
                info!("Шпион {} отогнан, пушки сделали своё дело!", ip);
                return;
            }
            if !check_rate_limit(&state_clone, &ip).await {
                warn!("Гость {} слишком шустрый, трюм трещит!", ip);
                let resp = match build_h3_response(StatusCode::TOO_MANY_REQUESTS, None) {
                    Ok(resp) => resp,
                    Err(e) => {
                        error!("Не удалось собрать шапку для шустрого {}: {}", ip, e);
                        return;
                    }
                };
                if let Err(e) = stream.send_response(resp).await {
                    error!("Ошибка отправки шапки шустрому {}: {}", ip, e);
                    return;
                }
                if let Err(e) = stream.send_data(Bytes::from("Трюм трещит, жди!")).await {
                    error!("Ошибка отправки данных шустрому {}: {}", ip, e);
                    return;
                }
                if let Err(e) = stream.finish().await {
                    error!("Ошибка завершения шлюза для шустрого {}: {}", ip, e);
                    return;
                }
                info!("Гость {} притормозил, трюм спасён!", ip);
                return;
            }
            let is_privileged = check_auth(req.headers(), &state_clone, &ip).await;
            manage_session(&state_clone, &ip, is_privileged).await;
            let locations = state_clone.locations.read().await;
            let path = req.uri().path();
            let location = locations.iter().find(|loc| path.starts_with(&loc.path));
            let response_body = location
                .map(|loc| loc.response.clone())
                .unwrap_or_else(|| "404 — звезда не найдена!".to_string());
            let response_bytes = Bytes::from(response_body.clone());
            state_clone.cache.insert(url, CacheEntry {
                response_body: response_bytes.to_vec(),
                expiry: Instant::now() + Duration::from_secs(60),
            });
            let resp = match build_h3_response(StatusCode::OK, location.and_then(|loc| loc.headers.clone())) {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Не удалось собрать шапку для {}: {}", ip, e);
                    return;
                }
            };
            if let Err(e) = stream.send_response(resp).await {
                error!("Ошибка отправки шапки HTTP/3 для {}: {}", ip, e);
                return;
            }
            if let Err(e) = stream.send_data(response_bytes).await {
                error!("Ошибка отправки добычи HTTP/3 для {}: {}", ip, e);
                return;
            }
            if let Err(e) = stream.finish().await {
                error!("Ошибка завершения шлюза HTTP/3 для {}: {}", ip, e);
                return;
            }
            info!("Гость {} забрал добычу по HTTP/3, полный вперёд на звёзды!", ip);
        });
    }
    info!("Гиперскоростной порт HTTP/3 затих, ждём новых гостей!");
}

// Обрабатываем QUIC-соединения, гиперскорость или старый ром!
async fn handle_quic_connection(connection: Connection, state: Arc<ProxyState>) {
	// Пушки на входе
	let client_ip = connection.remote_address();
    let req = hyper::Request::new(());
    if let Err(_) = restrict_access(&req, Some(client_ip.ip()), state.clone()).await {
        return; // Шпион — за борт!
    }	
    info!("Гость на гиперскорости из {:?}", client_ip);
	
    let h3_attempt = H3Connection::new(H3QuinnConnection::new(connection.clone())).await;
    match h3_attempt {
        Ok(h3_conn) => {
            info!("Гость {} использует HTTP/3, переключаемся на гиперскорость!", client_ip);
            handle_http3_request(h3_conn, state, client_ip).await;
        }
        Err(_) => {
            info!("Гость {} использует чистый QUIC, обрабатываем вручную!", client_ip);
            match connection.accept_bi().await {
                Ok((mut send, mut recv)) => {
                    let mut buffer = vec![0; 4096];
                    match recv.read(&mut buffer).await {
                        Ok(Some(n)) => {
                            let response = format!("QUIC добыча: {}", String::from_utf8_lossy(&buffer[..n]));
                            if let Err(e) = send.write_all(response.as_bytes()).await {
                                error!("Ошибка отправки через QUIC для {}: {}", client_ip, e);
                            }
                        }
                        Ok(None) => info!("Гость {} закрыл QUIC поток, ждем новых сигналов!", client_ip),
                        Err(e) => error!("Ошибка чтения QUIC потока для {}: {}", client_ip, e),
                    }
                }
                Err(e) => {
                    warn!("Гость {} не открыл QUIC поток, шторм или разведка? {}", client_ip, e);
                }
            }
        }
    }
}

// Запускаем порт HTTP для шлюпок!
async fn run_http_server(config: Config, state: Arc<ProxyState>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.http_port));
    let listener = match TcpListener::bind(addr).await {
        Ok(listener) => {
            info!(target: "console", "{}", format!("HTTP порт открыт на {}, шлюпки на подходе!", addr).green());
            *state.http_running.write().await = true;
            listener
        }
        Err(e) => {
            error!(target: "console", "{}", format!("Шторм побрал HTTP порт {}: {}", addr, e).red());
            *state.http_running.write().await = false;
            return;
        }
    };
    loop {
        match listener.accept().await {
            Ok((stream, client_ip)) => {
                let stream = TokioIo::new(stream);
                let https_port = config.https_port;
                let trusted_host = config.trusted_host.clone();
				let state_clone = state.clone();
                tokio::spawn(async move {
                    let mut builder = AutoBuilder::new(TokioExecutor::new());
                    builder.http1().max_buf_size(16_384);
                    let service = service_fn(move |req| handle_http_request(req, https_port, trusted_host.clone(), state_clone.clone()));
                    match tokio::time::timeout(Duration::from_secs(10), builder.serve_connection(stream, service)).await {
                        Ok(Ok(())) => info!("Шлюпка {} отработала, курс на HTTPS!", client_ip),
                        Ok(Err(e)) => error!("Шлюпка {} попала в шторм: {}", client_ip, e),
                        Err(_) => warn!("Шлюпка {} слишком медленная, шторм её утащил!", client_ip),
                    }
                });
            }
            Err(e) => error!("Сигнал с моря пропал: {}", e),
        }
    }
}

// Запускаем порт HTTPS для крейсеров!
async fn run_https_server(state: Arc<ProxyState>, config: Config) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.https_port));
	let listener = match TcpListener::bind(addr).await {
		Ok(listener) => {
			info!(target: "console", "{}", format!("HTTPS порт открыт на {}, броня крепка, капитан!", addr).green());
			*state.https_running.write().await = true;
			listener
		}
		Err(e) => {
			error!(target: "console", "{}", format!("Шторм порвал HTTPS порт на {}: {}", addr, e).red());
			*state.https_running.write().await = false;
			return;
		}
	};

	let tls_acceptor = match load_tls_config(&config) {
		Ok(cfg) => TlsAcceptor::from(Arc::new(cfg)),
		Err(e) => {
			error!(target: "console", "{}", format!("Шифры утонули в шторме для HTTPS на {}: {}", addr, e).red());
			return;
		}
	};
    loop {
        match listener.accept().await {
            Ok((stream, client_ip)) => {
                if let Err(e) = stream.set_nodelay(true) {
                    warn!("Крейсер {} барахлит, нет ветра в проводах: {}", client_ip, e);
                }
                let state = state.clone();
                let acceptor = tls_acceptor.clone();
                tokio::spawn(async move {
                    match acceptor.accept(stream).await {
                        Ok(tls_stream) => {
                            let tls_stream = TokioIo::new(tls_stream);
                            let service = service_fn(move |req| handle_https_request(req, state.clone(), Some(client_ip.ip())));
                            let mut builder = AutoBuilder::new(TokioExecutor::new());
                            builder.http1().max_buf_size(16_384);
                            builder.http2().max_frame_size(16_384);
                            match tokio::time::timeout(Duration::from_secs(10), builder.serve_connection(tls_stream, service)).await {
                                Ok(Ok(())) => info!("Крейсер {} отработал, добыча в трюме!", client_ip),
                                Ok(Err(e)) => error!("Крейсер {} попал в шторм: {}", client_ip, e),
                                Err(_) => warn!("Крейсер {} слишком медленный, шторм его утащил!", client_ip),
                            }
                        }
                        Err(e) => error!("TLS-рукопожатие сорвалось для {}: {}", client_ip, e),
                    }
                });
            }
            Err(e) => error!("Сигнал с крейсера пропал: {}", e),
        }
    }
}

// Запускаем порт QUIC для звездолетов!
async fn run_quic_server(config: Config, state: Arc<ProxyState>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], config.quic_port));
	let quinn_config = match load_quinn_config(&config) {
		Ok(cfg) => cfg,
		Err(e) => {
			error!(target: "console", "{}", format!("Шифры для QUIC утонули в шторме на {}: {}", addr, e).red());
			return;
		}
	};

	let endpoint = match Endpoint::server(quinn_config, addr) {
		Ok(endpoint) => {
			info!(target: "console", "{}", format!("QUIC и HTTP/3 порт открыт на {}", addr).green());
			*state.quic_running.write().await = true;
			endpoint
		}
		Err(e) => {
			error!(target: "console", "{}", format!("Не удалось привязать QUIC порт {}: {}", addr, e).red());
			*state.quic_running.write().await = false;
			return;
		}
	};

    while let Some(conn) = endpoint.accept().await {
        let state = state.clone();
        tokio::spawn(async move {
            match conn.await {
                Ok(connection) => {
                    handle_quic_connection(connection, state).await;
                }
                Err(e) => {
                    error!("Ошибка принятия QUIC-соединения на {}: {}", addr, e);
                }
            }
        });
    }
}

// Обновляем карту каждые 60 секунд, новые звезды зовут!
async fn reload_config(state: Arc<ProxyState>) {
    let mut current_config = state.config.lock().await.clone();
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        match load_config().await {
            Ok(new_config) => {
                if current_config != new_config {
                    match validate_config(&new_config) {
                        Ok(()) => {
                            info!(target: "console", "Новая карта, капитан! HTTP={}, HTTPS={}, QUIC={}", 
                                new_config.http_port, new_config.https_port, new_config.quic_port);
                            *state.config.lock().await = new_config.clone();
                            *state.locations.write().await = new_config.locations.clone();
                            if !*state.http_running.read().await {
                                let handle = tokio::spawn(run_http_server(new_config.clone(), state.clone()));
                                *state.http_handle.lock().await = Some(handle);
                                info!("HTTP порт перезапущен с новым курсом!");
                            }
                            if !*state.https_running.read().await {
                                let handle = tokio::spawn(run_https_server(state.clone(), new_config.clone()));
                                *state.https_handle.lock().await = Some(handle);
                                info!("HTTPS порт перезапущен, паруса подняты!");
                            }
                            if !*state.quic_running.read().await {
                                let handle = tokio::spawn(run_quic_server(new_config.clone(), state.clone()));
                                *state.quic_handle.lock().await = Some(handle);
                                info!("QUIC порт перезапущен, гиперскорость включена!");
                            }
                            current_config = new_config;
                        }
                        Err(e) => error!(target: "console", "Новая карта кривая: {}, держим старый курс!", e),
                    }
                }
            }
            Err(e) => error!(target: "console", "Не удалось загрузить новую карту: {}, держим старый курс!", e),
        }
    }
}

// Настраиваем рацию для криков через звезды и записи в трюмные журналы!
pub fn setup_logging() -> Result<(), Box<dyn std::error::Error>> {
    // Сундук для логов: полный журнал рейда с сокровищами данных!
	let file = File::create("yuaiserver.log")?; // Открываем трюм для записи, новый свиток каждый раз!
    let file_layer = fmt::layer()
        .with_writer(file)       // Складываем добычу в сундук, чернила готовы!
        .with_ansi(false)        // Без цветных парусов, только чистый текст в трюме!
        .with_target(true)       // Названия отсеков — чтоб знать, откуда ветер дует!
        .with_level(true)        // Уровень шторма: TRACE — шепот, ERROR — буря!
        .with_thread_ids(true)   // Номера юнг, кто кричал, — порядок на палубе!
        .with_thread_names(true) // Имена юнг, чтоб знать, кто спит на вахте!
        .with_filter(LevelFilter::TRACE); // Хватаем всё, даже шорох парусов!

    // Рация для консоли: короткие вопли юнги с мачты!
    let console_layer = fmt::layer()
        .with_writer(std::io::stdout) // Кричим в эфир через главный рупор!
        .without_time()         // Убираем звёздное время, чтоб не путаться в часах!
        .with_target(false)     // Без названий отсеков, только чистые вести!
        .with_level(false)      // Уровень шума не нужен, орём как есть!
        .with_filter(
            Targets::new()
                .with_target("console", LevelFilter::TRACE) // Орём на мостик любую весть!
                .with_default(LevelFilter::OFF),           // Остальное — молчок, как в штиль!
        );

    // Включаем обе рации: одна орет на мостике, другая пишет в трюм!
    tracing_subscriber::registry()
        .with(console_layer) // Громкоговоритель для мостика, полный вперед!
        .with(file_layer)    // Писарь в трюме, перо скрипит по пергаменту!
        .init();             // Зажигаем факелы, рация трещит, рейд начался!

    // Проверяем, заработала ли рация, кричим в эфир!
    info!(target: "console", "{}", "Рация на мостике, юнга орет:".bright_magenta());
    Ok(()) // Паруса подняты, шторм нас не остановит!
}


// Главная палуба космопорта! Здесь всё начинается!
fn main() -> Result<(), Box<dyn std::error::Error>> {
    
	setup_logging()?; // Настраиваем рацию для криков о победах и бедах!
	
	// Устанавливаем шифры для TLS, как броню на корабль!
    if let Err(e) = rustls::crypto::ring::default_provider().install_default() {
        warn!(target: "console", "{}", format!("Шифры сломаны: {:?}, летим на свой страх и риск!", e).bright_red());
    }

    // Запускаем атомные двигатели на полную! 
    let runtime = match Builder::new_multi_thread().enable_all().build() 
    {
        Ok(rt) => rt,
        Err(e) => {
            error!(target: "console", "Не удалось собрать звездолёт, шторм побери: {}", e);
            return Err(Box::new(e));
        }
    };

    // Взлетаем в гиперпространство, капитан на мостике!
    runtime.block_on(async {
        // Загружаем начальную карту сокровищ, как свиток перед рейдом!
        let initial_config = match setup::setup_config().await {
            Ok(config) => config,
            Err(e) => {
                error!(target: "console", "Ошибка загрузки карты: {}", e);
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Запуск отменён, шторм нас побери: {}", e),
                )) as Box<dyn std::error::Error>);
            }
        };
		
		

        // Собираем штурвал и сундуки для управления космопортом!
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

        // Запускаем шлюпки HTTP в космос через юнгу!
        let http_handle = tokio::spawn(run_http_server(initial_config.clone(), state.clone()));
        *state.http_handle.lock().await = Some(http_handle);
        info!(target: "console", "{}", "Шлюпки HTTP отправлены в рейд, юнга за штурвалом!".magenta());

        // Отправляем бронированные крейсеры HTTPS на орбиту с другим юнгой!
        let https_handle = tokio::spawn(run_https_server(state.clone(), initial_config.clone()));
        *state.https_handle.lock().await = Some(https_handle);
		info!(target: "console", "{}", "Крейсеры HTTPS подняли броню, полный вперёд!".magenta());
		
        // Выпускаем гиперскоростные звездолёты QUIC в галактику с третьим юнгой!
        let quic_handle = tokio::spawn(run_quic_server(initial_config.clone(), state.clone()));
        *state.quic_handle.lock().await = Some(quic_handle);
		info!(target: "console", "{}", "Звездолёты QUIC на гиперскорости, ветер в парусах!".magenta());

        // Юнга чистит трюм от старого хлама каждые 5 минут!
        tokio::spawn(clean_cache(state.clone()));
		info!(target: "console", "{}", "Юнга на вахте, трюм будет сверкать!".magenta());

        // Юнга обновляет карту каждые 60 секунд, как штурман на мостике!
        tokio::spawn(reload_config(state.clone()));
		info!(target: "console", "{}", "Штурман следит за звёздами, карта всегда свежая!".magenta());

        // Врубаем консоль для приказов с капитанского мостика!
        tokio::spawn(console::run_console(state.clone()));
		info!(target: "console", "{}", "Консоль на мостике, капитан кричит команды!".magenta());

        // Ждём, пока все паруса поднимутся и двигатели загудят!
        while !(*state.http_running.read().await && *state.https_running.read().await && *state.quic_running.read().await) {
            tokio::time::sleep(Duration::from_millis(10)).await; // Штиль на 10 мгновений!
        }

        info!(target: "console", "{}", "Космопорт на орбите, полный вперёд!".green());
		
        // Показываем карту и статус на мостике!
        let initial_status = console::get_server_status(&state, true).await;
		info!(target: "console", "{}", format!("{}", initial_status).magenta());

        // Ждём сигнала с мостика (Ctrl+C) для посадки!
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!("Сигнал с мостика пропал в шторме: {}", e);
            return Err(Box::new(e) as Box<dyn std::error::Error>);
        }
        info!(target: "console", "{}", "Пора уходить в гиперпространство, закрываем шлюзы и гасим огни".magenta());

        // Гасим шлюпки, крейсеры и звездолёты!
        if let Some(handle) = state.http_handle.lock().await.take() {
            handle.abort();
            info!("Шлюпки HTTP заглушены, юнга отдыхает!");
        }
        if let Some(handle) = state.https_handle.lock().await.take() {
            handle.abort();
            info!("Крейсеры HTTPS свернули броню, штиль на палубе!");
        }
        if let Some(handle) = state.quic_handle.lock().await.take() {
            handle.abort();
            info!("Звездолёты QUIC приземлились, гиперскорость отключена!");
        }
		
		info!(target: "console", "{}", "Космопорт покинул эту галактику, до новых приключений, капитан!".green());
        Ok::<(), Box<dyn std::error::Error>>(()) // Паруса подняты, шторм позади!
    })?;

    Ok(()) // Космопорт улетел, рейд завершён, йо-хо-хо!
}
