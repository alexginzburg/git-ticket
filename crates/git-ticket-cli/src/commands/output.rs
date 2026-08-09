use crate::cli::OutputFormat;
use serde::Serialize;

pub fn print_json<T: Serialize>(value: &T) {
    println!("{}", serde_json::to_string(value).expect("value serializes"));
}

pub fn error_exit(format: OutputFormat, message: &str) -> ! {
    match format {
        OutputFormat::Json => eprintln!("{}", serde_json::json!({ "error": message })),
        OutputFormat::Text => eprintln!("error: {message}"),
    }
    std::process::exit(1);
}
