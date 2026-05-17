#!/usr/bin/env bash
# Start the EnaOS AI Runtime.
# Requires: Python 3.11+, Ollama running locally, enad running.

set -e

cd "$(dirname "$0")"

echo "=== EnaOS AI Runtime ==="

# Check prerequisites.
if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 not found"
    exit 1
fi

if ! python3 -c "import fastapi" 2>/dev/null; then
    echo "Installing dependencies..."
    pip install -r requirements.txt
fi

# Check Ollama.
if ! curl -s http://localhost:11434/api/tags >/dev/null 2>&1; then
    echo "WARNING: Ollama not running at http://localhost:11434"
    echo "  Start with: ollama serve"
fi

# Check enad.
if [ ! -S /tmp/enad.sock ]; then
    echo "WARNING: enad socket not found at /tmp/enad.sock"
    echo "  Start with: cd runtimes/enad && cargo run"
fi

echo "Starting AI runtime..."
python3 -m src.main
