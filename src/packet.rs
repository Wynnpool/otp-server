use std::io::{self, Cursor, Read};
use byteorder::ReadBytesExt;

pub fn read_varint(cursor: &mut Cursor<&[u8]>) -> io::Result<i32> {
    let mut result = 0;
    for i in 0..5 {
        let byte = cursor.read_u8()?;
        result |= (byte as i32 & 0x7F) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(result);
        }
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "VarInt too big"))
}

pub fn write_varint(value: i32) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut val = value as u32;
    loop {
        let mut temp = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            temp |= 0x80;
        }
        buf.push(temp);
        if val == 0 {
            break;
        }
    }
    buf
}

pub fn write_string(s: &str) -> Vec<u8> {
    let str_bytes = s.as_bytes();
    let mut buf = write_varint(str_bytes.len() as i32);
    buf.extend_from_slice(str_bytes);
    buf
}

pub fn build_packet(id: i32, payload: Vec<u8>) -> Vec<u8> {
    let mut packet = write_varint(id);
    packet.extend(payload);
    let mut result = write_varint(packet.len() as i32);
    result.extend(packet);
    result
}

pub fn build_kick_packet(reason: &str) -> Vec<u8> {
    build_packet(0x00, write_string(reason))
}

/// Read a full length-prefixed packet from a `TcpStream`.
/// This reads the VarInt length prefix first (1..=5 bytes), decodes it,
/// then reads exactly `length` bytes for the packet payload.
/// Returns the packet payload (i.e. bytes after the length prefix).
pub fn read_packet_from_stream(stream: &mut std::net::TcpStream) -> io::Result<Vec<u8>> {
    // Read the packet length VarInt (1-5 bytes)
    let mut header = Vec::with_capacity(5);
    let mut one = [0u8; 1];
    for _ in 0..5 {
        stream.read_exact(&mut one)?;
        header.push(one[0]);
        if one[0] & 0x80 == 0 {
            break;
        }
    }

    let mut header_cursor = Cursor::new(&header[..]);
    let packet_len = read_varint(&mut header_cursor)? as usize;

    // Sanity check to avoid allocating huge sizes from bad/malicious input
    const MAX_PACKET_SIZE: usize = 2 * 1024 * 1024; // 2 MiB
    if packet_len > MAX_PACKET_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Packet too large"));
    }

    let mut packet = vec![0u8; packet_len];
    // read_exact will block until we have the whole packet or the connection closes
    stream.read_exact(&mut packet)?;
    Ok(packet)
}
