mod blacklist;
mod auth;
mod handlers;

use hyper::{service::{make_service_fn, service_fn}, Server};
use tokio::net::TcpListener;
use std::net::SocketAddr;
use handlers::http_handler;
use blacklist::Blacklist;

#[tokio::main]
async fn main() {
    let addr: SocketAddr = "0.0.0.0:443".parse().unwrap();
    let listener = TcpListener::bind(addr).await.unwrap();
    let blacklist = Blacklist::new().await;

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
