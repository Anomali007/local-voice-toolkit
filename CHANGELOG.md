# Changelog

All notable changes to Blah³ will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial Speech-to-Text (STT) engine using whisper-rs with CoreML/Metal acceleration
- Text-to-Speech (TTS) placeholder using kokoroxide (Kokoro-82M)
- Global hotkey support for dictation (⌘+⇧+D) and screen reading (⌘+⇧+S)
- Model manager UI for downloading and switching STT/TTS models
- Real-time waveform visualization during recording
- Voice preview for TTS voice selection
- Settings panel for configuring hotkeys and preferences
- System tray integration with quick access menu
- Floating overlay for dictation status
- React frontend with Tailwind CSS styling
- Tauri v2 backend with Rust
- macOS Sonoma (14.0+) support optimized for Apple Silicon

### Changed
- N/A

### Deprecated
- N/A

### Removed
- N/A

### Fixed
- N/A

### Security
- N/A

---

## Version History

<!--
When releasing a new version:
1. Move items from [Unreleased] to a new version section
2. Add the release date
3. Create a git tag: git tag -a v0.1.0 -m "Release v0.1.0"

Example:

## [0.1.0] - 2024-01-15

### Added
- Feature descriptions...

### Fixed
- Bug fix descriptions...
-->
