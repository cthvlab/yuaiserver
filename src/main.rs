// Главная палуба космопорта YUAI! 
mod setup;    // Подготовка звездолёта — паруса на мачты, ром в трюм!
mod console;  // Капитанская консоль, где гремят команды и сверкают звёзды!
mod gui;      // Голографический интерфейс, как карта галактики на ладони!
mod kgb;      // Секретный отсек: шифры, пропуска и пушки на шпионов!
mod tls;      // Броня TLS — защита от космических бурь и пиратов!
mod logging;  // Журнал рейда: каждый шторм и каждая победа записаны!
mod websocket;// Телепорт WebSocket — прыжки через гиперпространство!
mod webrtc;   // Мост WebRTC — связь между звездолётами, быстрее света!
mod servers;  // Двигатели серверов: HTTP, HTTPS, QUIC — полный вперёд!
mod routing;  // Звёздные маршруты, чтоб не заблудиться в туманностях!
mod adapters; // Переходники для грузов, чтоб всё влезло в трюм!
mod maintenance; // Юнги на вахте: чистят трюм, чинят паруса!

use dashmap::DashMap; // Быстрый сундук для добычи, открывается одним взглядом!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use std::time::{Duration, Instant}; // Часы капитана: сколько ждать до следующей звезды?
use tokio::sync::{Mutex as TokioMutex, RwLock}; // Замки: один пишет, все читают — порядок на мостике!
use tokio::runtime::Builder; // Строитель звездолёта, возводит космопорт с нуля!
use colored::Colorize; // Красота в консоли, как звёзды в ночном небе!
use serde::{Deserialize, Serialize}; // Чтение карт из свитков — магия древних!
use std::panic::AssertUnwindSafe; // Страховка от паники, как спасательный круг!
use futures::FutureExt; // Будущее под контролем, звёзды уже близко!
use std::panic; // Ловим панику, как шторм в паруса!
use tracing::{error, warn, info}; // Рация: кричим о штормах и победах!
use crate::servers::{run_http_server, run_https_server, run_quic_server}; // Порты для шлюпок, крейсеров и звездолётов!
use kgb::{KGB, KGBConfig}; // Секретная служба капитана, шпионы не пройдут!
use tls::TlsConfig; // Настройки брони, чтоб шторм не пробил корпус!
use webrtc::RTCPeerConnection; // Мосты для телепортации, связь через галактики!

// Тип маршрута — как звезда на карте: монолит, прокси или микросервис!
#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
enum RouteType {
    Monolith,     // Единый блок, как старый добрый фрегат!
    Proxy,        // Переправа грузов через звёзды!
    Microservice, // Мелкие шлюпки, каждая со своей задачей!
    Dynamic,      // Маршруты, что меняются, как туманности!
    Static,       // Неподвижные звёзды, всегда на месте!
}

impl Default for RouteType {
    fn default() -> Self {
        RouteType::Monolith // По умолчанию — старый фрегат, надёжный как ром!
    }
}

// Локация на звёздной карте: путь, ответ и паруса для ответа!
#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
pub struct Location {
    pub path: String,                       // Путь — "налево у третьей звезды"!
    pub response: String,                   // Добыча для гостей — ром или сокровища!
    pub headers: Option<Vec<(String, String)>>, // Особые паруса для ответа!
    pub route_type: Option<String>,         // Тип маршрута, как метка на карте!
    pub csp: Option<String>,                // Броня безопасности, шпионы не пролезут!
}

impl Default for Location {
    fn default() -> Self {
        Location {
            path: "/".to_string(),             // Главный путь, как вход в космопорт!
            response: "404 — звезда не найдена!".to_string(), // Если звезда пропала — шторм!
            headers: None,                     // Без парусов, чистый ответ!
            route_type: None,                  // Без метки, сами разберёмся!
            csp: None,                         // Без брони, но шпионы всё равно за бортом!
        }
    }
}

// Настройки для неподвижных звёзд — статические файлы и их броня!
#[derive(Deserialize, Serialize, Clone, PartialEq, Debug, Default)]
pub struct StaticSettings {
    pub static_extensions: Option<Vec<String>>, // Типы файлов, как метки на грузах!
    pub default_csp: Option<String>,           // Броня по умолчанию, шпионы не нюхают!
}

// Главный атлас космопорта, все настройки для рейда!
#[derive(Deserialize, Serialize, Clone, PartialEq, Debug)]
pub struct Config {
    pub http_port: u16,               // Порт для шлюпок HTTP, номер причала!
    pub https_port: u16,              // Порт для крейсеров HTTPS, броня крепка!
    pub quic_port: u16,               // Порт для звездолётов QUIC, быстрее света!
    #[serde(flatten)]
    pub tls_config: TlsConfig,        // Броня TLS, шифры как сундук с золотом!
    pub jwt_secret: String,           // Код для пропусков, как печать капитана!
    pub locations: Vec<Location>,     // Маршруты, звёздная карта для гостей!
    pub ice_servers: Option<Vec<String>>, // Маяки для телепортации, фонари в ночи!
    pub guest_rate_limit: u32,        // Лимит для гостей, не наглей, матрос!
    pub whitelist_rate_limit: u32,    // Лимит для друзей, им больше рома!
    pub blacklist_rate_limit: u32,    // Лимит для шпионов, пушки наготове!
    pub rate_limit_window: u64,       // Сколько ждать, пока трюм откроется!
    pub trusted_host: String,         // Надёжный порт для перенаправления!
    pub max_request_body_size: usize, // Лимит трюма, шпионы не затопят!
    pub log_path: String,             // Путь к журналу рейда, свитки приключений!
    pub graphics: Option<String>,     // Режим отображения: консоль, GUI или 3D!
    pub static_settings: Option<StaticSettings>, // Настройки для неподвижных звёzd!
}

impl Default for Config {
    fn default() -> Self {
        Config {
            http_port: 8888,              // Причал для шлюпок, стандартный порт!
            https_port: 8443,             // Причал для крейсеров, броня на месте!
            quic_port: 8444,              // Причал для звездолётов, гиперскорость!
            tls_config: TlsConfig::default(), // Броня по умолчанию, как старый сундук!
            jwt_secret: "password".to_string(), // Код для пропусков, не забудь сменить!
            locations: vec![],            // Карта пуста, звёзды ждут!
            ice_servers: Some(vec!["stun:stun.l.google.com:19302".to_string()]), // Маяки для телепортации!
            guest_rate_limit: 10,         // Гости не наглеют, 10 запросов в минуту!
            whitelist_rate_limit: 100,    // Друзья капитана, ром рекой!
            blacklist_rate_limit: 5,      // Шпионы на цепи, 5 попыток и за борт!
            rate_limit_window: 60,        // Окно лимита — минута, как вахта юнги!
            trusted_host: "localhost".to_string(), // Надёжный порт, как родная гавань!
            max_request_body_size: 1048576, // Трюм на миллион байт, не больше!
            log_path: "yuaiserver.log".to_string(), // Журнал рейда, перо готово!
            graphics: Some("console".to_string()), // Консоль по умолчанию, как старый штурвал!
            static_settings: Some(StaticSettings {
                static_extensions: Some(vec![
                    "html".to_string(), "css".to_string(), "js".to_string(), // Веб-грузы!
                    "png".to_string(), "jpg".to_string(), "jpeg".to_string(), "gif".to_string(), // Картинки звёzd!
                    "ico".to_string(), "svg".to_string(), // Иконки и вектора!
                    "woff".to_string(), "woff2".to_string(), // Шрифты, как письмена!
                ]),
                default_csp: Some("default-src 'self'".to_string()), // Броня для статических звёzd!
            }),
        }
    }
}

impl Config {
    // Достаём броню TLS из сундука, как ключ от гиперпространства!
    fn to_tls_config(&self) -> TlsConfig {
        self.tls_config.clone() // Копия брони, оригинал в сундуке!
    }
}

// Добыча в сундуке, хранится недолго, как ром перед штормом!
#[derive(Clone)]
struct CacheEntry {
    response_body: Vec<u8>, // Байты — цифровое золото в трюме!
    expiry: Instant,        // Когда протухнет, как старый ром!
}

// Штурвал космопорта, всё под контролем капитана!
struct ProxyState {
    config: Arc<RwLock<Config>>,             // Атлас под замком, только для капитана!
    cache: DashMap<String, CacheEntry>,      // Склад добычи, быстрый сундук!
    webrtc_peers: DashMap<String, Arc<RTCPeerConnection>>, // Телепортационные мосты!
    locations: Arc<RwLock<Vec<Location>>>,   // Карта маршрутов, звёзды на месте!
    http_running: Arc<RwLock<bool>>,         // HTTP порт спит или работает?
    https_running: Arc<RwLock<bool>>,        // HTTPS порт на ходу?
    quic_running: Arc<RwLock<bool>>,         // QUIC порт на гиперскорости?
    http_handle: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>, // Рычаг для HTTP шлюпок!
    https_handle: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>, // Рычаг для HTTPS крейсеров!
    quic_handle: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>, // Рычаг для QUIC звездолётов!
    kgb: Arc<KGB>,                           // Секретная служба, шпионы под прицелом!
}

// Читаем карту сокровищ, чтоб знать, куда лететь!
async fn load_config() -> Result<Config, String> {
    let content = match tokio::fs::read_to_string("config.toml").await {
        Ok(content) => content, // Карта в руках, звёзды зовут!
        Err(e) => return Err(format!("Карта пропала в шторме: {}", e)), // Шторм утащил свиток!
    };
    toml::from_str(&content).map_err(|e| format!("Шторм и гром, карта порвана: {}", e)) // Карта рваная, как паруса в бурю!
}

// Проверяем карту, чтоб не врезаться в астероид!
fn validate_config(config: &Config) -> Result<(), String> {
    info!("Проверяем карту, капитан! Все ли звезды на месте?");
    if config.http_port == config.https_port || config.http_port == config.quic_port || config.https_port == config.quic_port {
        return Err(format!("Порты дерутся, как пираты за ром! HTTP={}, HTTPS={}, QUIC={}", 
            config.http_port, config.https_port, config.quic_port)); // Порты не делят причал!
    }
    if !std::path::Path::new(&config.tls_config.cert_path).exists() || !std::path::Path::new(&config.tls_config.key_path).exists() {
        return Err("Сундук с шифрами потерян в черной дыре!".to_string()); // Шифры пропали, шторм их утащил!
    }
    if config.guest_rate_limit == 0 || config.whitelist_rate_limit == 0 || config.blacklist_rate_limit == 0 {
        return Err("Лимиты скорости не могут быть 0, капитан!".to_string()); // Без лимитов — хаос в трюме!
    }
    if config.rate_limit_window == 0 {
        return Err("Окно лимита 0 секунд? Это как ром без бочки!".to_string()); // Лимит без времени — пустая бочка!
    }
    if config.max_request_body_size == 0 || config.max_request_body_size > 1024 * 1024 * 100 {
        return Err("Лимит трюма должен быть от 1 байта до 100 МБ!".to_string()); // Трюм не бездонный!
    }
    match &config.ice_servers {
        Some(servers) if servers.is_empty() => Err("Маяки пусты, как трюм после шторма!".to_string()), // Маяки молчат!
        None => {
            warn!("Маяков нет, берем старый STUN!"); // Старый маяк, как запасной ром!
            Ok(())
        }
        Some(_) => Ok(()), // Маяки светят, курс верный!
    }?;
    // Проверяем режим графики, чтоб не улететь в чёрную дыру!
    if let Some(graphics) = &config.graphics {
        if !["console", "gui", "3d"].contains(&graphics.as_str()) {
            return Err(format!("Недопустимый режим графики: {}. Допустимые: console, gui, 3d", graphics)); // Кривая карта графики!
        }
    }
    Ok(()) // Карта чиста, звёзды на месте!
}

// Обновляем карту каждые 60 секунд, как штурман на вахте!
async fn reload_config(state: Arc<ProxyState>) {
    let result = AssertUnwindSafe(async {
        let mut current_config = match load_config().await {
            Ok(config) => config, // Новая карта, звёзды сияют!
            Err(e) => {
                error!("Ошибка начальной загрузки конфига: {}. Используем дефолт...", e); // Карта пропала, берём запасную!
                Config::default()
            }
        };

        loop {
            tokio::time::sleep(Duration::from_secs(60)).await; // Ждём смены вахты!
            match load_config().await {
                Ok(new_config) => {
                    if validate_config(&new_config).is_err() {
                        warn!("Получена некорректная конфигурация. Пропускаем."); // Карта кривая, держим курс!
                        continue;
                    }
                    if current_config != new_config {
                        info!(target: "console", "Новая карта, капитан! Меняем курс и перезапускаем двигатели!");
                        current_config = new_config.clone(); // Новая карта в руках!
                        *state.config.write().await = new_config.clone(); // Обновляем атлас!
                        *state.locations.write().await = new_config.locations.clone(); // Новые маршруты!
                        state.cache.clear(); // Чистим трюм, старый ром за борт!
                        info!(target: "console", "Кэш очищен, все маршруты готовы к обновлению!");
                        let locations = new_config.locations.iter().map(|loc| loc.path.clone()).collect::<Vec<_>>();
                        info!(target: "console", "Обновленные локации: {:?}", locations); // Показываем новую карту!
                        let kgb_config = KGBConfig {
                            jwt_secret: new_config.jwt_secret.clone(), // Новый код для пропусков!
                            guest_rate_limit: new_config.guest_rate_limit, // Лимиты для гостей!
                            whitelist_rate_limit: new_config.whitelist_rate_limit, // Лимиты для друзей!
                            blacklist_rate_limit: new_config.blacklist_rate_limit, // Лимиты для шпионов!
                            rate_limit_window: new_config.rate_limit_window, // Новое окно вахты!
                            trusted_host: new_config.trusted_host.clone(), // Новый надёжный порт!
                            max_request_body_size: new_config.max_request_body_size, // Новый лимит трюма!
                            http_port: new_config.http_port, // Новый причал для шлюпок!
                            https_port: new_config.https_port, // Новый причал для крейсеров!
                            quic_port: new_config.quic_port, // Новый причал для звездолётов!
                        };
                        state.kgb.update_config(kgb_config).await; // Секретная служба получила новый приказ!
                        shutdown_all(state.clone()).await; // Гасим двигатели, новый курс!
                        *state.http_handle.lock().await = Some(tokio::spawn(run_http_server(new_config.clone(), state.clone()))); // Новые шлюпки!
                        *state.https_handle.lock().await = Some(tokio::spawn(run_https_server(state.clone(), new_config.clone()))); // Новые крейсеры!
                        *state.quic_handle.lock().await = Some(tokio::spawn(run_quic_server(new_config.clone(), state.clone()))); // Новые звездолёты!
                    }
                }
                Err(e) => {
                    warn!("Ошибка загрузки новой конфигурации: {}", e); // Карта пропала в шторме!
                }
            }
        }
    }).catch_unwind().await; // Ловим панику, как шторм в паруса!
    if let Err(e) = result {
        error!("Паника в reload_config: {:?}", e); // Шторм на мостике, держимся!
    }
}

// Гасим все двигатели, космопорт уходит в гиперпространство!
async fn shutdown_all(state: Arc<ProxyState>) {
    if let Some(h) = state.http_handle.lock().await.take() {
        h.abort(); // Шлюпки заглушены!
        info!(target: "console", "{}", "HTTP шлюзы закрыты!".magenta());
    }
    if let Some(h) = state.https_handle.lock().await.take() {
        h.abort(); // Крейсеры встали!
        info!(target: "console", "{}", "HTTPS погасил огни!".magenta());
    }
    if let Some(h) = state.quic_handle.lock().await.take() {
        h.abort(); // Звездолёты приземлились!
        info!(target: "console", "{}", "QUIC ушел в гиперпространство!".magenta());
    }
    tokio::time::sleep(Duration::from_millis(500)).await; // Штиль на миг, прощаемся!
    info!(target: "console", "{}", "Космопорт ушел в другую галактику! До новых приключений, капитан!".bright_magenta());
}

// Главная палуба космопорта! Здесь всё начинается, капитан на мостике!
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Устанавливаем ловушку для паники, как сеть для штормов!
    panic::set_hook(Box::new(|panic_info| {
        error!("Глобальная паника: {:?}", panic_info); // Шторм накрыл, кричим в рацию!
    }));

    // Настраиваем рацию, чтоб орать о победах и бедах!
    if let Err(e) = logging::setup_logging() {
        warn!(target: "console", "{}", format!("Не удалось настроить логирование: {:?}. Продолжаем без логов.", e).bright_red());
    }

    // Грузим шифры, как броню на звездолёт!
    if let Err(e) = rustls::crypto::ring::default_provider().install_default() {
        warn!(target: "console", "{}", format!("Шифры сломаны: {:?}", e).bright_red()); // Шифры треснули, но летим!
    }

    // Строим атомные двигатели для космопорта!
    let runtime = match Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt, // Двигатели гудят, полный вперёд!
        Err(e) => {
            eprintln!("Не удалось создать Tokio Runtime: {}. Завершаем работу.", e); // Двигатели заглохли!
            return Err(Box::new(e));
        }
    };

    // Взлетаем в гиперпространство, капитан на мостике!
    runtime.block_on(async {
        // Загружаем начальную карту, как свиток перед рейдом!
        let initial_config = match setup::setup_config().await {
            Ok(cfg) => cfg, // Карта в руках, звёзды зовут!
            Err(e) => {
                error!("Ошибка загрузки конфигурации: {}. Используем дефолт.", e); // Карта пропала, берём запасную!
                Config::default()
            }
        };

        // Настраиваем секретную службу KGB, шпионы не пройдут!
        let kgb_config = KGBConfig {
            jwt_secret: initial_config.jwt_secret.clone(), // Код для пропусков!
            guest_rate_limit: initial_config.guest_rate_limit, // Лимиты для гостей!
            whitelist_rate_limit: initial_config.whitelist_rate_limit, // Лимиты для друзей!
            blacklist_rate_limit: initial_config.blacklist_rate_limit, // Лимиты для шпионов!
            rate_limit_window: initial_config.rate_limit_window, // Окно вахты!
            trusted_host: initial_config.trusted_host.clone(), // Надёжный порт!
            max_request_body_size: initial_config.max_request_body_size, // Лимит трюма!
            http_port: initial_config.http_port, // Причал для шлюпок!
            https_port: initial_config.https_port, // Причал для крейсеров!
            quic_port: initial_config.quic_port, // Причал для звездолётов!
        };

        let kgb = KGB::new(kgb_config); // Секретная служба на страже!
        let state = Arc::new(ProxyState {
            config: Arc::new(RwLock::new(initial_config.clone())), // Атлас под замком!
            cache: DashMap::new(), // Сундук для добычи!
            webrtc_peers: DashMap::new(), // Мосты для телепортации!
            locations: Arc::new(RwLock::new(initial_config.locations.clone())), // Карта маршрутов!
            http_running: Arc::new(RwLock::new(false)), // HTTP порт спит!
            https_running: Arc::new(RwLock::new(false)), // HTTPS порт спит!
            quic_running: Arc::new(RwLock::new(false)), // QUIC порт спит!
            http_handle: Arc::new(TokioMutex::new(None)), // Рычаг для шлюпок!
            https_handle: Arc::new(TokioMutex::new(None)), // Рычаг для крейсеров!
            quic_handle: Arc::new(TokioMutex::new(None)), // Рычаг для звездолётов!
            kgb, // Секретная служба на борту!
        });

        // Юнги на вахте: следят за серверами и чистят трюм!
        tokio::spawn(maintenance::supervise_servers(initial_config.clone(), state.clone())); // Мониторим двигатели!
        tokio::spawn(maintenance::supervise_task("Clean Cache".to_string(), state.clone(), maintenance::clean_cache)); // Чистим трюм!
        tokio::spawn(maintenance::supervise_task("Reload Config".to_string(), state.clone(), reload_config)); // Обновляем карту!

        info!(target: "console", "{}", "Добро пожаловать в Космопорт!".bright_magenta()); // Космопорт открыт!

        // Ловим сигнал Ctrl+C, как штормовой ветер!
        let shutdown_state = state.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                info!("Ctrl+C пойман. Начинаем отключение..."); // Сигнал пойман, гасим огни!
                shutdown_all(shutdown_state).await; // Закрываем шлюзы!
                std::process::exit(0); // Уходим в гиперпространство!
            }
        });

        // Выбираем режим отображения: консоль, GUI или 3D!
        let args: Vec<String> = std::env::args().collect();
        let use_console = args.iter().any(|a| a == "--console"); // Консольный штурвал?
        let use_3d = args.iter().any(|a| a == "--3d"); // 3D-карта галактики?
        let graphics_mode = if use_console {
            "console".to_string() // Консоль, как старый добрый штурвал!
        } else if use_3d {
            "3d".to_string() // 3D, как голограмма звёzd!
        } else {
            initial_config.graphics.unwrap_or("console".to_string()) // По умолчанию — консоль!
        };

        let result: Result<(), Box<dyn std::error::Error>> = match graphics_mode.as_str() {
            "console" => {
                info!("Запускаем режим консоли..."); // Консоль гудит, команды летят!
                console::run_console(state.clone()).await;
                Ok(())
            }
            "gui" => {
                info!("Запускаем GUI..."); // Голограмма зажглась!
                match AssertUnwindSafe(gui::run_gui(state.clone())).catch_unwind().await {
                    Ok(Ok(())) => {
                        info!("GUI завершился успешно"); // Голограмма погасла без шторма!
                        Ok(())
                    }
                    Ok(Err(e)) => {
                        error!("GUI завершился с ошибкой: {}", e); // Шторм в голограмме!
                        Ok(())
                    }
                    Err(e) => {
                        error!("Паника в GUI: {:?}", e); // Чёрная дыра в голограмме!
                        Ok(())
                    }
                }
            }
            "3d" => {
                info!("Режим 3D пока не реализован, капитан!"); // 3D-карта ещё в трюме!
                Ok(())
            }
            _ => {
                error!("Неизвестный режим графики: {}. Запускаем GUI по умолчанию.", graphics_mode); // Кривая карта, берём GUI!
                match AssertUnwindSafe(gui::run_gui(state.clone())).catch_unwind().await {
                    Ok(Ok(())) => {
                        info!("GUI завершился успешно"); // Голограмма погасла без шторма!
                        Ok(())
                    }
                    Ok(Err(e)) => {
                        error!("GUI завершился с ошибкой: {}", e); // Шторм в голограмме!
                        Ok(())
                    }
                    Err(e) => {
                        error!("Паника в GUI: {:?}", e); // Чёрная дыра в голограмме!
                        Ok(())
                    }
                }
            }
        };

        // Завершаем рейд, гасим огни!
        match result {
            Ok(_) => info!("Интерфейс завершён"), // Паруса свернуты, штиль!
            Err(e) => error!("Интерфейс завершился с ошибкой: {}", e), // Шторм на мостике!
        }

        shutdown_all(state).await; // Закрываем шлюзы, уходим в гиперпространство!
        Ok::<(), Box<dyn std::error::Error>>(()) // Рейд завершён, йо-хо-хо!
    })?;

    Ok(()) // Космопорт улетел, звёзды позади!
}
