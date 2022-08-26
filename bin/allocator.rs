#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL:mimalloc::MiMalloc = mimalloc::MiMalloc;