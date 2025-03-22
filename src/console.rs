use std::sync::Arc; // Общий штурвал для юнг
use tokio::io::{self as aio, AsyncBufReadExt, BufReader as AsyncBufReader, AsyncWriteExt}; // Рация: слушать и кричать
use tokio::select; // Выбирать, что делать первым, как штурман на развилке
use crate::{ProxyState, Location, load_config, validate_config}; // Карта, маршруты и штурвал космопорта

// Запускаем капитанскую консоль — место, где отдаем приказы!
pub async fn run_console(state: Arc<ProxyState>) {
    let stdin = aio::stdin(); // Ухо для приказов капитана
    let mut reader = AsyncBufReader::new(stdin); // Юнга, который слушает
    let mut stdout = aio::stdout(); // Рупор для криков капитана

    // Показываем, как дела в порту
    stdout
        .write_all(get_server_status(&state, false).await.as_bytes()) // Пишем статус
        .await
        .unwrap();
    print_help(&mut stdout).await; // Показываем список приказов
    stdout.write_all(b"\n> ").await.unwrap(); // Говорим: "Давай, капитан!"
    stdout.flush().await.unwrap(); // Убеждаемся, что все слышали

    let mut input = String::new(); // Сундучок для команд

    loop { // Вечно слушаем капитана
        select! { // Слушаем, что скажет первым
            result = reader.read_line(&mut input) => { // Читаем приказ
                match result {
                    Ok(0) => { // Если капитан сказал "хватит" (Ctrl+D/Z)
                        stdout.write_all("\nЗавершение консоли...\n".as_bytes()).await.unwrap();
                        break; // Закрываем мостик
                    }
                    Ok(_) => { // Приказ получен!
                        let command = input.trim(); // Убираем лишнее с команды
                        match command {
                            "1" => { // "Как дела, юнга?"
                                stdout.write_all(get_server_status(&state, true).await.as_bytes()).await.unwrap();
                            }
                            "2" => { // "Обнови карту!"
                                match load_config().await { // Берем новую карту
                                    Ok(new_config) if validate_config(&new_config).is_ok() => { // Карта годная?
                                        *state.config.lock().await = new_config.clone(); // Меняем старую
                                        *state.locations.write().await = new_config.locations.clone(); // Обновляем маршруты
                                        stdout.write_all("\nКонфигурация перезагружена\n".as_bytes()).await.unwrap();
                                    }
                                    _ => { // Карта кривая
                                        stdout.write_all("\nОшибка валидации конфигурации\n".as_bytes()).await.unwrap();
                                    }
                                }
                            }
                            "3" => { // "Добавь новый путь!"
                                if let Some((path, response)) = prompt_new_location(&mut stdout, &mut reader).await {
                                    state.locations.write().await.push(Location { path, response }); // Добавляем маршрут
                                    stdout.write_all("\nЛокация добавлена\n".as_bytes()).await.unwrap();
                                }
                            }
                            "4" => { // "Кто в порту?"
                                let sessions: Vec<_> = state.sessions.iter().map(|e| e.key().clone()).collect(); // Список гостей
                                stdout.write_all(format!("\nАктивные сессии: {:?}\n", sessions).as_bytes()).await.unwrap();
                            }
                            "help" => { // "Что я могу?"
                                print_help(&mut stdout).await; // Показываем приказы
                            }
                            "" => {} // Пусто? Молчим!
                            _ => { // Чушь какая-то
                                stdout.write_all("\nНеверная команда, введите 'help' для списка команд\n".as_bytes()).await.unwrap();
                            }
                        }
                        stdout.write_all("\n> ".as_bytes()).await.unwrap(); // "Еще приказы, капитан?"
                        stdout.flush().await.unwrap(); // Убеждаемся, что крикнули
                    }
                    Err(e) => { // Рация сломалась
                        tracing::error!("Ошибка чтения ввода: {}", e);
                        break;
                    }
                }
                input.clear(); // Чистим сундучок для нового приказа
            }
        }
    }
}

// Показываем, как дела в порту!
async fn get_server_status(state: &ProxyState, full: bool) -> String {
    let config = state.config.lock().await; // Берем карту под замок
    let base_status = format!(
        "\x1b[95m\nСтатус сервера:\nСессий: {} Размер кэша: {} Попыток авторизации: {}\x1b[0m",
        state.sessions.len(), // Сколько гостей
        state.cache.len(), // Сколько добычи в складе
        state.auth_attempts.len() // Сколько раз ломились
    );
    
    if full { // Если капитан хочет все подробности
        format!(
            "{}\nHTTP: {} (порт: {})\nHTTPS: {} (порт: {})\nQUIC: {} (порт: {})",
            base_status,
            if *state.http_running.read().await { "работает" } else { "не работает" }, // Шлюпки
            config.http_port,
            if *state.https_running.read().await { "работает" } else { "не работает" }, // Крейсеры
            config.https_port,
            if *state.quic_running.read().await { "работает" } else { "не работает" }, // Звездолеты
            config.quic_port
        )
    } else {
        base_status // Только основное
    }
}

// Кричим в рупор список приказов!
async fn print_help(stdout: &mut tokio::io::Stdout) {
    stdout
        .write_all(
            "\n=== Список команд ===\n\
             1 - Показать статус\n\
             2 - Перезагрузить конфигурацию\n\
             3 - Добавить локацию\n\
             4 - Показать активные сессии\n\
             help - Показать этот список\n"
                .as_bytes(),
        )
        .await
        .unwrap();
    stdout.flush().await.unwrap(); // Убеждаемся, что все слышали
}

// Спрашиваем у капитана новый маршрут!
async fn prompt_new_location(
    stdout: &mut tokio::io::Stdout,
    reader: &mut AsyncBufReader<tokio::io::Stdin>,
) -> Option<(String, String)> {
    let mut path = String::new(); // Куда лететь?
    let mut response = String::new(); // Что дать?

    stdout
        .write_all("\nВведите путь (например, /test): ".as_bytes()) // "Куда, капитан?"
        .await
        .unwrap();
    stdout.flush().await.unwrap();
    if reader.read_line(&mut path).await.unwrap_or(0) == 0 { // Если молчит
        return None;
    }

    stdout
        .write_all("Введите ответ: ".as_bytes()) // "Что отдать, капитан?"
        .await
        .unwrap();
    stdout.flush().await.unwrap();
    if reader.read_line(&mut response).await.unwrap_or(0) == 0 { // Если опять молчит
        return None;
    }

    let path = path.trim().to_string(); // Чистим путь
    let response = response.trim().to_string(); // Чистим ответ
    if path.is_empty() || response.is_empty() { // Пусто? Ничего не делаем
        None
    } else {
        Some((path, response)) // Новый маршрут готов!
    }
}
