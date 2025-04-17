// Рация космопорта YUAI! Логируем шторма, победы и шпионов, как вахтенный журнал!
use std::fs::File; // Свиток для записи логов, как пергамент в трюме!
use tracing::{info}; // Рация для криков о славе и штормах!
use tracing_subscriber::{fmt, prelude::*}; // Настройки рации, чтобы слышать нужное!
use tracing_subscriber::filter::{LevelFilter, Targets}; // Фильтры, как пушки на шпионов!
use colored::Colorize; // Красим рацию в яркие цвета, как звёзды!

// Настраиваем рацию, чтобы ловить сигналы и записывать их в журнал!
pub fn setup_logging() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create("yuaiserver.log")?; // Создаём свиток для логов!
    let file_layer = fmt::layer()
        .with_writer(file) // Пишем в свиток!
        .with_ansi(false) // Без цветных чернил, чистый текст!
        .with_target(true) // Указываем, кто кричит!
        .with_level(true) // Уровень сигнала, как громкость!
        .with_thread_ids(true) // Номер юнги, кто на рации!
        .with_thread_names(true) // Имя юнги, для порядка!
        .with_filter(Targets::new()
            .with_target("KGB", LevelFilter::INFO) // Ловим сигналы KGB: кто подключился, где шныряет!
            .with_target("console", LevelFilter::INFO) // Ловим команды капитана с консоли!
            .with_default(LevelFilter::ERROR)); // Всё остальное — только шторма!

    let console_layer = fmt::layer()
        .with_writer(std::io::stdout) // Кричим в эфир, на консоль!
        .without_time() // Без часов, время в космосе не главное!
        .with_target(false) // Без лишних меток, чистый крик!
        .with_level(false) // Без громкости, всё важно!
        .with_filter(Targets::new()
            .with_target("console", LevelFilter::TRACE) // Ловим все команды капитана!
            .with_default(LevelFilter::OFF)); // Остальное молчит!

    tracing_subscriber::registry()
        .with(console_layer) // Включаем рацию для консоли!
        .with(file_layer) // Включаем свиток для журнала!
        .init(); // Зажигаем рацию!

    info!(target: "console", "{}", "Рация на мостике!".bright_magenta()); // Первый крик: мы на связи!
    Ok(()) // Рация гудит, всё готово!
}
