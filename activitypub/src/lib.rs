#![feature(generators, nll, bind_by_move_pattern_guards, never_type)]

#[macro_use]
extern crate kroeg_tap;

extern crate diesel;
extern crate futures_await as futures;
extern crate jsonld;
extern crate openssl;
extern crate url;

#[macro_use]
extern crate serde_json;

pub mod handlers;

#[macro_use]
pub mod test;
