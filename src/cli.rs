use std::env;
use std::process;

const VERSION: &str = env!("CARGO_PKG_VERSION");

macro_rules! ensure_option_valid {
    ($config:expr, $opt_name:expr, [$( $pattern:pat => $name:expr ),+ $(,)?]) => {
        if !( $( matches!($config.command, $pattern) )||+ ) {
            eprintln!(
                "sketch: '{}' option is only valid for {}",
                $opt_name,
                vec![$($name),+].join(" and ")
            );
            std::process::exit(1);
        }
    };
}

#[derive(Debug)]
pub enum Command {
    Shell,
    Run(Vec<String>, RunOptions),
    Commit(Vec<String>),
    List,
    Status,
    Clean,
}

#[derive(Debug, Default)]
pub struct RunOptions {
    pub timeout: Option<u64>,
    pub env_vars: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct Config {
    pub command: Command,
    pub verbose: bool,
    pub name: Option<String>,
    pub x11: bool,
    pub as_root: bool,
}

pub fn parse_args() -> Config {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut config = Config {
        command: Command::Shell,
        verbose: false,
        name: None,
        x11: false,
        as_root: false,
    };
    let mut positional = Vec::new();

    if args.is_empty() {
        return config; // No args, default to shell
    }

    let mut i = 0;

    // determine command
    if args[0].starts_with('-') {
        // No command, assume shell
        config.command = Command::Shell;
    } else {
        // First arg is command, parse it and then options
        match args[0].as_str() {
            "shell" => config.command = Command::Shell,
            "run" => config.command = Command::Run(Vec::new(), RunOptions::default()),
            "commit" => config.command = Command::Commit(Vec::new()),
            "list" | "ls" => config.command = Command::List,
            "status" => config.command = Command::Status,
            _ => {
                eprintln!("sketch: unknown command '{}'", args[0]);
                eprintln!("Try 'sketch --help' for more information.");
                process::exit(1);
            }
        }
        i += 1; // Move past command
    }

    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                process::exit(0);
            }
            "--version" | "-v" => {
                println!("sketch {}", VERSION);
                process::exit(0);
            }
            "--verbose" => config.verbose = true,
            "--clean" => {
                config.command = Command::Clean;
                return config;
            }
            "--" => {
                positional.extend_from_slice(&args[i + 1..]);
                break;
            }
            "--name" => {
                ensure_option_valid!(
                    config,
                    "--name",
                    [
                        Command::Run(_, _) => "run",
                        Command::Shell => "shell"
                    ]
                );
                i += 1;
                if i >= args.len() {
                    eprintln!("sketch: '--name' requires a value");
                    process::exit(1);
                }
                config.name = Some(args[i].clone());
            }
            "--x11" => {
                ensure_option_valid!(
                    config,
                    "--x11",
                    [
                        Command::Run(_, _) => "run",
                        Command::Shell => "shell"
                    ]
                );
                config.x11 = true;
            }
            "--as-root" => {
                ensure_option_valid!(
                    config,
                    "--as-root",
                    [
                        Command::Run(_, _) => "run",
                        Command::Shell => "shell"
                    ]
                );
                config.as_root = true;
            }
            arg if arg.starts_with('-') && positional.is_empty() => {
                eprintln!("sketch: unknown option '{}'", arg);
                eprintln!("Try 'sketch --help' for more information.");
                process::exit(1);
            }
            _ => {
                positional.push(args[i].clone());
            }
        }
        i += 1;
    }

    if !positional.is_empty() {
        ensure_option_valid!(
            config,
            "positional arguments",
            [
                Command::Run(_, _) => "run",
                Command::Commit(_) => "commit",
            ]
        );
        config.command = match config.command {
            Command::Run(_, _) => parse_run_command(&positional),
            Command::Commit(_) => parse_commit_command(&positional),
            _ => {
                eprintln!("sketch: unexpected positional arguments");
                eprintln!("Try 'sketch --help' for more information.");
                process::exit(1);
            }
        };
    };

    config
}

fn parse_run_command(args: &[String]) -> Command {
    let mut options = RunOptions::default();
    let mut cmd_args = Vec::new();
    let mut past_separator = false;

    let mut i = 0;
    while i < args.len() {
        if past_separator {
            cmd_args.push(args[i].clone());
            i += 1;
            continue;
        }
        match args[i].as_str() {
            "--" => {
                past_separator = true;
            }
            "--timeout" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("sketch: '--timeout' requires a value in seconds");
                    process::exit(1);
                }
                match args[i].parse::<u64>() {
                    Ok(t) => options.timeout = Some(t),
                    Err(_) => {
                        eprintln!("sketch: invalid timeout value '{}'", args[i]);
                        process::exit(1);
                    }
                }
            }
            "--env" | "-e" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("sketch: '--env' requires a KEY=VALUE argument");
                    process::exit(1);
                }
                match args[i].split_once('=') {
                    Some((key, val)) => {
                        if key.is_empty() {
                            eprintln!("sketch: invalid env var '{}': empty key", args[i]);
                            process::exit(1);
                        }
                        options.env_vars.push((key.to_string(), val.to_string()));
                    }
                    None => {
                        eprintln!(
                            "sketch: invalid env format '{}', expected KEY=VALUE",
                            args[i]
                        );
                        process::exit(1);
                    }
                }
            }
            arg if arg.starts_with('-') => {
                eprintln!("sketch: unknown run option '{}'", arg);
                process::exit(1);
            }
            _ => {
                // Treat remaining args as the command (no -- required)
                cmd_args.extend_from_slice(&args[i..]);
                break;
            }
        }
        i += 1;
    }

    if cmd_args.is_empty() {
        eprintln!("sketch: 'run' requires a command");
        eprintln!("Usage: sketch run [--name NAME] [--timeout SECS] [--env KEY=VALUE] -- COMMAND [ARGS...]");
        process::exit(1);
    }

    Command::Run(cmd_args, options)
}

fn parse_commit_command(args: &[String]) -> Command {
    if args.is_empty() {
        eprintln!("sketch: 'commit' requires at least one file argument");
        eprintln!("Usage: sketch commit [--recursive] FILE [FILE...]");
        process::exit(1);
    }
    Command::Commit(args.to_vec())
}

fn print_help() {
    println!(
        "\
sketch {} - ephemeral disposable machine sessions

USAGE:
    sketch [OPTIONS] [COMMAND]

OPTIONS:
    -h, --help       Show this help message
    -v, --version    Show version
    --verbose        Enable verbose output
    --clean          Clean up orphaned overlay mounts
    --as-root        Run session with root privileges (default is to switch to a non-root user)

COMMANDS:
    shell                  Start interactive shell session (default)
    run [OPTIONS] -- CMD   Run a command non-interactively (for scripting/CI)
    commit [FILE...]       Persist files to base filesystem (inside session only)
    list                   Show active sessions
    status                 Show system information and diagnostics

RUN OPTIONS:
    --name NAME            Label the session for identification
    --timeout SECONDS      Kill session after timeout
    -e, --env KEY=VALUE    Set environment variable (repeatable)

COMMIT:
    The 'commit' command works only inside an active sketch session.
    It marks files to be persisted to the base filesystem when the session ends.
    Example: sketch commit /etc/myconfig /home/user/.bashrc

If no command is given, an interactive shell session is started.

All modifications made during a session exist only in a temporary overlay.
When you exit, everything is discarded and the host system remains unchanged.",
        VERSION
    );
}
