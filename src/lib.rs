use std::hint::black_box;

use bumpalo::Bump;

#[cfg(any(
    all(
        feature = "global-bump",
        any(
            feature = "jemalloc",
            feature = "mimalloc",
            feature = "rpmalloc",
            feature = "snmalloc",
            feature = "tcmalloc"
        )
    ),
    all(
        feature = "jemalloc",
        any(
            feature = "mimalloc",
            feature = "rpmalloc",
            feature = "snmalloc",
            feature = "tcmalloc"
        )
    ),
    all(
        feature = "mimalloc",
        any(feature = "rpmalloc", feature = "snmalloc", feature = "tcmalloc")
    ),
    all(feature = "rpmalloc", any(feature = "snmalloc", feature = "tcmalloc")),
    all(feature = "snmalloc", feature = "tcmalloc")
))]
compile_error!("allocator features are mutually exclusive");

#[cfg(feature = "global-bump")]
#[global_allocator]
static GLOBAL_ALLOCATOR: bump_alloc2::BumpAlloc =
    bump_alloc2::BumpAlloc::with_size(8 * 1024 * 1024 * 1024);

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "rpmalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: rpmalloc::RpMalloc = rpmalloc::RpMalloc;

#[cfg(feature = "snmalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: snmalloc_rs::SnMalloc = snmalloc_rs::SnMalloc;

#[cfg(feature = "tcmalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: tcmalloc_better::TCMalloc = tcmalloc_better::TCMalloc;

#[derive(Clone, Copy)]
struct Payload {
    id: usize,
    bytes: [u8; 24],
}

impl Payload {
    fn new(id: usize) -> Self {
        Self {
            id,
            bytes: [(id & 0xff) as u8; 24],
        }
    }

    fn checksum(self) -> usize {
        self.id.wrapping_add(self.bytes[0] as usize)
    }
}

pub mod ast {
    use super::*;

    pub struct BoxTree {
        root: Box<Expr>,
    }

    enum Expr {
        Literal(Payload),
        Add {
            payload: Payload,
            left: Box<Expr>,
            right: Box<Expr>,
        },
    }

    impl BoxTree {
        pub fn new(count: usize) -> Self {
            assert!(count > 0 && count % 2 == 1);
            let mut id = 0;
            let root = build_boxed(count, &mut id);
            debug_assert_eq!(id, count);
            Self { root }
        }

        pub fn checksum(&self) -> usize {
            checksum_boxed(&self.root)
        }
    }

    fn build_boxed(count: usize, id: &mut usize) -> Box<Expr> {
        let payload = Payload::new(*id);
        *id += 1;

        if count == 1 {
            Box::new(Expr::Literal(payload))
        } else {
            let child_count = count / 2;
            Box::new(Expr::Add {
                payload,
                left: build_boxed(child_count, id),
                right: build_boxed(child_count, id),
            })
        }
    }

    fn checksum_boxed(expr: &Expr) -> usize {
        match expr {
            Expr::Literal(payload) => payload.checksum(),
            Expr::Add {
                payload,
                left,
                right,
            } => payload
                .checksum()
                .wrapping_add(checksum_boxed(left))
                .wrapping_add(checksum_boxed(right)),
        }
    }

    enum ArenaExpr<'a> {
        Literal(Payload),
        Add {
            payload: Payload,
            left: &'a ArenaExpr<'a>,
            right: &'a ArenaExpr<'a>,
        },
    }

    // `root` points into `_arena`. Bump allocations stay at stable addresses
    // when the Bump handle moves, and the pointer is never exposed past `self`.
    pub struct ArenaTree {
        root: *const (),
        _arena: Bump,
    }

    impl ArenaTree {
        pub fn new(count: usize) -> Self {
            assert!(count > 0 && count % 2 == 1);
            let bytes = count
                .checked_mul(std::mem::size_of::<ArenaExpr<'_>>())
                .expect("tree is too large");
            let arena = Bump::with_capacity(bytes);
            let mut id = 0;
            let root = build_arena(&arena, count, &mut id);
            debug_assert_eq!(id, count);
            let root = root as *const ArenaExpr<'_> as *const ();
            Self {
                root,
                _arena: arena,
            }
        }

        fn root(&self) -> &ArenaExpr<'_> {
            // SAFETY: `root` was allocated by `_arena`; `_arena` is alive for
            // `self`, and bump allocations never move.
            unsafe { &*self.root.cast::<ArenaExpr<'_>>() }
        }

        pub fn checksum(&self) -> usize {
            checksum_arena(self.root())
        }
    }

    fn build_arena<'a>(arena: &'a Bump, count: usize, id: &mut usize) -> &'a ArenaExpr<'a> {
        let payload = Payload::new(*id);
        *id += 1;

        if count == 1 {
            arena.alloc(ArenaExpr::Literal(payload))
        } else {
            let child_count = count / 2;
            let left = build_arena(arena, child_count, id);
            let right = build_arena(arena, child_count, id);
            arena.alloc(ArenaExpr::Add {
                payload,
                left,
                right,
            })
        }
    }

    fn checksum_arena(expr: &ArenaExpr<'_>) -> usize {
        match expr {
            ArenaExpr::Literal(payload) => payload.checksum(),
            ArenaExpr::Add {
                payload,
                left,
                right,
            } => payload
                .checksum()
                .wrapping_add(checksum_arena(left))
                .wrapping_add(checksum_arena(right)),
        }
    }
}

pub mod dag {
    use std::sync::Arc;

    use super::*;

    fn children(id: usize) -> [Option<usize>; 2] {
        [
            id.checked_sub(1).map(|index| index / 2),
            id.checked_sub(2).map(|index| index / 2),
        ]
    }

    struct ArcNode {
        payload: Payload,
        children: [Option<Arc<ArcNode>>; 2],
    }

    pub struct ArcDag {
        nodes: Vec<Arc<ArcNode>>,
    }

    impl ArcDag {
        pub fn new(count: usize) -> Self {
            let mut nodes = Vec::with_capacity(count);
            for id in 0..count {
                let [left, right] = children(id);
                nodes.push(Arc::new(ArcNode {
                    payload: Payload::new(id),
                    children: [
                        left.map(|index| Arc::clone(&nodes[index])),
                        right.map(|index| Arc::clone(&nodes[index])),
                    ],
                }));
            }
            Self { nodes }
        }

        pub fn checksum(&self) -> usize {
            self.nodes.iter().fold(0, |sum, node| {
                node.children
                    .iter()
                    .flatten()
                    .fold(sum.wrapping_add(node.payload.checksum()), |sum, child| {
                        sum.wrapping_add(child.payload.id)
                    })
            })
        }
    }

    struct ArenaNode<'a> {
        payload: Payload,
        children: [Option<&'a ArenaNode<'a>>; 2],
    }

    pub struct ArenaDag {
        nodes: Vec<*const ()>,
        _arena: Bump,
    }

    impl ArenaDag {
        pub fn new(count: usize) -> Self {
            let bytes = count
                .checked_mul(std::mem::size_of::<ArenaNode<'_>>())
                .expect("DAG is too large");
            let arena = Bump::with_capacity(bytes);
            let mut nodes: Vec<*const ()> = Vec::with_capacity(count);

            for id in 0..count {
                let [left, right] = children(id);
                let left = left.map(|index| unsafe { node_from_ptr(nodes[index]) });
                let right = right.map(|index| unsafe { node_from_ptr(nodes[index]) });
                let node = arena.alloc(ArenaNode {
                    payload: Payload::new(id),
                    children: [left, right],
                });
                nodes.push(node as *const ArenaNode<'_> as *const ());
            }

            Self {
                nodes,
                _arena: arena,
            }
        }

        pub fn checksum(&self) -> usize {
            self.nodes.iter().fold(0, |sum, pointer| {
                // SAFETY: every pointer belongs to `_arena`, which is alive.
                let node = unsafe { node_from_ptr(*pointer) };
                node.children
                    .iter()
                    .flatten()
                    .fold(sum.wrapping_add(node.payload.checksum()), |sum, child| {
                        sum.wrapping_add(child.payload.id)
                    })
            })
        }
    }

    unsafe fn node_from_ptr<'a>(pointer: *const ()) -> &'a ArenaNode<'a> {
        // SAFETY: callers only pass pointers returned by the live arena.
        unsafe { &*pointer.cast::<ArenaNode<'a>>() }
    }
}

pub mod nested {
    use super::*;

    const DEGREE: usize = 8;

    #[derive(Clone, Copy)]
    struct Edge {
        target: u32,
        weight: u32,
    }

    fn edge(node: usize, slot: usize, count: usize) -> Edge {
        Edge {
            target: ((node + slot + 1) % count) as u32,
            weight: slot as u32,
        }
    }

    struct NestedNode {
        payload: Payload,
        edges: Vec<Edge>,
    }

    pub struct NestedGraph {
        nodes: Vec<NestedNode>,
    }

    impl NestedGraph {
        pub fn new(count: usize) -> Self {
            let mut nodes = Vec::with_capacity(count);
            for id in 0..count {
                let edges = (0..DEGREE).map(|slot| edge(id, slot, count)).collect();
                nodes.push(NestedNode {
                    payload: Payload::new(id),
                    edges,
                });
            }
            Self { nodes }
        }

        pub fn checksum(&self) -> usize {
            checksum_nodes(self.nodes.iter().map(|node| (node.payload, &*node.edges)))
        }
    }

    struct ArenaNode<'a> {
        payload: Payload,
        edges: &'a [Edge],
    }

    pub struct ArenaGraph {
        nodes: *const (),
        len: usize,
        _arena: Bump,
    }

    impl ArenaGraph {
        pub fn new(count: usize) -> Self {
            let bytes_per_node =
                std::mem::size_of::<ArenaNode<'_>>() + DEGREE * std::mem::size_of::<Edge>();
            let bytes = count
                .checked_mul(bytes_per_node)
                .expect("graph is too large");
            let arena = Bump::with_capacity(bytes);
            let nodes = arena.alloc_slice_fill_with(count, |id| {
                let edges = arena.alloc_slice_fill_with(DEGREE, |slot| edge(id, slot, count));
                ArenaNode {
                    payload: Payload::new(id),
                    edges,
                }
            });
            let pointer = nodes.as_ptr() as *const ();

            Self {
                nodes: pointer,
                len: count,
                _arena: arena,
            }
        }

        fn nodes(&self) -> &[ArenaNode<'_>] {
            // SAFETY: `nodes` points to `len` initialized elements in `_arena`.
            unsafe { std::slice::from_raw_parts(self.nodes.cast::<ArenaNode<'_>>(), self.len) }
        }

        pub fn checksum(&self) -> usize {
            checksum_nodes(self.nodes().iter().map(|node| (node.payload, node.edges)))
        }
    }

    fn checksum_nodes<'a>(nodes: impl Iterator<Item = (Payload, &'a [Edge])>) -> usize {
        nodes.fold(0, |sum, (payload, edges)| {
            edges
                .iter()
                .fold(sum.wrapping_add(payload.checksum()), |sum, edge| {
                    sum.wrapping_add(edge.target as usize)
                        .wrapping_add(edge.weight as usize)
                })
        })
    }
}

pub mod list {
    use super::*;

    struct RecursiveNode {
        payload: Payload,
        next: Option<Box<RecursiveNode>>,
    }

    pub struct RecursiveList {
        head: Option<Box<RecursiveNode>>,
    }

    impl RecursiveList {
        pub fn new(count: usize) -> Self {
            let mut head = None;
            for id in 0..count {
                head = Some(Box::new(RecursiveNode {
                    payload: Payload::new(id),
                    next: head,
                }));
            }
            black_box(&head);
            Self { head }
        }

        pub fn checksum(&self) -> usize {
            let mut sum = 0usize;
            let mut node = self.head.as_deref();
            while let Some(current) = node {
                sum = sum.wrapping_add(current.payload.checksum());
                node = current.next.as_deref();
            }
            sum
        }
    }

    struct ArenaNode<'a> {
        payload: Payload,
        next: Option<&'a ArenaNode<'a>>,
    }

    pub struct ArenaList {
        head: *const (),
        _arena: Bump,
    }

    impl ArenaList {
        pub fn new(count: usize) -> Self {
            let bytes = count
                .checked_mul(std::mem::size_of::<ArenaNode<'_>>())
                .expect("list is too large");
            let arena = Bump::with_capacity(bytes);
            let mut head = None;
            for id in 0..count {
                head = Some(&*arena.alloc(ArenaNode {
                    payload: Payload::new(id),
                    next: head,
                }));
            }
            black_box(&head);
            let head = head
                .map(|node| node as *const ArenaNode<'_> as *const ())
                .unwrap_or(std::ptr::null());
            Self {
                head,
                _arena: arena,
            }
        }

        fn head(&self) -> Option<&ArenaNode<'_>> {
            if self.head.is_null() {
                None
            } else {
                // SAFETY: `head` points into `_arena`, which is alive for self.
                Some(unsafe { &*self.head.cast::<ArenaNode<'_>>() })
            }
        }

        pub fn checksum(&self) -> usize {
            let mut sum = 0usize;
            let mut node = self.head();
            while let Some(current) = node {
                sum = sum.wrapping_add(current.payload.checksum());
                node = current.next;
            }
            sum
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn representations_have_equivalent_contents() {
        let boxed = ast::BoxTree::new(31).checksum();
        assert_eq!(boxed, ast::ArenaTree::new(31).checksum());

        let arc = dag::ArcDag::new(100).checksum();
        assert_eq!(arc, dag::ArenaDag::new(100).checksum());

        let nested = nested::NestedGraph::new(100).checksum();
        assert_eq!(nested, nested::ArenaGraph::new(100).checksum());

        let recursive = list::RecursiveList::new(100).checksum();
        assert_eq!(recursive, list::ArenaList::new(100).checksum());
    }
}
