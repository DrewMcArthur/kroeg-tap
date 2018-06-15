#![feature(proc_macro, generators)]

#[macro_use]
extern crate kroeg_tap;

extern crate futures_await as futures;
extern crate jsonld;
extern crate kroeg_cellar;

mod entitystores;
pub use entitystores::*;

pub mod handlers;
