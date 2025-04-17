// Телепорт космопорта YUAI! WebRTC — как межзвёздные маяки, связываем гостей через галактику!
use crate::ProxyState; // Штурвал космопорта, всё под контролем!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use tracing::{info, warn}; // Рация: кричим о штормах и победах!
use webrtc::{
    api::APIBuilder, // Создатель маяков, зажигаем связь!
    peer_connection::{configuration::RTCConfiguration}, // Карта для маяков!
    peer_connection::sdp::session_description::RTCSessionDescription, // Послания через космос!
    ice_transport::ice_server::RTCIceServer, // Маяки для навигации!
};
use crate::routing; // Звёздный маршрутизатор, прокладываем курс!
pub use webrtc::peer_connection::RTCPeerConnection; // Маяк для связи, как телепорт!
use serde::Deserialize; // Читаем звёздные карты!
use serde_json; // Расшифровываем послания, как древние свитки!

// Послание с маршрутом и сигналом, как бутылка с картой!
#[derive(Deserialize)]
struct OfferWithRoute {
    route: String, // Курс, куда летим!
    offer: String, // Сигнал для телепорта!
}

// Обрабатываем сигнал WebRTC, зажигаем телепорт!
pub async fn handle_webrtc_offer(offer_sdp: String, state: Arc<ProxyState>, client_ip: String) -> Result<String, String> {
    info!("Гость {} вызывает телепорт WebRTC!", client_ip); // Доклад в рацию!

    // Проверяем, не слишком ли тяжёлый сигнал!
    if offer_sdp.len() > 10 * 1024 {
        warn!("Гость {} тащит сигнал больше 10 КБ!", client_ip);
        return Err("Слишком большой сигнал!".to_string());
    }

    // Расшифровываем послание, ищем маршрут!
    let offer_with_route: OfferWithRoute = match serde_json::from_str(&offer_sdp) {
        Ok(data) => data, // Послание читаемо!
        Err(_) => {
            warn!("Гость {} прислал кривой JSON вместо оффера!", client_ip);
            return Err("Неверный формат сигнала — нужен JSON с route и offer".into());
        }
    };

    // Прокладываем курс через звёздный маршрутизатор!
    let route = routing::resolve_route(&offer_with_route.route, &state).await;
    if let Some(loc) = route {
        info!("Гость {} направляется в маршрут: {}", client_ip, loc.path); // Курс верный!
    } else {
        warn!("Гость {} заблудился в гиперпространстве: {}", client_ip, offer_with_route.route); // Потерялись в космосе!
    }

    // Зажигаем маяк WebRTC!
    let api = APIBuilder::new().build(); // Создаём маяк!
    let ice_servers: Vec<RTCIceServer> = vec!["stun:stun.l.google.com:19302".to_string()]
        .into_iter()
        .map(|url| RTCIceServer { urls: vec![url], ..Default::default() }) // Маяк Google для навигации!
        .collect();

    let peer_config = RTCConfiguration {
        ice_servers, // Карта маяков!
        ..Default::default()
    };

    // Создаём телепорт для гостя!
    let peer_connection = Arc::new(api.new_peer_connection(peer_config).await.map_err(|e| format!("Ошибка WebRTC: {}", e))?);
    let data_channel = peer_connection.create_data_channel("proxy", None).await.map_err(|e| format!("Ошибка канала: {}", e))?; // Канал связи открыт!

    // Читаем сигнал от гостя!
    let offer = RTCSessionDescription::offer(offer_with_route.offer).map_err(|e| format!("Сигнал кривой: {}", e))?;
    peer_connection.set_remote_description(offer).await.map_err(|e| format!("Ошибка метки: {}", e))?; // Устанавливаем сигнал!
    let answer = peer_connection.create_answer(None).await.map_err(|e| format!("Ошибка ответа: {}", e))?; // Готовим ответный сигнал!
    peer_connection.set_local_description(answer.clone()).await.map_err(|e| format!("Ошибка флага: {}", e))?; // Устанавливаем флаг!

    // Записываем гостя в журнал телепорта!
    state.webrtc_peers.insert(client_ip.clone(), peer_connection.clone());

    // Слушаем послания через канал!
    data_channel.on_message(Box::new(move |msg| {
        info!("Гость {} прислал через WebRTC: {:?}", client_ip, msg.data.to_vec()); // Доклад: послание получено!
        Box::pin(async move {}) // Пока молчим, но готовы слушать!
    }));

    Ok(answer.sdp) // Отправляем ответный сигнал, телепорт готов!
}
