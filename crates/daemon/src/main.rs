mod app;
mod logging;


use app::App;


#[tokio::main]
async fn main() {


    logging::init_logging();


    let app = App::new();


    app.run()
        .await;

}