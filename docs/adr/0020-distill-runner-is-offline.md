# Distill runner is offline

The Distill runner performs no network access and does not parse web pages, PDFs, or proprietary document formats. Runtime-specific intake executors use their available file, browser, and connector tools to submit source metadata, retainable raw bytes, normalized text, extraction details, and content hashes; the runner validates and freezes that submission as the requirement input snapshot. This keeps authentication and format support out of the portable state-machine core.
