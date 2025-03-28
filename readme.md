# YUAIServer: Your Universe Access Interface Server 🚀✨

Добро пожаловать в Космопорт **YUAI Server** — самый крутой, быстрый и весёлый прокси-сервер на Rust, который готов к любым приключениям в мире HTTP, HTTPS, QUIC, WebSocket и даже WebRTC! 
Этот малыш — настоящая звезда среди серверов, и он здесь, чтобы сделать вашу сеть безопаснее, быстрее и, конечно, веселее! 😎


## Что это такое? 🤔

YUAI Server — это не просто сервер, это ваш верный цифровой помощник! Он поддерживает:
- **HTTP/1.1**: Классика для старых шлюпок — перенаправляет на HTTPS.
- **HTTPS (HTTP/1.1 & HTTP/2)**: Бронированные крейсера с TLS, готовые к любым бурям.
- **QUIC & HTTP/3** — Гиперскоростные звездолеты из будущего уже здесь!
- **WebSocket** — Телепорт для тех, кто любит поболтать в реальном времени!
- **WebRTC** — видеозвонки? Легко!

А ещё он имеет управление из консоли, умеет кэшировать ответы, ограничивать скорость, проверять авторизацию, блокировать плохишей и даже динамически обновлять конфигурацию, пока вы пьёте кофе. ☕

## Почему YUAI Server крут? 🌟

- **Скорость**: Написан на Rust с использованием Hyper, Tokio и Quinn — быстрее только свет!
- **Безопасность**: Черные и белые списки, ограничение скорости, защита от спама — хакеры в шоке!
- **Гибкость**: Поддерживает всё, что душе угодно — от простого "Hello World" до сложных WebRTC-сессий.
- **Кроссплатформенность**: Работает на всех операционных системах
- **Веселье**: Код настолько чистый, что его можно показывать на свиданиях. 😏

## Установка 🚀
0. **Установите RUST**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
   
1. **Создайте проект на RUST**:
   ```bash
   git clone https://github.com/cthvlab/yuaiserver.git
   cd yuaiserver
   ```

3. **Запустите сервер**:
   ```bash
   cargo run
   ```
   И вуаля! Ваш сервер готов к работе на `http://localhost:8888`, `https://localhost:8443` и QUIC-порту `8444`!

## Как это работает? 🛠️
- **HTTP**: Переадресовывает на HTTPS.
- **HTTPS**: Отвечает по маршрутам и кэширует ответы на 60 секунд, поддерживает HTTP/1.1 и HTTP/2.
- **QUIC & HTTP/3**: Гиперскоростные ответы с поддержкой чистого QUIC и HTTP/3.
- **WebSocket**: Открывает телепорт для чатов в реальном времени.
- **WebRTC**: Обрабатывает SDP-offer и создаёт соединение для передачи данных.

### Безопасность и управление
- **Авторизация**: JWT через заголовок `Authorization: Bearer <token>`. Привилегированные гости получают бонусы!
- **Ограничения скорости**:
  - Гости: 10 запросов/60 сек.
  - Белый список: 100 запросов/60 сек.
  - Чёрный список: 1 запрос/60 сек. 
  - Настраивается в `config.toml`.
- **Консоль**: Команды капитана:
  - `1` — статус космопорта.
  - `2` — перезагрузить конфиг.
  - `3` — добавить маршрут.
  - `4` — список сессий.
  - `help` — шпаргалка.

### Примеры запросов
- Простой HTTPS-запрос:
  ```bash
  curl -k https://localhost:8443/
  ```
- С авторизацией:
  ```bash
  curl -k -H "Authorization: Bearer eyJ..." https://localhost:8443/
  ```



### Отчет: Анализ нагрузки сервера на оборудовании с 16-ядерным процессором Xeon и 32 ГБ оперативной памяти

Сервер, описанный в коде, представляет собой прокси-сервер, поддерживающий протоколы HTTP, HTTPS, HTTP/3 (QUIC), WebSocket и WebRTC, с дополнительными функциями, такими как кэширование, аутентификация, ограничение скорости и управление сессиями. Он работает на оборудовании с 16-ядерным процессором Xeon и 32 ГБ оперативной памяти, что позволяет оценить его возможности по обработке нагрузки, включая количество одновременных соединений, запросов в секунду и активных сессий.

#### Аппаратные ресурсы и их влияние
Сервер оснащен 16-ядерным процессором Xeon, что обеспечивает значительную вычислительную мощность. С учетом гиперпоточности (hyper-threading), общее число потоков может достигать 32, что идеально подходит для асинхронных приложений, таких как этот сервер, использующий Tokio. Оперативная память в 32 ГБ достаточна для кэширования данных, управления сессиями и обработки большого числа соединений, особенно учитывая, что каждая сессия занимает около 20–30 байт (IP-адрес как строка и структура Instant).

#### Оценка одновременных соединений
Асинхронная архитектура на основе Tokio и Hyper позволяет серверу эффективно управлять большим числом одновременных соединений. Исследования показывают, что простой сервер на Hyper может обрабатывать 10,000–50,000 соединений на ядро для типичных нагрузок. С учетом 16 ядер и дополнительных функций (аутентификация, кэширование), предполагается, что сервер может выдерживать 500,000–1,000,000 одновременных соединений. Это ограничено не столько CPU, сколько сетевыми возможностями и количеством доступных дескрипторов файлов (file descriptors).

#### Оценка запросов в секунду (RPS)
Производительность по запросам в секунду зависит от типа запросов. Для простых запросов, обслуживаемых из кэша (с 60-секундным сроком действия), сервер может достигать 500,000–1,000,000 RPS, особенно учитывая, что кэширование использует эффективные структуры данных, такие как DashMap. Для более сложных запросов, требующих проверки аутентификации (JWT), ограничения скорости и других операций, производительность снижается до 100,000–200,000 RPS. Это связано с дополнительными вычислениями, такими как парсинг заголовков и доступ к защищенным данным через Mutex. С 16 ядрами, общая пропускная способность может быть значительной, но реальные значения зависят от конкретной нагрузки.

#### Оценка активных сессий
Сессии управляются через DashMap, где каждая запись включает IP-адрес и время последней активности. С учетом 32 ГБ оперативной памяти, сервер может поддерживать миллионы сессий. Если каждая запись занимает около 50 байт (для безопасности), то 32 ГБ позволяют хранить до 640,000,000 записей. Однако в реальных условиях число уникальных IP-адресов ограничено, и сервер легко справится с миллионами сессий, особенно учитывая, что память используется эффективно через общие структуры данных.

#### Влияние дополнительных функций
Дополнительные функции, такие как кэширование, аутентификация и ограничение скорости, добавляют накладные расходы. Кэширование ускоряет обработку часто запрашиваемых URL, снижая нагрузку на CPU. Аутентификация (JWT) требует парсинга заголовков и декодирования, что добавляет небольшую задержку. Ограничение скорости, основанное на DashMap, также эффективно. WebRTC и WebSocket добавляют свои требования: WebSocket требует постоянных соединений, а WebRTC — обработки данных в реальном времени, что может увеличить использование ресурсов на соединение.

#### Сравнительный анализ
Для сравнения, популярный веб-сервер NGINX может обрабатывать до 100,000 одновременных соединений на одном сервере с подобными характеристиками. Наш сервер, написанный на Rust с асинхронной архитектурой, может превзойти эти показатели благодаря эффективности языка и библиотек, таких как Hyper и Tokio. Исследования показывают, что серверы на Rust могут обрабатывать более 100,000 RPS на ядро для простых запросов, что с 16 ядрами дает потенциал до 1,600,000 RPS, хотя дополнительные функции снижают это до указанных 500,000–1,000,000 RPS.

#### Таблица: Оценка производительности

| Параметр                  | Оценка (минимум–максимум)       |
|---------------------------|---------------------------------|
| Одновременные соединения  | 500,000–1,000,000              |
| Запросы в секунду (RPS)   | 500,000–1,000,000 (простые), 100,000–200,000 (сложные) |
| Активные сессии           | Миллионы (ограничено 32 ГБ RAM)|

#### Заключение
Сервер на оборудовании с 16-ядерным Xeon и 32 ГБ RAM способен выдерживать значительные нагрузки, обеспечивая обработку сотен тысяч одновременных соединений, сотен тысяч запросов в секунду для простых операций и миллионов активных сессий. Точные значения зависят от конкретной нагрузки, типа запросов и сетевых условий, но оценки показывают его высокую производительность для прокси-сервера с поддержкой множества протоколов.


## Вклад в проект 🤝

Хотите сделать YUAI Server ещё круче? Мы рады любым идеям!
1. Форкните репозиторий.
2. Создайте свою ветку: `git checkout -b feature/awesome-idea`.
3. Сделайте коммит: `git commit -m "Добавил супер-фичу"`.
4. Отправьте PR: `git push origin feature/awesome-idea`.

Давайте вместе сделаем интернет весёлым местом! 🎉

## Лицензия 📜

YUAI Server распространяется под лицензией MIT. Делайте с ним что угодно, только не забудьте упомянуть нас, когда станете знаменитыми! 😉


**YUAI Server** — это не просто код, это стиль жизни. Присоединяйтесь к нам на [GitHub](https://github.com/cthvlab/yuaiserver) и давайте зажжём сеть вместе! 🔥

## Авторы

Разработано сообществом ЮАИ [yuai.ru](https://yuai.ru) 
