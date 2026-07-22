use rust_drop_tax::list::{ArenaList, RecursiveList};

fn main() {
    divan::main();
}

const ARGS: &[usize] = &[1_000, 10_000, 25_000];
const SAMPLE_COUNT: u32 = if cfg!(feature = "global-bump") { 1 } else { 10 };

#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod build {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn recursive_box(bencher: divan::Bencher, count: usize) {
        bencher.bench_local(|| RecursiveList::new(count));
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher.bench_local(|| ArenaList::new(count));
    }
}

#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod traverse {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn recursive_box(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| RecursiveList::new(count))
            .bench_local_refs(|list| list.checksum());
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArenaList::new(count))
            .bench_local_refs(|list| list.checksum());
    }
}

// Keep recursive cases below intentionally dangerous depths. A long enough
// automatically dropped Box chain can overflow the thread stack.
#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod drop {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn recursive_box(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| RecursiveList::new(count))
            .bench_local_values(std::mem::drop);
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArenaList::new(count))
            .bench_local_values(std::mem::drop);
    }
}
