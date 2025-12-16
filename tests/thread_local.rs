use opool::*;

struct SimpleAllocator;

impl PoolAllocator<Box<usize>> for SimpleAllocator {
    fn allocate(&self) -> Box<usize> {
        Box::new(10)
    }
}

#[test]
fn test_new() {
    let pool = LocalPool::new(10, SimpleAllocator);
    assert_eq!(**pool.get(), 10);
}

#[test]
fn test_new_prefilled() {
    let pool = LocalPool::new_prefilled(10, SimpleAllocator);
    assert_eq!(**pool.get(), 10);
}

#[test]
fn test_get_into_inner() {
    let pool = LocalPool::new_prefilled(10, SimpleAllocator);
    let guard = pool.get().into_inner();
    assert_eq!(*guard, 10);
}

#[test]
fn test_try_get_rc() {
    let pool = LocalPool::new_prefilled(1, SimpleAllocator).to_rc();
    let guard = pool.clone().try_get_rc().unwrap();
    let guard2 = pool.clone().try_get_rc();
    assert_eq!(**guard, 10);
    assert_eq!(guard2, None);
}

#[test]
fn test_get_rc() {
    let pool = LocalPool::new_prefilled(10, SimpleAllocator).to_rc();
    let guard = pool.clone().get_rc();
    assert_eq!(**guard, 10);
}

#[test]
fn test_get_rc_into_inner() {
    let pool = LocalPool::new_prefilled(10, SimpleAllocator).to_rc();
    let guard = pool.clone().get_rc().into_inner();
    assert_eq!(*guard, 10);
}
