use std::any::TypeId;

pub struct BlobVec {
    data: Vec<u8>,
    type_id: TypeId,
    elem_size: usize,
    drop_fn: fn(*mut u8),
}

impl BlobVec {
    pub fn new<T: 'static>() -> Self {
        Self {
            data: Vec::new(),
            type_id: TypeId::of::<T>(),
            elem_size: size_of::<T>(),
            drop_fn: |ptr| unsafe { std::ptr::drop_in_place(ptr as *mut T) },
        }
    }
    //
    // fn grow(&mut self) {
    //     let new_capacity = if self.capacity == 0 { 4 } else { self.capacity * 2 };
    //     self.data.reserve(new_capacity - self.data.capacity());
    // }

    pub fn push<T: 'static>(&mut self, element: T) {
        debug_assert_eq!(self.type_id, TypeId::of::<T>());
        let bytes = unsafe {
            std::slice::from_raw_parts(&element as *const T as *const u8, size_of::<T>())
        };
        self.data.extend_from_slice(bytes);
        std::mem::forget(element);
    }

    pub fn get<T: 'static>(&self, index: usize) -> &T {
        debug_assert_eq!(self.type_id, TypeId::of::<T>());
        debug_assert!(index * self.elem_size < self.data.len());
        unsafe { &*(self.data.as_ptr().add(index * self.elem_size) as *const T) }
    }

    pub fn swap_remove(&mut self, index: usize) {
        debug_assert!(index * self.elem_size < self.data.len());
        let buffer_ptr = self.data.as_mut_ptr();
        unsafe {
            let elem_ptr = buffer_ptr.add(index * self.elem_size);
            let last_ptr = buffer_ptr.add((self.data.len() - 1) * self.elem_size);
            (self.drop_fn)(elem_ptr);

            if elem_ptr < last_ptr {
                std::ptr::copy_nonoverlapping(last_ptr, elem_ptr, self.elem_size);
            }
            self.data.set_len(self.data.len() - self.elem_size);
        }
    }

    pub fn as_slice<T: 'static>(&self) -> &[T] {
        debug_assert_eq!(self.type_id, TypeId::of::<T>());
        unsafe {
            std::slice::from_raw_parts(
                self.data.as_ptr() as *const T,
                self.data.len() / self.elem_size,
            )
        }
    }

    pub fn get_mut<T: 'static>(&mut self, index: usize) -> &mut T {
        debug_assert_eq!(self.type_id, TypeId::of::<T>());
        let offset = index * self.elem_size;
        unsafe { &mut *(self.data.as_mut_ptr().add(offset) as *mut T) }
    }

    pub fn as_slice_mut<T: 'static>(&mut self) -> &mut [T] {
        debug_assert_eq!(self.type_id, TypeId::of::<T>());
        unsafe {
            std::slice::from_raw_parts_mut(
                self.data.as_mut_ptr() as *mut T,
                self.data.len() / self.elem_size,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_test() {
        assert_eq!(1, 1);
    }
}
