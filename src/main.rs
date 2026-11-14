use std::{env, error::Error, fs, path::PathBuf};

use eng_sft::domain::{
    custom::{CustomElection, CustomElectionConfig, CustomOutcome, CustomRuleStrategy},
    election::{Election, Open},
};
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let config_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("src/config.json"));
    if args.next().is_some() {
        return Err("Usage: eng-sft [path/to/config.json]".into());
    }

    let config_json = fs::read_to_string(&config_path)?;
    let config: CustomElectionConfig = serde_json::from_str(&config_json)?;
    let strategy = CustomRuleStrategy::new(config.rules)?;
    let mut election: CustomElection<Open> = Election::new(strategy);

    for vote in config.votes {
        election.cast_vote(vote)?;
    }

    let outcome: CustomOutcome = election.close().compute_result()?;
    println!("{outcome:?}");

    Ok(())
}
