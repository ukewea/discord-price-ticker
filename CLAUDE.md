# CLAUDE.md - AI Assistant Development Guide

## Project Overview

**discord-price-ticker** is a Rust-based service that updates cryptocurrency and stock prices on Discord servers. It runs as a background service that periodically fetches price data from external APIs (CoinGecko) and updates Discord bot nicknames and status to display current prices and 24-hour price changes.

**Language**: Rust (Edition 2021)
**Runtime**: Tokio async runtime
**Primary APIs**: CoinGecko API (crypto prices), Discord API (via Serenity)

## Codebase Structure

```
discord-price-ticker/
├── src/
│   ├── main.rs                    # Application entry point, orchestrates all components
│   ├── config.rs                  # Configuration structures (Config, TickerConfig)
│   ├── bot_update.rs              # Bot update data structure
│   ├── discord.rs                 # Discord module namespace
│   │   └── client.rs              # Discord client wrapper (Serenity)
│   └── quote/                     # Price fetching module
│       ├── mod.rs                 # Module exports
│       ├── request.rs             # AssetQuoteRequest structure
│       ├── response.rs            # AssetQuoteResponse structure
│       ├── error.rs               # Custom error types for quote operations
│       └── req_consumer.rs        # Consumer that fetches prices from CoinGecko
├── Cargo.toml                     # Dependencies and project metadata
├── Cargo.lock                     # Locked dependency versions (in version control)
├── app_config.sample.json         # Sample configuration file
├── README.md                      # Project description
├── README_Design.md               # Detailed design documentation
└── .github/workflows/test.yml     # CI workflow for running tests

```

## Architecture and Design Patterns

### High-Level Architecture

The application follows a **producer-consumer pattern** with **mpsc channels** for communication:

1. **Configuration Parser** → Reads `app_config.json` at startup
2. **Task Scheduler** → Spawns async tasks for each configured ticker
3. **Price Fetch Queue** → Receives quote requests via mpsc channel
4. **Price Consumer** → Fetches prices from CoinGecko API
5. **Bot Update Queue** → Receives bot update requests via mpsc channel
6. **Discord Client** → Updates bot nickname and status in all guilds

### Data Flow

```
Config File → Task Scheduler → [Ticker Task 1, Ticker Task 2, ...]
                                        ↓
                         AssetQuoteRequest → Price Fetch Queue
                                        ↓
                         CoinGecko API Consumer (with retries)
                                        ↓
                         AssetQuoteResponse → Ticker Task
                                        ↓
                         BotUpdateInfo → Bot Update Queue
                                        ↓
                         Discord Client → Discord API
```

### Concurrency Model

- **Tokio async runtime** with full features enabled
- **Multiple concurrent tasks**: One per configured ticker
- **mpsc unbounded channels** for inter-task communication
- **Graceful shutdown**: Oneshot channels for stop signals (Ctrl+C handling)

## Key Modules and Their Responsibilities

### `main.rs` (src/main.rs)

**Primary responsibilities:**
- Application initialization and configuration loading
- Spawning periodic fetch loops for each ticker
- Managing stop signals and graceful shutdown
- Coordinating mpsc channels between components

**Key functions:**
- `read_config()`: Loads JSON configuration asynchronously
- `run_periodic_crypto_fetch_job_loop()`: Main ticker loop (periodic price fetch)
- `format_price()`: Formats BigDecimal prices with specified decimal places
- `format_price_change()`: Formats 24h price change with +/- prefix
- `generate_discord_bot_name()`: Creates bot nickname (e.g., "$1234.56")
- `generate_discord_bot_status()`: Creates bot status (e.g., "+2.34% | ADAUSD")
- `is_bot_token_valid()`: Validates Discord bot tokens

**Important patterns:**
- Uses `#[instrument]` macro for tracing spans
- Implements macro `break_if_signaled!` for graceful shutdown checks
- Validates bot tokens before spawning tasks

### `config.rs` (src/config.rs)

**Structures:**
- `Config`: Root configuration containing API key and ticker list
- `TickerConfig`: Per-ticker configuration

**Fields:**
- `coingecko_api_key`: CoinGecko API key (can be empty string for free tier)
- `ticker`: Symbol displayed in bot status (e.g., "ADAUSD")
- `name`: CoinGecko ID for price lookup (e.g., "cardano")
- `crypto`: Boolean flag (true for crypto, false for stocks - stocks not implemented)
- `frequency`: Update interval in seconds
- `decimals`: Number of decimal places for price display
- `discord_bot_token`: Discord bot authentication token

### `quote/req_consumer.rs` (src/quote/req_consumer.rs)

**Primary responsibility:** Consumes price requests from the queue and fetches data from CoinGecko API

**Key function:**
- `consume_crypto_price_requests()`: Infinite loop consuming from mpsc receiver

**Implementation details:**
- **Retry logic**: 3 attempts with 1-second sleep between retries
- **Error handling**: Sends error results back through response channel
- **API format**: `https://api.coingecko.com/api/v3/simple/price?ids={name}&vs_currencies={vs_currency}&include_24hr_change=true`
- **Response parsing**: Extracts `usd` and `usd_24h_change` fields
- **BigDecimal usage**: Ensures precise decimal arithmetic for prices

**Important patterns:**
- Uses `sleep_then_continue!` macro for retry logic
- Validates API response structure before parsing
- Always sends response (success or error) to avoid deadlocks

### `discord/client.rs` (src/discord/client.rs)

**Primary responsibility:** Discord API interaction via Serenity library

**Key functions:**
- `new()`: Creates Discord client, spawns shard runners
- `update_bot()`: Updates nickname in all guilds and sets bot activity status
- `get_guilds()`: Retrieves all guilds the bot is in (with pagination)

**Implementation details:**
- Uses Arc for shared HTTP client and shard manager
- Spawns client in background task
- Updates nickname via `edit_nickname()` for each guild
- Sets custom activity status via shard runners

**Important patterns:**
- Pagination for guild fetching (limit: 100, retries: 3)
- 3-second retry delay for API failures
- Cloneable client via Arc wrappers

### `quote/error.rs` (src/quote/error.rs)

**Custom error enum:**
- `HttpRequest(reqwest::Error)`: Network/HTTP errors
- `JsonParse(serde_json::Error)`: JSON parsing errors
- `ParseBigDecimal(bigdecimal::ParseBigDecimalError)`: Decimal parsing errors
- `Other(String)`: Catch-all for string-based errors

**Implements:**
- `Display` trait for error formatting
- `Error` trait for error chaining
- `From` conversions for automatic error type conversion

## Development Workflow

### Prerequisites

- Rust toolchain (stable, 2021 edition)
- `app_config.json` file (copy from `app_config.sample.json`)
- Discord bot tokens for each ticker
- CoinGecko API key (optional, can use free tier)

### Building

```bash
cargo build
cargo build --release  # For production
```

### Running

```bash
# Default: uses app_config.json in current directory
cargo run

# Specify custom config file path
cargo run -- --config /path/to/config.json
cargo run -- -c custom_config.json

# Show help
cargo run -- --help
```

**CLI Arguments:**
- `--config, -c`: Path to configuration file (default: `app_config.json`)

### Testing

```bash
cargo test
cargo test --all-features  # CI command
```

**Test coverage:**
- Unit tests in `main.rs` for formatting functions
- Unit tests in `quote/error.rs` for error display
- Tests use standard Rust test framework

### CI/CD

**GitHub Actions workflow:** `.github/workflows/test.yml`
- Runs on: `push`, `pull_request`
- Commands: `cargo test --all-features`
- Uses: `dtolnay/rust-toolchain@stable`

### Logging

**Tracing framework:**
- Log level: `DEBUG` (configurable in main)
- Key log points:
  - Config loading
  - Timer ticks for each ticker
  - Price fetch requests/responses
  - Discord API updates
  - Error conditions
  - Shutdown signals

**Log patterns:**
- Use `#[instrument]` for function spans
- Include ticker symbol in log context
- Log at appropriate levels: `trace`, `debug`, `info`, `warn`, `error`

### Docker Deployment

**Building the Docker image:**

```bash
# Build the image
docker build -t discord-price-ticker:latest .

# Build with specific tag
docker build -t discord-price-ticker:v0.1.0 .
```

**Running with Docker:**

```bash
# Run with mounted config file
docker run -v $(pwd)/app_config.json:/app/app_config.json discord-price-ticker:latest

# Run with custom config path
docker run -v $(pwd)/my_config.json:/etc/ticker/config.json \
  discord-price-ticker:latest --config /etc/ticker/config.json

# Run in detached mode with auto-restart
docker run -d --restart unless-stopped \
  --name price-ticker \
  -v $(pwd)/app_config.json:/app/app_config.json \
  discord-price-ticker:latest
```

**Docker Compose example:**

```yaml
version: '3.8'
services:
  discord-price-ticker:
    build: .
    container_name: discord-price-ticker
    restart: unless-stopped
    volumes:
      - ./app_config.json:/app/app_config.json:ro
```

**Container features:**
- Multi-stage build (small final image size)
- Non-root user for security
- Debian slim base with SSL support
- Configurable via mounted config file or CLI arguments

**Important notes:**
- Config file should be mounted as read-only (`:ro`) for security
- Container runs as non-root user (UID 1000)
- Requires network access for CoinGecko and Discord APIs
- No persistent storage needed (stateless application)

## Configuration

### app_config.json Structure

```json
{
    "coingecko_api_key": "CG-...",
    "tickers": [
        {
            "ticker": "ADAUSD",
            "name": "cardano",
            "crypto": true,
            "frequency": 63,
            "decimals": 2,
            "discord_bot_token": "YOUR_DISCORD_BOT_TOKEN"
        }
    ]
}
```

**Configuration rules:**
- Each ticker gets its own Discord bot token (one bot per ticker)
- `frequency` must be reasonable (avoid API rate limits)
- `decimals` controls price display precision
- Bot tokens are validated before task spawning

## Dependencies

### Core Dependencies

```toml
bigdecimal = "0.4"              # Precise decimal arithmetic
clap = { version = "4.5", features = ["derive"] }  # CLI argument parsing
reqwest = { version = "0.12", features = ["blocking"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = { version = "1.0", features = ["arbitrary_precision"] }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"                 # Structured logging
tracing-subscriber = "0.3"      # Log subscriber
serenity = { version = "0.12", features = ["full"] }  # Discord API
```

### Why These Dependencies?

- **bigdecimal**: Cryptocurrency prices require arbitrary precision
- **clap**: CLI argument parsing with derive macros for config file path
- **tokio**: Async runtime for concurrent ticker tasks
- **serenity**: High-level Discord API wrapper
- **tracing**: Structured logging with spans and fields
- **serde_json**: `arbitrary_precision` feature for exact number parsing

## Important Patterns and Conventions

### Async/Await Usage

- All I/O operations are async (file reading, HTTP requests, Discord API)
- Use `tokio::spawn()` for concurrent tasks
- Use `tokio::time::sleep()` for delays
- Use `tokio::signal::ctrl_c()` for graceful shutdown

### Error Handling

- Return `Result<T, E>` from fallible functions
- Use `?` operator for error propagation
- Log errors before returning/continuing
- Send error responses through channels to avoid deadlocks

### Channel Communication

- Use **unbounded channels** for simplicity (no backpressure needed)
- Always send responses (success or error) to avoid receiver deadlocks
- Clone senders for multiple producers
- Use oneshot channels for one-time signals (shutdown)

### Testing Conventions

- Unit tests in same file as implementation (using `#[cfg(test)]`)
- Test function naming: `test_<function_name>` or `test_<scenario>`
- Use `assert_eq!` for value comparisons
- Test edge cases (empty strings, zero values, boundary conditions)

### Code Style

- **Formatting**: Use `rustfmt` defaults
- **Imports**: Group by std, external crates, internal modules
- **Naming**: Snake_case for functions/variables, PascalCase for types
- **Comments**: Inline comments for field descriptions, doc comments for public APIs
- **Macros**: Use macros to reduce repetitive code (e.g., `break_if_signaled!`)

### Git Conventions

**Commit message format** (from recent commits):
- `feat:` for new features
- `fix:` for bug fixes
- `refactor:` for code restructuring
- `chore:` for tooling/maintenance
- `other:` for miscellaneous changes

**Examples:**
- `feat: update Discord bots' name after price fetch`
- `chore: add test GHA workflow`
- `refactor; add tests` (note: should be `refactor:`)
- `other: include Cargo.lock in version control.`

**Important:**
- `Cargo.lock` is committed to version control (binary project)
- Use descriptive commit messages
- Keep commits focused and atomic

## AI Assistant Guidelines

### When Making Changes

1. **Read before writing**: Always read files before editing
2. **Run tests**: Execute `cargo test` after changes
3. **Check compilation**: Ensure `cargo build` succeeds
4. **Update tests**: Add tests for new functionality
5. **Follow conventions**: Match existing code style and patterns

### Common Tasks

#### Adding a New Ticker
1. Add entry to `app_config.json`
2. Ensure valid Discord bot token
3. Restart service (no hot reload)

#### Adding New Price Source
1. Create new module in `quote/` directory
2. Implement request/response structures
3. Create consumer function similar to `consume_crypto_price_requests()`
4. Add queue and spawning logic in `main.rs`

#### Modifying Price Format
- Edit `format_price()` or `generate_discord_bot_name()` in `main.rs`
- Update tests in `tests` module at bottom of `main.rs`

#### Adding Logging
- Use `tracing` macros: `trace!`, `debug!`, `info!`, `warn!`, `error!`
- Add `#[instrument]` attribute for function-level tracing
- Include relevant context in log messages

### Things to Avoid

- **Don't** modify error handling to ignore errors silently
- **Don't** remove retry logic without replacement
- **Don't** hardcode values that should be configurable
- **Don't** use `unwrap()` on user-provided input
- **Don't** block the async runtime with synchronous I/O
- **Don't** push directly to main branch (use feature branches)

### Security Considerations

- Bot tokens are sensitive - never log them
- Validate all user input (currently only config file)
- Use HTTPS for all external API calls
- Rate limiting is handled by retry delays (basic approach)

## Known Limitations and Future Work

(Based on README_Design.md)

### Not Yet Implemented

1. **Stock price fetching**: Only crypto currently supported
2. **Dynamic configuration**: Requires restart to reload config
3. **Persistent queuing**: Uses in-memory channels (data loss on crash)
4. **Advanced retry strategies**: No exponential backoff
5. **Health monitoring**: No metrics/monitoring endpoints
6. **Resource limits**: No memory/CPU caps
7. **Graceful shutdown**: Incomplete (in-flight requests may be lost)

### If Implementing New Features

- Consider adding health check endpoints
- Implement configuration hot-reload
- Add Prometheus metrics
- Implement exponential backoff for retries
- Add rate limiting to respect API quotas
- Consider persistent queue (e.g., Redis)

## Quick Reference

### File Locations

- **Main logic**: `src/main.rs:40-160` (run_periodic_crypto_fetch_job_loop)
- **Config parsing**: `src/main.rs:33-37`
- **Price consumer**: `src/quote/req_consumer.rs:15-161`
- **Discord client**: `src/discord/client.rs:14-108`
- **Error types**: `src/quote/error.rs:5-64`

### Key Constants

- Default VS currency: `"usd"` (src/main.rs:47)
- Currency symbol prefix: `"$"` (src/main.rs:48)
- Price fetch retries: `3` (src/quote/req_consumer.rs:29)
- Guild fetch retries: `3` (src/discord/client.rs:76)
- Guild fetch limit: `100` (src/discord/client.rs:78)

### CLI Arguments

- `--config, -c <PATH>`: Path to configuration file (default: `app_config.json`)
- `--help`: Display help information
- `--version`: Display version information

### Running Commands

```bash
# Development
cargo run
cargo run -- --config custom.json

# Testing
cargo test

# Production build
cargo build --release

# Check without building
cargo check

# Format code
cargo fmt

# Lint
cargo clippy

# Docker
docker build -t discord-price-ticker .
docker run -v $(pwd)/app_config.json:/app/app_config.json discord-price-ticker
```

---

**Last Updated**: 2025-11-15
**Project Version**: 0.1.0
**Rust Edition**: 2021
