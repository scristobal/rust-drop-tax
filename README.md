# rust-drop-tax

Focused Divan benchmarks for data structures where idiomatic Rust ownership makes teardown expensive. Each workload compares a deliberately difficult idiomatic representation with a lifetime-based arena representation.

## Workloads

- `ast`: a recursively owned `Box<Expr>` tree versus arena references
- `shared_dag`: an `Arc<Node>` DAG versus arena references
- `nested_allocations`: one edge `Vec` per node versus arena-allocated edge slices
- `deep_list`: a recursively dropped boxed list versus an arena list

Alternative representations such as indexed `Vec` storage, CSR, and iterative custom drop are intentionally omitted. They can solve much of the same problem as an arena, but would obscure the comparison this project is intended to make.

## Part I: Drop

The primary question is how much latency Rust's automatic ownership teardown can introduce when it requires an O(n) walk containing recursive drop glue, reference-count operations, or one deallocation per object.

### Drop measurement

Divan constructs each input outside the timed region. The measured operation is only:

```rust
drop(input);
```

The arena contains only dropless data with one shared lifetime. Dropping it releases its backing chunks without visiting individual nodes.

### Idiomatic lower bound and arena result

The global bump diagnostic gives an empirical lower bound for the **idiomatic representation**. Its `dealloc` is a no-op, so it removes the cost of actually freeing allocations while retaining all recursive drop glue, nested-container visits, and `Arc` reference-count operations.

These are five-sample medians from an Apple M2 (`aarch64`, Linux, Rust 1.97). Mimalloc v3 is used for the reclaiming idiomatic and arena columns.

| Workload | Nodes | Idiomatic drop | Global-bump lower bound | Arena drop |
|---|---:|---:|---:|---:|
| Boxed AST | 1,048,575 | 3.67 ms | 2.33 ms | 0.79 µs |
| `Arc` DAG | 1,000,000 | 7.04 ms | 6.19 ms | 0.87 µs |
| Nested edge `Vec`s | 1,000,000 | 3.16 ms | 1.37 ms | 1.33 µs |
| Deep boxed list | 25,000 | 240 µs | 171 µs | 0.17 µs |

The arena can be faster than the global-bump lower bound because the bound applies to the original idiomatic representation. A no-op `dealloc` does not stop Rust from recursively walking a boxed tree, decrementing `Arc` counts, or dropping every nested `Vec`; arena teardown avoids that per-object work entirely.

The lower-bound measurements isolate the irreducible work imposed by those ownership representations:

- The boxed AST still spends 2.33 ms recursively running drop glue when freeing is free.
- The `Arc` DAG still spends 6.19 ms walking nodes and updating reference counts.
- The nested structure still spends 1.37 ms visiting and dropping one `Vec` per node.
- The recursive boxed list still spends 171 µs walking its chain.

### General-purpose allocator overhead

The following table keeps the same idiomatic drop measurements while varying only the global allocator. The last column is the geometric mean of the four per-workload slowdowns relative to the global-bump lower bound:

```text
overhead = geometric_mean(allocator time / global-bump time)
```

| Global allocator | Boxed AST | `Arc` DAG | Nested `Vec`s | Boxed list | Overhead vs bump |
|---|---:|---:|---:|---:|---:|
| Global bump diagnostic | 2.33 ms | 6.19 ms | 1.37 ms | 171 µs | 1.00× |
| **mimalloc v3** | **3.67 ms** | **7.04 ms** | **3.16 ms** | **240 µs** | **1.55×** |
| snmalloc | 3.80 ms | 7.41 ms | 3.42 ms | 287 µs | 1.69× |
| rpmalloc | 4.41 ms | 8.23 ms | 3.76 ms | 287 µs | 1.85× |
| jemalloc | 9.56 ms | 13.23 ms | 9.10 ms | 459 µs | 3.54× |
| TCMalloc | 13.63 ms | 16.98 ms | 11.41 ms | 532 µs | 4.52× |
| System | 14.85 ms | 17.07 ms | 12.85 ms | 904 µs | 5.43× |

Mimalloc was the reclaiming allocator closest to the lower bound and the fastest in every idiomatic drop workload, so it is selected by default. A better allocator substantially reduces the tax, but cannot remove the O(n) ownership work.

The global bump allocator is only a diagnostic: all memory remains allocated until process exit. It defaults to one sample and reserves 8 GiB of virtual address space. Increasing its sample count can exhaust physical memory.

Sub-microsecond arena drop means mimalloc accepted the arena's few backing allocations into its caches; it does not imply that all physical pages were synchronously returned to the operating system. RSS, forced purging, and cross-thread deallocation require separate measurements.

## Part II: Build and traversal trade-offs

Drop latency alone is not enough to choose a representation. The additional measurements check whether an arena pays for fast teardown through slower construction or access. For a structure built once, traversed `k` times, and then destroyed, the approximate total is:

```text
build + k × traverse + drop
```

A combined lifecycle benchmark is omitted because it hides which phase is responsible and is redundant with these separate measurements.

Each cell below shows **idiomatic / arena** using mimalloc v3.

| Workload | Nodes | Build | Traverse |
|---|---:|---:|---:|
| Boxed AST | 1,048,575 | 10.18 / 3.41 ms | 2.45 / 1.70 ms |
| `Arc` DAG | 1,000,000 | 7.75 / 2.68 ms | 2.23 / 1.67 ms |
| Nested edge `Vec`s | 1,000,000 | 6.11 / 5.90 ms | 7.21 / 7.22 ms |
| Deep boxed list | 25,000 | 131 / 92 µs | 51.7 / 39.1 µs |

### Why build improves

The idiomatic structures call the global allocator for every `Box`, `Arc`, or nested edge `Vec`. The arena reserves a large chunk and usually services each request with alignment plus a local pointer increment. It avoids repeated size-class lookup, allocator metadata updates, and synchronization. Building the DAG also avoids the atomic reference-count increments performed by `Arc::clone` for every edge.

### Why traversal can improve as a side-effect bonus

Traversal does not allocate, so bump allocation does not directly make the traversal instructions faster. The improvement is a side effect of the layout created during construction:

- Arena nodes are densely packed into a small number of chunks.
- Related nodes are more likely to occupy nearby cache lines and pages.
- Dense placement can reduce cache misses, TLB misses, allocator padding, and fragmentation.
- `Arc` nodes carry control-block metadata and live in separate allocations; arena references point directly into dense node storage.

No reference counts are changed during the measured DAG traversal—the improvement there is primarily locality. The nested-edge benchmark is a useful control: both representations still follow a pointer to each edge slice, and their traversal times are effectively identical. The traversal gain is therefore a useful bonus for these particular layouts, not an inherent guarantee of every arena.

Inputs are freshly constructed immediately before traversal, so these measurements represent a relatively warm first pass. Repeated hot traversal and explicitly cold-cache traversal would be useful separate experiments.

## Running

Mimalloc is selected by default:

```console
cargo bench
```

Run with the system allocator or another allocator:

```console
cargo bench --no-default-features
cargo bench --no-default-features --features jemalloc
cargo bench --no-default-features --features rpmalloc
cargo bench --no-default-features --features snmalloc
cargo bench --no-default-features --features tcmalloc
```

Allocator features are mutually exclusive. Run the non-reclaiming global bump diagnostic separately:

```console
cargo bench --no-default-features --features global-bump
```

Run one workload, phase, or size:

```console
cargo bench --bench ast
cargo bench --bench ast -- drop
cargo bench --bench ast -- 1048575 --sample-count 5
```

Check every benchmark once without collecting statistics:

```console
cargo bench -- --test
```

## Semantic boundary

`bumpalo::Bump` does not run destructors for objects allocated in it. The arena cases therefore contain only data whose lifetime can safely end in bulk. Skipping destructors for values that own files, locks, reference counts, or independent heap allocations would change program semantics rather than optimize teardown.
