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

bench_entry!(bench_towers, "towers");
bench_entry!(bench_qsort, "qsort");
bench_entry!(bench_rsort, "rsort");
bench_entry!(bench_median, "median");
bench_entry!(bench_multiply, "multiply");
bench_entry!(bench_vvadd, "vvadd");
bench_entry!(bench_memcpy, "memcpy");
bench_entry!(bench_dhrystone, "dhrystone");
