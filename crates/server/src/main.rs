use std::{env, error::Error, fs, path::PathBuf};

use serde_json::Value;
use server::{
    api,
    application::ElectionService,
    domain::factory::ElectionFactory,
    infrastructure::{MemoryInfrastructure, db::init_db},
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => run_server().await,
        [command] if command == "serve" => run_server().await,
        [config_path] => run_config(PathBuf::from(config_path)).await,
        _ => Err("Usage: eng-sft [serve | path/to/config.json]".into()),
    }
}

async fn run_server() -> Result<(), Box<dyn Error>> {
    let address = "127.0.0.1:3000";
    let infrastructure = init_db("sqlite://eng-sft.db").await?;
    let listener = TcpListener::bind(address).await?;
    println!("HTTP API listening at http://{address}");
    let service = ElectionService::new(infrastructure);
    axum::serve(listener, api::app_router(service)).await?;
    Ok(())
}

async fn run_config(config_path: PathBuf) -> Result<(), Box<dyn Error>> {
    let json = fs::read_to_string(&config_path)?;
    let manifest = ElectionFactory::parse_manifest(&json)?;
    let infrastructure = MemoryInfrastructure::<Value, Value>::new();
    let service = ElectionService::new(infrastructure);
    let name = format!("Configuration: {}", config_path.display());
    let id = service.create_election(name, manifest).await?;
    let outcome = service.close_election(&id).await?;
    println!("{outcome}");

    Ok(())
}
