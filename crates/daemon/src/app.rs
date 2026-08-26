use crate::config::Config;

use tokio::time::{
    sleep,
    Duration,
};


pub struct App {

    config: Config,

}


impl App {


    pub fn new(config: Config) -> Self {

        Self {
            config,
        }

    }


    pub async fn run(&self) {


        tracing::info!(
            "Pookie daemon running"
        );


        tracing::info!(
            "History limit: {}",
            self.config.max_history_items
        );


        loop {

            tracing::info!(
                "Daemon heartbeat"
            );


            sleep(Duration::from_secs(10))
                .await;

        }

    }

}