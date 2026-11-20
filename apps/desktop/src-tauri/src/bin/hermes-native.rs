#[tokio::main]
async fn main() {
    if let Err(error) = hermes_grok_bot_lib::native_ipc::run().await {
        eprintln!("Hermes native service: {error}");
        std::process::exit(1);
    }
}
