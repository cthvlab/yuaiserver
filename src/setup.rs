// Это модуль для подготовки космопорта — карты и шифры на месте, капитан!
use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::Config; // Импортируем структуру Config из main
use colored::Colorize;

// Дефолтный config.toml как строка
const DEFAULT_CONFIG_TOML: &str = r#"
worker_threads = 16 # Сколько юнг бегает по палубе
trusted_host = "localhost"

http_port = 8888 		# Порт для старых шлюпок (HTTP)
https_port = 8443 		# Порт для бронированных крейсеров (HTTPS)
quic_port = 8444 		# Порт для гиперскоростных звездолетов (QUIC)

# Где лежит сундук с шифрами
cert_path = "cert.pem"
# Где лежит ключ от сундука	
key_path = "key.pem"
# Где лежат свитки рейда	
log_path = "yuaiserver.log"
# Тайный пиратский код для проверки пропусков	
jwt_secret = "password"

# Лимиты скорости — сколько добычи можно утащить за раз!
guest_rate_limit = 10          # Лимит для гостей, что шатаются без дела
whitelist_rate_limit = 100     # Лимит для друзей капитана, что в белом списке
blacklist_rate_limit = 5       # Лимит для шпионов, чтоб не лезли слишком часто
rate_limit_window = 60         # Сколько секунд ждать, пока трюм снова откроется
# Ограничение на добычу в трюме, чтоб шпионы не затопили корабль!
max_request_body_size = 1048576 # 1 МБ — предел трюма для запросов, больше не лезет!

# Маяки для телепортации
ice_servers = ["stun:stun.l.google.com:19302", "stun:stun.example.com:3478"]

# Космический атлас — куда лететь и что отдавать!
[[locations]]
path = "/"
response = "Добро пожаловать в космопорт, пришелец!"
headers = [
    ["Content-Type", "text/plain; charset=utf-8"],
    ["X-Welcome", "Yo-ho-ho"]
]
[[locations]]
path = "/api"
response = "{\"message\": \"Это API endpoint планеты Солярис!\"}"
headers = [
    ["Content-Type", "application/json"]
]
[[locations]]
path = "/static"
response = "Через гиперпространство в Торговый Пост, йо-хо-хо!"
"#;

// Дефолтный самоподписанный сертификат
const DEFAULT_CERT_PEM: &str = r#"-----BEGIN CERTIFICATE-----
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
-----END CERTIFICATE-----
"#;

// Дефолтный приватный ключ
const DEFAULT_KEY_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
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
KKdrsDlYVnmsAs6H0fmxviToaulJUW0=
-----END PRIVATE KEY-----
"#;

// Создаем файлы, если их нет рядом с бинарником!
pub fn ensure_default_files() -> Result<(), String> {
    // Проверяем и создаем config.toml
    if !Path::new("config.toml").exists() {
        tracing::info!(target: "console", "{}", "Пустой запуск, отчаянный звездный покоритель!\nПроверь не блокирует ли имперский флот порты!\nДостаточно ли у тебя прав, открывать космопорт в этой галактике, капитан!".magenta());
		let mut file = File::create("config.toml")
            .map_err(|e| format!("Не могу вырезать карту config.toml, шторм побери: {}", e))?;
        file.write_all(DEFAULT_CONFIG_TOML.as_bytes())
            .map_err(|e| format!("Не могу записать карту config.toml, юнга: {}", e))?;
		tracing::info!(target: "console", "{}", "Картограф нарисовал новую карту config.toml".green());
    }

    // Проверяем и создаем cert.pem
    if !Path::new("cert.pem").exists() {
        let mut file = File::create("cert.pem")
            .map_err(|e| format!("Не могу выковать шифр cert.pem, шторм и гром: {}", e))?;
        file.write_all(DEFAULT_CERT_PEM.as_bytes())
            .map_err(|e| format!("Не могу записать шифр cert.pem, юнга: {}", e))?;
		tracing::info!(target: "console", "{}", "Новый сундук с шифрами cert.pem готов!".green());
    }

    // Проверяем и создаем key.pem
    if !Path::new("key.pem").exists() {
        let mut file = File::create("key.pem")
            .map_err(|e| format!("Не могу выковать ключ key.pem, шторм побери: {}", e))?;
        file.write_all(DEFAULT_KEY_PEM.as_bytes())
            .map_err(|e| format!("Не могу записать ключ key.pem, юнга: {}", e))?;
		tracing::info!(target: "console", "{}", "Новый ключ key.pem для сундука брошен на пляже!".green());
    }
	

    Ok(())
}

// Проверяем, все ли готово для рейда, и возвращаем конфиг!
pub async fn setup_config() -> Result<Config, String> {
    ensure_default_files()?; // Убеждаемся, что файлы на месте
    let config = crate::load_config().await?; // Грузим карту
    crate::validate_config(&config)?; // Проверяем, не кривая ли она
    Ok(config)
}
