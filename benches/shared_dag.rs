use rust_drop_tax::dag::{ArcDag, ArenaDag};

fn main() {
    divan::main();
}

const ARGS: &[usize] = &[1_000, 10_000, 100_000, 1_000_000];
const SAMPLE_COUNT: u32 = if cfg!(feature = "global-bump") { 1 } else { 10 };

#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod build {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn arc(bencher: divan::Bencher, count: usize) {
        bencher.bench_local(|| ArcDag::new(count));
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher.bench_local(|| ArenaDag::new(count));
    }
}

#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod traverse {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn arc(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArcDag::new(count))
            .bench_local_refs(|dag| dag.checksum());
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArenaDag::new(count))
            .bench_local_refs(|dag| dag.checksum());
    }
}

// Arc is the intentionally expensive idiomatic case: dropping the graph must
// update atomic reference counts for its edges and free every node separately.
#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod drop {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn arc(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArcDag::new(count))
            .bench_local_values(std::mem::drop);
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArenaDag::new(count))
            .bench_local_values(std::mem::drop);
    }
}
