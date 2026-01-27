use crate::conf::PASSWORD_LEN;

use anyhow::{anyhow, Result};
use rand::RngCore;

/// Base62 alphabet (alphanumeric, no ambiguous chars)
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";

pub fn validate_generate_pw(pw: Option<String>) -> Result<String> {
    let pass = pw.unwrap_or_else(generate_random_password);
    match validate_pw(&pass) {
        true => Ok(pass),
        false => Err(anyhow!(
            "Password too short (minimum {} characters).",
            PASSWORD_LEN
        )),
    }
}

pub fn validate_pw(pw: &String) -> bool {
    PASSWORD_LEN <= pw.len()
}

/// Generate a cryptographically secure random password.
/// Uses base56 alphabet (alphanumeric minus ambiguous chars: 0, O, I, l, 1).
/// 16 chars × ~5.8 bits/char ≈ 93 bits of entropy.
pub fn generate_random_password() -> String {
    let mut rng = rand::thread_rng();
    let mut password = String::with_capacity(PASSWORD_LEN);

    for _ in 0..PASSWORD_LEN {
        let idx = (rng.next_u32() as usize) % ALPHABET.len();
        password.push(ALPHABET[idx] as char);
    }

    password
}
