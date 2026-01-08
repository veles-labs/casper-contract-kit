#[cfg(not(target_arch = "wasm32"))]
#[allow(unused_imports)]
use veles_casper_ffi_shim::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm_allocator"))]
use lol_alloc::{AssumeSingleThreaded, FreeListAllocator};

#[cfg(all(target_arch = "wasm32", feature = "wasm_allocator"))]
#[global_allocator]
static ALLOCATOR: AssumeSingleThreaded<FreeListAllocator> =
    unsafe { AssumeSingleThreaded::new(FreeListAllocator::new()) };

#[cfg(all(
    target_arch = "wasm32",
    feature = "wasm_panic_handler",
    not(feature = "std")
))]
#[panic_handler]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    crate::log!("Panic occurred: {}", info);
    casper_contract::contract_api::runtime::revert(casper_types::ApiError::User(0))
}
