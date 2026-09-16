use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=LIBARCHIVE_NO_PKG_CONFIG");
    println!("cargo:rerun-if-env-changed=LIBARCHIVE_STATIC");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    println!("cargo:rerun-if-env-changed=VCPKG_ROOT");
    println!("cargo:rerun-if-env-changed=VCPKGRS_DYNAMIC");

    let target = env::var("TARGET").expect("Cargo must set TARGET for build scripts");
    if target.contains("windows-msvc") {
        vcpkg::Config::new()
            .emit_includes(true)
            .find_package("libarchive")
            .expect("libarchive must be installed for the target through vcpkg");
    } else {
        pkg_config::Config::new()
            .atleast_version("3.8.9")
            .probe("libarchive")
            .expect("libarchive >= 3.8.9 and its pkg-config metadata are required");
    }
}
