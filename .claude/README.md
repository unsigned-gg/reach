# `.claude/` — federated harness surface

Federated under [terrarium](https://github.com/cerebral-work/terrarium) standards.

| Hook | Event | What | Bypass |
|---|---|---|---|
| `guard-main-push` | PreToolUse Bash | deny direct push to main | `# allow-direct-push` |
