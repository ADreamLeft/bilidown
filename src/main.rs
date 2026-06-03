#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    if let Err(err) = bilidown::cli::run().await {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
}
