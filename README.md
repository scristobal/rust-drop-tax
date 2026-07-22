# rust-drop-tax

Focused Divan benchmarks for data structures where idiomatic Rust ownership makes teardown expensive. Each workload compares a deliberately difficult idiomatic representation with a lifetime-based arena representation.

## Workloads

- `ast`: a recursively owned `Box<Expr>` tree versus arena references
- `shared_dag`: an `Arc<Node>` DAG versus arena references
- `nested_allocations`: one edge `Vec` per node versus arena-allocated edge slices
- `deep_list`: a recursively dropped boxed list versus an arena list

Alternative representations such as indexed `Vec` storage, CSR, and iterative custom drop are intentionally omitted. They can solve much of the same problem as an arena, but would obscure the comparison this project is intended to make.

## What is measured

Each representation has three independent measurements:

- `build`: construct the data structure; Divan drops the result outside the timer
- `traverse`: calculate an equivalent checksum; construction and destruction occur outside the timer
- `drop`: construct outside the timer and measure only destruction

For a structure built once, traversed `k` times, and then destroyed, its approximate total cost is:

```text
build + k × traverse + drop
```

A combined lifecycle benchmark is omitted because it hides which phase is responsible and is redundant with the separate measurements.

## Allocator comparison: idiomatic drop only

The allocator comparison used the system allocator, jemalloc, mimalloc v3, rpmalloc, snmalloc, and Google's current TCMalloc. Every number in this table is the median time for **only `drop(input)`** on the idiomatic representation; build and traversal are excluded. Results come from five-sample runs on an Apple M2 (`aarch64`, Linux, Rust 1.97).

The non-reclaiming global bump allocator defines the empirical lower boundary. Its `dealloc` is a no-op, but Rust must still execute recursive drop glue, visit nested containers, and update reference counts. The final column is the geometric mean of the four per-workload slowdowns relative to that boundary:

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

Mimalloc was the reclaiming allocator closest to the lower boundary and the fastest in every idiomatic drop workload, so it is selected as the default feature. Rpmalloc won some construction cases, but mimalloc was the best fit for this project's focus on teardown and remained competitive in build and traversal.

The global bump allocator is not a general-purpose winner: all memory remains allocated until process exit. It is included only as a diagnostic. Its results isolate costs that remain when freeing memory is free:

- The boxed AST still spends 2.33 ms recursively running drop glue.
- The `Arc` DAG still spends 6.19 ms performing reference-count operations and walking nodes.
- The nested structure still spends 1.37 ms visiting and dropping one `Vec` per node.

Because it leaks every benchmark input, the global-bump feature defaults to one sample and reserves 8 GiB of virtual address space. Increasing its sample count can exhaust physical memory.

## Arena comparison with the selected allocator

These medians use mimalloc v3. Each cell shows **idiomatic / arena**.

| Workload | Nodes | Build | Traverse | Drop |
|---|---:|---:|---:|---:|
| Boxed AST | 1,048,575 | 10.18 / 3.41 ms | 2.45 / 1.70 ms | 3.67 ms / 0.79 µs |
| `Arc` DAG | 1,000,000 | 7.75 / 2.68 ms | 2.23 / 1.67 ms | 7.04 ms / 0.87 µs |
| Nested edge `Vec`s | 1,000,000 | 6.11 / 5.90 ms | 7.21 / 7.22 ms | 3.16 ms / 1.33 µs |
| Deep boxed list | 25,000 | 131 / 92 µs | 51.7 / 39.1 µs | 240 µs / 0.17 µs |

For these intentionally adverse ownership patterns, idiomatic teardown cost was not recovered elsewhere:

- Arena traversal was faster for the AST, DAG, and list.
- Nested traversal was effectively identical.
- Arena construction was faster or approximately equal.
- Arena drop required only a few allocator calls, while idiomatic drop retained its O(n) walk.

Sub-microsecond arena drop means mimalloc accepted the arena's backing allocations into its caches; it does not imply that all physical pages were synchronously returned to the operating system. RSS, forced purging, and cross-thread deallocation require separate measurements.

The key result is not that Rust's `drop` instruction is intrinsically slow. The bottleneck appears when ownership semantics require an O(n) walk containing allocator calls, reference-count operations, or recursive drop glue, while the application's actual lifetime model would permit O(number of arena chunks) reclamation.

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
