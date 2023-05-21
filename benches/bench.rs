use criterion::{black_box, criterion_group, criterion_main, Criterion};
use opool::*;

struct Allocator {}

const POOL_OBJECT_SIZE: usize = 1024 * 1024 * 1024;
impl PoolAllocator<Vec<u8>> for Allocator {
    #[inline(always)]
    fn allocate(&self) -> Vec<u8> {
        Vec::with_capacity(POOL_OBJECT_SIZE)
    }

    #[inline(always)]
    fn reset(&self, obj: &mut Vec<u8>) {
        obj.clear()
    }

    #[inline(always)]
    fn is_valid(&self, obj: &Vec<u8>) -> bool {
        obj.capacity() == POOL_OBJECT_SIZE
    }
}

fn allocate(c: &mut Criterion) {
    c.bench_function("opool", |b| {
        let pool = Pool::new(1024, Allocator {});
        b.iter(|| {
            let obj = black_box(pool.get());
            black_box(obj.capacity())
        })
    });
    c.bench_function("opool_thread_local", |b| {
        let pool = LocalPool::new(1024, Allocator {});
        b.iter(|| {
            let obj = black_box(pool.get());
            black_box(obj.capacity())
        })
    });
    c.bench_function("system", |b| {
        let alloc: Allocator = Allocator {};
        b.iter(|| {
            let obj = black_box(alloc.allocate());
            black_box(obj.capacity())
        })
    });
}

fn allocate_multi(c: &mut Criterion) {
    use rayon::prelude::*;
    c.bench_function("opool_multi", |b| {
        let pool = Pool::new(1024, Allocator {});
        b.iter(|| {
            (0..8192).into_par_iter().for_each(|_i| {
                let obj = black_box(pool.get());
                black_box(obj.capacity());
            });
        })
    });

    c.bench_function("system_multi", |b| {
        let alloc: Allocator = Allocator {};
        b.iter(|| {
            (0..8192).into_par_iter().for_each(|_i| {
                let obj = black_box(alloc.allocate());
                black_box(obj.capacity());
            });
        })
    });
}
criterion_group!(benches, allocate, allocate_multi);
criterion_main!(benches);

struct MyAllocator {}
const BUF_SIZE: usize = 1024 * 8;
impl PoolAllocator<Vec<u8>> for MyAllocator {
    #[inline]
    fn allocate(&self) -> Vec<u8> {
        vec![0; BUF_SIZE]
    }

    // Optionally you can provide
    #[inline]
    fn reset(&self, _obj: &mut Vec<u8>) {
        // You can clear or zero object fields here
    }

    #[inline]
    fn is_valid(&self, obj: &Vec<u8>) -> bool {
        // you can optionally is_valid if object is good to be pushed back to the pool
        obj.capacity() == BUF_SIZE
    }
}
