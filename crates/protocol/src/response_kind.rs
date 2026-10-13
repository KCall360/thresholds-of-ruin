//! Successful root discriminator probing uses no input-proportional allocation.
//! Malformed input may allocate fixed-size error storage.
//! This is not a JSON decoder: ordinary syntax/schema validation remains in Serde.
//! In particular, neither escaped member names nor values enter Serde's scratch
//! strings before the private byte ceiling has been selected.
use super::{DecodeError, MAX_JSON_DEPTH};

fn malformed() -> DecodeError {
    DecodeError::Json(serde::de::Error::custom(
        "Invalid response discriminator envelope",
    ))
}
fn whitespace(bytes: &[u8], at: &mut usize) {
    while bytes.get(*at).is_some_and(u8::is_ascii_whitespace) {
        *at += 1;
    }
}
fn hex(bytes: &[u8], at: &mut usize) -> Result<u16, DecodeError> {
    let mut value = 0;
    for _ in 0..4 {
        let digit = match bytes.get(*at) {
            Some(b'0'..=b'9') => bytes[*at] - b'0',
            Some(b'a'..=b'f') => bytes[*at] - b'a' + 10,
            Some(b'A'..=b'F') => bytes[*at] - b'A' + 10,
            _ => return Err(malformed()),
        };
        *at += 1;
        value = value * 16 + u16::from(digit);
    }
    Ok(value)
}
/// Compare a decoded JSON string with one small ASCII constant without owning it.
fn string(bytes: &[u8], at: &mut usize, expected: &[u8]) -> Result<bool, DecodeError> {
    if bytes.get(*at) != Some(&b'"') {
        return Err(malformed());
    }
    *at += 1;
    let mut matched = true;
    let mut length = 0;
    loop {
        let byte = *bytes.get(*at).ok_or_else(malformed)?;
        *at += 1;
        let scalar = match byte {
            b'"' => return Ok(matched && length == expected.len()),
            0..=31 => return Err(malformed()),
            b'\\' => {
                let escaped = *bytes.get(*at).ok_or_else(malformed)?;
                *at += 1;
                match escaped {
                    b'"' | b'\\' | b'/' => u32::from(escaped),
                    b'b' => 8,
                    b'f' => 12,
                    b'n' => 10,
                    b'r' => 13,
                    b't' => 9,
                    b'u' => {
                        let first = hex(bytes, at)?;
                        if (0xd800..=0xdbff).contains(&first) {
                            if bytes.get(*at..*at + 2) != Some(b"\\u") {
                                return Err(malformed());
                            }
                            *at += 2;
                            let second = hex(bytes, at)?;
                            if !(0xdc00..=0xdfff).contains(&second) {
                                return Err(malformed());
                            }
                            0x10000 + ((u32::from(first) - 0xd800) << 10) + u32::from(second)
                                - 0xdc00
                        } else if (0xdc00..=0xdfff).contains(&first) {
                            return Err(malformed());
                        } else {
                            u32::from(first)
                        }
                    }
                    _ => return Err(malformed()),
                }
            }
            _ => u32::from(byte),
        };
        matched &= expected
            .get(length)
            .is_some_and(|value| u32::from(*value) == scalar);
        length += 1;
    }
}
/// Skip one root member value using a fixed depth-bounded stack, never recursion.
fn value(bytes: &[u8], at: &mut usize) -> Result<(), DecodeError> {
    whitespace(bytes, at);
    match bytes.get(*at) {
        Some(b'"') => {
            string(bytes, at, b"")?;
            return Ok(());
        }
        Some(b'{' | b'[') => {}
        Some(b',' | b'}' | b']') | None => return Err(malformed()),
        _ => {
            let start = *at;
            while bytes.get(*at).is_some_and(|byte| {
                !byte.is_ascii_whitespace() && !matches!(byte, b',' | b'}' | b']')
            }) {
                *at += 1;
            }
            return if *at > start {
                Ok(())
            } else {
                Err(malformed())
            };
        }
    }
    let mut stack = [0u8; MAX_JSON_DEPTH];
    let mut depth = 0;
    loop {
        match *bytes.get(*at).ok_or_else(malformed)? {
            b'"' => {
                string(bytes, at, b"")?;
            }
            opening @ (b'{' | b'[') => {
                // The root object itself already occupies one depth level.
                if depth + 1 >= MAX_JSON_DEPTH {
                    return Err(DecodeError::TooDeep {
                        limit: MAX_JSON_DEPTH,
                    });
                }
                stack[depth] = if opening == b'{' { b'}' } else { b']' };
                depth += 1;
                *at += 1;
            }
            closing @ (b'}' | b']') => {
                if depth == 0 || stack[depth - 1] != closing {
                    return Err(malformed());
                }
                depth -= 1;
                *at += 1;
                if depth == 0 {
                    return Ok(());
                }
            }
            _ => *at += 1,
        }
    }
}

/// A cap probe may stop as soon as it knows the narrower byte policy applies.
pub(super) fn private_strike_page(text: &str) -> Result<bool, DecodeError> {
    root_kind(text, b"creature_strike_page", false)
}

/// Recovery admission must inspect every root member: a later duplicate cannot
/// turn a Snapshot prefix into authorization for allocating a typed body.
pub(super) fn snapshot(text: &str) -> Result<bool, DecodeError> {
    root_kind(text, b"snapshot", true)
}

fn finish(bytes: &[u8], mut at: usize, matched: bool, complete: bool) -> Result<bool, DecodeError> {
    if complete {
        at += 1; // root closing brace
        whitespace(bytes, &mut at);
        if at != bytes.len() {
            return Err(malformed());
        }
    }
    Ok(matched)
}

fn root_kind(text: &str, expected: &[u8], complete: bool) -> Result<bool, DecodeError> {
    let bytes = text.as_bytes();
    let mut at = 0;
    whitespace(bytes, &mut at);
    if bytes.get(at) != Some(&b'{') {
        return Ok(false);
    }
    at += 1;
    let mut kind_seen = false;
    let mut matched = false;
    loop {
        whitespace(bytes, &mut at);
        if bytes.get(at) == Some(&b'}') {
            return finish(bytes, at, matched, complete);
        }
        let discriminator = string(bytes, &mut at, b"type")?;
        whitespace(bytes, &mut at);
        if bytes.get(at) != Some(&b':') {
            return Err(malformed());
        }
        at += 1;
        whitespace(bytes, &mut at);
        if discriminator {
            if kind_seen {
                // Ambiguous root kinds never enter expected-Snapshot decoding.
                return if complete {
                    Ok(false)
                } else {
                    Err(malformed())
                };
            }
            kind_seen = true;
            matched = string(bytes, &mut at, expected)?;
            if matched && !complete {
                return Ok(true);
            }
        } else {
            value(bytes, &mut at)?;
        }
        whitespace(bytes, &mut at);
        match bytes.get(at) {
            Some(b',') => at += 1,
            Some(b'}') => return finish(bytes, at, matched, complete),
            _ => return Err(malformed()),
        }
    }
}

/// Select the stricter owner-page byte cap before allocating any DTO body.
pub(super) fn advancement_page(text: &str) -> Result<bool, DecodeError> {
    root_kind(text, b"advancement_page", false)
}
