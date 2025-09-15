mod handler;
mod mojang;
mod packet;
mod store;

use base64::prelude::*;
use dotenvy::dotenv;
use redis::Commands;
use std::env;
use std::io;
use std::net::TcpListener;
use std::sync::Arc;

#[tokio::main]
async fn main() -> io::Result<()> {
    // Load environment variables
    let _ = dotenv();

    let port: u16 = env::var("SERVER_PORT")
        .expect("SERVER_PORT must be set in .env or environment")
        .parse()
        .expect("SERVER_PORT must be a valid u16");

    let favicon = std::fs::read("server-icon.png")
        .map(|png| format!("data:image/png;base64,{}", BASE64_STANDARD.encode(&png)))
        .ok();

    if favicon.is_none() {
        println!("No server-icon.png found, skipping favicon.");
    }

    let redis_url =
        env::var("REDIS_URL").expect("REDIS_URL must be set in .env file or environment");

    let redis_client = redis::Client::open(redis_url)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Redis client failed: {}", e)))?;

    // Test connection
    let mut con = redis_client.get_connection().map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Redis connection test failed: {}", e),
        )
    })?;

    let _: () = con
        .set("wynnpool:test", "connection_ok")
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Redis test failed: {}", e)))?;

    println!("Redis connection established successfully");

    let listener = TcpListener::bind(("0.0.0.0", port))?;
    println!("Wynnpool Verification Server listening on port {}", port);

    let favicon_arc = Arc::new(favicon);
    let redis_arc = Arc::new(redis_client);

    for stream in listener.incoming() {
        let favicon_clone = Arc::clone(&favicon_arc);
        let redis_clone = Arc::clone(&redis_arc);

        match stream {
            Ok(stream) => {
                std::thread::spawn(move || {
                    if let Err(e) =
                        handler::handle_client(stream, favicon_clone.as_ref().clone(), redis_clone)
                    {
                        eprintln!("Error handling client: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Connection failed: {}", e);
            }
        }
    }

    Ok(())
}
