use crate::conf::PASSWORD_LEN;

use anyhow::{anyhow, Result};
use names::{Generator, Name};

pub fn validate_generate_pw(pw: Option<String>) -> Result<String> {
    let pass = pw.unwrap_or_else(generate_random_password);
    match validate_pw(&pass) {
        true => Ok(pass),
        false => Err(anyhow!("Password too short.")),
    }
}

pub fn validate_pw(pw: &String) -> bool {
    PASSWORD_LEN <= pw.len()
}

pub fn generate_random_password() -> String {
    let mut generator = Generator::with_naming(Name::Numbered);
    match generator.next() {
        Some(s) if PASSWORD_LEN <= s.len() => s,
        _ => generate_random_password(),
    }
}
