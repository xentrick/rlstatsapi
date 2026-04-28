# rlstats-example-lib

This is a small example Python library that consumes the `rlstatsapi` PyO3 bindings.

## Prerequisites

- Python 3.11+
- `uv`

This example is configured to resolve `rlstatsapi` from the local repository path via `[tool.uv.sources]`.

## Install the Example Library

```bash
cd examples/python_library
uv sync
```

## Run the Demo

```bash
uv run demo.py
```

The demo connects to `127.0.0.1:49123` by default and prints normalized event objects.
