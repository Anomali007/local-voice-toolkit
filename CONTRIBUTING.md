# Contributing to Blah³

Thank you for your interest in contributing to Blah³! This document provides guidelines and instructions for contributing.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Commit Messages](#commit-messages)
- [Code Style](#code-style)
- [Pull Request Process](#pull-request-process)
- [License](#license)

## Code of Conduct

Please be respectful and constructive in all interactions. We're building tools to help people communicate better—let's model that in our community.

## Getting Started

### Prerequisites

- macOS 14.0 (Sonoma) or later
- Apple Silicon (M1/M2/M3) recommended
- 16GB+ RAM recommended

### Setup

1. **Install system dependencies:**
   ```bash
   # Xcode CLI tools
   xcode-select --install

   # Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env

   # Node.js and pnpm
   brew install node pnpm

   # espeak-ng (for TTS)
   brew install espeak-ng

   # Tauri CLI
   cargo install tauri-cli --version "^2"
   ```

2. **Clone and setup:**
   ```bash
   git clone https://github.com/your-org/blah3.git
   cd blah3
   pnpm install
   ```

3. **Verify your setup:**
   ```bash
   cargo tauri dev
   ```

## Development Workflow

### Branching

- Create feature branches from `main`
- Use descriptive branch names: `feat/voice-activity-detection`, `fix/audio-crackling`
- Keep branches focused on a single feature or fix

### Running the App

```bash
# Development mode with hot reload
cargo tauri dev

# Run frontend only (for UI work)
pnpm dev

# Run tests
pnpm test        # Frontend (watch mode)
pnpm test:run    # Frontend (CI mode)
cd src-tauri && cargo test  # Backend
```

### Linting

Before committing, ensure your code passes all checks:

```bash
# TypeScript
pnpm lint

# Rust
cd src-tauri
cargo clippy -- -D warnings
cargo fmt --check
```

## Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/). Each commit message should be structured as:

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Code style (formatting, etc.) |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `test` | Adding or updating tests |
| `build` | Build system or dependencies |
| `ci` | CI configuration |
| `chore` | Other changes |

### Scopes

| Scope | Description |
|-------|-------------|
| `stt` | Speech-to-text engine |
| `tts` | Text-to-speech engine |
| `audio` | Audio capture/playback |
| `ui` | React components |
| `hotkeys` | Global shortcuts |
| `models` | Model management |
| `settings` | User preferences |
| `a11y` | Accessibility |

### Examples

```
feat(stt): add support for whisper-large-v3 model

Adds the ability to download and use the whisper-large-v3 model
for improved transcription accuracy.

Closes #42
```

```
fix(audio): resolve crackling on M3 Max chips

The audio buffer size was too small for the M3 Max's sample rate.
Increased buffer to 4096 samples.
```

## Code Style

### TypeScript/React

- Use functional components with hooks
- Prefer `interface` over `type` for object shapes
- Use meaningful variable names
- Keep components focused and small
- Add JSDoc comments for public functions

```typescript
interface DictationPanelProps {
  onTranscript: (text: string) => void;
  isRecording: boolean;
}

/**
 * Panel for voice dictation with real-time transcription display.
 */
export function DictationPanel({ onTranscript, isRecording }: DictationPanelProps) {
  // ...
}
```

### Rust

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `thiserror` for custom errors
- Use `anyhow` for error propagation in application code
- Prefer `.expect("reason")` over `.unwrap()` when panicking is acceptable
- Use `?` for error propagation

```rust
use anyhow::{Context, Result};

/// Loads a whisper model from the specified path.
pub fn load_model(path: &Path) -> Result<WhisperModel> {
    let model = WhisperContext::new(path)
        .context("Failed to load whisper model")?;
    Ok(model)
}
```

### Formatting

- TypeScript: Handled by VS Code/Prettier (follow existing patterns)
- Rust: Run `cargo fmt` before committing

## Pull Request Process

1. **Create your branch:**
   ```bash
   git checkout -b feat/your-feature
   ```

2. **Make your changes** with clear, focused commits

3. **Run all checks:**
   ```bash
   pnpm lint
   pnpm test:run
   cd src-tauri && cargo clippy -- -D warnings && cargo test
   ```

4. **Push and create PR:**
   ```bash
   git push -u origin feat/your-feature
   ```

5. **Fill out the PR template** with:
   - Summary of changes
   - Testing performed
   - Screenshots (for UI changes)
   - Related issues

6. **Address review feedback** with additional commits

7. **Squash merge** will be used when merging to main

### PR Checklist

Before requesting review:

- [ ] Code builds without errors
- [ ] All tests pass
- [ ] No clippy/lint warnings
- [ ] Documentation updated if needed
- [ ] CHANGELOG.md updated for user-facing changes
- [ ] Commits follow conventional commits format

## Reporting Issues

When reporting bugs, please include:

- macOS version and chip (e.g., "macOS 14.2, M2 Pro")
- Steps to reproduce
- Expected vs actual behavior
- Relevant logs from Console.app

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

---

Questions? Open a discussion or reach out to the maintainers. We're happy to help!
