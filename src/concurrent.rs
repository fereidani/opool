use crate::PoolAllocator;
use alloc::{fmt, sync::Arc};
use core::{
    hash::{Hash, Hasher},
    mem::{forget, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr,
};
use crossbeam_queue::ArrayQueue;

/// A struct representing an object pool.
///
/// This struct uses an allocator to create and manage objects, and stores them
/// in an ArrayQueue.
#[derive(Debug)]
pub struct Pool<P, T> {
    allocator: P,
    storage: ArrayQueue<T>,
}

// If T is Send it is safe to move object pool between threads
unsafe impl<P: Send, T: Send> Send for Pool<P, T> {}

impl<P, T> Pool<P, T> {
    /// Creates a new Object Pool with a given size and allocator.
    ///
    /// Unlike [`Self::new_prefilled`], this method does not immediately fill
    /// the pool with objects.
    pub fn new(pool_size: usize, allocator: P) -> Self {
        let storage = ArrayQueue::new(pool_size);
        Pool { allocator, storage }
    }

    /// Wraps the pool allocator with an atomic reference counter, enabling the
    /// use of [`Self::get_rc`] to obtain pool-allocated objects that rely on
    /// reference counted references instead of borrowed references.
    pub fn to_rc(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// Gets the number of objects currently in the pool.
    ///
    /// Returns the length of the internal storage, indicating the number of
    /// objects that are ready to be recycled from the pool.
    pub fn len(&self) -> usize {
        self.storage.len()
    }

    /// Gets the capacity of the pool.
    ///
    /// Returns the maximum number of objects that the pool can hold. This does
    /// not indicate the maximum number of objects that can be allocated,
    /// but maximum objects that can be stored and recycled from the pool.
    pub fn cap(&self) -> usize {
        self.storage.capacity()
    }
}

impl<P: PoolAllocator<T>, T> Pool<P, T> {
    /// Creates a new Pool with a given size and allocator.
    ///
    /// This method immediately fills the pool with new objects created by the
    /// allocator.
    pub fn new_prefilled(pool_size: usize, allocator: P) -> Self {
        let storage = ArrayQueue::new(pool_size);
        for _ in 0..pool_size {
            let _ = storage.push(allocator.allocate());
        }
        Pool { allocator, storage }
    }

    /// Gets an object from the pool.
    ///
    /// If the pool is empty, a new object is created using the allocator.
    pub fn get(&self) -> RefGuard<P, T> {
        match self.storage.pop() {
            Some(mut obj) => {
                self.allocator.reset(&mut obj);
                RefGuard::new(obj, self)
            }
            None => RefGuard::new(self.allocator.allocate(), self),
        }
    }

    /// Gets an object from the pool that holds an arc reference to the owning
    /// pool. Allocated objects are not as efficient as those allocated by
    /// [`Self::get`] method but they are easier to move as they are not limited
    /// by allocator lifetime directly.
    ///
    /// If the pool is empty, a new object is created using the allocator.
    pub fn get_rc(self: Arc<Self>) -> RcGuard<P, T> {
        match self.storage.pop() {
            Some(mut obj) => {
                self.allocator.reset(&mut obj);
                RcGuard::new(obj, &self)
            }
            None => RcGuard::new(self.allocator.allocate(), &self),
        }
    }
}

/// A struct representing a guard over an object in the pool.
///
/// This struct ensures that the object is returned to the pool when it is
/// dropped.
pub struct RefGuard<'a, P: PoolAllocator<T>, T> {
    obj: MaybeUninit<T>,
    pool: &'a Pool<P, T>,
}

impl<'a, P: PoolAllocator<T>, T> RefGuard<'a, P, T> {
    /// Creates a new Guard for an object and a reference to the pool it
    /// belongs to.
    fn new(obj: T, pool: &'a Pool<P, T>) -> Self {
        RefGuard {
            obj: MaybeUninit::new(obj),
            pool,
        }
    }

    /// Consumes the guard and returns the object, without returning it to the
    /// pool.
    ///
    /// This method should be used with caution, as it leads to objects not
    /// being returned to the pool.
    pub fn into_inner(self) -> T {
        let obj = unsafe { self.obj.as_ptr().read() };
        forget(self);
        obj
    }
}

impl<'a, P: PoolAllocator<T>, T> Deref for RefGuard<'a, P, T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.obj.as_ptr() }
    }
}

impl<'a, P: PoolAllocator<T>, T> DerefMut for RefGuard<'a, P, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.obj.as_mut_ptr() }
    }
}

/// Implementation of the Drop trait for Guard.
///
/// This ensures that the object is returned to the pool when the guard is
/// dropped, unless the object fails validation.
impl<'a, P: PoolAllocator<T>, T> Drop for RefGuard<'a, P, T> {
    fn drop(&mut self) {
        if self.pool.allocator.is_valid(self.deref()) {
            let _ = self
                .pool
                .storage
                .push(unsafe { ptr::read(self.obj.as_mut_ptr()) });
        } else {
            unsafe {
                ptr::drop_in_place(self.obj.as_mut_ptr());
            }
        }
    }
}

impl<'a, P: PoolAllocator<T>, T: Hash> Hash for RefGuard<'a, P, T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}
impl<'a, P: PoolAllocator<T>, T: fmt::Display> fmt::Display for RefGuard<'a, P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
impl<'a, P: PoolAllocator<T>, T: fmt::Debug> fmt::Debug for RefGuard<'a, P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}
impl<'a, P: PoolAllocator<T>, T> fmt::Pointer for RefGuard<'a, P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&(&**self as *const T), f)
    }
}
impl<'a, P: PoolAllocator<T>, T: PartialEq> PartialEq for RefGuard<'a, P, T> {
    #[inline]
    fn eq(&self, other: &RefGuard<'a, P, T>) -> bool {
        self.deref().eq(other)
    }
}
impl<'a, P: PoolAllocator<T>, T: Eq> Eq for RefGuard<'a, P, T> {}
impl<'a, P: PoolAllocator<T>, T: PartialOrd> PartialOrd for RefGuard<'a, P, T> {
    #[inline]
    fn partial_cmp(&self, other: &RefGuard<'a, P, T>) -> Option<core::cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
    #[inline]
    fn lt(&self, other: &RefGuard<'a, P, T>) -> bool {
        **self < **other
    }
    #[inline]
    fn le(&self, other: &RefGuard<'a, P, T>) -> bool {
        **self <= **other
    }
    #[inline]
    fn gt(&self, other: &RefGuard<'a, P, T>) -> bool {
        **self > **other
    }
    #[inline]
    fn ge(&self, other: &RefGuard<'a, P, T>) -> bool {
        **self >= **other
    }
}
impl<'a, P: PoolAllocator<T>, T: Ord> Ord for RefGuard<'a, P, T> {
    #[inline]
    fn cmp(&self, other: &RefGuard<'a, P, T>) -> core::cmp::Ordering {
        (**self).cmp(&**other)
    }
}
impl<'a, P: PoolAllocator<T>, T> core::borrow::Borrow<T> for RefGuard<'a, P, T> {
    #[inline(always)]
    fn borrow(&self) -> &T {
        self
    }
}
impl<'a, P: PoolAllocator<T>, T> AsRef<T> for RefGuard<'a, P, T> {
    #[inline(always)]
    fn as_ref(&self) -> &T {
        self
    }
}

/// A struct representing a guard over an object in the pool.
///
/// This struct ensures that the object is returned to the pool when it is
/// dropped.
pub struct RcGuard<P: PoolAllocator<T>, T> {
    obj: MaybeUninit<T>,
    pool: Arc<Pool<P, T>>,
}

impl<P: PoolAllocator<T>, T> RcGuard<P, T> {
    /// Creates a new Guard for an object and a reference to the pool it
    /// belongs to.
    fn new(obj: T, pool: &Arc<Pool<P, T>>) -> Self {
        Self {
            obj: MaybeUninit::new(obj),
            pool: pool.clone(),
        }
    }

    /// Consumes the guard and returns the object, without returning it to the
    /// pool.
    ///
    /// This method should be used with caution, as it leads to objects not
    /// being returned to the pool.
    pub fn into_inner(mut self) -> T {
        let obj = unsafe { self.obj.as_ptr().read() };
        // Drop the arc reference
        unsafe { ptr::drop_in_place(&mut self.pool) }
        forget(self);
        obj
    }
}

impl<P: PoolAllocator<T>, T> Deref for RcGuard<P, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.obj.as_ptr() }
    }
}

impl<P: PoolAllocator<T>, T> DerefMut for RcGuard<P, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.obj.as_mut_ptr() }
    }
}

/// Implementation of the Drop trait for Guard.
///
/// This ensures that the object is returned to the pool when the guard is
/// dropped, unless the object fails validation.
impl<P: PoolAllocator<T>, T> Drop for RcGuard<P, T> {
    fn drop(&mut self) {
        if self.pool.allocator.is_valid(self.deref()) {
            let _ = self
                .pool
                .storage
                .push(unsafe { ptr::read(self.obj.as_mut_ptr()) });
        } else {
            unsafe {
                ptr::drop_in_place(self.obj.as_mut_ptr());
            }
        }
    }
}

impl<P: PoolAllocator<T>, T: Hash> Hash for RcGuard<P, T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}
impl<P: PoolAllocator<T>, T: fmt::Display> fmt::Display for RcGuard<P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
impl<P: PoolAllocator<T>, T: fmt::Debug> fmt::Debug for RcGuard<P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}
impl<P: PoolAllocator<T>, T> fmt::Pointer for RcGuard<P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&(&**self as *const T), f)
    }
}
impl<P: PoolAllocator<T>, T: PartialEq> PartialEq for RcGuard<P, T> {
    #[inline]
    fn eq(&self, other: &RcGuard<P, T>) -> bool {
        self.deref().eq(other)
    }
}
impl<P: PoolAllocator<T>, T: Eq> Eq for RcGuard<P, T> {}
impl<P: PoolAllocator<T>, T: PartialOrd> PartialOrd for RcGuard<P, T> {
    #[inline]
    fn partial_cmp(&self, other: &RcGuard<P, T>) -> Option<core::cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
    #[inline]
    fn lt(&self, other: &RcGuard<P, T>) -> bool {
        **self < **other
    }
    #[inline]
    fn le(&self, other: &RcGuard<P, T>) -> bool {
        **self <= **other
    }
    #[inline]
    fn gt(&self, other: &RcGuard<P, T>) -> bool {
        **self > **other
    }
    #[inline]
    fn ge(&self, other: &RcGuard<P, T>) -> bool {
        **self >= **other
    }
}
impl<P: PoolAllocator<T>, T: Ord> Ord for RcGuard<P, T> {
    #[inline]
    fn cmp(&self, other: &RcGuard<P, T>) -> core::cmp::Ordering {
        (**self).cmp(&**other)
    }
}
impl<P: PoolAllocator<T>, T> core::borrow::Borrow<T> for RcGuard<P, T> {
    #[inline(always)]
    fn borrow(&self) -> &T {
        self
    }
}
impl<P: PoolAllocator<T>, T> AsRef<T> for RcGuard<P, T> {
    #[inline(always)]
    fn as_ref(&self) -> &T {
        self
    }
}
