// Отсек адаптеров космопорта YUAI! Здесь шлюпки, крейсеры и звездолёты подключаются к звёздным маршрутам!
use hyper::{Request, Response, StatusCode, header, Error as HttpError}; // Двигатель HTTP: запросы, ответы и шторма в эфире!
use hyper_tungstenite::{is_upgrade_request, upgrade}; // Телепорт WebSocket для прыжков через гиперпространство!
use http_body_util::{BodyExt, Full, Limited}; // Упаковка грузов в трюм и проверка их размера!
use bytes::{Bytes, Buf}; // Байты — цифровое золото, звенит в трюме!
use std::net::{IpAddr, SocketAddr}; // Координаты звёzd: IP и порты!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use std::time::Duration; // Часы капитана: сколько ждать до следующей звезды?
use quinn::Connection; // QUIC-соединение, как гиперскоростной звездолёт!
use h3_quinn::Connection as H3QuinnConnection; // HTTP/3 на QUIC — двигатель для скорости света!
use h3::server::Connection as H3Connection; // Палуба HTTP/3, шустрые гости welcome!
use tracing::{error, warn, info}; // Рация: кричим о штормах и победах!
use crate::{ProxyState, CacheEntry}; // Штурвал космопорта и сундук с добычей!
use crate::websocket; // Модуль телепорта WebSocket, прыжки через звёзды!
use crate::webrtc; // Модуль WebRTC, мосты между звездолётами!
use crate::routing; // Звёздные маршруты, чтоб не заблудиться в туманностях!

// Шлюпки HTTP перенаправляем на HTTPS, полный вперёд!
pub async fn handle_http_request(
    req: Request<hyper::body::Incoming>, // Входящий запрос, как шлюпка у причала!
    https_port: u16, // Порт крейсеров HTTPS, там броня крепка!
    trusted_host: String, // Надёжный порт, как родная гавань!
    state: Arc<ProxyState>, // Штурвал космопорта, всё под контролем!
    client_ip: SocketAddr, // Координаты гостя, кто стучится в шлюз?
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> { // Ответ или шторм, но без паники!
    // Пушки на входе! Секретная служба KGB проверяет гостя!
    if let Err(response) = state.kgb.protect(&req, Some(client_ip.ip())).await {
        return Ok(response); // Шпион пойман, за борт!
    }
    // Собираем курс на HTTPS, как карту к сокровищам!
    let redirect_url = format!(
        "https://{}:{}{}",
        trusted_host,
        https_port,
        req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("") // Путь или пустой трюм!
    );
    info!("Шлюпка летит на HTTPS: {}", redirect_url); // В рацию: шлюпка меняет курс!

    // Поднимаем паруса редиректа!
    let response = Response::builder()
        .status(StatusCode::MOVED_PERMANENTLY) // Флаг: "Лети на HTTPS!"
        .header(header::LOCATION, redirect_url) // Курс на новую звезду!
        .header(header::SERVER, "YUAI CosmoPort") // Флаг нашего корабля!
        .body(Full::new(Bytes::new())) // Пустой трюм, только направление!
        .map_err(|e| {
            error!("Ошибка сборки редиректа: {}", e); // Шторм в паруса!
            format!("Ошибка сборки редиректа: {}", e)
        });

    match response {
        Ok(resp) => Ok(resp), // Шлюпка ушла на HTTPS, йо-хо-хо!
        Err(e) => {
            error!("Ошибка редиректа: {}", e); // Шторм сорвал паруса!
            Ok(Response::new(Full::new(Bytes::from_static(b"Internal Server Error")))) // Запасной плот в эфир!
        }
    }
}

// Главная палуба HTTPS для крейсеров, броня наготове!
pub async fn handle_https_request(
    mut req: Request<hyper::body::Incoming>, // Входящий запрос, как крейсер у шлюза!
    state: Arc<ProxyState>, // Штурвал космопорта, всё под контролем!
    client_ip: Option<IpAddr>, // Координаты гостя, кто ломится в трюм?
) -> Result<Response<Full<Bytes>>, HttpError> { // Ответ или шторм в эфире!
    // Пушки на входе! KGB проверяет пропуска!
    let ip = match state.kgb.protect(&req, client_ip).await {
        Ok(ip) => ip, // Гость чист, добро пожаловать!
        Err(response) => return Ok(response), // Шпион, пушки на тебя!
    };
    info!("Гость {} ломится в HTTPS!", ip); // В рацию: крейсер на подходе!

    // Даём 30 секунд, чтоб не застрять в туманности!
    tokio::time::timeout(Duration::from_secs(30), async {
        // Проверяем, не хочет ли гость прыгнуть через WebSocket!
        if is_upgrade_request(&req) {
            info!("Гость {} прыгает через WebSocket!", ip); // Телепорт зажёгся!
            match upgrade(&mut req, None) {
                Ok((response, websocket)) => {
                    // Юнга на телепорт, прыжок в гиперпространство!
                    tokio::spawn(websocket::handle_websocket(websocket, state.clone(), ip.clone()));
                    return Ok(response.map(|_| Full::new(Bytes::new()))); // Ответ пустой, телепорт открыт!
                }
                Err(e) => {
                    error!("Телепорт сломался: {}", e); // Шторм в телепорте!
                    return build_response(StatusCode::BAD_REQUEST, Bytes::from("Шторм в эфире!"), None);
                }
            }
        }
        // Гость вызывает WebRTC, зажигаем звёзды!
        if req.uri().path() == "/webrtc/offer" {
            info!("Гость {} вызывает WebRTC!", ip); // Мост WebRTC активирован!
            let kgb_config = state.kgb.get_config().await; // Достаём коды из сейфа KGB!
            let limited_body = Limited::new(req.into_body(), kgb_config.max_request_body_size); // Проверяем размер трюма!
            let body = match limited_body.collect().await {
                Ok(collected) => collected.to_bytes(), // Груз в трюме, всё чисто!
                Err(e) => {
                    warn!("Гость {} закинул слишком большую добычу: {}", ip, e); // Трюм трещит!
                    return build_response(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Bytes::from("Трюм переполнен, шпион! Слишком большая добыча!"),
                        None,
                    );
                }
            };
            let offer_sdp = String::from_utf8_lossy(&body).to_string(); // Сигнал WebRTC, как маяк в ночи!
            match webrtc::handle_webrtc_offer(offer_sdp, state.clone(), ip.clone()).await {
                Ok(answer_sdp) => {
                    // Мост установлен, сигнал возвращён!
                    return build_response(
                        StatusCode::OK,
                        Bytes::from(answer_sdp),
                        Some(vec![("Content-Type".to_string(), "application/sdp".to_string())]),
                    );
                }
                Err(e) => {
                    error!("WebRTC барахлит: {}", e); // Шторм в мосту WebRTC!
                    return build_response(StatusCode::INTERNAL_SERVER_ERROR, Bytes::from("Шторм в эфире!"), None);
                }
            }
        }
        // Отправляем запрос по звёздным маршрутам!
        routing::handle_route(req, state.clone(), Some(client_ip.unwrap_or(IpAddr::from([127, 0, 0, 1])))).await
    }).await.unwrap_or_else(|_| {
        warn!("Гость {} слишком долго копался!", ip); // Гость застрял в туманности!
        Ok(build_response(StatusCode::REQUEST_TIMEOUT, Bytes::from("Запрос утонул в шторме!"), None)
            .unwrap_or_else(|e| {
                error!("Ошибка создания ответа на тайм-аут: {}", e); // Шторм сорвал ответ!
                Response::new(Full::new(Bytes::from_static(b"Request Timeout"))) // Запасной плот!
            }))
    })
}

// Обрабатываем QUIC-соединения, гиперскорость или старый ром!
pub async fn handle_quic_connection(
    connection: Connection, // QUIC-соединение, как звездолёт на гиперскорости!
    state: Arc<ProxyState>, // Штурвал космопорта, всё под контролем!
) {
    let client_ip = connection.remote_address(); // Координаты гостя, кто летит?
    let req = Request::new(()); // Фиктивный запрос, пушки наготове!
    if let Err(_) = state.kgb.protect(&req, Some(client_ip.ip())).await {
        return; // Шпион пойман, за борт!
    }
    info!("Гость на гиперскорости из {:?}", client_ip); // В рацию: звездолёт на подходе!

    // Пробуем HTTP/3, как гипердвигатель!
    let h3_attempt = H3Connection::new(H3QuinnConnection::new(connection.clone())).await;
    match h3_attempt {
        Ok(h3_conn) => {
            // HTTP/3 работает, летим на полной!
            handle_http3_request(h3_conn, state, client_ip).await;
        }
        Err(_) => {
            info!("Гость {} использует чистый QUIC!", client_ip); // Чистый QUIC, как старый ром!
            if let Ok((mut send, mut recv)) = connection.accept_bi().await {
                let mut buffer = vec![0; 4096]; // Трюм на 4К байт, хватит для малого груза!
                if let Ok(Some(n)) = recv.read(&mut buffer).await {
                    let input = String::from_utf8_lossy(&buffer[..n]); // Читаем сигнал, как свиток!
                    let mut lines = input.lines();
                    let path = lines.next().unwrap_or("/"); // Путь или главная звезда!

                    // Ищем маршрут, как звезду на карте!
                    let response_body = if let Some(loc) = routing::resolve_route(path, &state).await {
                        loc.response.clone() // Добыча найдена!
                    } else {
                        "404 — звезда не найдена!".into() // Звезда пропала, шторм!
                    };

                    let _ = send.write_all(response_body.as_bytes()).await; // Отправляем груз в эфир!
                }
            }
        }
    }
}

// Палуба HTTP/3 для гиперскоростных звездолётов!
pub async fn handle_http3_request(
    mut conn: H3Connection<H3QuinnConnection, Bytes>, // Соединение HTTP/3, быстрее света!
    state: Arc<ProxyState>, // Штурвал космопорта, всё под контролем!
    client_ip: SocketAddr, // Координаты гостя, кто на гиперскорости?
) {
    let dummy_req = Request::new(()); // Фиктивный запрос для проверки!
    if let Err(_) = state.kgb.protect(&dummy_req, Some(client_ip.ip())).await {
        return; // Шпион, пушки на тебя!
    }
    info!("Гость прилетел на HTTP/3!"); // В рацию: гиперскорость активирована!

    // Принимаем запросы, как грузы с орбиты!
    while let Ok(Some((req, mut stream))) = conn.accept().await {
        let state_clone = state.clone(); // Копия штурвала для юнги!
        let ip = client_ip.ip().to_string(); // Координаты гостя!
        tokio::spawn(async move {
            let url = req.uri().to_string(); // Путь, как звезда на карте!
            // Проверяем сундук с добычей!
            if let Some(entry) = state_clone.cache.get(&url) {
                if entry.expiry > std::time::Instant::now() {
                    info!("Добыча для {} найдена!", ip); // Золото в трюме!
                    let resp = build_h3_response(StatusCode::OK, None).unwrap_or_else(|e| {
                        error!("Ошибка сборки ответа HTTP/3: {}", e); // Шторм в эфире!
                        Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .header(header::SERVER, "YUAI CosmoPort")
                            .body(())
                            .unwrap_or_else(|_| Response::new(())) // Запасной плот!
                    });
                    if stream.send_response(resp).await.is_err() {
                        warn!("Не удалось отправить ответ для {}", ip); // Сигнал пропал!
                        return;
                    }
                    if stream.send_data(Bytes::from(entry.response_body.clone())).await.is_err() {
                        warn!("Не удалось отправить данные для {}", ip); // Груз застрял!
                        return;
                    }
                    if stream.finish().await.is_err() {
                        warn!("Не удалось завершить поток для {}", ip); // Шлюз заклинило!
                    }
                    return;
                }
            }
            // Собираем груз из потока!
            let mut body_bytes: Vec<u8> = Vec::new();
            while let Ok(Some(mut chunk)) = stream.recv_data().await {
                let bytes = chunk.copy_to_bytes(chunk.remaining()); // Груз в трюм!
                body_bytes.extend_from_slice(&bytes);
            }
            let path = req.uri().path(); // Путь, как звезда на карте!
            let location = routing::resolve_route(path, &state_clone).await; // Ищем маршрут!
            let response_body = location
                .as_ref()
                .map(|loc| loc.response.clone()) // Добыча найдена!
                .unwrap_or_else(|| "404 — звезда не найдена!".to_string()); // Звезда пропала!
            let response_bytes = Bytes::from(response_body.clone()); // Груз готов!
            // Складываем добычу в сундук!
            state_clone.cache.insert(url, CacheEntry {
                response_body: response_bytes.to_vec(),
                expiry: std::time::Instant::now() + std::time::Duration::from_secs(60), // Храним минуту!
            });
            let resp = build_h3_response(StatusCode::OK, location.and_then(|loc| loc.headers.clone())).unwrap_or_else(|e| {
                error!("Ошибка сборки ответа HTTP/3: {}", e); // Шторм в эфире!
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .header(header::SERVER, "YUAI CosmoPort")
                    .body(())
                    .unwrap_or_else(|_| Response::new(())) // Запасной плот!
            });
            if stream.send_response(resp).await.is_err() {
                warn!("Не удалось отправить ответ для {}", ip); // Сигнал пропал!
                return;
            }
            if stream.send_data(response_bytes).await.is_err() {
                warn!("Не удалось отправить данные для {}", ip); // Груз застрял!
                return;
            }
            if stream.finish().await.is_err() {
                warn!("Не удалось завершить поток для {}", ip); // Шлюз заклинило!
            }
            info!("Гость {} забрал добычу по HTTP/3!", ip); // Груз доставлен, йо-хо-хо!
        });
    }
}

// Собираем посылку для HTTP/1.1 и HTTP/2, как ром в бочку!
pub fn build_response(
    status: StatusCode, // Флаг состояния: "Всё в порядке" или "Шторм на горизонте"!
    body: Bytes, // Груз — цифровое золото!
    custom_headers: Option<Vec<(String, String)>>, // Особые паруса для ответа!
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let mut builder = Response::builder().status(status); // Поднимаем флаг!

    // Проверяем, есть ли Content-Type в кастомных парусах!
    let has_ct = custom_headers
        .as_ref()
        .map_or(false, |hs| hs.iter().any(|(n, _)| n.eq_ignore_ascii_case("content-type")));
    if !has_ct {
        builder = builder.header(header::CONTENT_TYPE, "text/plain; charset=utf-8"); // Текст по умолчанию, как ром в бочке!
    }

    // Поднимаем стандартные паруса безопасности!
    builder = builder
        .header(header::SERVER, "YUAI CosmoPort") // Флаг нашего корабля!
        .header(header::CACHE_CONTROL, "no-cache") // Не храним добычу, если не сказано!
        .header(header::CONTENT_LENGTH, body.len().to_string()) // Сколько золота в трюме!
        .header("X-Content-Type-Options", "nosniff") // Не нюхай наш ром, шпион!
        .header("X-Frame-Options", "DENY") // Никаких рамок, это наш трюм!
        .header("Content-Security-Policy", "default-src 'none'") // Только наш ром, никаких чужих сокровищ!
        .header("Strict-Transport-Security", "max-age=31536000; includeSubDomains"); // Броня на год!

    // Добавляем кастомные паруса, если есть!
    if let Some(headers) = custom_headers {
        for (name, value) in headers {
            builder = builder.header(name, value); // Поднимаем их на мачты!
        }
    }

    Ok(builder.body(Full::new(body)).unwrap_or_else(|e| {
        error!("Ошибка сборки ответа: {}", e); // Шторм сорвал паруса!
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Full::new(Bytes::from("Internal Server Error")))
            .expect("Failed to build error response") // Запасной плот в эфир!
    }))
}

// Шапка для HTTP/3, добыча летит отдельно, как звездолёт!
pub fn build_h3_response(
    status: StatusCode, // Флаг состояния, быстрый и чёткий!
    custom_headers: Option<Vec<(String, String)>>, // Особые паруса для гиперскорости!
) -> Result<Response<()>, String> {
    let mut builder = Response::builder()
        .status(status) // Флаг состояния, полный вперёд!
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8") // Текст по умолчанию, как ром!
        .header(header::SERVER, "YUAI CosmoPort") // Флаг нашего корабля!
        .header(header::CACHE_CONTROL, "no-cache"); // Не кэшируем, летим налегке!

    // Поднимаем кастомные паруса, если есть!
    if let Some(headers) = custom_headers {
        for (name, value) in headers {
            builder = builder.header(name.as_str(), value.as_str()); // На мачты звездолёта!
        }
    }

    builder.body(()).map_err(|e| format!("Не удалось собрать шапку для HTTP/3: {}", e)) // Шапка готова или шторм!
}
