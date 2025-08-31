#[allow(dead_code)]
pub fn fixture() {
    let _ =
        rustls::crypto::CryptoProvider::install_default(rustls::crypto::aws_lc_rs::default_provider());
}
