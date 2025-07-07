#![cfg(not(test))]
#![deny(warnings)]
#![allow(unused_variables, unused_mut)]

use ctest_next_test::t1::*;

include!(concat!(env!("OUT_DIR"), "/t1gen.rs"));
