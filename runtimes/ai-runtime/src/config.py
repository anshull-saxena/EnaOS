"""Runtime configuration."""

from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    """AI runtime settings."""

    # enad connection
    enad_socket: str = "/tmp/enad.sock"

    # Ollama
    ollama_url: str = "http://localhost:11434"
    ollama_model: str = "llama3.2"

    # API server
    host: str = "127.0.0.1"
    port: int = 8900

    # Context
    max_context_entries: int = 50
    context_ttl_seconds: int = 300

    model_config = {"env_prefix": "ENA_"}


settings = Settings()
