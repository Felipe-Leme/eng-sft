use std::{env, error::Error, fs, path::PathBuf};

use schemars::schema_for;
use server::domain::manifest::Manifest;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let output_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("src/config.schema.json"));
    if args.next().is_some() {
        return Err("Usage: generate_schema [output-path]".into());
    }

    let schema = schema_for!(Manifest);
    let json = serde_json::to_string_pretty(&schema)?;
    fs::write(&output_path, format!("{json}\n"))?;
    println!("Wrote JSON Schema to {}", output_path.display());

    Ok(())
}
