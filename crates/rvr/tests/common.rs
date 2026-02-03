pub const MAX_TEST_THREADS: usize = 5;

pub fn cap_threads(args: &mut libtest_mimic::Arguments) {
    let requested = args.test_threads.unwrap_or(MAX_TEST_THREADS);
    args.test_threads = Some(requested.min(MAX_TEST_THREADS));
}
