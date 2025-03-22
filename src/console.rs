use std::sync::Arc;
use tokio::io::{self as aio, AsyncBufReadExt, BufReader as AsyncBufReader, AsyncWriteExt};
use tokio::select;
use crate::{ProxyState, Location, load_config, validate_config};

pub async fn run_console(state: Arc<ProxyState>) {
    let stdin = aio::stdin();
    let mut reader = AsyncBufReader::new(stdin);
    let mut stdout = aio::stdout();

    stdout
        .write_all(get_server_status(&state, false).await.as_bytes())
        .await
        .unwrap();
    print_help(&mut stdout).await;
    stdout.write_all(b"\n> ").await.unwrap();
    stdout.flush().await.unwrap();

    let mut input = String::new();

    loop {
        select! {
            result = reader.read_line(&mut input) => {
                match result {
                    Ok(0) => {
                        stdout.write_all("\nЗавершение консоли...\n".as_bytes()).await.unwrap();
                        break; // EOF (Ctrl+D / Ctrl+Z)
                    }
                    Ok(_) => {
                        let command = input.trim();
                        match command {
                            "1" => {
                                stdout.write_all(get_server_status(&state, true).await.as_bytes()).await.unwrap();
                            }
                            "2" => {
                                match load_config().await {
                                    Ok(new_config) if validate_config(&new_config).is_ok() => {
                                        *state.config.lock().await = new_config.clone();
                                        *state.locations.write().await = new_config.locations.clone();
                                        stdout.write_all("\nКонфигурация перезагружена\n".as_bytes()).await.unwrap();
                                    }
                                    _ => {
                                        stdout.write_all("\nОшибка валидации конфигурации\n".as_bytes()).await.unwrap();
                                    }
                                }
                            }
                            "3" => {
                                if let Some((path, response)) = prompt_new_location(&mut stdout, &mut reader).await {
                                    state.locations.write().await.push(Location { path, response });
                                    stdout.write_all("\nЛокация добавлена\n".as_bytes()).await.unwrap();
                                }
                            }
                            "4" => {
                                let sessions: Vec<_> = state.sessions.iter().map(|e| e.key().clone()).collect();
                                stdout.write_all(format!("\nАктивные сессии: {:?}\n", sessions).as_bytes()).await.unwrap();
                            }
                            "help" => {
                                print_help(&mut stdout).await;
                            }
                            "" => {}
                            _ => {
                                stdout.write_all("\nНеверная команда, введите 'help' для списка команд\n".as_bytes()).await.unwrap();
                            }
                        }
                        stdout.write_all("\n> ".as_bytes()).await.unwrap();
                        stdout.flush().await.unwrap();
                    }
                    Err(e) => {
                        tracing::error!("Ошибка чтения ввода: {}", e);
                        break;
                    }
                }
                input.clear();
            }
        }
    }
}

async fn get_server_status(state: &ProxyState, full: bool) -> String {
    let config = state.config.lock().await; // Получаем доступ к конфигурации
    let base_status = format!(
        "\x1b[95m\nСтатус сервера:\nСессий: {} Размер кэша: {} Попыток авторизации: {}\x1b[0m",
        state.sessions.len(),
        state.cache.len(),
        state.auth_attempts.len()
    );
    
    if full {
        format!(
            "{}\nHTTP: {} (порт: {})\nHTTPS: {} (порт: {})\nQUIC: {} (порт: {})",
            base_status,
            if *state.http_running.read().await { "работает" } else { "не работает" },
            config.http_port,
            if *state.https_running.read().await { "работает" } else { "не работает" },
            config.https_port,
            if *state.quic_running.read().await { "работает" } else { "не работает" },
            config.quic_port
        )
    } else {
        base_status
    }
}

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
    stdout.flush().await.unwrap();
}

async fn prompt_new_location(
    stdout: &mut tokio::io::Stdout,
    reader: &mut AsyncBufReader<tokio::io::Stdin>,
) -> Option<(String, String)> {
    let mut path = String::new();
    let mut response = String::new();

    stdout
        .write_all("\nВведите путь (например, /test): ".as_bytes())
        .await
        .unwrap();
    stdout.flush().await.unwrap();
    if reader.read_line(&mut path).await.unwrap_or(0) == 0 {
        return None;
    }

    stdout
        .write_all("Введите ответ: ".as_bytes())
        .await
        .unwrap();
    stdout.flush().await.unwrap();
    if reader.read_line(&mut response).await.unwrap_or(0) == 0 {
        return None;
    }

    let path = path.trim().to_string();
    let response = response.trim().to_string();
    if path.is_empty() || response.is_empty() {
        None
    } else {
        Some((path, response))
    }
}
