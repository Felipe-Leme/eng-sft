use std::{env, error::Error, path::PathBuf};

use eng_sft::domain::factory::ElectionFactory;
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let config_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("src/config.json"));
    if args.next().is_some() {
        return Err("Usage: eng-sft [path/to/config.json]".into());
    }

    let election = ElectionFactory::from_file(config_path)?;
    let outcome = election.close().map_err(std::io::Error::other)?;
    println!("{outcome}");

    Ok(())
}
