use std::sync::Arc;
use tokio::io::{self as aio, AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::select;
use crate::{ProxyState, Location, load_config, validate_config};
use tokio::fs; // Для записи в файл
use toml; // Для сериализации в TOML
use tracing::{info, error}; // Рация для капитана: вести журнал событий и кричать о бедах
use colored::Colorize; // Красота в консоли

// Запускаем капитанскую консоль — место, где отдаём приказы!
pub async fn run_console(state: Arc<ProxyState>) {
    let stdin = aio::stdin();
    let mut reader = AsyncBufReader::new(stdin);
    
    print_help().await; // Сразу показываем список команд через рацию капитана

    let mut input = String::new();

    loop {
        select! {
            result = reader.read_line(&mut input) => {
                match result {
                    Ok(0) => {
                        info!(target: "console", "Завершение консоли, капитан отдал приказ!");
                        break;
                    }
                    Ok(_) => {
                        let command = input.trim();
                        match command {
                            "1" => {
                                let status = get_server_status(&state, true).await;
                                info!(target: "console", "{}", format!("{}", status).bright_magenta());
                            }
                            "2" => {
                                match load_config().await {
                                    Ok(new_config) if validate_config(&new_config).is_ok() => {
                                        *state.config.lock().await = new_config.clone();
                                        *state.locations.write().await = new_config.locations.clone();
                                        info!(target: "console", "Капитан перезагрузил конфигурацию звездного атласа!");
                                    }
                                    _ => {
                                        info!(target: "console", "Ошибка валидации новой карты, капитан!");
                                    }
                                }
                            }
                            "3" => {
                                if let Some((path, response)) = prompt_new_location(&mut reader).await {
                                    let mut locations = state.locations.write().await;
                                    let mut config = state.config.lock().await;
                                    locations.push(Location { path: path.clone(), response: response.clone(), headers: None, });
                                    config.locations.push(Location { path, response, headers: None, });
                                    // Сохраняем конфигурацию в файл
                                    match save_config(&config).await {
                                        Ok(()) => info!(target: "console", "Капитан добавил новую локацию в звездный атлас!"),
                                        Err(e) => info!(target: "console", "Ошибка сохранения звездной карты: {}", e),
                                    }
                                } else {
                                    info!(target: "console", "Капитан не указал путь или ответ для новой локации, шторм его побери!");
                                }
                            }
                            "4" => {
                                let sessions: Vec<_> = state.sessions.iter().map(|e| e.key().clone()).collect();
                                info!(target: "console", "Капитан запросил список активных сессий: {:?}", sessions);
                            }
                            "help" => {
                                info!(target: "console", "Капитан запросил список команд!");
                                print_help().await;
                            }
                            "" => {}
                            _ => {
                                info!(target: "console", "Капитан ввел неверную команду: {}. Введи 'help' для списка команд!", command);
                            }
                        }
                        // Показываем приглашение для следующего приказа
                       // info!(target: "console", "\n ");
                    }
                    Err(e) => {
                        error!(target: "console", "Ошибка чтения ввода капитана: {}", e);
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
        "Статус космопорта:\nСессий: {} Размер кэша: {} Попыток авторизации: {}",
        state.sessions.len(),
        state.cache.len(),
        state.auth_attempts.len()
    );
    
    if full {
        format!(
            "{}\nHTTP: {} (порт: {})\nHTTPS: {} (порт: {})\nQUIC: {} (порт: {})",
            base_status,
            if *state.http_running.read().await { "работает" } else { "\x1b[31mне работает\x1b[0m" },
            config.http_port,
            if *state.https_running.read().await { "работает" } else { "\x1b[31mне работает\x1b[0m" },
            config.https_port,
            if *state.quic_running.read().await { "работает" } else { "\x1b[31mне работает\x1b[0m" },
            config.quic_port
        )
    } else {
        base_status
    }
}

// Выводим список команд через рацию капитана
async fn print_help() {
    println!(
        "{}",
        "\n=== Карта команд, капитан! ===\n\
         1 - Взглянуть на статус космопорта\n\
         2 - Перегрузить звездную карту\n\
         3 - Нанести новую звезду на карту\n\
         4 - Кто шныряет по палубе?\n\
         help - Выдать эту шпаргалку!\n".yellow()
    );
}

// Запрашиваем новую локацию
async fn prompt_new_location(
    reader: &mut AsyncBufReader<tokio::io::Stdin>,
) -> Option<(String, String)> {
    let mut path = String::new();
    let mut response = String::new();

    // Запрашиваем путь для новой звезды
    info!(target: "console", "Введите путь (например, /test): ");
    if reader.read_line(&mut path).await.unwrap_or(0) == 0 {
        return None;
    }

    // Запрашиваем добычу для этого пути
    info!(target: "console", "Введите ответ: ");
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
