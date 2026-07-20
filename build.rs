// build.rs — emit linker flags for the cdylib target only.
//
// cargo:rustc-cdylib-link-arg is the correct mechanism for passing
// extra flags to the linker when building a cdylib; it is a no-op
// for the bin target so the daemon build is unaffected.
fn main() {
    // Version script: controls which symbols are exported and at which
    // LIBINPUT_0.x version node, matching the upstream libinput ABI.
    println!("cargo:rustc-cdylib-link-arg=-Wl,--version-script=libinput.map");

    // soname: tells the dynamic linker the canonical name of this .so
    // so that `ldconfig` and `ln -s libinput.so.0 libinput.so` work.
    println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libinput.so.0");

    // Re-run this script only when the version map changes.
    println!("cargo:rerun-if-changed=libinput.map");
}
