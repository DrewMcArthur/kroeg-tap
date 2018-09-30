#![feature(generators, use_extern_macros)]

#[macro_use]
extern crate kroeg_tap;

extern crate diesel;
extern crate futures_await as futures;
extern crate jsonld;
extern crate openssl;

#[macro_use]
extern crate serde_json;

pub mod handlers;