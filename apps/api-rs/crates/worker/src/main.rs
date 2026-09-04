#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() {
    println!("worker stub — will be implemented in Plan 1");
}
