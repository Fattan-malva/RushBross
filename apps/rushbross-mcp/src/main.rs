fn main() {
    let (addr, token) = match (
        std::env::var("RUSHBROSS_MCP_ADDR"),
        std::env::var("RUSHBROSS_MCP_TOKEN"),
    ) {
        (Ok(addr), Ok(token)) if !addr.is_empty() && !token.is_empty() => (addr, token),
        _ => {
            let path = std::env::temp_dir().join("rushbross-agent.json");
            let Ok(text) = std::fs::read_to_string(path) else {
                eprintln!("rushbross-mcp: no active RushBross session found");
                std::process::exit(1);
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                eprintln!("rushbross-mcp: RushBross session file is corrupt");
                std::process::exit(1);
            };
            let addr = value
                .get("addr")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let token = value
                .get("token")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            (addr, token)
        }
    };
    if addr.is_empty() || token.is_empty() {
        eprintln!("rushbross-mcp: address or token is empty");
        std::process::exit(1);
    }
    if let Err(error) = rb_agent::serve_stdio(&addr, &token) {
        eprintln!("rushbross-mcp: {error}");
        std::process::exit(1);
    }
}
