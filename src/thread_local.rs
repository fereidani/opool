use crate::PoolAllocator;
use alloc::{collections::VecDeque, fmt, rc::Rc};
use core::{
    cell::UnsafeCell,
    hash::{Hash, Hasher},
    marker::PhantomData,
    mem::{forget, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr,
};

/// A struct representing an object pool for local thread, it cannot be moved
/// between threads.
///
/// This struct uses an allocator to create and manage objects, and stores them
/// in an array.
#[derive(Debug)]
pub struct LocalPool<P: PoolAllocator<T>, T> {
    allocator: P,
    storage: UnsafeCell<VecDeque<T>>,
    // force the struct to be !Send
    _phantom: PhantomData<*mut usize>,
}

impl<P: PoolAllocator<T>, T> LocalPool<P, T> {
    /// Creates a new LocalPool with a given size and allocator.
    ///
    /// This method immediately fills the pool with new objects created by the
    /// allocator.
    pub fn new_prefilled(pool_size: usize, allocator: P) -> Self {
        let mut storage = VecDeque::with_capacity(pool_size);
        for _ in 0..pool_size {
            storage.push_back(allocator.allocate());
        }
        LocalPool {
            allocator,
            storage: UnsafeCell::new(storage),
            _phantom: PhantomData,
        }
    }

    /// Creates a new Object Pool with a given size and allocator.
    ///
    /// Unlike [`Self::new_prefilled`], this method does not immediately fill
    /// the pool with objects.
    pub fn new(pool_size: usize, allocator: P) -> Self {
        LocalPool {
            allocator,
            storage: UnsafeCell::new(VecDeque::with_capacity(pool_size)),
            _phantom: PhantomData,
        }
    }

    /// Get storage as mutable reference
    /// Safety: it's safe to call only if the pool is used by a single threaded.
    #[allow(clippy::mut_from_ref)]
    fn storage_mut(&self) -> &mut VecDeque<T> {
        unsafe { &mut *self.storage.get() }
    }

    /// Borrows storage as immutable reference
    /// Safety: it's safe to call only if the pool is used by a single threaded.
    #[allow(clippy::mut_from_ref)]
    fn storage_borrow(&self) -> &VecDeque<T> {
        unsafe { &*self.storage.get() }
    }

    /// Wraps the pool allocator with an reference counter, enabling the
    /// use of [`Self::get_rc`] to obtain pool-allocated objects that rely on
    /// reference counted references instead of borrowed references.
    pub fn to_rc(self) -> Rc<Self> {
        Rc::new(self)
    }

    /// Attempts to get an object from the pool.
    ///
    /// If the pool is empty, None is returned.
    pub fn try_get(&self) -> Option<RefLocalGuard<'_, P, T>> {
        self.storage_mut().pop_front().map(|mut obj| {
            self.allocator.reset(&mut obj);
            RefLocalGuard::new(obj, self)
        })
    }

    /// Gets an object from the pool.
    ///
    /// If the pool is empty, a new object is created using the allocator.
    pub fn get(&'_ self) -> RefLocalGuard<'_, P, T> {
        match self.storage_mut().pop_front() {
            Some(mut obj) => {
                self.allocator.reset(&mut obj);
                RefLocalGuard::new(obj, self)
            }
            None => RefLocalGuard::new(self.allocator.allocate(), self),
        }
    }

    /// Attempts to get an object from the pool that holds an rc reference to the owning
    /// pool. Allocated objects are not as efficient as those allocated by
    /// [`Self::get`] method but they are easier to move as they are not limited
    /// by allocator lifetime directly.
    ///
    /// If the pool is empty, None is returned.
    pub fn try_get_rc(self: Rc<Self>) -> Option<RcLocalGuard<P, T>> {
        self.storage_mut().pop_front().map(|mut obj| {
            self.allocator.reset(&mut obj);
            RcLocalGuard::new(obj, &self)
        })
    }

    /// Gets an object from the pool that holds an rc reference to the owning
    /// pool. Allocated objects are not as efficient as those allocated by
    /// [`Self::get`] method but they are easier to move as they are not limited
    /// by allocator lifetime directly.
    ///
    /// If the pool is empty, a new object is created using the allocator.
    pub fn get_rc(self: Rc<Self>) -> RcLocalGuard<P, T> {
        match self.storage_mut().pop_front() {
            Some(mut obj) => {
                self.allocator.reset(&mut obj);
                RcLocalGuard::new(obj, &self)
            }
            None => RcLocalGuard::new(self.allocator.allocate(), &self),
        }
    }

    /// Gets the number of objects currently in the pool.
    ///
    /// Returns the length of the internal storage, indicating the number of
    /// objects that are ready to be recycled from the pool.
    pub fn len(&self) -> usize {
        self.storage_borrow().len()
    }

    /// Checks if the pool is empty.
    ///
    /// Returns `true` if there are no objects currently in the pool that are
    /// ready to be recycled.
    pub fn is_empty(&self) -> bool {
        self.storage_borrow().is_empty()
    }

    /// Gets the capacity of the pool.
    ///
    /// Returns the maximum number of objects that the pool can hold. This does
    /// not indicate the maximum number of objects that can be allocated,
    /// but maximum objects that can be stored and recycled from the pool.
    pub fn cap(&self) -> usize {
        self.storage_borrow().capacity()
    }
}

/// A struct representing a guard over an object in the pool.
///
/// This struct ensures that the object is returned to the pool when it is
/// dropped.
pub struct RefLocalGuard<'a, P: PoolAllocator<T>, T> {
    obj: MaybeUninit<T>,
    pool: &'a LocalPool<P, T>,
}

impl<'a, P: PoolAllocator<T>, T> RefLocalGuard<'a, P, T> {
    /// Creates a new Guard for an object and a reference to the pool it
    /// belongs to.
    fn new(obj: T, pool: &'a LocalPool<P, T>) -> Self {
        RefLocalGuard {
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

impl<'a, P: PoolAllocator<T>, T> Deref for RefLocalGuard<'a, P, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.obj.as_ptr() }
    }
}

impl<'a, P: PoolAllocator<T>, T> DerefMut for RefLocalGuard<'a, P, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.obj.as_mut_ptr() }
    }
}

/// Implementation of the Drop trait for Guard.
///
/// This ensures that the object is returned to the pool when the guard is
/// dropped, unless the object fails validation.
impl<'a, P: PoolAllocator<T>, T> Drop for RefLocalGuard<'a, P, T> {
    fn drop(&mut self) {
        let storage = self.pool.storage_mut();
        if self.pool.allocator.is_valid(self.deref()) && storage.len() < storage.capacity() {
            // Safety: object is not moved and valid for this single move to the pool.
            storage.push_back(unsafe { ptr::read(self.obj.as_mut_ptr()) });
        } else {
            // Safety: object is not moved back to the pool it is safe to drop it in place.
            unsafe {
                ptr::drop_in_place(self.obj.as_mut_ptr());
            }
        }
    }
}

impl<'a, P: PoolAllocator<T>, T: Hash> Hash for RefLocalGuard<'a, P, T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}
impl<'a, P: PoolAllocator<T>, T: fmt::Display> fmt::Display for RefLocalGuard<'a, P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
impl<'a, P: PoolAllocator<T>, T: fmt::Debug> fmt::Debug for RefLocalGuard<'a, P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}
impl<'a, P: PoolAllocator<T>, T> fmt::Pointer for RefLocalGuard<'a, P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&(&**self as *const T), f)
    }
}
impl<'a, P: PoolAllocator<T>, T: PartialEq> PartialEq for RefLocalGuard<'a, P, T> {
    #[inline]
    fn eq(&self, other: &RefLocalGuard<'a, P, T>) -> bool {
        self.deref().eq(other)
    }
}
impl<'a, P: PoolAllocator<T>, T: Eq> Eq for RefLocalGuard<'a, P, T> {}
impl<'a, P: PoolAllocator<T>, T: PartialOrd> PartialOrd for RefLocalGuard<'a, P, T> {
    #[inline]
    fn partial_cmp(&self, other: &RefLocalGuard<'a, P, T>) -> Option<core::cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
    #[inline]
    fn lt(&self, other: &RefLocalGuard<'a, P, T>) -> bool {
        **self < **other
    }
    #[inline]
    fn le(&self, other: &RefLocalGuard<'a, P, T>) -> bool {
        **self <= **other
    }
    #[inline]
    fn gt(&self, other: &RefLocalGuard<'a, P, T>) -> bool {
        **self > **other
    }
    #[inline]
    fn ge(&self, other: &RefLocalGuard<'a, P, T>) -> bool {
        **self >= **other
    }
}
impl<'a, P: PoolAllocator<T>, T: Ord> Ord for RefLocalGuard<'a, P, T> {
    #[inline]
    fn cmp(&self, other: &RefLocalGuard<'a, P, T>) -> core::cmp::Ordering {
        (**self).cmp(&**other)
    }
}
impl<'a, P: PoolAllocator<T>, T> core::borrow::Borrow<T> for RefLocalGuard<'a, P, T> {
    #[inline(always)]
    fn borrow(&self) -> &T {
        self
    }
}
impl<'a, P: PoolAllocator<T>, T> AsRef<T> for RefLocalGuard<'a, P, T> {
    #[inline(always)]
    fn as_ref(&self) -> &T {
        self
    }
}

/// A struct representing a guard over an object in the pool.
///
/// This struct ensures that the object is returned to the pool when it is
/// dropped.
pub struct RcLocalGuard<P: PoolAllocator<T>, T> {
    obj: MaybeUninit<T>,
    pool: Rc<LocalPool<P, T>>,
}

impl<P: PoolAllocator<T>, T> RcLocalGuard<P, T> {
    /// Creates a new Guard for an object and a reference to the pool it
    /// belongs to.
    fn new(obj: T, pool: &Rc<LocalPool<P, T>>) -> Self {
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

impl<P: PoolAllocator<T>, T> Deref for RcLocalGuard<P, T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.obj.as_ptr() }
    }
}

impl<P: PoolAllocator<T>, T> DerefMut for RcLocalGuard<P, T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.obj.as_mut_ptr() }
    }
}

/// Implementation of the Drop trait for Guard.
///
/// This ensures that the object is returned to the pool when the guard is
/// dropped, unless the object fails validation.
impl<P: PoolAllocator<T>, T> Drop for RcLocalGuard<P, T> {
    fn drop(&mut self) {
        let storage = self.pool.storage_mut();
        if self.pool.allocator.is_valid(self.deref()) && storage.len() < storage.capacity() {
            // Safety: object is not moved and valid for this single move to the pool.
            storage.push_back(unsafe { ptr::read(self.obj.as_mut_ptr()) });
        } else {
            // Safety: object is not moved back to the pool it is safe to drop it in place.
            unsafe {
                ptr::drop_in_place(self.obj.as_mut_ptr());
            }
        }
    }
}

impl<P: PoolAllocator<T>, T: Hash> Hash for RcLocalGuard<P, T> {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        (**self).hash(state);
    }
}
impl<P: PoolAllocator<T>, T: fmt::Display> fmt::Display for RcLocalGuard<P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}
impl<P: PoolAllocator<T>, T: fmt::Debug> fmt::Debug for RcLocalGuard<P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}
impl<P: PoolAllocator<T>, T> fmt::Pointer for RcLocalGuard<P, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Pointer::fmt(&(&**self as *const T), f)
    }
}
impl<P: PoolAllocator<T>, T: PartialEq> PartialEq for RcLocalGuard<P, T> {
    #[inline]
    fn eq(&self, other: &RcLocalGuard<P, T>) -> bool {
        self.deref().eq(other)
    }
}
impl<P: PoolAllocator<T>, T: Eq> Eq for RcLocalGuard<P, T> {}
impl<P: PoolAllocator<T>, T: PartialOrd> PartialOrd for RcLocalGuard<P, T> {
    #[inline]
    fn partial_cmp(&self, other: &RcLocalGuard<P, T>) -> Option<core::cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
    #[inline]
    fn lt(&self, other: &RcLocalGuard<P, T>) -> bool {
        **self < **other
    }
    #[inline]
    fn le(&self, other: &RcLocalGuard<P, T>) -> bool {
        **self <= **other
    }
    #[inline]
    fn gt(&self, other: &RcLocalGuard<P, T>) -> bool {
        **self > **other
    }
    #[inline]
    fn ge(&self, other: &RcLocalGuard<P, T>) -> bool {
        **self >= **other
    }
}
impl<P: PoolAllocator<T>, T: Ord> Ord for RcLocalGuard<P, T> {
    #[inline]
    fn cmp(&self, other: &RcLocalGuard<P, T>) -> core::cmp::Ordering {
        (**self).cmp(&**other)
    }
}
impl<P: PoolAllocator<T>, T> core::borrow::Borrow<T> for RcLocalGuard<P, T> {
    #[inline(always)]
    fn borrow(&self) -> &T {
        self
    }
}
impl<P: PoolAllocator<T>, T> AsRef<T> for RcLocalGuard<P, T> {
    #[inline(always)]
    fn as_ref(&self) -> &T {
        self
    }
}
