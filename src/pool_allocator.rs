/// A trait defining the interface for a pool allocator.
///
/// This trait provides methods for resetting and creating new objects,
/// as well as validating objects before they are stored back in the object
/// pool.
pub trait PoolAllocator<T> {
    /// Resets the state of an object to its initial state if necessary.
    ///
    /// By default, this method do nothing. Override this method to provide
    /// custom reset logic.
    #[inline(always)]
    fn reset(&self, _obj: &mut T) {}

    /// Creates a new object of type T.
    fn allocate(&self) -> T;

    /// validates that an object is in a good state to be stored back in the
    /// object pool.
    ///
    /// By default, this method always returns true. Override this method to
    /// provide custom validation logic.
    #[inline(always)]
    fn is_valid(&self, _obj: &T) -> bool {
        true
    }
}
