use std::{collections::HashSet, error::Error, fmt};

use election_core::{
    domain::{
        BudgetProject, ElectionSummary, ElectionVote, Manifest, TypedManifest,
        academic::{AcademicOutcome, Slate},
        budget::BudgetOutcome,
        condo::{CondoOutcome, CondoVote},
        custom::{
            CustomOutcome, CustomRuleSet, CustomVote, QuorumRule, ScoringRule, TieBreakerRule,
            WeightingRule,
        },
    },
    types::Percentage,
};
use inquire::{InquireError, Select, Text};
use reqwest::{Client, Response};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

const SERVER_URL: &str = "http://localhost:3000/api/elections";

struct ElectionChoice(ElectionSummary);

impl fmt::Display for ElectionChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let election = &self.0;
        let (election_type, icon) = match &election.manifest {
            Manifest::Typed(TypedManifest::Academic { .. }) => ("Academic", "🎓"),
            Manifest::Typed(TypedManifest::Budget { .. }) => ("Budget", "💰"),
            Manifest::Typed(TypedManifest::Condo { .. }) => ("Condo", "🏢"),
            Manifest::Typed(TypedManifest::Custom { .. }) | Manifest::Custom(_) => {
                ("Custom", "⚙️ ")
            }
        };
        let status = if election.is_closed {
            "🔒 Closed"
        } else {
            "🟢 Open"
        };
        write!(
            formatter,
            "{icon} {} | {} | Type: {} | ID: {}",
            election.name, status, election_type, election.id
        )
    }
}

#[derive(Clone)]
enum AccessMode {
    User,
    Admin,
    Reroll,
    Exit,
}

impl fmt::Display for AccessMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::User => formatter.write_str("🙋 User Access"),
            Self::Admin => formatter.write_str("🔐 Admin Access"),
            Self::Reroll => formatter.write_str("🎲 Reroll voter UUID"),
            Self::Exit => formatter.write_str("🚪 Exit"),
        }
    }
}

#[derive(Clone)]
enum UserAction {
    CastVote,
    ViewResults,
    Back,
}

impl fmt::Display for UserAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CastVote => formatter.write_str("🗳️ Cast vote"),
            Self::ViewResults => formatter.write_str("📊 View results"),
            Self::Back => formatter.write_str("🔙 Back"),
        }
    }
}

#[derive(Clone)]
enum AdminAction {
    Create,
    TallyClose,
    Back,
}

impl fmt::Display for AdminAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create => formatter.write_str("✨ Create election"),
            Self::TallyClose => formatter.write_str("🛑 Tally & Close elections"),
            Self::Back => formatter.write_str("🔙 Back"),
        }
    }
}

#[derive(Clone)]
enum ElectionSelection {
    Election(ElectionSummary),
    Back,
}

impl fmt::Display for ElectionSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Election(election) => ElectionChoice(election.clone()).fmt(formatter),
            Self::Back => formatter.write_str("🔙 Back"),
        }
    }
}

#[derive(Clone)]
enum ElectionKind {
    Academic,
    Budget,
    Condo,
    Custom,
}

impl fmt::Display for ElectionKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Academic => formatter.write_str("🎓 Academic Center (Simple Plurality)"),
            Self::Budget => formatter.write_str("💰 Participatory Budgeting (Ranked Borda)"),
            Self::Condo => formatter.write_str("🏢 Condo Assembly (Weighted Quorum)"),
            Self::Custom => formatter.write_str("⚙️  Custom Rule Set (Build your own)"),
        }
    }
}

#[derive(Clone)]
enum ScoringChoice {
    Plurality,
    Approval,
    RankedBorda,
}

impl fmt::Display for ScoringChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plurality => formatter.write_str("🎯 Plurality (Pick exactly one)"),
            Self::Approval => formatter.write_str("✅ Approval (Pick multiple, equal weight)"),
            Self::RankedBorda => {
                formatter.write_str("🔢 Ranked Borda (Rank in order of preference)")
            }
        }
    }
}

#[derive(Clone)]
enum WeightingChoice {
    Uniform,
    Weighted,
}

impl fmt::Display for WeightingChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uniform => formatter.write_str("⚖️ Uniform (1 Person = 1 Vote)"),
            Self::Weighted => {
                formatter.write_str("📈 Weighted (Multiplier based on voter fraction/shares)")
            }
        }
    }
}

#[derive(Clone)]
enum QuorumChoice {
    None,
    MinBallots,
    MinTotalWeight,
}

impl fmt::Display for QuorumChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("🕊️ None (No minimum participation required)"),
            Self::MinBallots => formatter.write_str("👥 Minimum Total Ballots Cast"),
            Self::MinTotalWeight => formatter.write_str("⚖️ Minimum Total Weight Represented"),
        }
    }
}

#[derive(Clone)]
enum TieBreakerChoice {
    AllowTie,
    Alphabetical,
    RegistrationOrder,
}

impl fmt::Display for TieBreakerChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllowTie => formatter.write_str("🤝 Allow Ties (Multiple winners possible)"),
            Self::Alphabetical => formatter.write_str("🔤 Alphabetical (A -> Z wins)"),
            Self::RegistrationOrder => {
                formatter.write_str("⏱️ Registration Order (First registered wins)")
            }
        }
    }
}

#[derive(Deserialize)]
struct CreateElectionResponse {
    id: String,
}

#[derive(Serialize)]
struct CreateElectionRequest<'a> {
    name: &'a str,
    manifest: &'a Manifest,
}

#[derive(Clone)]
enum RankingChoice {
    Option(String),
    Finish,
}

impl fmt::Display for RankingChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Option(option) => formatter.write_str(option),
            Self::Finish => formatter.write_str("✅ Finish selection"),
        }
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    println!("🏛️  Verdigris Election Engine - CLI");
    let http = Client::builder().build()?;
    let mut voter_id = Uuid::new_v4();
    let mut voted_elections = HashSet::new();
    println!("Voter UUID: {voter_id}\n");

    loop {
        let access_mode = match select_prompt(
            "Choose access",
            vec![
                AccessMode::User,
                AccessMode::Admin,
                AccessMode::Reroll,
                AccessMode::Exit,
            ],
            "Choose user or admin access, reroll the voter UUID for testing, or exit. Use the arrow keys and Enter; Esc exits.",
        ) {
            Ok(access_mode) => access_mode,
            Err(error) if is_prompt_cancellation(error.as_ref()) => return Ok(()),
            Err(error) => return Err(error),
        };
        match access_mode {
            AccessMode::User => {
                if let Err(error) = user_access(&http, voter_id, &mut voted_elections).await {
                    handle_access_error(error)?;
                }
            }
            AccessMode::Admin => {
                if let Err(error) = admin_access(&http).await {
                    handle_access_error(error)?;
                }
            }
            AccessMode::Reroll => {
                voter_id = Uuid::new_v4();
                voted_elections.clear();
                println!("New voter UUID: {voter_id}");
            }
            AccessMode::Exit => return Ok(()),
        }
    }
}

async fn user_access(
    http: &Client,
    voter_id: Uuid,
    voted_elections: &mut HashSet<String>,
) -> Result<(), Box<dyn Error>> {
    let elections = fetch_elections(http).await?;
    if elections.is_empty() {
        println!("⚠️ No elections are available.");
    }

    let mut choices: Vec<_> = elections
        .into_iter()
        .map(ElectionSelection::Election)
        .collect();
    choices.push(ElectionSelection::Back);
    let ElectionSelection::Election(mut election) = select_prompt(
        "Select an election",
        choices,
        "Choose an election to vote or view its results. Closed elections are marked; Back returns to the access menu. Esc cancels to the access menu.",
    )?
    else {
        return Ok(());
    };

    loop {
        election = fetch_elections(http)
            .await?
            .into_iter()
            .find(|candidate| candidate.id == election.id)
            .ok_or("Selected election is no longer available")?;

        let mut actions = Vec::new();
        if !election.is_closed && !voted_elections.contains(&election.id) {
            actions.push(UserAction::CastVote);
        }
        if election.outcome.is_some() {
            actions.push(UserAction::ViewResults);
        }
        actions.push(UserAction::Back);

        match select_prompt(
            "Choose an action",
            actions,
            "Choose an available action for this election. Back returns to the access menu; Esc cancels to the access menu.",
        )? {
            UserAction::CastVote => {
                cast_vote(http, &election, voter_id).await?;
                voted_elections.insert(election.id.clone());
            }
            UserAction::ViewResults => {
                if let Some(outcome) = &election.outcome {
                    println!(
                        "🏆 {}",
                        format_outcome(&election.manifest, outcome.clone())?
                    );
                }
            }
            UserAction::Back => return Ok(()),
        }
    }
}

async fn admin_access(http: &Client) -> Result<(), Box<dyn Error>> {
    fetch_elections(http).await?;
    loop {
        match select_prompt(
            "Choose an admin action",
            vec![
                AdminAction::Create,
                AdminAction::TallyClose,
                AdminAction::Back,
            ],
            "Create an election, tally and close an open election, or go Back to the access menu. Esc also returns to the access menu.",
        )? {
            AdminAction::Create => create_election(http).await?,
            AdminAction::TallyClose => tally_close_election(http).await?,
            AdminAction::Back => return Ok(()),
        }
    }
}

async fn fetch_elections(http: &Client) -> Result<Vec<ElectionSummary>, Box<dyn Error>> {
    let response = http.get(SERVER_URL).send().await?;
    ensure_success(response)
        .await?
        .json::<Vec<ElectionSummary>>()
        .await
        .map_err(Into::into)
}

fn format_outcome(manifest: &Manifest, outcome: Value) -> Result<String, Box<dyn Error>> {
    let display = match manifest {
        Manifest::Typed(TypedManifest::Academic { .. }) => {
            serde_json::from_value::<AcademicOutcome>(outcome)?.to_string()
        }
        Manifest::Typed(TypedManifest::Budget { .. }) => {
            serde_json::from_value::<BudgetOutcome>(outcome)?.to_string()
        }
        Manifest::Typed(TypedManifest::Condo { .. }) => {
            serde_json::from_value::<CondoOutcome>(outcome)?.to_string()
        }
        Manifest::Typed(TypedManifest::Custom { .. }) | Manifest::Custom(_) => {
            serde_json::from_value::<CustomOutcome>(outcome)?.to_string()
        }
    };
    Ok(display)
}

async fn create_election(http: &Client) -> Result<(), Box<dyn Error>> {
    let existing_names: Vec<_> = fetch_elections(http)
        .await?
        .into_iter()
        .map(|election| election.name)
        .collect();
    let name = loop {
        let name = prompt_nonempty("Election name")?;
        if is_duplicate_election_name(&name, &existing_names) {
            println!(
                "An election named \"{name}\" already exists. Names must be unique regardless of capitalization; enter a different name."
            );
        } else {
            break name;
        }
    };
    let manifest = prompt_manifest()?;
    let request = CreateElectionRequest {
        name: &name,
        manifest: &manifest,
    };
    let response = http.post(SERVER_URL).json(&request).send().await?;
    let response = ensure_success(response).await?;
    let created: CreateElectionResponse = response.json().await?;
    println!("Election \"{name}\" created with id {}.", created.id);
    Ok(())
}

fn is_duplicate_election_name(name: &str, existing_names: &[String]) -> bool {
    existing_names
        .iter()
        .any(|existing| existing.trim().eq_ignore_ascii_case(name.trim()))
}

async fn tally_close_election(http: &Client) -> Result<(), Box<dyn Error>> {
    let elections: Vec<_> = fetch_elections(http)
        .await?
        .into_iter()
        .filter(|election| !election.is_closed)
        .collect();
    if elections.is_empty() {
        println!("⚠️ There are no open elections to tally or close.");
        return Ok(());
    }

    let mut choices: Vec<_> = elections
        .into_iter()
        .map(ElectionSelection::Election)
        .collect();
    choices.push(ElectionSelection::Back);
    let ElectionSelection::Election(election) = select_prompt(
        "Select an election to tally and close",
        choices,
        "Choose an open election to tally and close. Back returns to the access menu; Esc cancels to the access menu.",
    )?
    else {
        return Ok(());
    };

    let response = http
        .post(format!("{SERVER_URL}/{}/close", election.id))
        .send()
        .await?;
    let outcome = ensure_success(response).await?.json::<Value>().await?;
    println!(
        "Election \"{}\" tallied and closed:\n{}",
        election.name,
        format_outcome(&election.manifest, outcome)?
    );
    Ok(())
}

fn prompt_manifest() -> Result<Manifest, Box<dyn Error>> {
    let kind = select_prompt(
        "Select election type",
        vec![
            ElectionKind::Academic,
            ElectionKind::Budget,
            ElectionKind::Condo,
            ElectionKind::Custom,
        ],
        "Choose the election rules to configure. The next prompts depend on this type; Esc cancels election creation.",
    )?;

    match kind {
        ElectionKind::Academic => {
            let slates = prompt_unique_names("Number of slates", "Slate name")?;
            let minimum_quorum = prompt_usize("Minimum quorum", 0, usize::MAX, 1)?;
            Ok(Manifest::Typed(TypedManifest::Academic {
                slates,
                minimum_quorum,
                votes: Vec::new(),
            }))
        }
        ElectionKind::Budget => {
            let count = prompt_usize("Number of projects", 1, usize::MAX, 1)?;
            let mut projects = Vec::with_capacity(count);
            for index in 1..=count {
                let name = loop {
                    let name = prompt_nonempty(&format!("Project {index} name"))?;
                    if projects
                        .iter()
                        .any(|project: &BudgetProject| project.name == name)
                    {
                        println!("Project names must be unique.");
                    } else {
                        break name;
                    }
                };
                let cost = prompt_decimal("Project cost", Decimal::ZERO, None)?;
                projects.push(BudgetProject { name, cost });
            }
            let budget_ceiling = prompt_decimal("Budget ceiling", Decimal::ZERO, None)?;
            Ok(Manifest::Typed(TypedManifest::Budget {
                projects,
                budget_ceiling,
                votes: Vec::new(),
            }))
        }
        ElectionKind::Condo => {
            let options = prompt_unique_names("Number of options", "Option name")?;
            let minimum_fraction_quorum = prompt_decimal(
                "Minimum fraction quorum (from 0 to 1)",
                Decimal::ZERO,
                Some(Decimal::ONE),
            )?;
            Ok(Manifest::Typed(TypedManifest::Condo {
                options,
                minimum_fraction_quorum,
                votes: Vec::new(),
            }))
        }
        ElectionKind::Custom => {
            let options = prompt_unique_names("Number of options", "Option name")?;
            let scoring = match select_prompt(
                "Scoring rule",
                vec![
                    ScoringChoice::Plurality,
                    ScoringChoice::Approval,
                    ScoringChoice::RankedBorda,
                ],
                "Choose how ballots are scored: one choice, multiple approvals, or ranked preferences. Esc cancels election creation.",
            )? {
                ScoringChoice::Plurality => ScoringRule::Plurality,
                ScoringChoice::Approval => ScoringRule::Approval {
                    max_choices: prompt_usize("Maximum approved choices", 1, options.len(), 1)?,
                },
                ScoringChoice::RankedBorda => ScoringRule::RankedBorda,
            };
            let weighting = match select_prompt(
                "Ballot weighting",
                vec![WeightingChoice::Uniform, WeightingChoice::Weighted],
                "Choose whether every ballot has equal weight or includes a voter-supplied weight. Esc cancels election creation.",
            )? {
                WeightingChoice::Uniform => WeightingRule::Uniform,
                WeightingChoice::Weighted => WeightingRule::Weighted,
            };
            let quorum = match select_prompt(
                "Quorum rule",
                vec![
                    QuorumChoice::None,
                    QuorumChoice::MinBallots,
                    QuorumChoice::MinTotalWeight,
                ],
                "Choose whether participation must meet a minimum ballot count or total weight. Esc cancels election creation.",
            )? {
                QuorumChoice::None => QuorumRule::None,
                QuorumChoice::MinBallots => QuorumRule::MinBallots {
                    min: prompt_usize("Minimum number of ballots", 0, usize::MAX, 1)?,
                },
                QuorumChoice::MinTotalWeight => QuorumRule::MinTotalWeight {
                    min: prompt_decimal("Minimum total weight", Decimal::ZERO, None)?,
                },
            };
            let tie_breaker = match select_prompt(
                "Tie breaker",
                vec![
                    TieBreakerChoice::AllowTie,
                    TieBreakerChoice::Alphabetical,
                    TieBreakerChoice::RegistrationOrder,
                ],
                "Choose how to handle a tie at the winner cutoff. Esc cancels election creation.",
            )? {
                TieBreakerChoice::AllowTie => TieBreakerRule::AllowTie,
                TieBreakerChoice::Alphabetical => TieBreakerRule::Alphabetical,
                TieBreakerChoice::RegistrationOrder => TieBreakerRule::RegistrationOrder,
            };
            let winner_count = prompt_usize("Number of winners", 1, options.len(), 1)?;

            Ok(Manifest::Typed(TypedManifest::Custom {
                rules: CustomRuleSet {
                    options,
                    scoring,
                    weighting,
                    quorum,
                    tie_breaker,
                    winner_count,
                },
                votes: Vec::new(),
            }))
        }
    }
}

fn prompt_unique_names(
    count_prompt: &str,
    item_prompt: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    let count = prompt_usize(count_prompt, 1, usize::MAX, 1)?;
    let mut names = Vec::with_capacity(count);
    for index in 1..=count {
        loop {
            let name = prompt_nonempty(&format!("{item_prompt} {index}"))?;
            if names.contains(&name) {
                println!("Names must be unique.");
            } else {
                names.push(name);
                break;
            }
        }
    }
    Ok(names)
}

fn select_prompt<T: fmt::Display + Clone>(
    prompt: &str,
    choices: Vec<T>,
    help: &str,
) -> Result<T, Box<dyn Error>> {
    Ok(Select::new(prompt, choices)
        .with_help_message(help)
        .prompt()?)
}

fn is_prompt_cancellation(error: &(dyn Error + 'static)) -> bool {
    matches!(
        error.downcast_ref::<InquireError>(),
        Some(InquireError::OperationCanceled | InquireError::OperationInterrupted)
    )
}

fn is_server_connection_error(error: &(dyn Error + 'static)) -> bool {
    error
        .downcast_ref::<reqwest::Error>()
        .is_some_and(|error| error.is_connect() || error.is_timeout())
}

fn handle_access_error(error: Box<dyn Error>) -> Result<(), Box<dyn Error>> {
    if is_prompt_cancellation(error.as_ref()) {
        println!("Operation cancelled; returning to the access menu.");
    } else if is_server_connection_error(error.as_ref()) {
        eprintln!(
            "Cannot reach the election server at {SERVER_URL}. Start it with `cargo run -p server`, then choose an access mode to retry."
        );
    } else {
        return Err(error);
    }
    Ok(())
}

fn prompt_nonempty(prompt: &str) -> Result<String, Box<dyn Error>> {
    loop {
        let value = Text::new(prompt)
            .with_help_message(
                "Enter a non-empty value; duplicate names are rejected where applicable. Esc cancels the current operation.",
            )
            .prompt()?
            .trim()
            .to_owned();
        if !value.is_empty() {
            return Ok(value);
        }
        println!("This value cannot be empty.");
    }
}

fn prompt_usize(
    prompt: &str,
    minimum: usize,
    maximum: usize,
    default: usize,
) -> Result<usize, Box<dyn Error>> {
    loop {
        let help = if maximum == usize::MAX {
            format!(
                "Enter a whole number of at least {minimum}; default is {default}. Esc cancels the current operation."
            )
        } else {
            format!(
                "Enter a whole number from {minimum} to {maximum}; default is {default}. Esc cancels the current operation."
            )
        };
        let value = Text::new(prompt)
            .with_default(&default.to_string())
            .with_help_message(&help)
            .prompt()?;
        match value.parse::<usize>() {
            Ok(value) if (minimum..=maximum).contains(&value) => return Ok(value),
            Ok(_) => println!("Enter a whole number from {minimum} to {maximum}."),
            Err(error) => println!("Invalid whole number: {error}"),
        }
    }
}

fn prompt_decimal(
    prompt: &str,
    minimum: Decimal,
    maximum: Option<Decimal>,
) -> Result<Decimal, Box<dyn Error>> {
    loop {
        let help = match maximum {
            Some(maximum) => format!(
                "Enter a decimal value from {minimum} to {maximum}. Esc cancels the current operation."
            ),
            None => format!(
                "Enter a decimal value greater than or equal to {minimum}. Esc cancels the current operation."
            ),
        };
        let value = Text::new(prompt).with_help_message(&help).prompt()?;
        match value.parse::<Decimal>() {
            Ok(value) if value >= minimum && maximum.is_none_or(|maximum| value <= maximum) => {
                return Ok(value);
            }
            Ok(_) => match maximum {
                Some(maximum) => println!("Enter a decimal value from {minimum} to {maximum}."),
                None => println!("Enter a decimal value greater than or equal to {minimum}."),
            },
            Err(error) => println!("Invalid decimal value: {error}"),
        }
    }
}

async fn cast_vote(
    http: &Client,
    election: &ElectionSummary,
    voter_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let vote = prompt_ballot(&election.manifest)?;
    let response = http
        .post(format!("{SERVER_URL}/{}/votes", election.id))
        .header("x-voter-id", voter_id.to_string())
        .json(&vote)
        .send()
        .await?;
    ensure_success(response).await?;
    println!("Vote submitted.");
    Ok(())
}

fn prompt_ballot(manifest: &Manifest) -> Result<ElectionVote, Box<dyn Error>> {
    match manifest {
        Manifest::Typed(TypedManifest::Academic { slates, .. }) => {
            let slates = slates.iter().map(|slate| slate.trim().to_owned()).collect();
            let slate = select_prompt(
                "Select a slate",
                slates,
                "Choose one slate for your ballot. Esc cancels the vote without submitting it.",
            )?;
            Ok(ElectionVote::Academic(Slate::new(&slate)?))
        }
        Manifest::Typed(TypedManifest::Budget { projects, .. }) => {
            let ranking = prompt_ordered(
                "Select projects in preference order",
                projects
                    .iter()
                    .map(|project| project.name.trim().to_owned())
                    .collect(),
                true,
                None,
            )?;
            Ok(ElectionVote::Budget(ranking))
        }
        Manifest::Typed(TypedManifest::Condo { options, .. }) => {
            let options = options
                .iter()
                .map(|option| option.trim().to_owned())
                .collect();
            let choice = select_prompt(
                "Select an option",
                options,
                "Choose one option, then enter your voting fraction. Esc cancels the vote without submitting it.",
            )?;
            let fraction = prompt_fraction()?;
            Ok(ElectionVote::Condo(CondoVote { choice, fraction }))
        }
        Manifest::Typed(TypedManifest::Custom { rules, .. }) => prompt_custom_ballot(rules),
        Manifest::Custom(config) => prompt_custom_ballot(&config.rules),
    }
}

fn prompt_custom_ballot(rules: &CustomRuleSet) -> Result<ElectionVote, Box<dyn Error>> {
    let choices = match &rules.scoring {
        ScoringRule::Plurality => {
            vec![select_prompt(
                "Select an option",
                rules.options.clone(),
                "Choose one option for your ballot. Esc cancels the vote without submitting it.",
            )?]
        }
        ScoringRule::Approval { max_choices } => prompt_ordered(
            "Select options (order does not affect approval voting)",
            rules.options.clone(),
            false,
            Some(*max_choices),
        )?,
        ScoringRule::RankedBorda => prompt_ordered(
            "Select options in preference order",
            rules.options.clone(),
            false,
            None,
        )?,
    };
    let weight = match rules.weighting {
        WeightingRule::Uniform => Decimal::ONE,
        WeightingRule::Weighted => prompt_positive_decimal("Ballot weight")?,
    };
    Ok(ElectionVote::Custom(CustomVote { choices, weight }))
}

fn prompt_ordered(
    prompt: &str,
    mut remaining: Vec<String>,
    allow_empty: bool,
    maximum: Option<usize>,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut selected = Vec::new();
    loop {
        if maximum.is_some_and(|maximum| selected.len() >= maximum) || remaining.is_empty() {
            return Ok(selected);
        }

        let mut choices: Vec<_> = remaining
            .iter()
            .cloned()
            .map(RankingChoice::Option)
            .collect();
        if allow_empty || !selected.is_empty() {
            choices.push(RankingChoice::Finish);
        }

        match select_prompt(
            prompt,
            choices,
            "Choose options one at a time. Finish selection completes this ballot section; Esc cancels the vote without submitting it.",
        )? {
            RankingChoice::Finish => return Ok(selected),
            RankingChoice::Option(option) => {
                remaining.retain(|candidate| candidate != &option);
                selected.push(option);
            }
        }
    }
}

fn prompt_fraction() -> Result<Percentage, Box<dyn Error>> {
    loop {
        let input = Text::new("Voting fraction (greater than 0 and at most 1)")
            .with_help_message(
                "Enter a decimal fraction greater than 0 and at most 1, for example 0.25. Esc cancels the vote without submitting it.",
            )
            .prompt()?;
        match input.parse::<Decimal>() {
            Ok(fraction) if fraction > Decimal::ZERO && fraction <= Decimal::ONE => {
                let percentage = fraction
                    .checked_mul(Decimal::ONE_HUNDRED)
                    .ok_or("Voting fraction is out of range")?;
                return Ok(Percentage::try_from(percentage)?);
            }
            Ok(_) => println!("The fraction must be greater than 0 and at most 1."),
            Err(error) => println!("Invalid decimal fraction: {error}"),
        }
    }
}

fn prompt_positive_decimal(prompt: &str) -> Result<Decimal, Box<dyn Error>> {
    loop {
        let input = Text::new(prompt)
            .with_help_message(
                "Enter a positive decimal value for this ballot. Esc cancels the vote without submitting it.",
            )
            .prompt()?;
        match input.parse::<Decimal>() {
            Ok(value) if value > Decimal::ZERO => return Ok(value),
            Ok(_) => println!("The value must be greater than 0."),
            Err(error) => println!("Invalid decimal value: {error}"),
        }
    }
}

async fn ensure_success(response: Response) -> Result<Response, Box<dyn Error>> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let body = response.text().await?;
    let message = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|body| body["error"].as_str().map(str::to_owned))
        .unwrap_or(body);
    Err(format!("Server returned {status}: {message}").into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_cancellation_is_distinguished_from_other_errors() {
        assert!(is_prompt_cancellation(&InquireError::OperationCanceled));
        assert!(is_prompt_cancellation(&InquireError::OperationInterrupted));
        assert!(!is_prompt_cancellation(&std::io::Error::other(
            "unrelated input error"
        )));
    }

    #[test]
    fn duplicate_election_names_are_checked_case_insensitively() {
        let existing_names = vec!["Student Council".to_owned()];
        assert!(is_duplicate_election_name(
            "  student council ",
            &existing_names
        ));
        assert!(!is_duplicate_election_name(
            "Student Council 2026",
            &existing_names
        ));
    }
}
