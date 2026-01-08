# Project Notes

## Build Commands

Always source cargo env before running make:

```bash
source "$HOME/.cargo/env" && make build
source "$HOME/.cargo/env" && make install
source "$HOME/.cargo/env" && make bundle
```

The system's non-interactive bash doesn't have cargo in PATH.
