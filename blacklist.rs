use redis::AsyncCommands;

const MAX_FAILED_ATTEMPTS: usize = 5;

#[derive(Clone)]
pub struct Blacklist {
    client: redis::Client,
}

impl Blacklist {
    pub async fn new() -> Self {
        let client = redis::Client::open("redis://127.0.0.1/").unwrap();
        Blacklist { client }
    }

    pub async fn is_blacklisted(&self, ip: &str) -> bool {
        let mut con = self.client.get_async_connection().await.unwrap();
        let exists: bool = con.sismember("blacklist", ip).await.unwrap();
        exists
    }

    pub async fn add(&self, ip: &str) {
        let mut con = self.client.get_async_connection().await.unwrap();
        let _: () = con.sadd("blacklist", ip).await.unwrap();
    }

    pub async fn increment_failed_attempts(&self, ip: &str) {
        let mut con = self.client.get_async_connection().await.unwrap();
        let key = format!("failed_attempts:{}", ip);
        let attempts: i32 = con.incr(&key, 1).await.unwrap();
        con.expire(&key, 600).await.unwrap(); // 10 минут хранения
        if attempts >= MAX_FAILED_ATTEMPTS as i32 {
            self.add(ip).await;
        }
    }
}
