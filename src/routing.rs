// 📡 Звёздный маршрутизатор космопорта YUAI! Прокладываем курс через галактику для всех протоколов!
// Поддерживает:
// - HTTP/1, HTTP/2 — через handle_route(Request<Incoming>), как шлюпки в космосе!
// - HTTP/3 (QUIC) — через resolve_route(path, state), вручную, как звездолёты!
// - WebSocket — resolve_route до upgrade или в момент рукопожатия, как маяк для юнг!
// - WebRTC — resolve_route по маршруту до обработки SDP, как сигнал в туманности!
// - QUIC (raw) — resolve_route вручную по URI или контексту, как древняя карта!

use bytes::Bytes; // Цифровое золото, звенит в трюме!
use hyper::{Request, Response, StatusCode, header, Error as HyperError}; // Двигатель HTTP, запросы и пушки!
use http_body_util::Full; // Упаковка грузов в трюм!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use std::time::{Duration, Instant, SystemTime}; // Часы капитана, отсчитываем время до шторма!
use std::collections::HashMap; // Сундук для точных маршрутов!
use tokio::fs::File; // Свиток с добычей, читаем из трюма!
use tokio::io::AsyncReadExt; // Читаем добычу, как древние карты!
use crate::{ProxyState, Config, Location}; // Штурвал космопорта, звёздная карта и звёзды!
use crate::adapters::build_response; // Собираем ответ, как послание в бутылке!
use matchit::Router as TrieRouter; // Древо маршрутов, как звёздная карта с ветвями!

// Маршрутизатор — компас космопорта, указывает путь к звёздам!
#[derive(Clone)]
pub struct Router {
    exact: HashMap<String, Location>, // Точные звёзды, прямой курс!
    prefixes: TrieRouter<Location>, // Префиксы, как туманности с путями!
    static_extensions: Vec<String>, // Метки для статичных грузов, как ром в бочках!
}

impl Router {
    // Создаём новый компас, прокладываем маршруты по звёздной карте!
    pub fn new(locations: Vec<Location>, config: &Config) -> Self {
        let mut exact = HashMap::new(); // Сундук для точных звёzd!
        let mut prefixes = TrieRouter::new(); // Древо для туманностей!

        // Достаём метки для статичных грузов из карты!
        let static_extensions = config.static_settings
            .as_ref()
            .and_then(|s| s.static_extensions.clone())
            .unwrap_or_default();

        // Распределяем звёзды по сундукам и ветвям!
        for loc in locations {
            if loc.route_type.as_deref() == Some("static") && loc.path.ends_with("/*") {
                let prefix = loc.path.trim_end_matches("/*").to_string(); // Обрезаем звёздный шлейф!
                let _ = prefixes.insert(&prefix, loc.clone()); // Ветвь в древе!
            } else {
                exact.insert(loc.path.clone(), loc); // Точная звезда в сундуке!
            }
        }

        Router {
            exact, // Сундук готов!
            prefixes, // Древо расцвело!
            static_extensions, // Метки для рома!
        }
    }

    // Ищем звезду по курсу, точную или в туманности!
    pub fn find(&self, path: &str) -> Option<&Location> {
        if let Some(loc) = self.exact.get(path) {
            return Some(loc); // Прямо в яблочко, звезда найдена!
        }
        if let Ok(matched) = self.prefixes.at(path) {
            if self.is_static_file(path) {
                return Some(matched.value); // Нашли в туманности, груз статичный!
            }
        }
        None // Звезда потеряна в космосе!
    }

    // Проверяем, статичный ли груз, как ром в бочке!
    pub fn is_static_file(&self, path: &str) -> bool {
        self.static_extensions
            .iter()
            .any(|ext| path.ends_with(&format!(".{}", ext))) // Есть метка? Это ром!
    }
}

// Обрабатываем запрос, прокладываем курс через галактику!
pub async fn handle_route(
    req: Request<hyper::body::Incoming>, // Запрос, как послание в бутылке!
    state: Arc<ProxyState>, // Штурвал космопорта!
    _client_ip: Option<std::net::IpAddr>, // Координаты гостя, пока не нужны!
) -> Result<Response<Full<Bytes>>, HyperError> {
    let cache_key = format!("{}:{}", req.method(), req.uri()); // Ключ для сундука с добычей!
    if let Some(entry) = state.cache.get(&cache_key) {
        if entry.expiry > Instant::now() {
            return Ok(Response::new(Full::new(Bytes::from(entry.response_body.clone())))); // Добыча свежая, выдаём!
        }
    }

    let config = state.config.read().await; // Достаём звёздную карту!
    let router = Router::new(config.locations.clone(), &*config); // Создаём компас!
    let path = req.uri().path(); // Курс, куда летим!
    let fallback_location = Location {
        path: "/".into(), // Запасной порт!
        response: "404 — звезда не найдена!".into(), // Чёрная дыра!
        headers: None, // Без парусов!
        route_type: None, // Без метки!
        csp: None, // Без брони!
    };
    let location = router.find(path).cloned().unwrap_or(fallback_location); // Ищем звезду или чёрную дыру!

    match location.route_type.as_deref() {
        Some("static") if router.is_static_file(path) => {
            let file_path = path.trim_start_matches('/'); // Обрезаем шлейф пути!
            let mut file = match File::open(file_path).await {
                Ok(f) => f, // Свиток с добычей открыт!
                Err(_) => {
                    let body = Bytes::from("File not found"); // Свиток пропал!
                    return build_response(StatusCode::NOT_FOUND, body, None);
                }
            };

            let mut response = Response::builder().status(StatusCode::OK); // Готовим послание!
            if let Some(headers) = &location.headers {
                for (name, value) in headers {
                    response = response.header(name.as_str(), value.as_str()); // Добавляем паруса!
                }
            }

            // Определяем тип добычи, как ром или золото!
            let content_type = match path.rsplit('.').next() {
                Some("html") => "text/html",
                Some("css") => "text/css",
                Some("js") => "application/javascript",
                Some("png") => "image/png",
                Some("jpg" | "jpeg") => "image/jpeg",
                _ => "application/octet-stream",
            };
            response = response.header(header::CONTENT_TYPE, content_type); // Метка добычи!

            let mut buffer = Vec::new(); // Трюм для добычи!
            if let Err(_) = file.read_to_end(&mut buffer).await {
                let body = Bytes::from("File read error"); // Шторм в трюме!
                return build_response(StatusCode::INTERNAL_SERVER_ERROR, body, None);
            }
            let body = Bytes::from(buffer); // Добыча в трюме!
            match response.body(Full::new(body)) {
                Ok(resp) => Ok(resp), // Послание отправлено!
                Err(_) => build_response(StatusCode::INTERNAL_SERVER_ERROR, Bytes::from("Response build error"), None), // Шторм в эфире!
            }
        }

        Some("proxy") => {
            let msg = "Proxy not implemented yet, sailor! Rust lacks a universal reverse proxy."; // Прокси-маяк ещё не зажжён!
            build_response(StatusCode::NOT_IMPLEMENTED, Bytes::from(msg), None)
        }

        Some("microservice") => {
            let response_bytes = Bytes::from(location.response.clone()); // Добыча из микрослужбы!
            state.cache.insert(cache_key.clone(), crate::CacheEntry {
                response_body: response_bytes.to_vec(), // Сохраняем в сундук!
                expiry: Instant::now() + Duration::from_secs(60), // Добыча свежа 60 секунд!
            });
            build_response(StatusCode::OK, response_bytes, location.headers.clone()) // Отправляем добычу!
        }

        Some("dynamic") => {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0); // Текущее время, как звезда на небе!
            let response_bytes = Bytes::from(format!("Dynamic time: {}", now)); // Динамическая добыча!
            state.cache.insert(cache_key.clone(), crate::CacheEntry {
                response_body: response_bytes.to_vec(), // Сохраняем в сундук!
                expiry: Instant::now() + Duration::from_secs(60), // Добыча свежа 60 секунд!
            });
            build_response(StatusCode::OK, response_bytes, location.headers.clone()) // Отправляем добычу!
        }

        _ => {
            let response_bytes = Bytes::from(location.response.clone()); // Добыча по умолчанию!
            state.cache.insert(cache_key.clone(), crate::CacheEntry {
                response_body: response_bytes.to_vec(), // Сохраняем в сундук!
                expiry: Instant::now() + Duration::from_secs(60), // Добыча свежа 60 секунд!
            });
            build_response(StatusCode::OK, response_bytes, location.headers.clone()) // Отправляем добычу!
        }
    }
}

// Прокладываем курс вручную, как по звёздной карте!
pub async fn resolve_route(path: &str, state: &Arc<ProxyState>) -> Option<Location> {
    let config = state.config.read().await; // Достаём звёздную карту!
    let router = Router::new(config.locations.clone(), &*config); // Создаём компас!
    router.find(path).cloned() // Ищем звезду и возвращаем её копию!
}
