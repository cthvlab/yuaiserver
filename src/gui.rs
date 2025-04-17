// Голографический интерфейс космопорта YUAI! Звёзды, планеты и корабли на твоей ладони, капитан!
use std::sync::Arc; // Общий штурвал для юнг, чтоб держали курс!
use eframe::{egui, App, Frame}; // Двигатель голограммы, рисуем звёзды и туманности!
use egui::{Context, RichText, Margin}; // Инструменты для голограммы, текст и отступы!
use crate::ProxyState; // Штурвал космопорта, всё под контролем!
use rand::Rng; // Генератор хаоса, как шторм в космосе!
use egui::epaint::{Mesh, Vertex, Shape}; // Кисти для рисования звёzd и кораблей!
use std::collections::HashMap; // Сундук для кораблей, каждый со своим IP!
use std::f32::consts::PI; // Число Пи, как древний компас для орбит!
use crate::kgb::KGBConfig; // Кодекс секретной службы, пропуска и лимиты!

// Запускаем голографический интерфейс, полный вперёд!
pub async fn run_gui(state: Arc<ProxyState>) -> Result<(), Box<dyn std::error::Error>> {
    let native_options = eframe::NativeOptions::default(); // Настройки голограммы по умолчанию!
    eframe::run_native(
        "YUAI CosmoPort GUI", // Имя нашего звездолёта!
        native_options,
        Box::new(move |_cc| Box::new(CosmoPortApp::new(state.clone()))), // Создаём голограмму!
    )?;
    Ok(()) // Голограмма зажглась, звёзды сияют!
}

// Звезда на небе, мерцает и движется!
struct Star {
    x: f32, // Координата X, как угол в галактике!
    y: f32, // Координата Y, как высота орбиты!
    z: f32, // Глубина, как расстояние до звезды!
    speed: f32, // Скорость, как ветер в парусах!
}

// Планета на карте, как звезда с маршрутом!
struct Planet {
    px: f32, // Координата X, центр орбиты!
    py: f32, // Координата Y, высота в космосе!
    radius: f32, // Размер планеты, как её масса!
    color: egui::Color32, // Цвет, как свет звезды!
    path: String, // Путь, как маршрут на карте!
    response: String, // Добыча, что ждёт гостей!
}

// Космический корабль, летит к планете!
struct Ship {
    angle: f32, // Угол орбиты, как поворот штурвала!
    radius: f32, // Расстояние от планеты, как орбита!
    center_px: f32, // Центр X, куда летим!
    center_py: f32, // Центр Y, курс на звезду!
    color: egui::Color32, // Цвет, как флаг корабля!
    ip: String, // IP гостя, как имя капитана!
    target_path: String, // Куда летим, маршрут!
    target_px: f32, // Цель X, координаты звезды!
    target_py: f32, // Цель Y, высота звезды!
    speed: f32, // Скорость, как гипердвигатель!
}

// Главный пульт голограммы, капитан на мостике!
pub struct CosmoPortApp {
    state: Arc<ProxyState>, // Штурвал космопорта!
    stars: Vec<Star>, // Небо полное звёzd!
    planets: Vec<Planet>, // Планеты, как точки на карте!
    ships: HashMap<String, Ship>, // Флот кораблей, каждый со своим IP!
    selected_planet: Option<(String, String, f32, egui::Pos2)>, // Выбранная планета, её путь, ответ, радиус и позиция!
    cached_config: Option<KGBConfig>, // Кэш кодекса KGB, чтоб не запрашивать лишний раз!
}

impl CosmoPortApp {
    // Создаём новую голограмму, звёзды зажигаются!
    pub fn new(state: Arc<ProxyState>) -> Self {
        let mut rng = rand::thread_rng(); // Генератор хаоса, как шторм!
        // Создаём звёзды, как россыпь в ночи!
        let stars = (0..1000)
            .map(|_| Star {
                x: rng.gen_range(-1.0..1.0), // Случайный угол!
                y: rng.gen_range(-1.0..1.0), // Случайная высота!
                z: rng.gen_range(0.1..1.0), // Глубина, чтоб мерцать!
                speed: rng.gen_range(0.00005..0.0002), // Скорость мерцания!
            })
            .collect();

        // Достаём маршруты, как карту звёzd!
        let locations = futures::executor::block_on(state.locations.read()).clone();

        // Цвета планет, как палитра космоса!
        let colors = [
            egui::Color32::from_rgb(180, 150, 220), // Фиолетовый, как туманность!
            egui::Color32::from_rgb(140, 255, 200), // Зелёный, как изумруд!
            egui::Color32::from_rgb(255, 200, 100), // Жёлтый, как звезда!
            egui::Color32::from_rgb(200, 100, 255), // Пурпурный, как галактика!
        ];

        let mut planets: Vec<Planet> = Vec::new();
        let main_page_color = egui::Color32::from_rgb(100, 180, 255); // Голубой для главной звезды!

        // Главная планета, если есть маршрут "/"!
        for loc in &locations {
            if loc.path == "/" {
                planets.push(Planet {
                    px: 0.5, // Центр космоса!
                    py: 0.5, // Высота орбиты!
                    radius: 0.03, // Большая звезда!
                    color: main_page_color, // Голубой свет!
                    path: loc.path.clone(), // Путь звезды!
                    response: loc.response.clone(), // Добыча звезды!
                });
                break;
            }
        }

        // Остальные планеты, как спираль галактики!
        let other_locations: Vec<_> = locations.iter().filter(|loc| loc.path != "/").collect();
        let spiral_radius = 0.1; // Начальный радиус спирали!
        let spiral_growth = 0.05; // Рост спирали, как волна!
        let angle_step = 2.0 * PI / 3.0; // Шаг угла, как поворот штурвала!

        for (i, loc) in other_locations.iter().enumerate() {
            let angle = i as f32 * angle_step; // Угол орбиты!
            let radius = spiral_radius + (i as f32 * spiral_growth); // Радиус спирали!
            let px = 0.5 + radius * angle.cos(); // Координата X!
            let py = 0.5 + radius * angle.sin(); // Координата Y!

            planets.push(Planet {
                px: px.clamp(0.1, 0.9), // Не улететь за край космоса!
                py: py.clamp(0.1, 0.9), // Держим орбиту!
                radius: 0.015, // Меньшая планета!
                color: colors[i % colors.len()], // Цвет из палитры!
                path: loc.path.clone(), // Путь звезды!
                response: loc.response.clone(), // Добыча звезды!
            });
        }

        // Достаём кодекс KGB для кэша!
        let cached_config = futures::executor::block_on(state.kgb.get_config());

        Self {
            state, // Штурвал космопорта!
            stars, // Небо звёzd!
            planets, // Карта планет!
            ships: HashMap::new(), // Флот пуст, ждём гостей!
            selected_planet: None, // Пока нет выбранной звезды!
            cached_config: Some(cached_config), // Кэш кодекса!
        }
    }

    // Синхронизируем флот кораблей с гостями космопорта!
    fn sync_ships(&mut self) {
        let urls: Vec<(String, String)> = self.state.kgb.get_sessions(); // Кто шныряет по орбите?
        for (ip, url) in urls {
            // Ищем планету, к которой летит корабль!
            if let Some(planet) = self.planets.iter().filter(|p| url.starts_with(&p.path)).max_by_key(|p| p.path.len()) {
                let ship = self.ships.entry(ip.clone()).or_insert_with(|| {
                    // Новый корабль, готов к полёту!
                    Ship {
                        angle: rand::thread_rng().gen_range(0.0..2.0 * PI), // Случайный угол орбиты!
                        radius: planet.radius + 0.01, // Орбита чуть дальше планеты!
                        center_px: rand::thread_rng().gen_range(0.0..1.0), // Случайная точка старта!
                        center_py: rand::thread_rng().gen_range(0.0..1.0), // Случайная высота!
                        target_px: planet.px, // Цель — планета!
                        target_py: planet.py, // Высота цели!
                        target_path: planet.path.clone(), // Маршрут к звезде!
                        color: planet.color, // Цвет, как у планеты!
                        ip: ip.clone(), // Имя капитана!
                        speed: 0.01, // Гипердвигатель на малой тяге!
                    }
                });

                // Если корабль сменил цель, обновляем курс!
                if ship.target_path != planet.path {
                    ship.target_px = planet.px; // Новая цель X!
                    ship.target_py = planet.py; // Новая цель Y!
                    ship.target_path = planet.path.clone(); // Новый маршрут!
                    ship.color = planet.color; // Новый цвет флага!
                }
            }
        }
    }

    // Рисуем звёзды, как мерцание в ночи!
    fn draw_stars(&mut self, painter: &egui::Painter, w: f32, h: f32, cx: f32, cy: f32) {
        for star in &mut self.stars {
            star.z -= star.speed; // Звезда приближается!
            if star.z <= 0.01 {
                // Звезда слишком близко, телепортируем её назад!
                star.x = rand::thread_rng().gen_range(-1.0..1.0);
                star.y = rand::thread_rng().gen_range(-1.0..1.0);
                star.z = 1.0;
            }
            let sx = cx + star.x / star.z * w; // Проецируем звезду на экран!
            let sy = cy + star.y / star.z * h;
            if sx >= 0.0 && sx <= w && sy >= 0.0 && sy <= h {
                let size = (0.4 / star.z).min(2.5); // Размер звезды, чем ближе, тем ярче!
                painter.circle_filled(egui::pos2(sx, sy), size, egui::Color32::WHITE); // Рисуем звезду!
            }
        }
    }

    // Рисуем планеты, как маяки на карте!
    fn draw_planets(&mut self, painter: &egui::Painter, w: f32, h: f32, ctx: &Context) {
        for planet in &self.planets {
            let pos = egui::pos2(planet.px * w, planet.py * h); // Позиция планеты!
            let radius = planet.radius * w; // Размер планеты!
            let rect = egui::Rect::from_center_size(pos, egui::vec2(radius * 2.0, radius * 2.0)); // Зона клика!

            // Проверяем, кликнул ли капитан на планету!
            if ctx.input(|i| i.pointer.any_click() && rect.contains(i.pointer.interact_pos().unwrap_or_default())) {
                if let Some((ref path, _, _, _)) = self.selected_planet {
                    if path == &planet.path {
                        self.selected_planet = None; // Снимаем выбор, звезда погасла!
                    } else {
                        self.selected_planet = Some((planet.path.clone(), planet.response.clone(), radius, pos)); // Новая звезда выбрана!
                    }
                } else {
                    self.selected_planet = Some((planet.path.clone(), planet.response.clone(), radius, pos)); // Первая звезда зажглась!
                }
            }

            painter.circle_filled(pos, radius, planet.color); // Рисуем планету, как яркий маяк!
        }
    }

    // Рисуем флот кораблей, как танец в космосе!
    fn draw_ships(&mut self, painter: &egui::Painter, w: f32, h: f32) {
        for ship in self.ships.values_mut() {
            // Движемся к цели, как звездолёт к планете!
            ship.center_px += (ship.target_px - ship.center_px) * ship.speed;
            ship.center_py += (ship.target_py - ship.center_py) * ship.speed;
            ship.angle += 0.01; // Вращаем орбиту, как штурвал!
            let x = (ship.center_px + ship.radius * ship.angle.cos()) * w; // Позиция X!
            let y = (ship.center_py + ship.radius * ship.angle.sin()) * h; // Позиция Y!

            // Создаём треугольник для корабля, как силуэт в космосе!
            let mut mesh = Mesh::default();
            let color = ship.color; // Цвет корабля!
            let p1 = egui::pos2(x, y); // Нос корабля!
            let p2 = egui::pos2(x - 5.0, y + 10.0); // Левый борт!
            let p3 = egui::pos2(x + 5.0, y + 10.0); // Правый борт!
            let base = mesh.vertices.len() as u32;

            mesh.vertices.push(Vertex { pos: p1, uv: egui::epaint::WHITE_UV, color }); // Нос!
            mesh.vertices.push(Vertex { pos: p2, uv: egui::epaint::WHITE_UV, color }); // Левый борт!
            mesh.vertices.push(Vertex { pos: p3, uv: egui::epaint::WHITE_UV, color }); // Правый борт!

            mesh.indices.extend([base, base + 1, base + 2]); // Соединяем точки!
            painter.add(Shape::mesh(mesh)); // Рисуем корабль!

            // Пишем имя капитана рядом с кораблём!
            painter.text(
                egui::pos2(x + 8.0, y),
                egui::Align2::LEFT_CENTER,
                &ship.ip,
                egui::TextStyle::Body.resolve(&egui::Style::default()),
                ship.color,
            );
        }
    }
}

impl App for CosmoPortApp {
    // Обновляем голограмму, звёзды танцуют!
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        ctx.request_repaint(); // Постоянно обновляем, как звёздное небо!

        let screen_rect = ctx.screen_rect(); // Размер космоса!
        let (w, h) = (screen_rect.width(), screen_rect.height()); // Ширина и высота экрана!
        let cx = w / 2.0; // Центр X!
        let cy = h / 2.0; // Центр Y!

        self.sync_ships(); // Синхронизируем флот с гостями!

        let painter = ctx.layer_painter(egui::LayerId::background()); // Кисть для космоса!
        painter.rect_filled(screen_rect, 0.0, egui::Color32::BLACK); // Чёрный космос, как ночь!

        self.draw_planets(&painter, w, h, ctx); // Рисуем планеты!
        self.draw_stars(&painter, w, h, cx, cy); // Рисуем звёзды!
        self.draw_ships(&painter, w, h); // Рисуем корабли!

        // Центральная панель, как мостик капитана!
        egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
            // Заголовок, как флаг на мачте!
            ui.heading(RichText::new("Космопорт на орбите, капитан!").color(egui::Color32::from_rgb(180, 120, 255)));
            ui.separator(); // Линия горизонта!

            let state = self.state.clone(); // Копия штурвала для юнги!
            tokio::spawn(async move {
                let config = state.kgb.get_config().await; // Достаём кодекс!
                tracing::debug!("Конфигурация обновлена: {:?}", config.http_port); // Шёпот в рацию!
            });

            // Панель управления, как штурвал!
            ui.horizontal(|ui| {
                let config = self.cached_config.clone().unwrap_or_else(|| {
                    tracing::warn!("Конфигурация ещё не загружена, используем заглушку!"); // Кодекс пропал!
                    KGBConfig {
                        jwt_secret: String::new(), // Пустой пропуск!
                        guest_rate_limit: 0, // Без лимитов!
                        whitelist_rate_limit: 0, // Друзья без лимитов!
                        blacklist_rate_limit: 0, // Шпионы без лимитов!
                        rate_limit_window: 0, // Без вахты!
                        trusted_host: String::new(), // Без порта!
                        max_request_body_size: 1024 * 1024, // Трюм на миллион!
                        http_port: 8080, // Шлюпки по умолчанию!
                        https_port: 8443, // Крейсеры по умолчанию!
                        quic_port: 8444, // Звездолёты по умолчанию!
                    }
                });
                // Показываем порты, как маяки!
                ui.label(format!("HTTP {}", config.http_port));
                ui.label(format!("HTTPS {}", config.https_port));
                ui.label(format!("QUIC {}", config.quic_port));

                // Кнопка перезапуска, как рычаг гипердвигателя!
                if ui.button("Перезапуск!").clicked() {
                    self.ships.clear(); // Флот за борт, новый рейд!
                    let state = self.state.clone(); // Копия штурвала!
                    tokio::spawn(async move {
                        let config = crate::load_config().await.unwrap_or_else(|_| {
                            tracing::warn!("Не удалось загрузить config.toml, используем заглушку!"); // Карта пропала!
                            crate::Config::default() // Запасная карта!
                        });
                        // Гасим двигатели!
                        *state.http_running.write().await = false;
                        *state.https_running.write().await = false;
                        *state.quic_running.write().await = false;
                        if let Some(h) = state.http_handle.lock().await.take() { h.abort(); } // Шлюпки заглушены!
                        if let Some(h) = state.https_handle.lock().await.take() { h.abort(); } // Крейсеры заглушены!
                        if let Some(h) = state.quic_handle.lock().await.take() { h.abort(); } // Звездолёты заглушены!

                        // Запускаем новые двигатели!
                        *state.http_handle.lock().await = Some(tokio::spawn(crate::run_http_server(config.clone(), state.clone()))); // Новые шлюпки!
                        *state.https_handle.lock().await = Some(tokio::spawn(crate::run_https_server(state.clone(), config.clone()))); // Новые крейсеры!
                        *state.quic_handle.lock().await = Some(tokio::spawn(crate::run_quic_server(config.clone(), state.clone()))); // Новые звездолёты!
                    });
                }
            });

            // Окно с информацией о планете, как свиток с добычей!
            if let Some((ref path, ref response, radius, pos)) = self.selected_planet {
                let window_width = w * 0.22; // Окно шире, как трюм!
                let window_height = h * 0.2; // Высота, как орбита!
                let rounding = window_width * 0.15; // Закругления, как туманность!
                let padding = window_width * 0.05; // Отступы, как штормовой зазор!

                // Позиция окна под планетой, как маяк!
                let window_x = pos.x;
                let window_y = pos.y + radius + 10.0; // 10 пикселей ниже планеты!

                // Не даём окну улететь за край космоса!
                let window_x = window_x.clamp(0.0, w - window_width);
                let window_y = window_y.clamp(0.0, h - window_height);

                // Создаём окно, как голограмму!
                egui::Window::new(RichText::new(format!("{}", path)).strong().color(egui::Color32::WHITE))
                    .fixed_size([window_width, window_height]) // Фиксированный размер!
                    .fixed_pos([window_x, window_y]) // Фиксированная позиция!
                    .collapsible(false) // Не сворачиваем, капитан!
                    .resizable(false) // Не тянем, это не паруса!
                    .frame(egui::Frame {
                        inner_margin: Margin::same(padding), // Отступы внутри!
                        rounding: egui::Rounding::same(rounding), // Закругления!
                        fill: egui::Color32::from_black_alpha(200), // Полупрозрачный чёрный, как космос!
                        ..Default::default()
                    })
                    .show(ctx, |ui| {
                        ui.style_mut().spacing.item_spacing = egui::vec2(0.0, 8.0); // Расстояние между строками!
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.label(RichText::new(format!("{}", response)).color(egui::Color32::WHITE)); // Добыча планеты!
                        });
                    });
            }
        });
    }

    // Закрываем голограмму, звёзды гаснут!
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        tracing::info!("Окно закрыто. Завершение GUI..."); // Рация: голограмма погасла!
    }
}
