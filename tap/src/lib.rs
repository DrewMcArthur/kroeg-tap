#![feature(never_type)]

/// Macro for translating short as:Note style IDs to full strings as used in
/// `Entity`. e.g. `as2!(name)`.
#[macro_export]
macro_rules! as2 {
    ($ident:ident) => {
        concat!("https://www.w3.org/ns/activitystreams#", stringify!($ident))
    };
}
#[macro_export]
macro_rules! ldp {
    ($ident:ident) => {
        concat!("http://www.w3.org/ns/ldp#", stringify!($ident))
    };
}
#[macro_export]
macro_rules! kroeg {
    ($ident:ident) => {
        concat!("https://puckipedia.com/kroeg/ns#", stringify!($ident))
    };
}
#[macro_export]
macro_rules! sec {
    ($ident:ident) => {
        concat!("https://w3id.org/security#", stringify!($ident))
    };
}

mod assemble;
pub use assemble::{assemble, untangle};

mod entity;
pub use entity::*;

mod entitystore;
pub use entitystore::*;

mod handler;
pub use handler::*;

mod user;
pub use user::*;

mod id;
pub use id::*;

mod auth;
pub use auth::*;

mod query;
pub use query::*;
