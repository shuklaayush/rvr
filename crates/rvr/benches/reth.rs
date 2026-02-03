#![feature(test)]

extern crate test;

use test::Bencher;

#[path = "support/bench_utils.rs"]
mod bench_utils;

#[bench]
fn bench_reth(b: &mut Bencher) {
    bench_utils::bench_case("reth", b);
}
