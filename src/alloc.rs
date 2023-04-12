use crate::EmplaceInitializer;
use std::alloc::{alloc, AllocError, Allocator};
use std::ptr::NonNull;

pub trait EmplaceAllocator {
    fn emplace<Init: EmplaceInitializer>(
        &self,
        init: Init,
    ) -> Result<NonNull<Init::Output>, (AllocError, Init)>;
}

impl<T: Allocator> EmplaceAllocator for T {
    #[inline(always)]
    fn emplace<Init: EmplaceInitializer>(
        &self,
        mut init: Init,
    ) -> Result<NonNull<Init::Output>, (AllocError, Init)> {
        match self.allocate(init.layout()) {
            Ok(mem) => Ok(init.emplace(mem.cast())),
            Err(e) => Err((e, init)),
        }
    }
}

#[inline(always)]
pub unsafe fn alloc_emplace<Init: EmplaceInitializer>(
    mut init: Init,
) -> Result<NonNull<Init::Output>, Init> {
    let mem = alloc(init.layout());
    let Some(mem) = NonNull::new(mem) else{
        return Err(init);
    };
    Ok(init.emplace(mem))
}
