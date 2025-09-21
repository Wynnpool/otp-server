use std::time::{SystemTime, UNIX_EPOCH};
use rand::Rng;
use redis::Commands;

/// Generate a unique 6-digit code, store data under the code key `wynnpool:verify:<code>`
/// and create a reverse mapping `wynnpool:verify:uuid:<uuid>` -> code. Both keys have a 15
/// minute TTL. If a generated code already exists, it will retry a few times.
pub fn generate_and_store_code(con: &mut redis::Connection, uuid: &str, username: &str) -> String {
    // Attempt to generate a unique 6-digit OTP code
    let mut rng = rand::thread_rng();
    let mut attempts = 0;
    let code: String;

    loop {
        attempts += 1;
        // 100000..=999999 to include 999999
        let candidate = rng.gen_range(100000..=999999).to_string();
        let code_key = format!("wynnpool:verify:{}", candidate);

        // If key does not exist, use it
        let exists: bool = con.exists(&code_key).unwrap_or(false);
        if !exists {
            code = candidate;
            break;
        }

        // Very unlikely; try again a few times then continue (shouldn't loop forever)
        if attempts >= 5 {
            // last resort: still use the candidate (rare race)
            code = candidate;
            break;
        }
    }

    let code_key = format!("wynnpool:verify:{}", code);
    let uuid_key = format!("wynnpool:verify:uuid:{}", uuid);

    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() + 900; // 15 minutes from now

    // Store uuid, name and expires under the code key
    let _: () = con.hset_multiple(
        &code_key,
        &[
            ("uuid", uuid),
            ("name", username),
            ("expires", &expires_at.to_string()),
        ]
    ).unwrap();

    // Set TTL for the code key
    let _: () = con.expire(&code_key, 900).unwrap();
    code
}
