//! Auth sandbox for dkod-swarm M5 E2E.
//!
//! Four modules with deliberate call-graph coupling so the partitioner
//! splits the public surface into three disjoint groups:
//! - login: `login` + `validate_creds`
//! - session lifecycle: `create_session` / `destroy_session` / `touch`
//! - passkey: `passkey_register` / `passkey_verify`
//! `logout` calls into session, so `logout` joins the session group.

pub mod login;
pub mod logout;
pub mod passkey;
pub mod session;
