fn main() {
    println!("cargo::rerun-if-env-changed=ENABLE_CASPER_LOG");
    println!("cargo::rustc-check-cfg=cfg(enable_casper_log)");

    if let Ok(val) = std::env::var("ENABLE_CASPER_LOG")
        && (val == "1" || val.eq_ignore_ascii_case("true"))
    {
        println!("cargo::rustc-cfg=enable_casper_log");
    }
}
