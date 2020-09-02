use clap::{App, Arg, ArgMatches};
use environment::Environment;
use slashing_protection::{interchange::Interchange, SlashingDatabase};
use std::fs::File;
use std::path::PathBuf;
use types::EthSpec;

pub const CMD: &str = "slashing-protection";
pub const IMPORT_CMD: &str = "import";
pub const EXPORT_CMD: &str = "export";

pub const IMPORT_FILE_ARG: &str = "import-file";
pub const EXPORT_FILE_ARG: &str = "export-file";

pub fn cli_app<'a, 'b>() -> App<'a, 'b> {
    App::new(CMD)
        .about("Import or export slashing protection data another client")
        .subcommand(
            App::new(IMPORT_CMD)
                .about("Import an interchange file")
                .arg(
                    Arg::with_name(IMPORT_FILE_ARG)
                        .takes_value(true)
                        .value_name("FILE")
                        .help("The slashing protection interchange file to import (.json)"),
                ),
        )
        .subcommand(
            App::new(EXPORT_CMD)
                .about("Export an interchange file")
                .arg(
                    Arg::with_name(EXPORT_FILE_ARG)
                        .takes_value(true)
                        .value_name("FILE")
                        .help("The filename to export the interchange file to"),
                ),
        )
}

pub fn cli_run<T: EthSpec>(matches: &ArgMatches<'_>, env: Environment<T>) -> Result<(), String> {
    // FIXME(sproul): reconcile this with datadir changes
    let data_dir = clap_utils::parse_path_with_default_in_home_dir(
        matches,
        "datadir",
        PathBuf::from(".lighthouse"),
    )?;
    let slashing_protection_db_path = data_dir.join("slashing_protection.sqlite");

    let genesis_validators_root = env
        .testnet
        .and_then(|testnet_config| {
            Some(
                testnet_config
                    .genesis_state
                    .as_ref()?
                    .genesis_validators_root,
            )
        })
        .ok_or_else(|| {
            "Unable to get genesis validators root from testnet config, has genesis occurred?"
        })?;

    match matches.subcommand() {
        (IMPORT_CMD, Some(matches)) => {
            let import_filename: PathBuf = clap_utils::parse_required(&matches, "import-file")?;
            let import_file = File::open(&import_filename).map_err(|e| {
                format!(
                    "Unable to open import file at {}: {:?}",
                    import_filename.display(),
                    e
                )
            })?;

            let interchange = Interchange::from_json_reader(&import_file)
                .map_err(|e| format!("Error parsing file for import: {:?}", e))?;

            let slashing_protection_database =
                SlashingDatabase::open_or_create(&slashing_protection_db_path).map_err(|e| {
                    format!(
                        "Unable to open database at {}: {:?}",
                        slashing_protection_db_path.display(),
                        e
                    )
                })?;

            slashing_protection_database
                .import_interchange_info(&interchange, genesis_validators_root)
                .map_err(|e| format!("Error during import: {:?}", e))?;

            Ok(())
        }
        (EXPORT_CMD, Some(matches)) => {
            let export_filename: PathBuf = clap_utils::parse_required(&matches, EXPORT_FILE_ARG)?;

            let slashing_protection_database = SlashingDatabase::open(&slashing_protection_db_path)
                .map_err(|e| {
                    format!(
                        "Unable to open database at {}: {:?}",
                        slashing_protection_db_path.display(),
                        e
                    )
                })?;

            let interchange = slashing_protection_database
                .export_interchange_info(genesis_validators_root)
                .map_err(|e| format!("Error during export: {:?}", e))?;

            let output_file = File::create(export_filename)
                .map_err(|e| format!("Error creating output file: {:?}", e))?;

            interchange
                .write_to(&output_file)
                .map_err(|e| format!("Error writing output file: {:?}", e))?;

            Ok(())
        }
        ("", _) => Err(format!("No subcommand provided, see --help for options")),
        (command, _) => Err(format!("No such subcommand `{}`", command)),
    }
}