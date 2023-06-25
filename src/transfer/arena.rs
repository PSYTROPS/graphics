use std::alloc::Layout;

const ALIGNMENT: usize = 8;

pub struct Arena {
    data: *mut u8,
    size: usize, //Content size
    layout: Layout //Capacity & alignment
}

impl Arena {
    pub fn new(size: usize) -> Self {
        let layout = Layout::from_size_align(size, ALIGNMENT).unwrap();
        let data = if size > 0 {
            unsafe {std::alloc::alloc(layout)}
        } else {
            std::ptr::null_mut()
        };
        Self {data, size: 0, layout}
    }

    pub fn extend<T>(&mut self, data: &[T]) -> usize {
        let size = (std::mem::size_of::<T>() * data.len() + ALIGNMENT - 1) & !(ALIGNMENT - 1);
        let offset = self.size;
        self.size += size;
        //Reallocate if necessary
        if self.size > self.layout.size() {
            let layout = Layout::from_size_align(self.size, ALIGNMENT).unwrap();
            if self.layout.size() > 0 {
                self.data = unsafe {
                    std::alloc::realloc(self.data, self.layout, self.size)
                };
            } else {
                self.data = unsafe {
                    std::alloc::alloc(layout)
                };
            }
            self.layout = layout;
        }
        //Write
        unsafe {
            data.as_ptr().copy_to_nonoverlapping(
                self.data.add(offset) as *mut T,
                data.len()
            );
        }
        offset
    }

    pub fn ptr(&self) -> *const u8 {
        self.data
    }

    pub fn size(&self) -> usize {
        self.layout.size()
    }

    pub fn clear(&mut self) {
        self.size = 0;
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        if self.layout.size() > 0 {
            unsafe {
                std::alloc::dealloc(self.data, self.layout);
            }
        }
    }
}
