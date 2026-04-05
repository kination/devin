# devin

An on-device AI coding assistant powered by `apfel`.

## Getting Started

### Prerequisites

- **apfel**: Ensure the `apfel` server is installed and available in your PATH.
  - Installation: `brew tap Arthur-Ficial/tap && brew install Arthur-Ficial/tap/apfel`
- **Rust**: Version 1.75 or later.

### Running the Assistant

To start an interactive TUI session:

```bash
cargo run -- chat
```

To include specific files as context:

```bash
cargo run -- chat -f src/main.rs -f src/apfel.rs
```

To ask a single question from the CLI:

```bash
cargo run -- ask "How do I use the TUI?"
```

## TUI Commands

Inside the chat interface, you can use the following commands:

- `/apply <n> [filename]`: Apply the $n$-th code block from the last assistant response to a file.
- `/run <command>`: Execute a shell command and optionally share the output with the assistant.
- `/exit` or `/quit`: Close the session.
- `PageUp` / `PageDown`: Scroll through the chat history.
- `Ctrl+C`: Force quit.

## Configuration

- `APFEL_BASE`: Backend URL (default: `http://localhost:11435`)
- `APFEL_MODEL`: Model name (default: `on-device`)
