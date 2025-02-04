use hyper::{service::{make_service_fn, service_fn}, Body, Request, Response, Server, header::{HeaderValue, SERVER, AUTHORIZATION}, Error, Method, StatusCode};
use hyper::server::conn::AddrStream;
use hyper_tls::TlsAcceptor;
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use futures_util::{StreamExt, SinkExt, TryStreamExt};
use std::{sync::{Arc, Mutex}, collections::HashMap, time::{Instant, Duration}, net::SocketAddr};
use quinn::{Endpoint, ServerConfig};
use rustls::{Certificate, PrivateKey};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::time::sleep;
use redis::AsyncCommands;

const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024;
const MAX_BODY_LENGTH: usize = 8192;
const ADMIN_USERNAME: &str = "admin";
const ADMIN_PASSWORD: &str = "password";
const MAX_FAILED_ATTEMPTS: usize = 5;
const REDIS_STREAM: &str = "request_queue";

#[derive(Clone)]
struct Blacklist {
    client: redis::Client,
}

impl Blacklist {
    async fn new() -> Self {
        let client = redis::Client::open("redis://127.0.0.1/").unwrap();
        Blacklist { client }
    }

    async fn is_blacklisted(&self, ip: &str) -> bool {
        let mut con = self.client.get_async_connection().await.unwrap();
        let exists: bool = con.sismember("blacklist", ip).await.unwrap();
        exists
    }

    async fn add(&self, ip: &str) {
        let mut con = self.client.get_async_connection().await.unwrap();
        let _: () = con.sadd("blacklist", ip).await.unwrap();
    }
}

async fn push_to_queue(ip: String) {
    let client = redis::Client::open("redis://127.0.0.1/").unwrap();
    let mut con = client.get_async_connection().await.unwrap();
    let _: () = con.xadd(REDIS_STREAM, "*", &[("ip", ip)]).await.unwrap();
}

async fn process_queue(blacklist: Blacklist) {
    let client = redis::Client::open("redis://127.0.0.1/").unwrap();
    let mut con = client.get_async_connection().await.unwrap();

    loop {
        let entries: Vec<redis::streams::StreamReadReply> = con.xread(&[REDIS_STREAM], &[">"]).await.unwrap();

        for entry in entries {
            for message in entry.ids {
                if let Some(ip) = message.get("ip") {
                    let ip_str = ip.to_string();
                    println!("Processing IP from queue: {}", ip_str);
                    blacklist.add(&ip_str).await;
                }
            }
        }

        sleep(Duration::from_secs(1)).await;
    }
}

async fn http_handler(req: Request<Body>, blacklist: Blacklist, peer_addr: SocketAddr) -> Result<Response<Body>, Error> {
    let mut response = Response::new(Body::empty());
    response.headers_mut().insert(SERVER, HeaderValue::from_static("YUAI SERVER"));
    
    let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
    if body_bytes.len() > MAX_BODY_LENGTH {
        return Ok(Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .body(Body::from("Request body too long"))?
        );
    }

    match (req.method(), req.uri().path()) {
        (&Method::POST, "/blacklist/add") => {
            let ip_to_block = String::from_utf8(body_bytes.to_vec()).unwrap();
            push_to_queue(ip_to_block).await;
            *response.body_mut() = Body::from("IP added to processing queue");
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
            *response.body_mut() = Body::from("404 Not Found");
        }
    }
    
    Ok(response)
}

async fn start_http_server(blacklist: Blacklist) {
    let addr: SocketAddr = "0.0.0.0:443".parse().unwrap();
    let listener = TcpListener::bind(addr).await.unwrap();

    println!("HTTP/1, HTTP/2 и WebSocket сервер запущен на {}", addr);

    loop {
        let (stream, peer_addr) = listener.accept().await.unwrap();
        let blacklist = blacklist.clone();

        tokio::spawn(async move {
            if blacklist.is_blacklisted(&peer_addr.to_string()).await {
                eprintln!("Blocked IP: {}", peer_addr);
                return;
            }

            let service = make_service_fn(move |_| {
                let blacklist = blacklist.clone();
                let peer_addr = peer_addr.clone();
                async { Ok::<_, hyper::Error>(service_fn(move |req| http_handler(req, blacklist.clone(), peer_addr))) }
            });

            if let Err(e) = Server::builder(hyper::server::accept::from_stream(tokio::stream::once(async { Some(Ok(stream)) })))
                .serve(service).await
            {
                eprintln!("Ошибка сервера: {}", e);
            }
        });
    }
}

async fn start_http3_server() {
    let addr: SocketAddr = "0.0.0.0:4433".parse().unwrap();
    let (endpoint, _server_cert) = setup_quic_server().await;

    println!("HTTP/3 сервер запущен на {}", addr);

    loop {
        if let Some(connecting) = endpoint.accept().await {
            tokio::spawn(async move {
                match connecting.await {
                    Ok(new_conn) => {
                        println!("Новое HTTP/3 соединение!");
                        let _ = new_conn.bi_streams.for_each_concurrent(None, |stream| async {
                            if let Ok((mut send, _recv)) = stream {
                                let _ = send.write_all(b"Welcome to HTTP/3 server").await;
                            }
                        }).await;
                    }
                    Err(e) => eprintln!("Ошибка HTTP/3 соединения: {}", e),
                }
            });
        }
    }
}

async fn setup_quic_server() -> (Endpoint, Vec<u8>) {
    let cert = fs::read("cert.pem").await.unwrap();
    let key = fs::read("key.pem").await.unwrap();

    let certs = vec![Certificate(cert.clone())];
    let priv_key = PrivateKey(key);

    let mut server_config = ServerConfig::with_single_cert(certs, priv_key).unwrap();
    server_config.transport = Arc::new(quinn::TransportConfig::default());

    let endpoint = Endpoint::server(server_config, "0.0.0.0:4433".parse().unwrap()).unwrap();
    (endpoint, cert)
}

#[tokio::main]
async fn main() {
    let blacklist = Blacklist::new().await;

    // Фоновый обработчик очереди
    let blacklist_clone = blacklist.clone();
    tokio::spawn(async move {
        process_queue(blacklist_clone).await;
    });

    // HTTP/1, HTTP/2 и WebSocket сервер
    let blacklist_clone = blacklist.clone();
    tokio::spawn(async move {
        start_http_server(blacklist_clone).await;
    });

    // HTTP/3 сервер
    start_http3_server().await;
}
