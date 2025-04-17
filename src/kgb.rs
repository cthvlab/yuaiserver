// Karma Guardian Balancer (KGB) — Страж равновесия кармы космопорта YUAI! Хулиганы, трепещите, пушки наготове!
use std::net::IpAddr; // Координаты шпионов, как звёзды на карте!
use std::str::FromStr; // Читаем карты IP, как древние свитки!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use std::time::{Duration, Instant}; // Часы капитана, отсчитываем время до шторма!
use dashmap::DashMap; // Быстрые сундуки для пропусков, шпионов и гостей!
use hyper::{Request, Response, StatusCode, header, HeaderMap}; // Двигатель HTTP, запросы и пушки!
use http_body_util::Full; // Упаковка грузов в трюм!
use bytes::Bytes; // Байты — цифровое золото, звенит в трюме!
use tokio::sync::Mutex as TokioMutex; // Замок на сейф с кодексом KGB!
use tracing::{info, warn}; // Рация: кричим о штормах и победах!
use jsonwebtoken::{decode, DecodingKey, Validation}; // Проверяем пропуска, как шпионов на входе!
use serde::{Deserialize, Serialize}; // Читаем и пишем кодексы, как звёздные карты!
use std::collections::HashMap; // Сундук для данных пропусков!

// Кодекс KGB — законы космопорта, как ром для пиратов!
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct KGBConfig {
    pub jwt_secret: String, // Секретный код для пропусков, как ключ от трюма!
    pub guest_rate_limit: u32, // Лимит для простых матросов, сколько добычи в трюм!
    pub whitelist_rate_limit: u32, // Лимит для друзей, больше рома!
    pub blacklist_rate_limit: u32, // Лимит для шпионов, скудная пайка!
    pub rate_limit_window: u64, // Окно вахты, сколько секунд держать лимиты!
    pub trusted_host: String, // Надёжный порт, как родная гавань!
    pub max_request_body_size: usize, // Размер трюма, сколько байт влезет!
    pub http_port: u16, // Причал для шлюпок HTTP!
    pub https_port: u16, // Причал для крейсеров HTTPS!
    pub quic_port: u16, // Причал для звездолётов QUIC!
}

// Главный страж космопорта, пушки наготове!
pub struct KGB {
    pub config: TokioMutex<KGBConfig>, // Сейф с кодексом, под замком!
    pub auth_cache: DashMap<String, AuthCacheEntry>, // Сундук с пропусками, быстрый доступ!
    pub whitelist: DashMap<String, ()>, // Список друзей, как верные юнги!
    pub blacklist: DashMap<String, ()>, // Список шпионов, за борт!
    pub sessions: DashMap<String, (String, Instant)>, // Журнал гостей, кто и где шныряет!
    pub rate_limits: DashMap<String, (Instant, u32)>, // Лимиты трюма, сколько добычи у каждого!
    pub privileged_clients: DashMap<String, Instant>, // Золотые пропуска для элиты!
}

// Пропуск в сундуке, свежий или протухший!
#[derive(Clone)]
pub struct AuthCacheEntry {
    is_valid: bool, // Пропуск годный?
    expiry: Instant, // Когда протухнет!
}

impl KGB {
    // Достаём список шпионов, как чёрные метки!
    pub fn get_blacklist(&self) -> Vec<String> {
        self.blacklist.iter().map(|e| e.key().clone()).collect()
    }
    
    // Создаём нового стража, пушки заряжены!
    pub fn new(config: KGBConfig) -> Arc<Self> {
        Arc::new(Self {
            config: TokioMutex::new(config), // Кодекс в сейфе!
            auth_cache: DashMap::new(), // Пустой сундук для пропусков!
            whitelist: DashMap::new(), // Друзья пока не пришли!
            blacklist: DashMap::new(), // Шпионы ещё не пойманы!
            sessions: DashMap::new(), // Журнал гостей пуст!
            rate_limits: DashMap::new(), // Трюмы чисты!
            privileged_clients: DashMap::new(), // Элита ещё не прибыла!
        })
    }

    // Проверяем гостя, шпион или матрос? Пушки наготове!
    pub async fn protect<B>(
        &self,
        req: &Request<B>,
        client_ip: Option<IpAddr>,
    ) -> Result<String, Response<Full<Bytes>>> {
        let ip = self.restrict_access(req, client_ip).await?; // Проверяем координаты!
        let is_privileged = self.check_auth(req.headers(), &ip).await; // Проверяем пропуск!

        // Трюм трещит? Хулиганы, держитесь!
        if !self.check_rate_limit(&ip).await {
            warn!("Гость {} слишком шустрый, трюм трещит!", ip);
            return Err(self.build_error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "Трюм трещит, жди своей очереди, шельмец!",
            ));
        }

        // Шпион в чёрном списке? Пушки палят!
        if self.blacklist.contains_key(&ip) {
            warn!("Шпион {} в черном списке, пушки наготове!", ip);
            return Err(self.build_error_response(
                StatusCode::FORBIDDEN,
                "Ты шпион, и трюм полон! Пушки на тебя!",
            ));
        }

        // Гость прошёл, записываем в журнал!
        self.manage_session(&ip, req.uri().path(), is_privileged).await;
        info!("Гость {} прошёл стража, добро пожаловать на борт!", ip);
        Ok(ip)
    }

    // Проверяем координаты гостя, шпион или свой?
    async fn restrict_access<B>(
        &self,
        req: &Request<B>,
        client_ip: Option<IpAddr>,
    ) -> Result<String, Response<Full<Bytes>>> {
        match get_client_ip(req, client_ip, None) {
            Ok(ip_str) => {
                info!("Гость {} прошёл досмотр, добро пожаловать!", ip_str);
                Ok(ip_str)
            }
            Err(e) => {
                warn!("Шпион пойман: {}", e);
                Err(self.build_error_response(StatusCode::FORBIDDEN, &e))
            }
        }
    }

    // Проверяем пропуск, золотой или рваный?
    async fn check_auth(&self, headers: &HeaderMap, ip: &str) -> bool {
        // Проверяем сундук с пропусками!
        if let Some(entry) = self.auth_cache.get(ip) {
            if entry.expiry > Instant::now() {
                info!("Пропуск для {} свежий, как ром из бочки!", ip);
                return entry.is_valid;
            } else {
                info!("Пропуск для {} протух, выкидываем за борт!", ip);
                self.auth_cache.remove(ip);
            }
        }

        // Золотой пропуск для элиты?
        if let Some(entry) = self.privileged_clients.get(ip) {
            if *entry > Instant::now() {
                info!("Гость {} с золотым пропуском, добро пожаловать на мостик!", ip);
                return true;
            }
        }

        let config = self.config.lock().await; // Достаём кодекс!
        let auth_header = headers.get("Authorization").and_then(|h| h.to_str().ok());
        let is_jwt_auth = auth_header.map(|auth| {
            auth.starts_with("Bearer ") &&
            decode::<HashMap<String, String>>(
                auth.trim_start_matches("Bearer "), // Читаем JWT!
                &DecodingKey::from_secret(config.jwt_secret.as_ref()), // Ключ от сейфа!
                &Validation::default(),
            ).is_ok()
        }).unwrap_or(false);

        // Сохраняем пропуск в сундук!
        self.auth_cache.insert(ip.to_string(), AuthCacheEntry {
            is_valid: is_jwt_auth,
            expiry: Instant::now() + Duration::from_secs(3600), // Пропуск на час!
        });

        if is_jwt_auth {
            self.privileged_clients.insert(ip.to_string(), Instant::now() + Duration::from_secs(3600)); // Золотой пропуск!
            info!("Гость {} получил золотой пропуск, йо-хо-хо!", ip);
            true
        } else {
            info!("Гость {} без пропуска, заходи как простой матрос!", ip);
            false
        }
    }

    // Проверяем трюм, не переполнен ли добычей?
    async fn check_rate_limit(&self, ip: &str) -> bool {
        let config = self.config.lock().await; // Достаём кодекс!
        let now = Instant::now();
        let mut entry = self.rate_limits.entry(ip.to_string()).or_insert((now, 0)); // Открываем трюм!
        let window = Duration::from_secs(config.rate_limit_window); // Окно вахты!
        let max_requests = if self.blacklist.contains_key(ip) {
            config.blacklist_rate_limit // Скудная пайка для шпионов!
        } else if self.whitelist.contains_key(ip) {
            config.whitelist_rate_limit // Щедрый ром для друзей!
        } else {
            config.guest_rate_limit // Обычная пайка для матросов!
        };

        if now.duration_since(entry.0) > window {
            *entry = (now, 1); // Трюм очищен, новая вахта!
            info!("Трюм {} очищен, начинаем заново!", ip);
            true
        } else if entry.1 < max_requests {
            entry.1 += 1; // Добавляем добычу в трюм!
            info!("Гость {} добавил добычу, осталось места: {}", ip, max_requests - entry.1);
            true
        } else {
            warn!("Трюм {} полон, жди своей очереди!", ip);
            false
        }
    }

    // Записываем гостя в журнал, как вахтенный лог!
    async fn manage_session(&self, ip: &str, path: &str, is_privileged: bool) {
        self.sessions.insert(ip.to_string(), (path.to_string(), Instant::now())); // Пишем в журнал!
        
        info!(target: "KGB", "Гость {} в {}", ip, path);
        if is_privileged {
            self.privileged_clients.insert(ip.to_string(), Instant::now() + Duration::from_secs(3600)); // Золотой пропуск на час!
            info!(target: "KGB", "Гость {} отмечен как привилегированный, золотой пропуск выдан!", ip);
        }
    }

    // Собираем ответ для шпионов, пушки палят!
    fn build_error_response(&self, status: StatusCode, message: &str) -> Response<Full<Bytes>> {
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8") // Текст, как чёрная метка!
            .header(header::SERVER, "YUAI CosmoPort") // Флаг нашего корабля!
            .body(Full::new(Bytes::from(message.to_string()))) // Послание шпиону!
            .unwrap_or_else(|e| {
                warn!("Ошибка сборки ответа: {}. Шторм в эфире!", e);
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Full::new(Bytes::from("Шторм в эфире, шпион утонул!")))
                    .unwrap_or_else(|e_inner| {
                        warn!("Не удалось собрать даже резервный ответ: {}. Возвращаем минимальный!", e_inner);
                        Response::new(Full::new(Bytes::from_static(b"Internal Server Error")))
                    })
            })
    }

    // Достаём кодекс для юнг!
    pub async fn get_config(&self) -> KGBConfig {
        self.config.lock().await.clone()
    }
    
    // Обновляем кодекс, новый закон в космопорте!
    pub async fn update_config(&self, new_config: KGBConfig) {
        *self.config.lock().await = new_config;
    }

    // Считаем гостей в журнале!
    pub fn get_sessions_len(&self) -> usize {
        self.sessions.len()
    }

    // Достаём журнал гостей, кто где шныряет!
    pub fn get_sessions(&self) -> Vec<(String, String)> {
        self.sessions.iter().map(|e| (e.key().clone(), e.value().0.clone())).collect()
    }
}

// Определяем координаты гостя, шпион или свой?
fn get_client_ip<B>(
    req: &Request<B>,
    client_ip: Option<IpAddr>,
    trusted_proxy: Option<&str>,
) -> Result<String, String> {
    // 1. Локальный мостик, свои юнги!
    if let Some(ip) = client_ip {
        if ip.is_loopback() {
            let ip_str = ip.to_string();
            info!("Гость {} — свой, с локального мостика!", ip_str);
            return Ok(ip_str);
        }
    }

    // 2. Прокси-маяк, проверяем записку!
    if let Some(proxy) = trusted_proxy {
        let forwarded = req.headers()
            .get("X-Forwarded-For")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').last())
            .map(str::trim)
            .map(str::to_string);

        if let Some(ip_str) = forwarded {
            if let Ok(parsed) = IpAddr::from_str(&ip_str) {
                info!("Гость {} приплыл через маяк {}", ip_str, proxy);
                return Ok(parsed.to_string());
            } else {
                return Err(format!("Фальшивая карта IP {} через маяк {}", ip_str, proxy));
            }
        } else {
            return Err(format!("Маяк {} молчит, записки нет!", proxy));
        }
    }

    // 3. Прямой компас TCP, надёжный курс!
    if let Some(ip) = client_ip {
        let ip_str = ip.to_string();
        info!("Гость {} найден по компасу TCP!", ip_str);
        return Ok(ip_str);
    }

    // 4. Шпион без координат, за борт!
    Err("Ни маяка, ни якоря — шпион без IP!".to_string())
}
