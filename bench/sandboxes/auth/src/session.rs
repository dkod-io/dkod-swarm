//! Session lifecycle. Three coupled functions: `create_session`,
//! `destroy_session`, and the helper `touch` they both call.

pub struct Session {
    pub id: String,
    pub user_id: String,
    pub last_active_ms: u64,
}

pub fn create_session(user_id: &str) -> Session {
    let id = format!("sess-{user_id}");
    let mut s = Session {
        id,
        user_id: user_id.to_string(),
        last_active_ms: 0,
    };
    touch(&mut s);
    s
}

pub fn destroy_session(s: &mut Session) {
    touch(s);
    s.id.clear();
}

fn touch(s: &mut Session) {
    s.last_active_ms = 1;
}
