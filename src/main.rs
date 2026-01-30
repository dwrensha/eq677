use eq677::*;

fn main() {
    setup_panic_hook();
    let _timer = Timer::new();

    M(89, 0).get().cycle_dump();
}
