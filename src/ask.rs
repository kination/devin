use crate::apfel::{ensure_server, Client, Message, build_file_context};
use crate::error::Result;

pub fn run(query: &str, files: &[String]) -> Result<()> {
    let _server = ensure_server()?;
    let client = Client::new();

    let content = build_prompt(query, files);
    let messages = vec![Message::user(content)];
    let response = client.complete(&messages)?;

    println!("{response}");
    Ok(())
}

fn build_prompt(query: &str, files: &[String]) -> String {
    let ctx = build_file_context(files);
    if ctx.is_empty() {
        query.to_string()
    } else {
        format!("{ctx}\n\n{query}")
    }
}
