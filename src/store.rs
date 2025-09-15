use std::time::{SystemTime, UNIX_EPOCH};
use rand::Rng;
use redis::Commands;

pub fn generate_and_store_code(con: &mut redis::Connection, redis_key: &str) -> String {
    // Generate 6-digit OTP code
    let mut rng = rand::thread_rng();
    let code = rng.gen_range(100000..999999).to_string();
    
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() + 900; // 15 minutes from now
        
    // Store code and expiration
    let _: () = con.hset_multiple(
        redis_key,
        &[
            ("code", &code),
            ("expires", &expires_at.to_string()),
        ]
    ).unwrap();
    
    // Set expiration
    let _: () = con.expire(redis_key, 900).unwrap();
    
    code
}
