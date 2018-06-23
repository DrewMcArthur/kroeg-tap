#![feature(proc_macro, generators)]

#[macro_use]
extern crate kroeg_tap;

extern crate openssl;
extern crate diesel;
extern crate futures_await as futures;
extern crate jsonld;
extern crate serde_json;

pub mod handlers;
