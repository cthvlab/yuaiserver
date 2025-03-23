use std::sync::Arc;
use tokio::io::{self as aio, AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::select;
use crate::{ProxyState, Location, load_config, validate_config};
use tokio::fs; // Для записи в файл
use toml; // Для сериализации в TOML
use tracing::{info, error}; // Рация для капитана: вести журнал событий и кричать о бедах

// Запускаем капитанскую консоль — место, где отдаём приказы!
pub async fn run_console(state: Arc<ProxyState>) {
    let stdin = aio::stdin();
    let mut reader = AsyncBufReader::new(stdin);

    // Сразу показываем список команд через рацию капитана
    print_help().await;

    let mut input = String::new();

    loop {
        select! {
            result = reader.read_line(&mut input) => {
                match result {
                    Ok(0) => {
                        info!("Завершение консоли, капитан отдал приказ!");
                        break;
                    }
                    Ok(_) => {
                        let command = input.trim();
                        match command {
                            "1" => {
                                let status = get_server_status(&state, true).await;
                                info!("Капитан запросил статус космопорта: {}", status);
                            }
                            "2" => {
                                match load_config().await {
                                    Ok(new_config) if validate_config(&new_config).is_ok() => {
                                        *state.config.lock().await = new_config.clone();
                                        *state.locations.write().await = new_config.locations.clone();
                                        info!("Капитан перезагрузил конфигурацию звездного атласа!");
                                    }
                                    _ => {
                                        info!("\x1b[91mОшибка валидации новой карты, капитан!\x1b[0m");
                                    }
                                }
                            }
                            "3" => {
                                if let Some((path, response)) = prompt_new_location(&mut reader).await {
                                    let mut locations = state.locations.write().await;
                                    let mut config = state.config.lock().await;
                                    locations.push(Location { path: path.clone(), response: response.clone() });
                                    config.locations.push(Location { path, response });
                                    // Сохраняем конфигурацию в файл
                                    match save_config(&config).await {
                                        Ok(()) => info!("\x1b[32mКапитан добавил новую локацию в звездный атлас!\x1b[0m"),
                                        Err(e) => info!("\x1b[91mОшибка сохранения звездной карты: {}\x1b[0m", e),
                                    }
                                } else {
                                    info!("Капитан не указал путь или ответ для новой локации, шторм его побери!");
                                }
                            }
                            "4" => {
                                let sessions: Vec<_> = state.sessions.iter().map(|e| e.key().clone()).collect();
                                info!("Капитан запросил список активных сессий: {:?}", sessions);
                            }
                            "help" => {
                                info!("Капитан запросил список команд!");
                                print_help().await;
                            }
                            "" => {}
                            _ => {
                                info!("Капитан ввел неверную команду: {}. Введи 'help' для списка команд!", command);
                            }
                        }
                        // Показываем приглашение для следующего приказа
                        info!("\n> ");
                    }
                    Err(e) => {
                        error!("Ошибка чтения ввода капитана: {}", e);
                        break;
                    }
                }
                input.clear();
            }
        }
    }
}

// Показываем статус сервера
pub async fn get_server_status(state: &ProxyState, full: bool) -> String {
    let config = state.config.lock().await;
    let base_status = format!(
        "\x1b[35m\nСтатус космопорта!:\nСессий: {} Размер кэша: {} Попыток авторизации: {}\x1b[0m",
        state.sessions.len(),
        state.cache.len(),
        state.auth_attempts.len()
    );
    
    if full {
        format!(
            "{}\n\x1b[95mHTTP: {} (порт: {})\nHTTPS: {} (порт: {})\nQUIC: {} (порт: {})\x1b[0m",
            base_status,
            if *state.http_running.read().await { "работает" } else { "\x1b[90mне работает\x1b[0m" },
            config.http_port,
            if *state.https_running.read().await { "работает" } else { "\x1b[90mне работает\x1b[0m" },
            config.https_port,
            if *state.quic_running.read().await { "работает" } else { "\x1b[90mне работает\x1b[0m" },
            config.quic_port
        )
    } else {
        base_status
    }
}

// Выводим список команд через рацию капитана
async fn print_help() {
    info!(
        "\x1b[90m\n=== Список команд ===\n\
         1 - Показать статус\n\
         2 - Перезагрузить конфигурацию\n\
         3 - Добавить локацию\n\
         4 - Показать активные сессии\n\
         help - Показать этот список\n\x1b[0m"
    );
}

// Запрашиваем новую локацию
async fn prompt_new_location(
    reader: &mut AsyncBufReader<tokio::io::Stdin>,
) -> Option<(String, String)> {
    let mut path = String::new();
    let mut response = String::new();

    // Запрашиваем путь для новой звезды
    info!("\nВведите путь (например, /test): ");
    if reader.read_line(&mut path).await.unwrap_or(0) == 0 {
        return None;
    }

    // Запрашиваем добычу для этого пути
    info!("Введите ответ: ");
    if reader.read_line(&mut response).await.unwrap_or(0) == 0 {
        return None;
    }

    let path = path.trim().to_string();
    let response = response.trim().to_string();
    if path.is_empty() || response.is_empty() {
        None // Если путь или ответ пусты, капитан ошибся!
    } else {
        Some((path, response)) // Новая звезда на карте, капитан!
    }
}

// Сохраняем конфигурацию в файл
async fn save_config(config: &crate::Config) -> Result<(), String> {
    // Читаем текущий config.toml, чтобы сохранить существующий порядок
    let current_content = fs::read_to_string("config.toml")
        .await
        .map_err(|e| format!("Не удалось открыть звездную карту: {}", e))?;
    let mut current_config: toml::Value = toml::from_str(&current_content)
        .map_err(|e| format!("Ошибка чтения текущей карты: {}", e))?;

    // Обновляем только поле locations
    let new_locations = config.locations.iter()
        .map(|loc| {
            let mut map = toml::value::Table::new();
            map.insert("path".to_string(), toml::Value::String(loc.path.clone()));
            map.insert("response".to_string(), toml::Value::String(loc.response.clone()));
            toml::Value::Table(map)
        })
        .collect::<Vec<_>>();
    current_config.as_table_mut()
        .ok_or("Звездная карта испорчена, нет таблицы!".to_string())?
        .insert("locations".to_string(), toml::Value::Array(new_locations));

    // Сериализуем обратно в строку
    let toml_string = toml::to_string_pretty(&current_config)
        .map_err(|e| format!("Ошибка сериализации конфигурации: {}", e))?;
    fs::write("config.toml", toml_string)
        .await
        .map_err(|e| format!("Ошибка записи в config.toml: {}", e))?;
    Ok(())
}
