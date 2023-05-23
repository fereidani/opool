# Opool: Fast lock-free concurrent and local object pool

[![Crates.io][crates-badge]][crates-url]
[![Documentation][doc-badge]][doc-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-badge]: https://img.shields.io/crates/v/opool.svg?style=for-the-badge
[crates-url]: https://crates.io/crates/opool
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg?style=for-the-badge
[mit-url]: https://github.com/fereidani/opool/blob/main/LICENSE
[doc-badge]: https://img.shields.io/docsrs/opool?style=for-the-badge
[doc-url]: https://docs.rs/opool

Opool is a high-performance Rust library that offers a concurrent and local object pool implementation. It aims to provide efficiency and flexibility, enabling you to manage the lifecycle of your objects and reuse them to minimize allocation overhead. Opool supports `no_std` with alloc available.

## Why Use Opool

- Superior Performance: Opool outperforms alternatives due to its design choices, particularly its utilization of the [`PoolAllocator`], which facilitates function inlining by the compiler. This results in better-executing code by reducing unnecessary function calls and jumps.
- Lock-Free Design: Opool operates without any mutexes, ensuring a lock-free implementation. It minimizes reliance on operating system syscalls, apart from those provided by the alloc crate, further enhancing performance.
- Enhanced Compatibility: Opool supports `no_std` environments with the availability of alloc, making it suitable for a wide range of Rust projects.
- Comprehensive Interface: Opool provides a complete interface that automates object allocation, cleanup, and verification for your object pool. You no longer need to manually clean up pool-allocated data, and you can optionally provide a related [`PoolAllocator::reset`] function to clean the object upon automatic collection.
- Reference Counted References: Opool supports reference-counted references, although it is recommended to use static references whenever possible. This feature simplifies the lifetimes of your Rust code, particularly in specific scenarios.

## Structures

- **[`PoolAllocator`] Trait**: This trait defines the interface for a pool allocator. It includes methods for allocating, resetting, and validating objects. The resetting and validating functions are optional.
- **[`Pool`] Struct**: This struct represents an object pool. It uses an ArrayQueue for storage and a PoolAllocator for object management.
- **[`LocalPool`] Struct**: This struct represents a thread-local object pool, restricted to use within the current thread. It utilizes a VecDeque for storage and a PoolAllocator for object management.
- **[`RefGuard`], [`RcGuard`], [`RefLocalGuard`] and [`RcLocalGuard`] Structs**: These structs are smart pointers that automatically return the object to the pool when they are dropped. They also provide methods for accessing the underlying object.

## Usage

First, define your allocator by implementing the [`PoolAllocator`] trait. This involves providing a [`PoolAllocator::allocate`] method to create new objects and optionally a [`PoolAllocator::reset`] method to reset objects to their initial state and a [`PoolAllocator::is_valid`] method to check if an object is still valid for pushing back into the pool.

Then, create a [`Pool`] or [`LocalPool`] with your allocator. You can use the `new` method to create an empty pool or the `new_prefilled` method to create a pool that is initially filled with a certain number of objects.

To get an object from the pool, use the `get` method. This will return a `RefGuard` or `RcGuard` depending on whether you called `get` or `get_rc`. These guards automatically return the object to the pool when they are dropped.

To use `get_rc` you need to convert the pool to reference counted flavor by calling `to_rc` on it.

Here is an example:

```rust
use opool::{Pool, PoolAllocator};
struct MyAllocator;

const BUF_SIZE: usize = 1024 * 8;
impl PoolAllocator<Vec<u8>> for MyAllocator {
    #[inline]
    fn allocate(&self) -> Vec<u8> {
        vec![0; BUF_SIZE]
    }

    /// OPTIONAL METHODS:

    #[inline]
    fn reset(&self, _obj: &mut Vec<u8>) {
        // Optionally you can clear or zero object fields here
    }

    #[inline]
    fn is_valid(&self, obj: &Vec<u8>) -> bool {
        // you can optionally is_valid if object is good to be pushed back to the pool
        obj.capacity() == BUF_SIZE
    }
}

let pool = Pool::new(64, MyAllocator);
let obj = pool.get();
// Use the object, and it will be automatically recycled after its lifetime ends.

```

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
opool = "0.1"
```

## License

Opool is licensed under the MIT license. Please see the `LICENSE` file for more details.
