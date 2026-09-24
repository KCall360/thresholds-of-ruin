#![allow(dead_code)]
use serde_json::{json, Value};
use std::path::Path;
/// Fixture encoder intentionally independent of production decoding.
pub fn frame(sequence: u64, value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(if sequence == 0 { b"TORB" } else { b"TORJ" });
    bytes.extend_from_slice(&5u16.to_le_bytes());
    bytes.extend_from_slice(&(if sequence == 0 { 0u16 } else { 1u16 }).to_le_bytes());
    bytes.extend_from_slice(&sequence.to_le_bytes());
    let payload = serde_json::to_vec(value).unwrap();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    let mut crc = !0u32;
    for b in bytes[4..].iter().chain(&payload) {
        crc ^= u32::from(*b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82f63b78 & 0u32.wrapping_sub(crc & 1));
        }
    }
    bytes.extend_from_slice(&(!crc).to_le_bytes());
    bytes.extend(payload);
    bytes
}
pub fn read(path: impl AsRef<Path>) -> Value {
    tor_server::inspect_save(path).unwrap()
}
pub fn write(path: impl AsRef<Path>, mut archive: Value) {
    let mut conn = rusqlite::Connection::open(path).unwrap();
    let base: Vec<u8> = conn
        .query_row("SELECT frame FROM journal WHERE sequence=0", [], |r| {
            r.get(0)
        })
        .unwrap();
    let base: Value = serde_json::from_slice(&base[24..]).unwrap();
    let id = &base["save_id"];
    let records = archive["records"].take();
    archive["records"] = json!([]);
    let tx = conn.transaction().unwrap();
    tx.execute("DELETE FROM journal", []).unwrap();
    tx.execute(
        "INSERT INTO journal VALUES (0,?1)",
        [frame(
            0,
            &json!({"save_id":id,"generation":0,"archive":archive}),
        )],
    )
    .unwrap();
    for (i, record) in records.as_array().unwrap().iter().enumerate() {
        let seq = i as u64 + 1;
        tx.execute(
            "INSERT INTO journal VALUES (?1,?2)",
            rusqlite::params![
                seq as i64,
                frame(seq, &json!({"save_id":id,"generation":0,"record":record}))
            ],
        )
        .unwrap();
    }
    tx.commit().unwrap();
}
