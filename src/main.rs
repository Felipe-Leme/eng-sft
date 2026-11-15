use std::{env, error::Error, io, path::PathBuf};

use eng_sft::{api, domain::factory::ElectionFactory, infrastructure::db::init_db};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => run_server().await,
        [command] if command == "serve" => run_server().await,
        [config_path] => run_config(PathBuf::from(config_path)),
        _ => Err("Usage: eng-sft [serve | path/to/config.json]".into()),
    }
}

async fn run_server() -> Result<(), Box<dyn Error>> {
    let address = "127.0.0.1:3000";
    let pool = init_db("sqlite://eng-sft.db").await?;
    let listener = TcpListener::bind(address).await?;
    println!("HTTP API listening at http://{address}");
    axum::serve(listener, api::app_router(pool)).await?;
    Ok(())
}

fn run_config(config_path: PathBuf) -> Result<(), Box<dyn Error>> {
    let election = ElectionFactory::from_file(config_path)?;
    let outcome = election.close().map_err(io::Error::other)?;
    println!("{outcome}");

    Ok(())
}
