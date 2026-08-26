use tokio::time::{sleep, Duration};


pub struct App {
}


impl App {

    pub fn new() -> Self {
        Self {}
    }


    pub async fn run(&self) {

        println!("Pookie daemon is running");


        loop {

            println!("Daemon heartbeat");


            sleep(Duration::from_secs(10))
                .await;

        }

    }

}