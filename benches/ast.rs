use rust_drop_tax::ast::{ArenaTree, BoxTree};

fn main() {
    divan::main();
}

const ARGS: &[usize] = &[1_023, 16_383, 131_071, 1_048_575];
const SAMPLE_COUNT: u32 = if cfg!(feature = "global-bump") { 1 } else { 10 };

#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod build {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn boxed(bencher: divan::Bencher, count: usize) {
        // Divan drops returned values outside the timed region.
        bencher.bench_local(|| BoxTree::new(count));
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher.bench_local(|| ArenaTree::new(count));
    }
}

#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod traverse {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn boxed(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| BoxTree::new(count))
            .bench_local_refs(|tree| tree.checksum());
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArenaTree::new(count))
            .bench_local_refs(|tree| tree.checksum());
    }
}

#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod drop {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn boxed(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| BoxTree::new(count))
            .bench_local_values(std::mem::drop);
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArenaTree::new(count))
            .bench_local_values(std::mem::drop);
    }
}
