fn main() {
    println!("cargo:rerun-if-env-changed=R2MODMAC_SPONSOR_PROXY_URL");
    tauri_build::build()
}
