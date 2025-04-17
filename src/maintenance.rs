// Машинное отделение космопорта YUAI! Чиним трюмы, следим за двигателями и перезапускаем всё, что дымит!
use crate::{ProxyState, Config}; // Штурвал космопорта и звёздная карта!
use crate::servers::{run_http_server, run_https_server, run_quic_server}; // Двигатели шлюпок, крейсеров и звездолётов!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use std::time::{Duration, Instant}; // Часы капитана, отсчитываем время до шторма!
use futures::FutureExt; // Ловим шторма, чтоб не утонуть!
use std::panic::AssertUnwindSafe; // Страховка от паники, как спасательный плот!
use tracing::{info, error, warn}; // Рация: кричим о штормах и победах!

/// Очистка трюма кэша, выкидываем протухший ром!
pub async fn clean_cache(state: Arc<ProxyState>) {
    let result = AssertUnwindSafe(async {
        let mut interval = tokio::time::interval(Duration::from_secs(300)); // Чистим каждые 5 минут!
        loop {
            interval.tick().await; // Ждём сигнала часов!
            let now = Instant::now(); // Текущее время, как звезда на небе!
            state.cache.retain(|_, entry| entry.expiry > now); // Выкидываем старый хлам!
            info!("Кэш очищен: устаревшие данные удалены."); // Доклад в рацию: трюм чист!
        }
    }).catch_unwind().await; // Ловим шторма, если юнга паникует!

    if let Err(e) = result {
        error!("Паника в clean_cache: {:?}", e); // Шторм в трюме, юнга споткнулся!
    }
}

/// Следим за юнгой, чтоб не сбежал, и перезапускаем, если упал!
pub async fn supervise_task<F, Fut>(
    name: String, // Имя юнги, как метка на карте!
    state: Arc<ProxyState>, // Штурвал космопорта!
    task: F, // Работа юнги, что он крутит!
) where
    F: Fn(Arc<ProxyState>) -> Fut + Send + Sync + Clone + 'static, // Юнга должен быть надёжным!
    Fut: futures::Future<Output = ()> + Send + 'static, // Работа юнги бесконечна!
{
    loop {
        let state_clone = state.clone(); // Копия штурвала для юнги!
        let task_clone = task.clone(); // Копия работы, чтоб юнга не скучал!
        let name_clone = name.clone(); // Метка юнги, чтоб не забыть!
        
        let handle = tokio::spawn(async move {
            let result = AssertUnwindSafe(task_clone(state_clone)).catch_unwind().await; // Ловим панику юнги!
            if let Err(e) = result {
                error!("Задача '{}' упала: {:?}", name_clone, e); // Юнга свалился за борт!
            }
        });

        handle.await.unwrap_or_else(|e| {
            error!("Задача '{}' завершилась с ошибкой: {:?}", name, e); // Юнга утонул в шторме!
        });

        warn!("Перезапуск задачи '{}' через 5 секунд...", name); // Даём юнге 5 секунд на отдых!
        tokio::time::sleep(Duration::from_secs(5)).await; // Ждём, пока юнга отдышится!
    }
}

/// Следим за двигателями серверов, чтоб шлюпки, крейсеры и звездолёты не заглохли!
pub async fn supervise_servers(config: Config, state: Arc<ProxyState>) {
    let config = Arc::new(config); // Звёздная карта в сейфе!
    
    let http_config = config.clone(); // Карта для шлюпок!
    let https_config = config.clone(); // Карта для крейсеров!
    let quic_config = config.clone(); // Карта для звездолётов!

    // Запускаем юнг следить за двигателями!
    let http_task = tokio::spawn(supervise_task(
        "HTTP Server".to_string(), // Юнга для шлюпок!
        state.clone(),
        move |s| run_http_server((*http_config).clone(), s), // Работа: крутить HTTP!
    ));
    let https_task = tokio::spawn(supervise_task(
        "HTTPS Server".to_string(), // Юнга для крейсеров!
        state.clone(),
        move |s| run_https_server(s, (*https_config).clone()), // Работа: крутить HTTPS!
    ));
    let quic_task = tokio::spawn(supervise_task(
        "QUIC Server".to_string(), // Юнга для звездолётов!
        state.clone(),
        move |s| run_quic_server((*quic_config).clone(), s), // Работа: крутить QUIC!
    ));

    let _ = tokio::try_join!(http_task, https_task, quic_task); // Все юнги на вахте, полный вперёд!
}
