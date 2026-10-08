# Provenance

- Upstream: [humanlayer/skills](https://github.com/humanlayer/skills)
- Source path: `plugins/show-me/skills/show-me/SKILL.md`
- Plugin: `show-me` v1.0.1
- Pinned commit: `ca7c8088db69e315a8b2deea43820270457f8f3c`
- Updated: 2026-10-08 (UTC)
- License: MIT

## Modifications

Vendored verbatim except for the final HTML-open step, which upstream writes as
Claude Code's `Bash(open path/to/show-me-{description}.html)` syntax. It now
asks the agent to open the rendered file in a browser or file-preview view, so
the skill stays runtime-agnostic.

Everything else is adopted from upstream as-is, including the changes picked up
in the 2026-10-08 update:

- `disable-model-invocation: true` in the SKILL.md frontmatter (adopted from
  upstream, not local).
- `agents/openai.yaml` (`policy.allow_implicit_invocation: false`), included
  verbatim.

## License

MIT License

Copyright (c) 2026 HumanLayer

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
