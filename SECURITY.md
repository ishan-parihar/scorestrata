# Security Policy

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Report privately to
[security@ishanparihar.com](mailto:security@ishanparihar.com) (or open a
[private advisory](https://github.com/ishan-parihar/scorestrata/security/advisories/new)).

We aim to acknowledge reports within 72 hours and ship a fix (or a documented
mitigation) as soon as practical.

## Scope

Scorestrata is a local-first music-generation compiler. The attack surface is limited,
but these areas matter:

- **SF2 / sample loading** — untrusted `.sf2` bank files are parsed; malformed files
  must fail gracefully, never panic.
- **MCP server** — tools should validate inputs and never execute arbitrary shell
  commands or read files outside the project directory.
- **Project JSON** — deserialization of untrusted `project.json` must be bounds-checked
  (pitch, note count, timeline length).

## Security expectations

- No remote code execution paths from project or sample data.
- No secrets (API keys, tokens) ever committed — including tool-state artifacts
  (`.cortexkit/`, `.contexty/`, session caches).
- Rendering is deterministic; a malicious project must not be able to write outside
  its output directory.
