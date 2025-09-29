fn main() {
    // No build-time copying of http templates; use --generate-http at runtime.
    println!("cargo:rerun-if-changed=http");
}
