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
# Start interactive chat
devin

# Start chat with specific files as context
devin -f src/main.rs -f src/lib.rs

# Single question (non-interactive)
devin "How do I use this?"

# Single question with file context and diff review before writing
devin "Add a logout method to Auth" -f src/auth.rs --diff

# Pipe-friendly raw output
devin "List all functions in this file" -f src/cli.rs --print
```

## .devin-context

Create a `.devin-context` file in your project root to automatically attach files using glob patterns:

```text
src/**/*.rs
docs/*.md
# comments are ignored
```

Use `--no-context` to skip this auto-attachment.

## Chat Commands

| Command | Description |
|---|---|
| `/run <cmd>` | Execute a shell command and share the output with devin. |
| `/exit`, `/quit` | End the session. |

In chat, you can also mention files using `@path/to/file` to instantly add them to the conversation context.

## Backend Configuration

devin uses Apple's built-in on-device LLM via apfel by default. No network, no API key.

### Default (built-in Mac LLM)

```bash
devin
```

Starts apfel on port `11435` and auto-detects the model from `/v1/models`.

### Ollama

```bash
APFEL_BASE=http://localhost:11434 devin chat
```

Model is auto-detected from Ollama's model list. To pin a specific model:

```bash
APFEL_BASE=http://localhost:11434 APFEL_MODEL=qwen2.5-coder:7b devin chat
```

### Any OpenAI-compatible server

```bash
APFEL_BASE=http://localhost:8080 APFEL_MODEL=my-model devin chat
```

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `APFEL_BASE` | `http://localhost:11435` | Backend URL. Set to `http://localhost:11434` for Ollama. |
| `APFEL_MODEL` | _(auto-detected)_ | Model name. Auto-detected from `/v1/models` when unset. |
