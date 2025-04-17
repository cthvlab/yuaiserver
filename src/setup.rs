// Это модуль для подготовки космопорта — карты и шифры на месте, капитан!
use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::Config; // Импортируем структуру Config из main
use colored::Colorize;

// Дефолтный config.toml как строка
const DEFAULT_CONFIG_TOML: &str = r#"
trusted_host = "localhost"
# Режим интерфейса: console, gui, 3d
graphics = "gui"

http_port = 8888
https_port = 8443
quic_port = 8444

cert_path = "cert.pem"
key_path = "key.pem"
log_path = "yuaiserver.log"
jwt_secret = "password"

guest_rate_limit = 10
whitelist_rate_limit = 100
blacklist_rate_limit = 5
rate_limit_window = 60
max_request_body_size = 1048576

ice_servers = ["stun:stun.l.google.com:19302", "stun:stun.example.com:3478"]

# Глобальные настройки статики
[static_settings]
static_extensions = ["html", "css", "js", "png", "jpg", "jpeg", "gif", "ico", "svg", "woff", "woff2"]
default_csp = "default-src 'self'"

# Космический атлас — куда лететь и что отдавать!
[[locations]]
path = "/"
response = "Добро пожаловать в космопорт, пришелец!"
route_type = "monolith"
csp = "default-src 'none'"
headers = [
    ["Content-Type", "text/plain; charset=utf-8"],
    ["X-Welcome", "Yo-ho-ho"]
]

[[locations]]
path = "/api"
response = "{\"message\": \"Это API endpoint планеты Солярис!\"}"
route_type = "microservice"
headers = [
    ["Content-Type", "application/json"]
]

[[locations]]
path = "/static"
response = "Через гиперпространство в Торговый Пост, йо-хо-хо!"
route_type = "static"
"#;

// Дефолтный самоподписанный сертификат
const DEFAULT_CERT_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIDODCCAiCgAwIBAgIUFytf5mXN0XjloQyk42U9TtDsGtUwDQYJKoZIhvcNAQEL
BQAwFDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI1MDQxMTA1NDI0OFoXDTI2MDQx
MTA1NDI0OFowFDESMBAGA1UEAwwJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEA2V/18SPjiU6XeibQ9u8H9eWdwjRGvNwh459UMfZbEYT7
FQeLsGvnw0in8y8ZqBCA+j/1R1ImI+IFA7x/jrLPKJMmtv+89m2XLMuJqg/HDVF0
a9rcoui/IDSfPsxhdOqyhBuT4SeEd8+Oxe2wFLOOzA12iAqHYkGbm98pViOv7mbP
Zp+INhP5YrIENXsTUKgBYYEs0tAiJ4FU5Zcq/Hx1DNHdYReZ6mIQMwT3QxUTTdv4
U470e3a7vfqbA106qgK0Gx7ImFbEgotgFlco4nwJ+zaBUpUlhdaTnD9aUPFhBRob
4sN1R7ysln/2i6UXUQL8tr6W33VM5gix3lnlmyyLpQIDAQABo4GBMH8wHQYDVR0O
BBYEFHHezmlPDItLpfTBZ4YfAQp5m8+CMB8GA1UdIwQYMBaAFHHezmlPDItLpfTB
Z4YfAQp5m8+CMA8GA1UdEwEB/wQFMAMBAf8wLAYDVR0RBCUwI4IJbG9jYWxob3N0
hwR/AAABhxAAAAAAAAAAAAAAAAAAAAABMA0GCSqGSIb3DQEBCwUAA4IBAQAk2jiK
Dv2dnSJ5zgE/nqK/zg94HbHZFpqie77oiVoktWaHkzBbQTJ1kuyorTH0Rq8ZHI1V
Eny4tYJUsRNKnJgeGShPRX68HnFB3DBwTbW8oKK6lrZo+siEUWENdHRUb4bNg79K
gjIu3na/V3YW1+soAV0id8Ow9Xkxz3aU1t5PURV8c5Bdhw2jPeax+l67TiF9+aF6
tzJFloEnQWTB4ZHnFYomkHmB12pqUMR6k8lAWhqcm2v10m3u6IFC1f4rnPouJ/PI
a8mgeQleNPoF7QlFj50Q7j+vuOBeZuUgThHgX9erCG1TPmWGNs/89FA97zAe3qK2
Bwx/o7Xh50IcJ9LX
-----END CERTIFICATE-----
"#;

// Дефолтный приватный ключ
const DEFAULT_KEY_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDZX/XxI+OJTpd6
JtD27wf15Z3CNEa83CHjn1Qx9lsRhPsVB4uwa+fDSKfzLxmoEID6P/VHUiYj4gUD
vH+Oss8okya2/7z2bZcsy4mqD8cNUXRr2tyi6L8gNJ8+zGF06rKEG5PhJ4R3z47F
7bAUs47MDXaICodiQZub3ylWI6/uZs9mn4g2E/lisgQ1exNQqAFhgSzS0CIngVTl
lyr8fHUM0d1hF5nqYhAzBPdDFRNN2/hTjvR7dru9+psDXTqqArQbHsiYVsSCi2AW
VyjifAn7NoFSlSWF1pOcP1pQ8WEFGhviw3VHvKyWf/aLpRdRAvy2vpbfdUzmCLHe
WeWbLIulAgMBAAECggEAFI4z06RxKoAkEt5YzwELilU29du7qgr8VBSyVtx/qshz
IH9LgQNd3I73qCdsEFh/LyryfCwTLzwlp+ogpdMtg8i71dJD1t8bd0DyuQIvmzLO
BOT4DZpfeHbWwuQmKHLehApCPFMK/nQXgYqt0UdULuJBszD2XHRCeBrs7cMHf/j2
6D7xebUQwm/MjGth1Mcm6xMRpoA3gpv1C9H9hBv1GiZmUdk5No6JkPQTrXtT5Fvu
Bo406Knj5vPhgE+j6b32jtHdCHICQznPdmQVjKYMxlfAG5HNjVWcF20rfyqO7wfy
8uhS4PORPFwjhkU32FHZ8i+ALunkA3TGxtstHTkGgQKBgQD1YuTnyZZnzqUHkKYH
AZriKh6VRHds91HhaQya5fO7C2e8NqnVH4yJwq9Ig7xncwczjSGYRuV9PHfgcUOa
ivZ89D13qeWGS6UdcF5Tc+99h4bOO4MncxkdQIDMS47RwCJ8WrxJ8TdjvP4hlZex
XvY7iB1/1XwtiUhLBNFD2UHLlwKBgQDixubyrsD94ZOjWrNptqqZ1kGuSxMsCe2z
77Fkb15EU/8KIhNs5dUUVuBJVy5QqNQCW/Pvv9pO/ouh8XJ0xfcNpW5D8qr2ZcRx
IBjyQxwFjQ4qlMnLcAVVp5bHO40obeGCqbXbM+LTbXGIomoBlgAadg9eC1HJVK15
GHYuoQ66IwKBgFnTB6PpYQtC05o/UudBqSac8HEHjQfwSKLQx3J0NpIDjMeS4vxG
/jT3dR6ASpk7vCkcwm1xllQPrHoWO/74W15GMbH6GTDdw+VQ2taFm+dBkxEvK8Vn
FoxkrkEab39Ma9NFi6Mtj28NIaG9lrew4UXNf67pwPkSRcpgSxIhFzqlAoGBALKl
1lHf8REPn2rUjPn+eK7v5pYTdmr+908EyE5CnZReR1AIQB1NgWqgddfQ4h+QoFOr
dKOuE6CfTgipWG09dr49UHwesyegS/yCOKTA0VZeJIbO55loCgvMFi+lbjInPbvd
TfH9EfdVBFeK+s99B1/RGZIQgMGF/5Kh/pgFMMDZAoGAG4Z9eZR1ga2Px98mgLMv
L82hlA+d+2yKkUsq8clDDhoyZ8WeZdi/A26LN2s4UvHmzoCrLVhae4RG9dcnTa7n
JfPiUGe2kn95PHTOaY4EGgiEQCTFNKCnRxd/jTQAinft8bLv08VggzyksN16NVb6
mp0YDseTSs+fAtb0X/SQiu4=
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
