// Капитанская консоль космопорта YUAI! Здесь отдаются приказы, а звёзды слушают!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use tokio::io::{self as aio, AsyncBufReadExt, BufReader as AsyncBufReader}; // Рация для команд, слушаем капитана!
use tokio::select; // Переключатель вахт, чтоб не пропустить приказ!
use crate::{ProxyState, Location, load_config, validate_config, shutdown_all, run_http_server, run_https_server, run_quic_server, KGBConfig}; // Всё для управления космопортом!
use tokio::fs; // Свитки с картами, читаем и пишем!
use toml; // Читаем звёздные карты, как древние свитки!
use tracing::{info, error}; // Рация: кричим о штормах и победах!
use colored::Colorize; // Красота в консоли, как звёзды в ночном небе!

// Главная палуба консоли, капитан отдаёт приказы!
pub async fn run_console(state: Arc<ProxyState>) {
    let stdin = aio::stdin(); // Рация капитана, слушаем приказы!
    let mut reader = AsyncBufReader::new(stdin); // Юнга на рации, читает команды!
    
    print_help().await; // Показываем карту команд, как звёзды на небе!

    let mut input = String::new(); // Свиток для записи приказов!

    loop {
        select! {
            result = reader.read_line(&mut input) => {
                match result {
                    Ok(0) => {
                        info!(target: "console", "Консоль закрыта, капитан!"); // Рация замолчала, штиль!
                        break;
                    }
                    Ok(_) => {
                        let command = input.trim(); // Читаем приказ, без лишних волн!
                        match command {
                            "1" => {
                                let status = get_server_status(&state, true).await; // Смотрим, как работает космопорт!
                                info!(target: "console", "{}", format!("{}", status).bright_magenta()); // Докладываем в ярких красках!
                            }
                            "2" => {
                                let sessions = state.kgb.get_sessions(); // Кто шныряет по палубе?
                                info!(target: "console", "Активные гости: {:?}", sessions); // Доклад о гостях!
                            }
                            "3" => {
                                let sessions_len = state.kgb.get_sessions_len(); // Сколько груза в трюме?
                                info!(target: "console", "Соединений в трюме: {}", sessions_len); // Считаем юнг!
                            }
                            "4" => {
                                let blacklisted = state.kgb.get_blacklist(); // Кто в чёрном списке?
                                info!(target: "console", "Хулиганы в чёрном списке: {:?}", blacklisted); // Шпионы под прицелом!
                            }
                            "5" => {
                                // Меняем порты, как паруса на ветру!
                                if let Some((http, https, quic)) = prompt_ports(&mut reader).await {
                                    let mut kgb_config = state.kgb.get_config().await; // Достаём коды из сейфа KGB!
                                    kgb_config.http_port = http; // Новый причал для шлюпок!
                                    kgb_config.https_port = https; // Новый причал для крейсеров!
                                    kgb_config.quic_port = quic; // Новый причал для звездолётов!
                                    state.kgb.update_config(kgb_config.clone()).await; // Секретная служба получила приказ!
                                    save_and_restart(state.clone(), kgb_config, None).await; // Сохраняем карту и перезапускаем двигатели!
                                    info!(target: "console", "Порты изменены: HTTP={}, HTTPS={}, QUIC={}", http, https, quic); // Новый курс принят!
                                } else {
                                    info!(target: "console", "Порты не заданы, капитан!"); // Курс не ясен, стоим!
                                }
                            }
                            "6" => {
                                // Меню лимитов, как ром в бочках!
                                print_limits_submenu(&state).await; // Показываем карту лимитов!
                                let mut sub_input = String::new(); // Новый свиток для подкоманд!
                                loop {
                                    info!(target: "console", "Выбери лимит (1-4, h, cancel): "); // Запрашиваем приказ!
                                    sub_input.clear(); // Чистим свиток!
                                    match reader.read_line(&mut sub_input).await {
                                        Ok(0) => {
                                            info!(target: "console", "Консоль закрыта, капитан!"); // Рация замолчала!
                                            return;
                                        }
                                        Ok(_) => {
                                            let sub_command = sub_input.trim(); // Читаем подприказ!
                                            match sub_command {
                                                "1" | "2" | "3" | "4" => {
                                                    // Меняем лимиты, как грузы в трюме!
                                                    if let Some((guest, whitelist, blacklist, body_size)) = prompt_limits(&mut reader, sub_command).await {
                                                        let mut kgb_config = state.kgb.get_config().await; // Достаём коды!
                                                        if let Some(g) = guest {
                                                            kgb_config.guest_rate_limit = g; // Новый лимит для гостей!
                                                        }
                                                        if let Some(w) = whitelist {
                                                            kgb_config.whitelist_rate_limit = w; // Новый лимит для друзей!
                                                        }
                                                        if let Some(b) = blacklist {
                                                            kgb_config.blacklist_rate_limit = b; // Новый лимит для шпионов!
                                                        }
                                                        if let Some(s) = body_size {
                                                            kgb_config.max_request_body_size = s; // Новый размер трюма!
                                                        }
                                                        state.kgb.update_config(kgb_config.clone()).await; // Секретная служба на страже!
                                                        save_and_restart(state.clone(), kgb_config, None).await; // Сохраняем и перезапускаем!
                                                        info!(target: "console", "Лимит обновлён, капитан!"); // Лимиты на месте!
                                                    }
                                                    print_limits_submenu(&state).await; // Показываем карту снова!
                                                }
                                                "h" => {
                                                    print_limits_submenu(&state).await; // Показываем карту лимитов!
                                                }
                                                "cancel" | "" => {
                                                    info!(target: "console", "Возвращаемся в главное меню, капитан!"); // Назад к звёздам!
                                                    break;
                                                }
                                                _ => {
                                                    info!(target: "console", "Неизвестная команда: {}. Введи 'h' для карты!", sub_command); // Кривая команда!
                                                    print_limits_submenu(&state).await; // Показываем карту снова!
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            error!(target: "console", "Ошибка ввода: {}. Остаёмся в порту!", e); // Шторм в рации!
                                            print_limits_submenu(&state).await; // Показываем карту!
                                        }
                                    }
                                }
                            }
                            "7" => {
                                // Перезагружаем звёздную карту!
                                match load_config().await {
                                    Ok(new_config) if validate_config(&new_config).is_ok() => {
                                        let mut kgb_config = state.kgb.get_config().await; // Достаём текущие коды!
                                        kgb_config.jwt_secret = new_config.jwt_secret.clone(); // Новый код для пропусков!
                                        kgb_config.guest_rate_limit = new_config.guest_rate_limit; // Лимиты для гостей!
                                        kgb_config.whitelist_rate_limit = new_config.whitelist_rate_limit; // Лимиты для друзей!
                                        kgb_config.blacklist_rate_limit = new_config.blacklist_rate_limit; // Лимиты для шпионов!
                                        kgb_config.rate_limit_window = new_config.rate_limit_window; // Новое окно вахты!
                                        kgb_config.trusted_host = new_config.trusted_host.clone(); // Новый надёжный порт!
                                        kgb_config.max_request_body_size = new_config.max_request_body_size; // Новый размер трюма!
                                        kgb_config.http_port = new_config.http_port; // Новый причал для шлюпок!
                                        kgb_config.https_port = new_config.https_port; // Новый причал для крейсеров!
                                        kgb_config.quic_port = new_config.quic_port; // Новый причал для звездолётов!
                                        state.kgb.update_config(kgb_config.clone()).await; // Секретная служба обновлена!
                                        *state.locations.write().await = new_config.locations.clone(); // Новая карта маршрутов!
                                        save_and_restart(state.clone(), kgb_config, Some(new_config.locations.clone())).await; // Сохраняем и перезапускаем!
                                        info!(target: "console", "Конфиг перезагружен, капитан!"); // Новая карта в руках!
                                    }
                                    _ => {
                                        info!(target: "console", "Ошибка в новой карте, капитан!"); // Карта рваная, шторм!
                                    }
                                }
                            }
                            "8" => {
                                // Перезапускаем двигатели, полный вперёд!
                                shutdown_all(state.clone()).await; // Гасим все шлюзы!
                                let config = load_config().await.unwrap_or_default(); // Берём карту или запасную!
                                *state.http_handle.lock().await = Some(tokio::spawn(run_http_server(config.clone(), state.clone()))); // Новые шлюпки!
                                *state.https_handle.lock().await = Some(tokio::spawn(run_https_server(state.clone(), config.clone()))); // Новые крейсеры!
                                *state.quic_handle.lock().await = Some(tokio::spawn(run_quic_server(config.clone(), state.clone()))); // Новые звездолёты!
                                info!(target: "console", "Сервера перезапущены, капитан!"); // Двигатели гудят!
                            }
                            "9" => {
                                // Новая звезда на карте!
                                if let Some((path, response)) = prompt_new_location(&mut reader).await {
                                    let mut locations = state.locations.write().await; // Достаём карту!
                                    locations.push(Location { 
                                        path: path.clone(), // Новый путь, как звезда!
                                        response: response.clone(), // Добыча для гостей!
                                        headers: None, // Без парусов!
                                        route_type: None, // Без метки!
                                        csp: None, // Без брони!
                                    });
                                    let kgb_config = state.kgb.get_config().await; // Достаём коды!
                                    save_and_restart(state.clone(), kgb_config, Some(locations.clone())).await; // Сохраняем и перезапускаем!
                                    info!(target: "console", "Новая звезда на карте: {}!", path); // Звезда зажглась!
                                } else {
                                    info!(target: "console", "Путь или ответ пусты, капитан!"); // Кривая звезда, шторм!
                                }
                            }
                            "h" => {
                                print_help().await; // Показываем карту команд!
                            }
                            "" => {} // Пустой приказ, стоим!
                            _ => {
                                info!(target: "console", "Неизвестная команда: {}. Введи 'h' для карты!", command); // Кривая команда!
                            }
                        }
                    }
                    Err(e) => {
                        error!(target: "console", "Ошибка ввода: {}", e); // Шторм в рации!
                        break;
                    }
                }
                input.clear(); // Чистим свиток для нового приказа!
            }
        }
    }
}

// Смотрим, как работает космопорт, капитан!
pub async fn get_server_status(state: &ProxyState, full: bool) -> String {
    let config = state.kgb.get_config().await; // Достаём коды из сейфа!
    let base_status = format!(
        "Статус космопорта:\nСессий: {} Размер кэша: {}",
        state.kgb.get_sessions_len(), // Считаем гостей!
        state.cache.len() // Считаем добычу в сундуке!
    );
    
    if full {
        format!(
            "{}\nHTTP: {} (порт: {})\nHTTPS: {} (порт: {})\nQUIC: {} (порт: {})",
            base_status,
            if *state.http_running.read().await { "работает".green() } else { "не работает".red() }, // Шлюпки на ходу?
            config.http_port, // Причал шлюпок!
            if *state.https_running.read().await { "работает".green() } else { "не работает".red() }, // Крейсеры на ходу?
            config.https_port, // Причал крейсеров!
            if *state.quic_running.read().await { "работает".green() } else { "не работает".red() }, // Звездолёты на ходу?
            config.quic_port // Причал звездолётов!
        )
    } else {
        base_status // Короткий доклад, как штормовой сигнал!
    }
}

// Показываем карту команд, как звёзды в ночи!
async fn print_help() {
    println!(
        "{}",
        "\n=== Карта команд, капитан! ===\n\
         1 - Статус космопорта\n\
         2 - Кто шныряет по палубе\n\
         3 - Сколько держим соединений\n\
         4 - Кто хулиганит?\n\
         5 - Изменить порты\n\
         6 - Установить лимиты (гостевой/белый/чёрный/размер запроса)\n\
         7 - Перезагрузить конфиг\n\
         8 - Перезапустить сервера\n\
         9 - Нанести новую звезду на карту\n\
         h - Выдать эту шпаргалку!\n".yellow()
    );
}

// Показываем подменю лимитов, как ром в бочках!
async fn print_limits_submenu(state: &Arc<ProxyState>) {
    let config = state.kgb.get_config().await; // Достаём коды!
    println!(
        "{}",
        format!(
            "\n=== Подменю лимитов, капитан! ===\n\
             1 - Гостевой лимит: {} запросов\n\
             2 - Белый лимит: {} запросов\n\
             3 - Чёрный лимит: {} запросов\n\
             4 - Размер запроса: {} байт\n\
             h - Показать эту карту\n\
             cancel - Вернуться в главное меню\n",
            config.guest_rate_limit.to_string().bright_green(),
            config.whitelist_rate_limit.to_string().bright_green(),
            config.blacklist_rate_limit.to_string().bright_green(),
            config.max_request_body_size.to_string().bright_green()
        ).yellow()
    );
}

// Запрашиваем новые порты, как координаты звёzd!
async fn prompt_ports(reader: &mut AsyncBufReader<tokio::io::Stdin>) -> Option<(u16, u16, u16)> {
    let mut http = String::new(); // Свиток для HTTP порта!
    let mut https = String::new(); // Свиток для HTTPS порта!
    let mut quic = String::new(); // Свиток для QUIC порта!

    info!(target: "console", "Новый HTTP порт (или 'cancel' для выхода): "); // Запрашиваем причал!
    reader.read_line(&mut http).await.ok()?; // Читаем приказ!
    let http = http.trim();
    if http.is_empty() || http.eq_ignore_ascii_case("cancel") {
        info!(target: "console", "Отмена, капитан! Возвращаемся в порт!"); // Курс отменён!
        return None;
    }

    info!(target: "console", "Новый HTTPS порт (или 'cancel' для выхода): "); // Запрашиваем причал!
    reader.read_line(&mut https).await.ok()?; // Читаем приказ!
    let https = https.trim();
    if https.is_empty() || https.eq_ignore_ascii_case("cancel") {
        info!(target: "console", "Отмена, капитан! Возвращаемся в порт!"); // Курс отменён!
        return None;
    }

    info!(target: "console", "Новый QUIC порт (или 'cancel' для выхода): "); // Запрашиваем причал!
    reader.read_line(&mut quic).await.ok()?; // Читаем приказ!
    let quic = quic.trim();
    if quic.is_empty() || quic.eq_ignore_ascii_case("cancel") {
        info!(target: "console", "Отмена, капитан! Возвращаемся в порт!"); // Курс отменён!
        return None;
    }

    let http_port = match http.parse::<u16>() {
        Ok(port) => port, // Причал для шлюпок готов!
        Err(_) => {
            info!(target: "console", "HTTP порт некорректен, капитан!"); // Кривая звезда!
            return None;
        }
    };

    let https_port = match https.parse::<u16>() {
        Ok(port) => port, // Причал для крейсеров готов!
        Err(_) => {
            info!(target: "console", "HTTPS порт некорректен, капитан!"); // Кривая звезда!
            return None;
        }
    };

    let quic_port = match quic.parse::<u16>() {
        Ok(port) => port, // Причал для звездолётов готов!
        Err(_) => {
            info!(target: "console", "QUIC порт некорректен, капитан!"); // Кривая звезда!
            return None;
        }
    };

    if http_port == https_port || http_port == quic_port || https_port == quic_port {
        info!(target: "console", "Порты не должны совпадать!"); // Порты дерутся, как пираты за ром!
        return None;
    }

    Some((http_port, https_port, quic_port)) // Новые координаты приняты!
}

// Запрашиваем новые лимиты, как ром в трюме!
async fn prompt_limits(reader: &mut AsyncBufReader<tokio::io::Stdin>, choice: &str) -> Option<(Option<u32>, Option<u32>, Option<u32>, Option<usize>)> {
    let mut input = String::new(); // Свиток для лимита!
    let prompt = match choice {
        "1" => "Гостевой лимит (запросов, или 'cancel' для выхода): ",
        "2" => "Белый лимит (запросов, или 'cancel' для выхода): ",
        "3" => "Чёрный лимит (запросов, или 'cancel' для выхода): ",
        "4" => "Макс. размер запроса (байт, или 'cancel' для выхода): ",
        _ => return None, // Кривая команда!
    };

    info!(target: "console", "{}", prompt); // Запрашиваем лимит!
    reader.read_line(&mut input).await.ok()?; // Читаем приказ!
    let input = input.trim();
    if input.is_empty() || input.eq_ignore_ascii_case("cancel") {
        info!(target: "console", "Отмена, капитан! Возвращаемся в порт!"); // Курс отменён!
        return None;
    }

    match choice {
        "1" => {
            let guest_limit = match input.parse::<u32>() {
                Ok(limit) if limit > 0 => limit, // Лимит для гостей принят!
                _ => {
                    info!(target: "console", "Гостевой лимит некорректен или 0, капитан!"); // Кривая звезда!
                    return None;
                }
            };
            Some((Some(guest_limit), None, None, None)) // Новый лимит для гостей!
        }
        "2" => {
            let whitelist_limit = match input.parse::<u32>() {
                Ok(limit) if limit > 0 => limit, // Лимит для друзей принят!
                _ => {
                    info!(target: "console", "Белый лимит некорректен или 0, капитан!"); // Кривая звезда!
                    return None;
                }
            };
            Some((None, Some(whitelist_limit), None, None)) // Новый лимит для друзей!
        }
        "3" => {
            let blacklist_limit = match input.parse::<u32>() {
                Ok(limit) if limit > 0 => limit, // Лимит для шпионов принят!
                _ => {
                    info!(target: "console", "Чёрный лимит некорректен или 0, капитан!"); // Кривая звезда!
                    return None;
                }
            };
            Some((None, None, Some(blacklist_limit), None)) // Новый лимит для шпионов!
        }
        "4" => {
            let body_size_limit = match input.parse::<usize>() {
                Ok(limit) if limit > 0 && limit <= 1024 * 1024 * 100 => limit, // Размер трюма принят!
                _ => {
                    info!(target: "console", "Размер запроса некорректен или вне 1–100 МБ, капитан!"); // Трюм не выдержит!
                    return None;
                }
            };
            Some((None, None, None, Some(body_size_limit))) // Новый размер трюма!
        }
        _ => None, // Кривая команда!
    }
}

// Запрашиваем новую звезду для карты!
async fn prompt_new_location(
    reader: &mut AsyncBufReader<tokio::io::Stdin>,
) -> Option<(String, String)> {
    let mut path = String::new(); // Свиток для пути!
    let mut response = String::new(); // Свиток для ответа!

    info!(target: "console", "Введите путь (например, /test, или 'cancel' для выхода): "); // Запрашиваем звезду!
    reader.read_line(&mut path).await.ok()?; // Читаем приказ!
    let path = path.trim();
    if path.is_empty() || path.eq_ignore_ascii_case("cancel") {
        info!(target: "console", "Отмена, капитан! Возвращаемся в порт!"); // Курс отменён!
        return None;
    }

    info!(target: "console", "Введите ответ (или 'cancel' для выхода): "); // Запрашиваем добычу!
    reader.read_line(&mut response).await.ok()?; // Читаем приказ!
    let response = response.trim();
    if response.is_empty() || response.eq_ignore_ascii_case("cancel") {
        info!(target: "console", "Отмена, капитан! Возвращаемся в порт!"); // Курс отменён!
        return None;
    }

    Some((path.to_string(), response.to_string())) // Новая звезда готова!
}

// Сохраняем карту и перезапускаем двигатели!
async fn save_and_restart(state: Arc<ProxyState>, kgb_config: KGBConfig, locations: Option<Vec<Location>>) {
    let locations = match locations {
        Some(locs) => locs, // Новая карта маршрутов!
        None => state.locations.read().await.clone(), // Берём текущую!
    };

    if let Err(e) = save_config(&kgb_config, &locations).await {
        info!(target: "console", "Ошибка сохранения карты: {}", e); // Шторм утащил свиток!
        return;
    }

    shutdown_all(state.clone()).await; // Гасим все шлюзы!
    let mut config = load_config().await.unwrap_or_default(); // Берём карту или запасную!
    config.http_port = kgb_config.http_port; // Новый причал для шлюпок!
    config.https_port = kgb_config.https_port; // Новый причал для крейсеров!
    config.quic_port = kgb_config.quic_port; // Новый причал для звездолётов!
    config.guest_rate_limit = kgb_config.guest_rate_limit; // Лимиты для гостей!
    config.whitelist_rate_limit = kgb_config.whitelist_rate_limit; // Лимиты для друзей!
    config.blacklist_rate_limit = kgb_config.blacklist_rate_limit; // Лимиты для шпионов!
    config.max_request_body_size = kgb_config.max_request_body_size; // Новый размер трюма!
    config.jwt_secret = kgb_config.jwt_secret.clone(); // Новый код для пропусков!
    config.trusted_host = kgb_config.trusted_host.clone(); // Новый надёжный порт!
    config.rate_limit_window = kgb_config.rate_limit_window; // Новое окно вахты!
    config.locations = locations; // Новая карта маршрутов!

    *state.http_handle.lock().await = Some(tokio::spawn(run_http_server(config.clone(), state.clone()))); // Новые шлюпки!
    *state.https_handle.lock().await = Some(tokio::spawn(run_https_server(state.clone(), config.clone()))); // Новые крейсеры!
    *state.quic_handle.lock().await = Some(tokio::spawn(run_quic_server(config.clone(), state.clone()))); // Новые звездолёты!
    info!(target: "console", "Сервера перезапущены после обновления!"); // Двигатели гудят, полный вперёд!
}

// Сохраняем звёздную карту в свиток!
async fn save_config(config: &KGBConfig, locations: &[Location]) -> Result<(), String> {
    let mut current_config: toml::Value = toml::from_str(
        &fs::read_to_string("config.toml")
            .await
            .map_err(|e| format!("Не удалось открыть звездную карту: {}", e))? // Шторм утащил свиток!
    )
    .map_err(|e| format!("Ошибка чтения текущей карты: {}", e))?; // Карта рваная!

    // Собираем новые маршруты, как звёзды на карте!
    let new_locations = locations
        .iter()
        .map(|loc| {
            let mut map = toml::value::Table::new();
            map.insert("path".to_string(), toml::Value::String(loc.path.clone())); // Путь звезды!
            map.insert("response".to_string(), toml::Value::String(loc.response.clone())); // Добыча звезды!
            toml::Value::Table(map)
        })
        .collect::<Vec<_>>();

    let table = current_config
        .as_table_mut()
        .ok_or("Звездная карта испорчена, нет таблицы!")?; // Карта без таблицы, шторм!

    // Обновляем свиток с новыми кодами!
    table.insert("locations".to_string(), toml::Value::Array(new_locations)); // Новая карта маршрутов!
    table.insert("jwt_secret".to_string(), toml::Value::String(config.jwt_secret.clone())); // Код для пропусков!
    table.insert("guest_rate_limit".to_string(), toml::Value::Integer(config.guest_rate_limit as i64)); // Лимиты для гостей!
    table.insert("whitelist_rate_limit".to_string(), toml::Value::Integer(config.whitelist_rate_limit as i64)); // Лимиты для друзей!
    table.insert("blacklist_rate_limit".to_string(), toml::Value::Integer(config.blacklist_rate_limit as i64)); // Лимиты для шпионов!
    table.insert("rate_limit_window".to_string(), toml::Value::Integer(config.rate_limit_window as i64)); // Окно вахты!
    table.insert("trusted_host".to_string(), toml::Value::String(config.trusted_host.clone())); // Надёжный порт!
    table.insert("max_request_body_size".to_string(), toml::Value::Integer(config.max_request_body_size as i64)); // Размер трюма!
    table.insert("http_port".to_string(), toml::Value::Integer(config.http_port as i64)); // Причал шлюпок!
    table.insert("https_port".to_string(), toml::Value::Integer(config.https_port as i64)); // Причал крейсеров!
    table.insert("quic_port".to_string(), toml::Value::Integer(config.quic_port as i64)); // Причал звездолётов!

    let toml_string = toml::to_string_pretty(&current_config)
        .map_err(|e| format!("Ошибка сериализации конфигурации: {}", e))?; // Не можем записать свиток!

    fs::write("config.toml", toml_string)
        .await
        .map_err(|e| format!("Ошибка записи в config.toml: {}", e))?; // Шторм утащил свиток!

    Ok(()) // Свиток сохранён, звёзды на месте!
}
