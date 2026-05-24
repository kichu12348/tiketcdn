use hmac::{Hmac, Mac, digest::KeyInit};
use sha2::Sha256;
use std::env;

type HmacSha256 = Hmac<Sha256>;

pub fn create_signature(filename: &str, expires: u64) -> String {
    let secret = env::var("SECRET").expect("SECRET env var not set");
    let key = secret.as_bytes();

    let mut mac = HmacSha256::new_from_slice(key).expect("failed to create mac");
    mac.update(filename.as_bytes());
    mac.update(expires.to_string().as_bytes());
    hex::encode(mac.finalize().into_bytes())
}
