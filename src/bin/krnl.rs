//! Native service contract validation CLI.
use krnlrs::validation::validate_manifest;
use std::{env, fs, process::ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 || args[1] != "validate" {
        eprintln!("Usage: krnl validate <manifest-file>");
        return ExitCode::from(2);
    }
    let text = match fs::read_to_string(&args[2]) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("Cannot read manifest {}: {error}", args[2]);
            return ExitCode::FAILURE;
        }
    };
    match validate_manifest(&text) {
        Ok(registry) => {
            println!("Validated {} services", registry.list().len());
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in errors {
                eprintln!("{}:{}: {}: {}", args[2], error.line, error.code, error.message);
            }
            ExitCode::FAILURE
        }
    }
}
