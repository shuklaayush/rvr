#![feature(test)]

extern crate test;

use test::Bencher;

#[path = "support/bench_utils.rs"]
mod bench_utils;

macro_rules! bench_entry {
    ($fn_name:ident, $name:expr) => {
        #[bench]
        fn $fn_name(b: &mut Bencher) {
            bench_utils::bench_case($name, b);
        }
    };
}

bench_entry!(bench_fib, "fib");
bench_entry!(bench_fib_asm, "fib-asm");
