// Телепорт WebSocket космопорта YUAI! Связываем гостей через звёзды, как межгалактический мост!
use hyper_tungstenite::{HyperWebsocket, tungstenite::Message}; // Маяки WebSocket, посылаем сигналы!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use std::time::Duration; // Часы капитана, отсчитываем время до шторма!
use tracing::{info, warn, error}; // Рация: кричим о штормах и победах!
use crate::{ProxyState, routing}; // Штурвал космопорта и звёздный маршрутизатор!
use futures::{StreamExt, SinkExt}; // Потоки для посланий, как звёздные течения!

// Открываем телепорт WebSocket, зажигаем межзвёздный мост!
pub async fn handle_websocket(websocket: HyperWebsocket, state: Arc<ProxyState>, ip: String) {
    info!("Открываем телепорт WebSocket для {}!", ip); // Доклад: гость на связи!
    let mut ws = match websocket.await {
        Ok(ws) => ws, // Телепорт зажёгся!
        Err(e) => {
            error!("Телепорт сломался: {}", e); // Шторм в эфире!
            return;
        }
    };

    // Курс для гостя, пока моковый, как старая карта!
    let path = "/ws";

    // Прокладываем курс через звёздный маршрутизатор!
    let route = routing::resolve_route(path, &state).await;
    if let Some(loc) = route {
        info!("Гость {} прибыл в маршрут: {}", ip, loc.path); // Курс верный!
    } else {
        warn!("Гость {} заблудился в гиперпространстве: {}", ip, path); // Потерялись в космосе!
    }

    // Слушаем послания через телепорт, даём 30 секунд на сигнал!
    while let Some(msg) = tokio::time::timeout(Duration::from_secs(30), ws.next()).await.ok().flatten() {
        match msg {
            Ok(msg) if msg.is_text() || msg.is_binary() => {
                // Проверяем, не слишком ли тяжёлый сундук!
                if msg.len() > 64 * 1024 {
                    warn!("Гость {} тащит сундук больше 64 КБ!", ip);
                    let _ = ws.send(Message::Text("Сундук слишком тяжёлый!".to_string())).await; // Кричим: брось лишнее!
                    break;
                }

                // Эхо-ответ, как отражение в звёздах!
                if let Err(e) = ws.send(msg).await {
                    error!("Телепорт барахлит: {}", e); // Шторм в эфире!
                    break;
                }
            }
            Err(e) => {
                error!("Телепорт рухнул: {}", e); // Телепорт взорвался!
                break;
            }
            _ => {} // Молчим, если сигнал пустой!
        }
    }
}
