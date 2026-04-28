//! Logout. Calls `session::destroy_session`, so the partitioner groups
//! `logout` with the session-lifecycle functions.

use crate::session::{Session, destroy_session};

pub fn logout(mut s: Session) {
    destroy_session(&mut s);
    clear_session(&mut s);
}

fn clear_session(s: &mut Session) {
    s.user_id.clear();
}
