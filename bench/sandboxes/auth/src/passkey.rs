//! Passkey register / verify. Two independent functions; this group is
//! a candidate for a parallel rewrite alongside the login + session
//! groups.

pub fn passkey_register(user_id: &str) -> String {
    format!("pk-{user_id}")
}

pub fn passkey_verify(user_id: &str, pk: &str) -> bool {
    pk == format!("pk-{user_id}")
}
