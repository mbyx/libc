#![cfg(not(test))]
// #![deny(warnings)]

use ctest_next_test::t1::*;
use libc::*;

include!(concat!(env!("OUT_DIR"), "/t1gen.rs"));
