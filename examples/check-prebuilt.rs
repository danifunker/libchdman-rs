//! Smoke test: confirms the libchdman_rs static archive linked in this
//! build is functional. Run with:
//!
//!   cargo run --example check-prebuilt --features prebuilt
//!
//! or without `--features prebuilt` for the source-build path. A successful
//! link + run is the full validation we can do without a real CHD file.

use libchdman_rs::sys;

fn main() {
    unsafe {
        let chd = sys::chd_shim_alloc();
        if chd.is_null() {
            eprintln!("FAIL: chd_shim_alloc returned null");
            std::process::exit(1);
        }
        let version = sys::chd_shim_version(chd);
        sys::chd_shim_free(chd);
        println!("libchdman_rs version field: {version}");
    }
    println!("OK: archive is linked and FFI entry points are callable.");
}
