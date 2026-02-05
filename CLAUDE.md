# Blah³ - Claude Code Commands

This file defines Claude Code slash commands for contributing to Blah³.

## Project Overview

Blah³ is a local voice toolkit for macOS providing Speech-to-Text and Text-to-Speech with AI models running 100% offline on Apple Silicon.

**Tech Stack:**
- Frontend: React + TypeScript + Tailwind CSS
- Backend: Rust + Tauri v2
- STT: whisper-rs (whisper.cpp with CoreML/Metal)
- TTS: kokoroxide (Kokoro-82M via ONNX)

---

## /commit

Create git commits following conventional commits format.

### Format
```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation only
- `style`: Code style (formatting, semicolons, etc.)
- `refactor`: Code change that neither fixes a bug nor adds a feature
- `perf`: Performance improvement
- `test`: Adding or updating tests
- `build`: Build system or external dependencies
- `ci`: CI configuration
- `chore`: Other changes that don't modify src or test files

### Blah³ Scopes
- `stt`: Speech-to-text engine and transcription
- `tts`: Text-to-speech engine and synthesis
- `audio`: Audio capture, playback, and processing
- `ui`: React components and user interface
- `hotkeys`: Global shortcuts and keyboard handling
- `models`: Model management, download, and switching
- `settings`: User preferences and configuration
- `a11y`: Accessibility features

### Examples
```bash
feat(stt): add support for whisper-large-v3 model
fix(audio): resolve crackling on M3 Max chips
docs(readme): update installation instructions
refactor(ui): extract WaveformViz into separate component
perf(tts): cache phoneme lookups for faster synthesis
```

### Workflow
1. Stage your changes: `git add <files>`
2. Run `/commit` to generate a commit message
3. Review and adjust the message if needed
4. Execute the commit

---

## /pr

Create well-documented pull requests.

### PR Template
```markdown
## Summary
<!-- Brief description of what this PR does -->

## Changes
<!-- List of specific changes -->
-

## Testing
<!-- How was this tested? -->
- [ ] Manual testing on macOS
- [ ] Unit tests pass (`pnpm test`)
- [ ] Rust tests pass (`cargo test`)

## Screenshots
<!-- If UI changes, add before/after screenshots -->

## Checklist
- [ ] Code follows project style guidelines
- [ ] Self-reviewed the code
- [ ] Added tests for new functionality
- [ ] Updated documentation if needed
- [ ] No new warnings from `cargo clippy`
- [ ] No TypeScript errors from `pnpm lint`
```

### Workflow
1. Ensure all changes are committed
2. Push your branch: `git push -u origin <branch-name>`
3. Run `/pr` to create the pull request
4. Fill in the template sections
5. Request reviewers

---

## /docs

Update project documentation consistently.

### Documentation Files
- `README.md`: Project overview, setup, and usage
- `CONTRIBUTING.md`: How to contribute
- `CHANGELOG.md`: Release notes (Keep a Changelog format)
- `ARCHITECTURE.md`: Technical design decisions (if exists)
- `src-tauri/README.md`: Rust backend documentation (if exists)

### Guidelines
- Use clear, concise language
- Include code examples where helpful
- Keep README focused on getting started
- Update CHANGELOG for user-facing changes
- Add JSDoc/rustdoc for public APIs

### Workflow
1. Identify which documentation needs updating
2. Run `/docs` with the target file
3. Review generated changes
4. Ensure accuracy and completeness

---

## /test

Run tests and create new tests.

### Frontend Tests (Vitest)
```bash
# Run all tests
pnpm test

# Run tests once (CI mode)
pnpm test:run

# Run with coverage
pnpm test:coverage

# Run specific test file
pnpm test src/components/DictationPanel.test.tsx
```

### Backend Tests (Cargo)
```bash
# Run all Rust tests
cd src-tauri && cargo test

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture
```

### Test File Locations
- Frontend: `src/**/*.test.tsx` or `src/**/*.test.ts`
- Backend: `src-tauri/src/**/*.rs` (inline `#[cfg(test)]` modules)

### Writing Tests
**React Components:**
```typescript
import { render, screen } from '@testing-library/react';
import { DictationPanel } from './DictationPanel';

describe('DictationPanel', () => {
  it('renders recording button', () => {
    render(<DictationPanel />);
    expect(screen.getByRole('button')).toBeInTheDocument();
  });
});
```

**Rust:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_capture() {
        // Test implementation
    }
}
```

---

## /lint

Run code quality checks.

### Commands
```bash
# TypeScript type checking
pnpm lint

# Rust linting
cd src-tauri && cargo clippy -- -D warnings

# Rust formatting check
cd src-tauri && cargo fmt --check

# Rust formatting apply
cd src-tauri && cargo fmt

# All checks (recommended before commit)
pnpm lint && cd src-tauri && cargo clippy -- -D warnings && cargo fmt --check
```

### Common Clippy Lints
- `clippy::unwrap_used`: Prefer `?` or `.expect()` with message
- `clippy::todo`: Remove or resolve TODO macros
- `clippy::dbg_macro`: Remove debug macros before commit

### TypeScript Guidelines
- No `any` types without justification
- Prefer `interface` over `type` for object shapes
- Use strict null checks

---

## /build

Build and verify the application.

### Development Build
```bash
# Start dev server with hot reload
cargo tauri dev

# Or using pnpm script
pnpm tauri:dev
```

### Production Build
```bash
# Full production build
cargo tauri build

# Or using pnpm script
pnpm tauri:build
```

### Build Output
- macOS app bundle: `src-tauri/target/release/bundle/macos/Blah³.app`
- DMG installer: `src-tauri/target/release/bundle/dmg/Blah³_<version>_<arch>.dmg`

### Verification Steps
1. Build completes without errors
2. App launches successfully
3. STT recording works (requires microphone permission)
4. TTS playback works
5. Global hotkeys register correctly
6. Model download/management works

### Troubleshooting
```bash
# Clean build artifacts
pnpm clean

# Check Rust dependencies
cd src-tauri && cargo tree

# Verify Tauri CLI version
cargo tauri --version
```

---

## /changelog

Update CHANGELOG.md following Keep a Changelog format.

### Format
```markdown
## [Unreleased]

### Added
- New features

### Changed
- Changes to existing functionality

### Deprecated
- Features to be removed in future

### Removed
- Removed features

### Fixed
- Bug fixes

### Security
- Security fixes
```

### Guidelines
- Write from user's perspective
- Use present tense ("Add" not "Added")
- Group related changes together
- Link to issues/PRs where relevant
- Most recent changes at the top

### Workflow
1. Run `/changelog` after completing a feature or fix
2. Add entry under appropriate section in [Unreleased]
3. Include brief, user-friendly description
4. Reference issue numbers if applicable

---

## /release

Prepare and tag releases with version bumping.

### Version Locations
Update version in these files:
- `package.json` → `"version": "X.Y.Z"`
- `src-tauri/Cargo.toml` → `version = "X.Y.Z"`
- `src-tauri/tauri.conf.json` → `"version": "X.Y.Z"`

### Semantic Versioning
- **MAJOR** (X.0.0): Breaking changes
- **MINOR** (0.X.0): New features, backward compatible
- **PATCH** (0.0.X): Bug fixes, backward compatible

### Release Workflow
1. Update version in all three files
2. Move [Unreleased] changelog entries to new version section
3. Commit: `chore(release): bump version to X.Y.Z`
4. Create git tag: `git tag -a vX.Y.Z -m "Release vX.Y.Z"`
5. Push with tags: `git push && git push --tags`
6. Create GitHub release with changelog

### Pre-release Checklist
- [ ] All tests pass
- [ ] No clippy warnings
- [ ] No TypeScript errors
- [ ] CHANGELOG is updated
- [ ] README is current
- [ ] Build succeeds on target platforms

---

## /review

Assist with code review on pull requests.

### Review Checklist

**General:**
- [ ] Code is readable and self-documenting
- [ ] No unnecessary complexity
- [ ] Error handling is appropriate
- [ ] No hardcoded values that should be configurable

**Rust Backend:**
- [ ] No `unwrap()` on user input or external data
- [ ] Proper error propagation with `?` and `anyhow`/`thiserror`
- [ ] Memory safety considerations for audio buffers
- [ ] Async operations don't block the main thread
- [ ] Resources (files, audio streams) are properly closed

**TypeScript Frontend:**
- [ ] Components are appropriately sized
- [ ] State management is clear
- [ ] No memory leaks in useEffect hooks
- [ ] Accessible (keyboard navigation, ARIA labels)
- [ ] Responsive design considerations

**Performance:**
- [ ] No unnecessary re-renders
- [ ] Large computations are memoized or offloaded
- [ ] Audio processing doesn't cause glitches

### Workflow
1. Run `/review` with PR number or branch name
2. Review generated checklist against the changes
3. Leave comments on specific lines
4. Approve or request changes

---

## /setup

Help new contributors set up their development environment.

### Prerequisites

**Required:**
- macOS 14.0 (Sonoma) or later
- Apple Silicon (M1/M2/M3) recommended (Intel supported with different features)
- 16GB+ RAM recommended
- Xcode Command Line Tools

**Install Dependencies:**
```bash
# Xcode CLI tools
xcode-select --install

# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Node.js (via Homebrew)
brew install node

# pnpm package manager
brew install pnpm
# or: npm install -g pnpm
# or: corepack enable && corepack prepare pnpm@latest --activate

# espeak-ng (required for TTS phonemization)
brew install espeak-ng

# Tauri CLI
cargo install tauri-cli --version "^2"
```

### Project Setup
```bash
# Clone the repository
git clone https://github.com/your-org/blah3.git
cd blah3

# Install frontend dependencies
pnpm install

# Verify Rust setup
cd src-tauri && cargo check && cd ..

# Start development server
cargo tauri dev
```

### Verification
```bash
# Frontend builds
pnpm build

# Rust compiles
cd src-tauri && cargo build

# Tests pass
pnpm test:run
cd src-tauri && cargo test

# Linting passes
pnpm lint
cd src-tauri && cargo clippy -- -D warnings
```

### Common Issues

**Whisper compilation fails:**
- Ensure Xcode CLI tools are installed
- Try `xcode-select --reset`

**espeak-ng not found:**
- Ensure Homebrew installed it: `brew list espeak-ng`
- Check PATH includes Homebrew bin

**Permission errors:**
- Grant microphone access in System Settings → Privacy & Security
- Grant accessibility access for global hotkeys

### IDE Setup

**VS Code Extensions:**
- rust-analyzer
- Tauri
- ES7+ React/Redux/React-Native snippets
- Tailwind CSS IntelliSense
- Error Lens

**Settings:**
```json
{
  "editor.formatOnSave": true,
  "rust-analyzer.check.command": "clippy"
}
```
