# Security Policy

## Supported versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a vulnerability

Please **do not** open a public issue for security vulnerabilities.
Report them privately using GitHub's
[private vulnerability reporting](https://github.com/lemonhub-io/chunk-log/security/advisories)
feature, or open a GitHub discussion if you prefer a less formal channel.

Please include:

- A description of the vulnerability and its impact.
- Steps to reproduce, if possible.
- Affected versions.
- Any suggested fix, if you have one.

We aim to acknowledge reports within 5 business days and to ship a fix in a
timely manner. Until a fix is released, please do not disclose the
vulnerability publicly.

## Scope

In scope: integrity of the object store, correctness of hash addressing,
path handling in the repository layout (e.g. branch names, object
filenames), and data loss risks in commit / checkout / gc operations.

Out of scope: chunk payload semantics (chunklog stores whatever bytes a
game provides), and game-side rendering logic.
