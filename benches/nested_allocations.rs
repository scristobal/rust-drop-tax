use rust_drop_tax::nested::{ArenaGraph, NestedGraph};

fn main() {
    divan::main();
}

const ARGS: &[usize] = &[1_000, 10_000, 100_000, 1_000_000];
const SAMPLE_COUNT: u32 = if cfg!(feature = "global-bump") { 1 } else { 10 };

#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod build {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn vec_of_vecs(bencher: divan::Bencher, count: usize) {
        bencher.bench_local(|| NestedGraph::new(count));
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher.bench_local(|| ArenaGraph::new(count));
    }
}

#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod traverse {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn vec_of_vecs(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| NestedGraph::new(count))
            .bench_local_refs(|graph| graph.checksum());
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArenaGraph::new(count))
            .bench_local_refs(|graph| graph.checksum());
    }
}

// Every idiomatic node owns an edge Vec, producing one destructor and one
// independent deallocation per node.
#[divan::bench_group(sample_count = SAMPLE_COUNT, sample_size = 1)]
mod drop {
    use super::*;

    #[divan::bench(args = ARGS)]
    fn vec_of_vecs(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| NestedGraph::new(count))
            .bench_local_values(std::mem::drop);
    }

    #[divan::bench(args = ARGS)]
    fn arena(bencher: divan::Bencher, count: usize) {
        bencher
            .with_inputs(|| ArenaGraph::new(count))
            .bench_local_values(std::mem::drop);
    }
}
