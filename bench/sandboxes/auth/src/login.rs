//! Password login. `login` calls `validate_creds`. Coupled pair.

pub fn login(username: &str, password: &str) -> Option<String> {
    if validate_creds(username, password) {
        Some(format!("token-{username}"))
    } else {
        None
    }
}

fn validate_creds(username: &str, password: &str) -> bool {
    !username.is_empty() && password.len() >= 8
}
