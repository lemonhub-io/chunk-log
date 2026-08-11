#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    chunklog::cli::run()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
