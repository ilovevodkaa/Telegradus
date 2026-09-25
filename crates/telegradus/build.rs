//! Lets a packaged Linux binary load `libtdjson` from its own directory.

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    // `$ORIGIN` reaches the linker verbatim: Cargo and rustc pass arguments
    // without a shell, so it needs no escaping here.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo::rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
}
