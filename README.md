# devin

> **Work in progress.** Expect breaking changes.

```sh
devin

 ▝▜▄     Devin CLI v0.0.1
   ▝▜▄
  ▗▟▀    Signed in as user
 ▝▀      Plan: apple-foundationmodel


Welcome to Devin CLI. Inspired by 'claude code', 'gemini cli'. Powered by build-in MacOS LLM(wrapped by apfel), and other open models
```

An on-device AI coding assistant powered by [`apfel`](https://github.com/Arthur-Ficial/apfel).

## Prerequisites

- **macOS 26+** with Apple Intelligence enabled (for the built-in on-device model)
- **apfel**: `brew tap Arthur-Ficial/tap && brew install Arthur-Ficial/tap/apfel`
- **Rust**: 1.75 or later

## Usage

```bash
# Interactive chat (default — no subcommand needed)
devin

# Attach files as context
devin -f src/main.rs -f src/lib.rs

# Single question, stdout
devin "What does ensure_server do?"

# Single question with file context
devin "Any bugs here?" -f src/apfel.rs

# Index a project for code context (run once before chatting)
devin index <path>
```

## Chat Commands

| Command | Description |
|---|---|
| `/apply <n> [path]` | Write the nth code block to a file. Path is auto-detected if omitted. |
| `/run <cmd>` | Run a shell command and share its output with the assistant. |
| `/exit` | End the session. |

## Backend Configuration

devin uses Apple's built-in on-device LLM via apfel by default. No network, no API key.

### Default (built-in Mac LLM)

```bash
devin
```

Starts apfel on port `11435` and auto-detects the model from `/v1/models`.

### Ollama

```bash
APFEL_BASE=http://localhost:11434 devin
```

Model is auto-detected from Ollama's model list. To pin a specific model:

```bash
APFEL_BASE=http://localhost:11434 APFEL_MODEL=qwen2.5-coder:7b devin
```

### Any OpenAI-compatible server

```bash
APFEL_BASE=http://localhost:8080 APFEL_MODEL=my-model devin
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `APFEL_BASE` | `http://localhost:11435` | Backend URL. Set to `http://localhost:11434` for Ollama. |
| `APFEL_MODEL` | _(auto-detected)_ | Model name. Auto-detected from `/v1/models` when unset. |
