use std::io::{self, Read, Write, Cursor};
use std::net::TcpStream;
use std::sync::Arc;
use byteorder::{BigEndian, ReadBytesExt};
use redis::Commands;
use serde_json::json;

use crate::packet::{read_varint, write_string, build_packet, build_kick_packet};
use crate::mojang::get_mojang_uuid;
use crate::store::generate_and_store_code;

pub fn handle_client(mut stream: TcpStream, favicon: Option<String>, redis_client: Arc<redis::Client>) -> io::Result<()> {
    // Read handshake packet fully (handles fragmented TCP reads)
    let packet = match crate::packet::read_packet_from_stream(&mut stream) {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            // Client closed connection early — treat as clean disconnect
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    let mut cursor = Cursor::new(&packet[..]);

    // Read handshake (packet payload already provided)
    let packet_id = read_varint(&mut cursor)?;
    if packet_id != 0x00 {
        return Ok(());
    }

    let _protocol_version = read_varint(&mut cursor)?;
    let addr_len = read_varint(&mut cursor)? as usize;
    cursor.set_position(cursor.position() + addr_len as u64);
    let _port = cursor.read_u16::<BigEndian>()?;
    let next_state = read_varint(&mut cursor)?;

    match next_state {
        1 => {
            // Status request
            let status_json = json!({
                // "version": {
                //     "name": "Wynnpool",
                //     "protocol": -1
                // },
                "description": {
                    "text": "              §6§lWynnpool§r §eVerification Server§r\n                    §7www.wynnpool.com§r",
                    "color": "white"
                },
                "favicon": favicon
            });

            let response = build_packet(0x00, write_string(&status_json.to_string()));
            stream.write_all(&response)?;

            // Wait for ping (read full packet)
            let ping_packet = match crate::packet::read_packet_from_stream(&mut stream) {
                Ok(p) => p,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
                Err(e) => return Err(e),
            };
            let mut ping_cursor = Cursor::new(&ping_packet[..]);

            let ping_id = read_varint(&mut ping_cursor)?;
            if ping_id != 0x01 {
                return Ok(());
            }

            let payload = ping_cursor.get_ref()[ping_cursor.position() as usize..].to_vec();
            let pong = build_packet(0x01, payload);
            stream.write_all(&pong)?;
        }
        2 => {
            // Login request
            // Read login packet fully
            let login_packet = match crate::packet::read_packet_from_stream(&mut stream) {
                Ok(p) => p,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
                Err(e) => return Err(e),
            };
            let mut login_cursor = Cursor::new(&login_packet[..]);

            let login_id = read_varint(&mut login_cursor)?;
            if login_id != 0x00 {
                return Ok(());
            }

            let name_len = read_varint(&mut login_cursor)? as usize;
            let mut name_buf = vec![0u8; name_len];
            login_cursor.read_exact(&mut name_buf)?;
            let player_name = String::from_utf8_lossy(&name_buf).to_string();

            // Create a runtime for the async call
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            // Get UUID from Mojang API
            let (uuid, verified_name) = match rt.block_on(get_mojang_uuid(&player_name)) {
                Ok((uuid, name)) => (uuid, name),
                Err(e) => {
                    println!("Failed to verify player {}: {}", player_name, e);
                    
                    // Kick with error message
                    let kick_reason = json!({
                        "text": "§cAuthentication failed§r\n§7Please try again with a valid Minecraft account.",
                        "color": "red"
                    });
                    
                    let kick_packet = build_kick_packet(&kick_reason.to_string());
                    stream.write_all(&kick_packet)?;
                    return Ok(());
                }
            };

            println!("Player {} (UUID: {}) connected", verified_name, uuid);

            // Connect to Redis
            let mut con = redis_client.get_connection().map_err(|e| {
                io::Error::new(io::ErrorKind::Other, format!("Redis connection failed: {}", e))
            })?;
            
            // Check if player already has a valid code by looking up uuid -> code mapping
            let uuid_key = format!("wynnpool:verify:uuid:{}", uuid);
            let mut code: Option<String> = con.get(&uuid_key).unwrap_or(None);

            if let Some(ref existing_code) = code {
                // Verify the code hasn't expired by checking the code hash
                let code_key = format!("wynnpool:verify:{}", existing_code);
                let existing_expires: Option<i64> = con.hget(&code_key, "expires").unwrap_or(None);

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;

                if let Some(expires) = existing_expires {
                    if now < expires {
                        println!("Reusing existing code for {}: {}", verified_name, existing_code);
                        // keep code as-is
                    } else {
                        // expired: generate a new code and replace mapping
                        let new_code = generate_and_store_code(&mut con, &uuid, &verified_name);
                        code = Some(new_code);
                    }
                } else {
                    // No expires field found — generate a new code
                    let new_code = generate_and_store_code(&mut con, &uuid, &verified_name);
                    code = Some(new_code);
                }
            } else {
                // No mapping exists; create a new code and mapping
                let new_code = generate_and_store_code(&mut con, &uuid, &verified_name);
                code = Some(new_code);
            }
            
            // Create kick message
            let kick_reason = json!({
                "text": "",
                "extra": [
                    {"text": "\n"},
                    {"text": "§6§lWynnpool Verification§r\n\n", "color": "gold"},
                    {"text": "Hewoooo, ", "color": "gray"},
                    {"text": verified_name, "color": "green"},
                    {"text": "!\n\n", "color": "gray"},
                    {"text": "Your verification code is:\n", "color": "gray"},
                    {"text": format!("§l§e{}\n\n", code.clone().unwrap_or_default()), "color": "yellow"},
                    {"text": "§7This code expires in 15 minutes", "color": "dark_gray"},
                    // {"text": uuid, "color": "dark_gray"}
                ]
            });

            let kick_packet = build_kick_packet(&kick_reason.to_string());
            stream.write_all(&kick_packet)?;
        }
        _ => {}
    }

    Ok(())
}
