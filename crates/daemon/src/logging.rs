use tracing_subscriber::EnvFilter;

pub fn init_logging() {
    let filter = EnvFilter::from_default_env()
        .add_directive("pookie_paste=info".parse().unwrap())
        .add_directive("daemon=info".parse().unwrap())
        .add_directive("pookie_clipboard=info".parse().unwrap());

    tracing_subscriber::fmt().with_env_filter(filter).init();
}
