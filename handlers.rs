use hyper::{Body, Request, Response, StatusCode, header::SERVER};
use crate::blacklist::Blacklist;
use crate::auth::is_authorized;
use std::net::SocketAddr;

const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024; // 10MB
const MAX_BODY_LENGTH: usize = 8192; // 8KB

pub async fn http_handler(req: Request<Body>, blacklist: Blacklist, peer_addr: SocketAddr) -> Result<Response<Body>, hyper::Error> {
    let mut response = Response::new(Body::empty());
    response.headers_mut().insert(SERVER, hyper::header::HeaderValue::from_static("YUAI SERVER"));
    
    let content_length = req.body().size_hint().upper().unwrap_or(0);
    if content_length > MAX_REQUEST_SIZE {
        return Ok(Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .body(Body::from("Payload Too Large"))?
        );
    }

    let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
    if body_bytes.len() > MAX_BODY_LENGTH {
        return Ok(Response::builder()
            .status(StatusCode::PAYLOAD_TOO_LARGE)
            .body(Body::from("Request body too long"))?
        );
    }

    match (req.method(), req.uri().path()) {
        (&hyper::Method::GET, "/") => {
            *response.body_mut() = Body::from("Welcome to YUAI SERVER");
        }
        (&hyper::Method::POST, "/blacklist/add") => {
            if !is_authorized(&req) {
                blacklist.increment_failed_attempts(&peer_addr.to_string()).await;
                return Ok(Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::from("Unauthorized"))?
                );
            }

            let ip_to_block = String::from_utf8(body_bytes.to_vec()).unwrap();
            blacklist.add(&ip_to_block).await;
            *response.body_mut() = Body::from(format!("IP {} added to blacklist", ip_to_block));
        }
        _ => {
            *response.status_mut() = StatusCode::NOT_FOUND;
            *response.body_mut() = Body::from("404 Not Found");
        }
    }
    
    Ok(response)
}
