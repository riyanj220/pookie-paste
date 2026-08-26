use tracing_subscriber::{
    EnvFilter,
};


pub fn init_logging() {

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(
                    "daemon=info"
                        .parse()
                        .unwrap(),
                ),
        )
        .init();

}