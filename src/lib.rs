use std::alloc::{Layout, alloc, dealloc, realloc};
use std::any::{TypeId, type_name};
use std::fmt::Debug;

macro_rules! dbg_assert_index {
    ($self:expr, $index:expr) => {
        #[cfg(debug_assertions)]
        debug_assert!(
            $index < $self.len,
            "BlobVec index (is {}) should be < len (is {})",
            $index,
            $self.len
        );
    };
}

macro_rules! dbg_assert_type_id {
    ($self:expr, $T:ty) => {
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            $self.meta.type_id,
            TypeId::of::<$T>(),
            "BlobVec type mismatch: expected {}, got {}",
            $self.meta.type_name,
            type_name::<$T>()
        );
    };
}

unsafe fn safe_dealloc(ptr: *mut u8, layout: Layout) {
    if layout.size() > 0 {
        unsafe { dealloc(ptr, layout) };
    }
}

#[derive(Debug, Copy, Clone)]
pub struct BlobVecMeta {
    item_layout: Layout,
    drop_fn: fn(*mut u8),

    #[cfg(debug_assertions)]
    type_id: TypeId,
    #[cfg(debug_assertions)]
    type_name: &'static str,
}

impl BlobVecMeta {
    pub fn new<T: Sized + 'static>() -> Self {
        Self {
            item_layout: Layout::new::<T>(),
            drop_fn: |ptr| unsafe { std::ptr::drop_in_place(ptr as *mut T) },

            #[cfg(debug_assertions)]
            type_id: TypeId::of::<T>(),
            #[cfg(debug_assertions)]
            type_name: type_name::<T>(),
        }
    }

    pub fn instantiate(&self) -> BlobVec {
        BlobVec::from_meta(*self)
    }
}

#[derive(Debug)]
pub struct BlobVec {
    meta: BlobVecMeta,
    data: *mut u8,
    len: usize,
    capacity: usize,
}

impl BlobVec {
    // --- construction ---
    pub fn new<T: Sized + 'static>() -> Self {
        Self::from_meta(BlobVecMeta::new::<T>())
    }

    pub fn with_capacity<T: Sized + 'static>(capacity: usize) -> Self {
        let mut this = Self::new::<T>();
        this.grow_to(capacity);
        this
    }

    pub fn from_meta(meta: BlobVecMeta) -> Self {
        Self {
            meta,
            data: std::ptr::NonNull::dangling().as_ptr(),
            len: 0,
            capacity: 0,
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        self.grow_to(self.capacity.checked_add(additional).unwrap());
    }

    // --- access ---
    pub fn get<T: Sized + 'static>(&self, index: usize) -> Option<&T> {
        dbg_assert_type_id!(self, T);
        if index >= self.len {
            return None;
        }
        unsafe {
            let ptr = self.data.add(index * self.meta.item_layout.size()) as *const T;
            Some(&*ptr)
        }
    }

    pub fn get_mut<T: Sized + 'static>(&mut self, index: usize) -> Option<&mut T> {
        dbg_assert_type_id!(self, T);
        if index >= self.len {
            return None;
        }
        unsafe {
            let ptr = self.data.add(index * self.meta.item_layout.size()) as *mut T;
            Some(&mut *ptr)
        }
    }

    pub fn get_ptr_of<T: Sized + 'static>(&self, index: usize) -> *const T {
        dbg_assert_index!(self, index);
        dbg_assert_type_id!(self, T);
        unsafe {
            let ptr = self.data.add(index * self.meta.item_layout.size()) as *const T;
            ptr
        }
    }

    pub fn get_ptr_of_mut<T: Sized + 'static>(&mut self, index: usize) -> *mut T {
        dbg_assert_index!(self, index);
        dbg_assert_type_id!(self, T);
        unsafe {
            let ptr = self.data.add(index * self.meta.item_layout.size()) as *mut T;
            ptr
        }
    }

    pub unsafe fn get_unchecked<T: Sized + 'static>(&self, index: usize) -> &T {
        dbg_assert_index!(self, index);
        dbg_assert_type_id!(self, T);
        unsafe {
            let ptr = self.data.add(index * self.meta.item_layout.size()) as *const T;
            &*ptr
        }
    }

    pub unsafe fn get_unchecked_mut<T: Sized + 'static>(&mut self, index: usize) -> &mut T {
        dbg_assert_index!(self, index);
        dbg_assert_type_id!(self, T);
        unsafe {
            let ptr = self.data.add(index * self.meta.item_layout.size()) as *mut T;
            &mut *ptr
        }
    }

    pub fn as_slice<T: Sized + 'static>(&self) -> &[T] {
        dbg_assert_type_id!(self, T);
        unsafe { std::slice::from_raw_parts(self.data as *const T, self.len) }
    }

    pub fn as_slice_mut<T: Sized + 'static>(&mut self) -> &mut [T] {
        dbg_assert_type_id!(self, T);
        unsafe { std::slice::from_raw_parts_mut(self.data as *mut T, self.len) }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn meta(&self) -> BlobVecMeta {
        self.meta
    }

    #[cfg(debug_assertions)]
    pub fn type_id(&self) -> TypeId {
        self.meta.type_id
    }

    #[cfg(debug_assertions)]
    pub fn type_name(&self) -> &'static str {
        self.meta.type_name
    }

    // --- insertion ---
    pub fn push<T: Sized + 'static>(&mut self, value: T) {
        dbg_assert_type_id!(self, T);
        if self.len == self.capacity {
            self.grow();
        }
        unsafe {
            let dst = self.data.add(self.len * self.meta.item_layout.size());
            std::ptr::write(dst as *mut T, value);
        }
        self.len += 1;
    }

    /// pushes item from ptr, ptr must be a valid ptr to an item of type T, deallocates the ptr after pushing.
    pub unsafe fn push_from_ptr(&mut self, src: *const u8) {
        if self.len == self.capacity {
            self.grow();
        }
        unsafe { self.push_from_ptr_unchecked(src) };
    }

    /// unchecked version of `push_from_ptr`, performs no bounds checks.
    pub unsafe fn push_from_ptr_unchecked(&mut self, src: *const u8) {
        unsafe {
            let dst = self.push_uninit_unchecked();
            std::ptr::copy_nonoverlapping(src, dst, self.meta.item_layout.size());
            safe_dealloc(src as *mut u8, self.meta.item_layout);
        }
    }

    /// unchecked version of `push_from_ptr`, performs no bounds checks, and does not deallocate the ptr.
    pub unsafe fn push_from_ptr_unchecked_no_dealloc(&mut self, src: *const u8) {
        unsafe {
            let dst = self.push_uninit_unchecked();
            std::ptr::copy_nonoverlapping(src, dst, self.meta.item_layout.size());
        }
    }

    /// pushes an uninitialized item, returns its ptr, performs no ptr writes.
    /// initializing the item is the users' responsibility
    pub unsafe fn push_uninit(&mut self) -> *mut u8 {
        if self.len == self.capacity {
            self.grow();
        }
        unsafe { self.push_uninit_unchecked() }
    }

    /// unchecked version of `push_uninit`, performs no bounds checks.
    pub unsafe fn push_uninit_unchecked(&mut self) -> *mut u8 {
        let ptr = unsafe { self.data.add(self.len * self.meta.item_layout.size()) };
        self.len += 1;
        ptr
    }

    // --- removal ---
    pub fn swap_remove(&mut self, index: usize) {
        dbg_assert_index!(self, index);
        unsafe {
            let size = self.meta.item_layout.size();
            let elem_ptr = self.data.add(index * size);
            (self.meta.drop_fn)(elem_ptr);

            self.len -= 1;
            if index != self.len {
                let last_ptr = self.data.add(self.len * size);
                std::ptr::copy_nonoverlapping(last_ptr, elem_ptr, size);
            }
        }
    }

    /// Swap-removes the element at `index`, writing its bytes into `dst`.
    /// `dst` must point to allocation of at least `self.meta.item_layout.size()` bytes.
    pub unsafe fn swap_remove_into(&mut self, index: usize, dst: *mut u8) {
        dbg_assert_index!(self, index);
        unsafe {
            let size = self.meta.item_layout.size();
            let elem_ptr = self.data.add(index * size);

            std::ptr::copy_nonoverlapping(elem_ptr, dst, size);

            self.len -= 1;
            if index != self.len {
                let last_ptr = self.data.add(self.len * size);
                std::ptr::copy_nonoverlapping(last_ptr, elem_ptr, size);
            }
        }
    }

    // --- private ---
    fn grow(&mut self) {
        let new_capacity = match self.capacity {
            0 => 4,
            n => n.checked_mul(2).expect("BlobVec Capacity overflow"),
        };
        self.grow_to(new_capacity);
    }

    fn grow_to(&mut self, new_capacity: usize) {
        debug_assert!(new_capacity > self.capacity);
        let item_size = self.meta.item_layout.size();
        let item_align = self.meta.item_layout.align();
        let new_layout = unsafe {
            // SAFETY: we know that alignment is valid and size is a checked_mul of alignment
            let size = item_size.checked_mul(new_capacity).unwrap();
            Layout::from_size_align_unchecked(size, item_align)
        };

        self.data = unsafe {
            if self.capacity == 0 {
                alloc(new_layout)
            } else {
                // SAFETY: we already know that the old layout is valid
                let size = item_size.unchecked_mul(self.capacity);
                let old_layout = Layout::from_size_align_unchecked(size, item_align);
                realloc(self.data, old_layout, new_layout.size())
            }
        };
        self.capacity = new_capacity;
    }
}

impl Drop for BlobVec {
    fn drop(&mut self) {
        if self.capacity > 0 {
            let item_size = self.meta.item_layout.size();
            let item_align = self.meta.item_layout.align();
            for i in 0..self.len {
                unsafe { (self.meta.drop_fn)(self.data.add(i * item_size)) };
            }
            unsafe {
                // SAFETY: we know that the layout is valid
                let layout =
                    Layout::from_size_align_unchecked(item_size * self.capacity, item_align);
                safe_dealloc(self.data, layout);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_test() {
        let mut vec = BlobVec::new::<u128>();
        vec.push(123u128);
        vec.push(456u128);
        vec.push(789u128);
        println!("{:?}", vec.as_slice::<u128>());

        vec.swap_remove(0);
        println!("{:?}", vec.as_slice::<u128>());

        vec.push(999u128);
        println!("{:?}", vec.as_slice::<u128>());

        vec.swap_remove(0);
        println!("{:?}", vec.as_slice::<u128>());

        println!("{:?}", vec);
    }
}
