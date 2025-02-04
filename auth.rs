use hyper::Request;
use hyper::header::AUTHORIZATION;

const ADMIN_USERNAME: &str = "admin";
const ADMIN_PASSWORD: &str = "password";

pub fn is_authorized(req: &Request<hyper::Body>) -> bool {
    if let Some(auth_header) = req.headers().get(AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            let expected_auth = format!("Basic {}", base64::encode(format!("{}:{}", ADMIN_USERNAME, ADMIN_PASSWORD)));
            return auth_str == expected_auth;
        }
    }
    false
}
