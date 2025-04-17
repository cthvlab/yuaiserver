// Двигатели космопорта YUAI! Шлюпки HTTP, крейсеры HTTPS и звездолёты QUIC готовы к полёту!
use hyper::service::service_fn; // Работа юнги, обслуживаем запросы!
use hyper_util::{rt::{TokioExecutor, TokioIo, TokioTimer}, server::conn::auto::Builder as AutoBuilder}; // Инструменты для шлюпок и крейсеров!
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr}; // Координаты портов, как звёзды на карте!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use std::time::Duration; // Часы капитана, отсчитываем время до шторма!
use tokio::net::{TcpListener}; // Причалы для шлюпок и крейсеров!
use tracing::{error, info, warn}; // Рация: кричим о штормах и победах!
use colored::Colorize; // Красим рацию в яркие цвета, как звёзды!
use quinn::{Endpoint, EndpointConfig}; // Двигатели звездолётов QUIC!
use std::panic::AssertUnwindSafe; // Страховка от паники, как спасательный плот!
use futures::FutureExt; // Ловим шторма, чтоб не утонуть!
use crate::{ProxyState, Config}; // Штурвал космопорта и звёздная карта!
use socket2::{Domain, Socket, Type}; // Сырьё для причалов, куём вручную!
use crate::adapters::{handle_http_request, handle_https_request, handle_quic_connection}; // Юнги для обработки запросов!

// Открываем причалы для шлюпок и крейсеров, готовим порты!
async fn bind_listener(port: u16, proto: &str) -> Result<Vec<TcpListener>, std::io::Error> {
    let mut listeners = Vec::new(); // Список причалов!

    // Причал для IPv6 — звёздный порт для всех!
    let addr_v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), port);
    let socket_v6 = Socket::new(Domain::IPV6, Type::STREAM, None)?; // Куём причал!
    socket_v6.set_reuse_address(true)?; // Повторное использование порта, как ром в бочке!
    #[cfg(unix)]
    socket_v6.set_reuse_port(true)?; // Делим порт между юнгами на Unix!
    socket_v6.bind(&addr_v6.into())?; // Привязываем причал к звезде!
    socket_v6.listen(1024)?; // Открываем ворота для 1024 гостей!
    let std_listener_v6 = std::net::TcpListener::from(socket_v6); // Преобразуем в стандартный причал!
    std_listener_v6.set_nonblocking(true)?; // Быстрый режим, без задержек!
    let listener_v6 = TcpListener::from_std(std_listener_v6)?; // Преобразуем в причал Tokio!
    #[cfg(unix)]
    if let Ok(_socket) = listener_v6.local_addr() {
        use std::os::unix::io::AsRawFd;
        unsafe {
            let fd = listener_v6.as_raw_fd();
            let optval: libc::c_int = 0;
            libc::setsockopt(
                fd,
                libc::IPPROTO_IPV6,
                libc::IPV6_V6ONLY,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
    }
    info!(target: "console", "{}", format!("{} порт открыт на {} (IPv6, dual-stack)!", proto, addr_v6).green()); // Доклад: причал готов!
    listeners.push(listener_v6); // Добавляем в список!

    // Причал для IPv4 — запасной порт для старых шлюпок!
    let addr_v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let socket_v4 = Socket::new(Domain::IPV4, Type::STREAM, None)?; // Куём ещё один причал!
    socket_v4.set_reuse_address(true)?; // Повторное использование порта!
    socket_v4.bind(&addr_v4.into())?; // Привязываем к звезде!
    socket_v4.listen(1024)?; // Открываем ворота!
    let std_listener_v4 = std::net::TcpListener::from(socket_v4); // Преобразуем в стандартный причал!
    std_listener_v4.set_nonblocking(true)?; // Быстрый режим!
    let listener_v4 = TcpListener::from_std(std_listener_v4)?; // Преобразуем в причал Tokio!
    info!(target: "console", "{}", format!("{} порт открыт на {} (IPv4)!", proto, addr_v4).green()); // Доклад: причал готов!
    listeners.push(listener_v4); // Добавляем в список!

    Ok(listeners) // Причалы готовы, полный вперёд!
}

// Запускаем шлюпки HTTP, лёгкие и быстрые!
pub async fn run_http_server(config: Config, state: Arc<ProxyState>) {
    let listeners = match bind_listener(config.http_port, "HTTP").await {
        Ok(listeners) => {
            *state.http_running.write().await = true; // Двигатели гудят!
            listeners
        }
        Err(e) => {
            error!(target: "console", "{}", format!("Шторм побрал HTTP порт: {}", e).red()); // Шторм в трюме!
            *state.http_running.write().await = false; // Двигатели заглохли!
            return;
        }
    };

    for listener in listeners {
        let state = state.clone(); // Копия штурвала для юнги!
        let config = config.clone(); // Копия звёздной карты!
        tokio::spawn(async move {
            let result = AssertUnwindSafe(async move {
                loop {
                    match listener.accept().await {
                        Ok((stream, client_ip)) => {
                            let stream = TokioIo::new(stream); // Поток в трюм!
                            let https_port = config.https_port; // Порт крейсеров!
                            let trusted_host = config.trusted_host.clone(); // Надёжный порт!
                            let state_clone = state.clone(); // Копия штурвала!

                            tokio::spawn(async move {
                                let result = AssertUnwindSafe(async move {
                                    let mut builder = AutoBuilder::new(TokioExecutor::new()); // Сборка шлюпки!
                                    builder.http1().max_buf_size(16_384).timer(TokioTimer::new()); // Настройки трюма!
                                    let service = service_fn(move |req| {
                                        handle_http_request(req, https_port, trusted_host.clone(), state_clone.clone(), client_ip) // Юнга обрабатывает запрос!
                                    });
                                    let _ = tokio::time::timeout(Duration::from_secs(10), builder.serve_connection(stream, service)).await; // Даём 10 секунд на работу!
                                }).catch_unwind().await;
                                if let Err(e) = result {
                                    error!(target: "console", "Паника в HTTP-потоке: {:?}", e); // Юнга свалился за борт!
                                }
                            });
                        }
                        Err(e) => {
                            warn!(target: "console", "Ошибка принятия соединения HTTP: {}", e); // Шторм у причала!
                            tokio::time::sleep(Duration::from_secs(1)).await; // Ждём секунду, пока уляжется!
                        }
                    }
                }
            }).catch_unwind().await;

            if let Err(e) = result {
                error!(target: "console", "Паника в HTTP слушателе: {:?}", e); // Шторм в машинном отделении!
            }
        });
    }

    futures::future::pending::<()>().await; // Держим шлюпки на орбите!
}

// Запускаем крейсеры HTTPS, защищённые и мощные!
pub async fn run_https_server(state: Arc<ProxyState>, config: Config) {
    let listeners = match bind_listener(config.https_port, "HTTPS").await {
        Ok(listeners) => {
            *state.https_running.write().await = true; // Двигатели гудят!
            listeners
        }
        Err(e) => {
            error!(target: "console", "{}", format!("Шторм порвал HTTPS порт: {}", e).red()); // Шторм в трюме!
            *state.https_running.write().await = false; // Двигатели заглохли!
            return;
        }
    };

    let tls_acceptor = match config.to_tls_config().build_tls_settings() {
        Ok(tls_settings) => tls_settings.acceptor, // Броня TLS готова!
        Err(e) => {
            error!(target: "console", "{}", format!("Ошибка TLS для HTTPS: {}", e).red()); // Шторм в броне!
            return;
        }
    };

    for listener in listeners {
        let tls_acceptor = tls_acceptor.clone(); // Копия брони для юнги!
        let state = state.clone(); // Копия штурвала!

        tokio::spawn(async move {
            let result = AssertUnwindSafe(async move {
                loop {
                    if let Ok((stream, client_ip)) = listener.accept().await {
                        let _ = stream.set_nodelay(true); // Быстрый режим, без задержек!
                        let acceptor = tls_acceptor.clone(); // Копия брони!
                        let state = state.clone(); // Копия штурвала!

                        tokio::spawn(async move {
                            let result = AssertUnwindSafe(async move {
                                match acceptor.accept(stream).await {
                                    Ok(tls_stream) => {
                                        let tls_stream = TokioIo::new(tls_stream); // Поток в трюм!
                                        let service = service_fn(move |req| {
                                            handle_https_request(req, state.clone(), Some(client_ip.ip())) // Юнга обрабатывает запрос!
                                        });
                                        let mut builder = AutoBuilder::new(TokioExecutor::new()); // Сборка крейсера!
                                        builder.http1().max_buf_size(16_384); // Трюм для HTTP/1!
                                        builder.http2().max_frame_size(16_384); // Трюм для HTTP/2!
                                        let _ = tokio::time::timeout(Duration::from_secs(10), builder.serve_connection(tls_stream, service)).await; // Даём 10 секунд на работу!
                                    }
                                    Err(e) => {
                                        warn!(target: "console", "TLS рукопожатие не удалось: {}", e); // Шторм в броне!
                                    }
                                }
                            }).catch_unwind().await;

                            if let Err(e) = result {
                                error!(target: "console", "Паника в HTTPS-потоке: {:?}", e); // Юнга свалился за борт!
                            }
                        });
                    }
                }
            }).catch_unwind().await;

            if let Err(e) = result {
                error!(target: "console", "Паника в HTTPS слушателе: {:?}", e); // Шторм в машинном отделении!
            }
        });
    }

    futures::future::pending::<()>().await; // Держим крейсеры на орбите!
}

// Запускаем звездолёты QUIC, быстрые и лёгкие, как кометы!
pub async fn run_quic_server(config: Config, state: Arc<ProxyState>) {
    // Достаём броню TLS для звездолётов!
    let tls_result: Result<_, Box<dyn std::error::Error + Send + Sync>> =
        config.to_tls_config().build_tls_settings().map_err(|e| format!("tls error: {e}").into());

    let tls_config = match tls_result {
        Ok(tls_settings) => tls_settings.quinn_config, // Броня для QUIC готова!
        Err(e) => {
            error!(target: "console", "{}", format!("Ошибка TLS для QUIC: {}", e).red()); // Шторм в броне!
            *state.quic_running.write().await = false; // Двигатели заглохли!
            return;
        }
    };

    let addrs = vec![
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), config.quic_port), // Порт для IPv6!
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), config.quic_port), // Порт для IPv4!
    ];

    for addr in addrs {
        let tls_config = tls_config.clone(); // Копия брони для юнги!
        let state = state.clone(); // Копия штурвала!

        tokio::spawn(async move {
            let result = AssertUnwindSafe(async move {
                loop {
                    let tls_config_clone = tls_config.clone(); // Копия брони для каждой вахты!
                    let bind_result: Result<(), String> = async {
                        // Куём причал для звездолётов!
                        let socket = match addr {
                            SocketAddr::V6(_) => Socket::new(Domain::IPV6, Type::DGRAM, None),
                            SocketAddr::V4(_) => Socket::new(Domain::IPV4, Type::DGRAM, None),
                        }
                        .map_err(|e| format!("Ошибка создания сокета для QUIC: {}", e))?;
                        socket
                            .set_reuse_address(true)
                            .map_err(|e| format!("Ошибка установки SO_REUSEADDR: {}", e))?;
                        socket
                            .bind(&addr.into())
                            .map_err(|e| format!("Ошибка привязки сокета к {}: {}", addr, e))?;
                        let socket: std::net::UdpSocket = socket.into();
                        socket
                            .set_nonblocking(true)
                            .map_err(|e| format!("Ошибка установки неблокирующего режима: {}", e))?;

                        // Запускаем звездолёт QUIC!
                        match Endpoint::new(
                            EndpointConfig::default(), // Настройки двигателя!
                            Some(tls_config_clone), // Броня TLS!
                            socket,
                            Arc::new(quinn::TokioRuntime), // Двигатель Tokio!
                        ) {
                            Ok(endpoint) => {
                                info!(target: "console", "{}", format!("QUIC порт открыт на {}!", addr).green()); // Доклад: звездолёт готов!
                                *state.quic_running.write().await = true; // Двигатели гудят!

                                while let Some(conn) = endpoint.accept().await {
                                    let state = state.clone(); // Копия штурвала для юнги!
                                    tokio::spawn(async move {
                                        let result = AssertUnwindSafe(async move {
                                            if let Ok(connection) = conn.await {
                                                handle_quic_connection(connection, state).await; // Юнга обрабатывает соединение!
                                            }
                                        }).catch_unwind().await;

                                        if let Err(e) = result {
                                            error!(target: "console", "Паника в QUIC-соединении: {:?}", e); // Юнга свалился за борт!
                                        }
                                    });
                                }

                                warn!(target: "console", "QUIC сервер на {} завершил работу. Перезапускаем...", addr); // Звездолёт заглох!
                                *state.quic_running.write().await = false; // Двигатели заглохли!
                            }
                            Err(e) => {
                                error!(target: "console", "{}", format!("Ошибка запуска QUIC на {}: {}", addr, e).red()); // Шторм в трюме!
                                tokio::time::sleep(Duration::from_secs(10)).await; // Ждём 10 секунд, пока уляжется!
                            }
                        }
                        Ok(())
                    }.await;

                    if let Err(e) = bind_result {
                        error!(target: "console", "{}", format!("Ошибка в цикле QUIC: {}", e).red()); // Шторм в машинном отделении!
                        tokio::time::sleep(Duration::from_secs(10)).await; // Ждём, пока уляжется!
                    }
                }
            }).catch_unwind().await;

            if let Err(e) = result {
                error!(target: "console", "Паника в QUIC слушателе: {:?}", e); // Шторм в машинном отделении!
            }
        });
    }

    futures::future::pending::<()>().await; // Держим звездолёты на орбите!
}
