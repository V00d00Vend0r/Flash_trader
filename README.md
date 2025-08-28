<!--
  This README aims to be a single source of truth for the Raveslinger trading bot.
  It documents the architecture, modules, configuration knobs and extension points
  to help you understand how the pieces fit together and how to adapt the code to
  your own needs.  Feel free to share this file as a summary of the entire
  project.
-->

# Raveslinger Trading Bot

Raveslinger is a modular and extensible trading bot written in Rust.  The goal of
this project is to provide a solid skeleton for building algorithmic trading
strategies while demonstrating modern Rust patterns such as async/await,
structured logging, trait‑based abstractions and composable pipelines.  Although
the code is currently tuned towards the Sui blockchain and constant product AMM
pools, it has been designed in a way that allows porting to other venues with
minimal effort.

Below you will find an overview of every major component in the repository,
their responsibilities and how they interact.  You can treat this README as
the definitive documentation for the trading bot; if you ever need to submit
a single file to represent the project, this is it.

## Table of contents

1. [Directory structure](#directory-structure)
2. [Quick start](#quick-start)
3. [Configuration](#configuration)
4. [Decision layer](#decision-layer)
   - [Signals and decisions](#signals-and-decisions)
   - [Arbitrage detection](#arbitrage-detection)
   - [Phi‑3 mini‑LLM integration](#phi-3-mini-llm-integration)
   - [Signal engine](#signal-engine)
5. [Execution layer](#execution-layer)
   - [Execution signals and orders](#execution-signals-and-orders)
   - [Quote provider](#quote-provider)
   - [Gas estimator](#gas-estimator)
   - [PTB builder](#ptb-builder)
   - [Submitter](#submitter)
   - [Trade executor](#trade-executor)
6. [Engine pipeline](#engine-pipeline)
   - [Latency engine](#latency-engine)
   - [Optimization engine](#optimization-engine)
   - [Other engines](#other-engines)
7. [Plugins](#plugins)
8. [Data providers](#data-providers)
9. [Move contracts](#move-contracts)
10. [Extending the bot](#extending-the-bot)
11. [Testing](#testing)
12. [License](#license)

## Directory structure

The repository is organised into logical modules.  A high‑level view of the
tree looks like this:

```
.
├── Cargo.toml           # Rust package manifest with dependencies and features
├── Move.toml            # Move package manifest for the Sui contracts (placeholders)
├── config/              # TOML configuration files (base config, per‑network overrides)
├── contracts/           # Move sources for flash‑loan router (currently placeholders)
├── src/                 # Rust source code split by responsibility
│   ├── bootstrap.rs     # High‑level entry point wiring the bot together
│   ├── core/            # Core types and simple helpers (math, risk gate)
│   ├── data/            # Traits and stub implementations for data ingestion
│   ├── decision/        # Decision making logic and stages
│   ├── engines/         # Resource management & performance engines
│   ├── execution/       # Order preparation and on‑chain execution
│   ├── llm/             # Client abstraction for calling a mini‑LLM (Phi‑3)
│   ├── plugins/         # Extension points for slippage, ROI, path finding etc.
│   └── utils/           # Utilities such as input sanitization and metrics
├── tests/               # Basic unit tests illustrating expected behaviour
├── tools/               # Helper scripts (e.g. starting the Phi‑3 sidecar)
└── scripts/             # Convenience scripts for different operating systems
```

### Config

`config/base.toml` contains all of the knobs you can adjust without touching
code.  Each section maps to a piece of the runtime:

- **llm.backend** selects which LLM client implementation to use (currently only
  `onnx` is supported).
- **llm.onnx.url** tells the bot where to find the Phi‑3 sidecar.  The
  [`tools/phi3_server.py`](tools/phi3_server.py) script implements a minimal web
  server that wraps the `onnxruntime` inference in a REST API.
- **decision.stages** defines weights for each decision stage used in the
  `SignalEngine`.  Stage names must match those configured at runtime.
- **engines.latency** and **engines.optimization** expose thresholds and
  penalties for the respective engines (see below).
- **execution.sui** allows overriding the RPC URL, gas price multiplier, router
  selection and pool parameters on Sui.

You can duplicate this file for different environments (e.g. devnet vs. mainnet)
and load the appropriate one at startup via `RAVESLINGER_CONFIG` or by
modifying `config/config.rs`.

### Move.toml and contracts

The `contracts/Move.toml` declares a Move package that would normally house
on‑chain logic such as a flash‑loan module and a router for constructing
parameterised swaps.  In this scaffold these Move sources are placeholders,
but the structure is ready for you to implement real Move modules.  The
embedded `Move.toml` uses the name `raveslinger-contracts` and version `0.0.1`.

## Quick start

To get the bot running end‑to‑end on your local machine:

1. **Optional: start the Phi‑3 sidecar**

   If you wish to leverage the mini‑LLM decision stage, launch the sidecar
   server.  A convenience script is provided for your platform:

   - Windows:  
     ```cmd
     scripts\start_phi3_server.bat
     ```
   - macOS/Linux:  
     ```bash
     bash scripts/start_phi3_server.sh
     ```

   The sidecar uses `onnxruntime` to load a quantised Phi‑3 model, exposes a
   `/generate` endpoint and expects JSON containing `system`, `prompt` and
   inference parameters.  The response includes a `text` field with the model’s
   output.

2. **Build and run the bot**

   The crate uses async Rust and compiles on stable.  It depends on
   [tokio](https://crates.io/crates/tokio) for concurrency, [`reqwest`](https://crates.io/crates/reqwest)
   with native TLS for HTTP requests, [anyhow](https://crates.io/crates/anyhow) for error handling and
   [tracing](https://crates.io/crates/tracing) for structured logs.

   ```bash
   cargo build   # compile the project
   cargo run     # run a single decision–execution cycle
   ```

   The `bootstrap()` function in `src/bootstrap.rs` demonstrates how to wire up
   the decision stages, feed a toy market snapshot, run the engine pipeline and
   dispatch an order through the execution adapters.  Logs are emitted to
   stdout; you can enable additional verbosity by setting
   `RUST_LOG=debug` prior to running.

   **Note:** on Windows using the GNU toolchain you may encounter an error
   about missing `gcc.exe` when compiling the `ring` dependency pulled in by
   `rustls`.  This scaffold uses `reqwest` with native TLS on Windows to avoid
   that issue, so no additional C compiler is required.  If you choose to
   enable `rustls-tls` again, make sure you have either Visual Studio Build
   Tools (MSVC) or a MinGW toolchain installed.

## Decision layer

### Signals and decisions

The `decision/types.rs` file defines the fundamental structures used in the
decision layer:

- **Signal** – represents a market snapshot.  It captures the current time in
  milliseconds (`ts_ms`), the trading `symbol` (e.g. `"SUI-USDC"`), mid/bid/ask
  prices, a flexible map of `features` and a `meta` map for additional
  metadata.  Features can include anything you deem relevant such as price
  feeds from multiple venues, transaction costs, volatility estimates and so
  on.

- **Action** – an enum of `Buy`, `Sell` or `Hold`.  Additional actions can be
  introduced by extending this enum.

- **Decision** – produced by a decision stage.  It includes the chosen
  `action`, a continuous `score` indicating how strong the signal is, a
  `confidence` value between 0 and 1 and a `notes` field holding arbitrary
  JSON.

- **Plan** – output of the `SignalEngine`, bridging decisions to execution.
  It contains the final `action`, quantity (`qty`), optional limit price
  (`limit_px`), maximum slippage expressed in basis points, a time‑to‑live in
  milliseconds and the combined `confidence`.

### DecisionStage trait

All concrete decision stages implement the `DecisionStage` trait defined in
`decision/stage.rs`.  A stage must provide:

- `id()` – a unique identifier.
- `evaluate(signal)` – an async method that takes a `Signal` and returns a
  `Decision`.
- `enabled(signal)` – optional filter used to short‑circuit a stage (defaults to
  `true`).

### Arbitrage detection

`ArbitrageDetectionStage` in `decision/arbitrage_detection.rs` demonstrates a
simple rule‑based strategy.  It reads three features from the signal:

* `venueA_bid` and `venueB_ask` – the best bid on venue A and the best ask on
  venue B.
* `tx_cost_bps` – estimated transaction cost in basis points.

It computes the gross edge in basis points between the two venues,
subtracts the transaction cost and classifies the result:

* If the net edge is greater than `min_edge_bps`, return a `Buy` decision.
* If the net edge is less than `-min_edge_bps`, return a `Sell` decision.
* Otherwise, `Hold`.

The `score` is the net edge normalised to a range of ±1 and the `confidence`
is proportional to the absolute value of the edge.

### Phi‑3 mini‑LLM integration

`Phi3DecisionStage` in `decision/phi3_stage.rs` shows how to incorporate a
large‑language model into the decision flow.  It holds:

* A unique `id`.
* A boxed `LLMClient` (see `llm/client.rs`) – an abstraction for any text
  generator.
* Inference parameters (`temperature`, `top_p`, `max_tokens`).

The `evaluate()` method builds a prompt containing the current market snapshot,
sends it to the sidecar via the `LLMClient` and attempts to parse the
returned JSON.  If parsing fails, it falls back to `Hold` with low confidence.

The default `LLMClient` implementation is `Phi3OnnxClient` in `llm/phi3_onnx.rs`.
It uses `reqwest` to POST to `/generate` on the configured sidecar and expects
a JSON response with a `text` field.  See `tools/phi3_server.py` for the
service implementation (requires `onnxruntime` and the appropriate model).

### Signal engine

`SignalEngine` in `decision/signal_engine.rs` blends the output of multiple
stages into a single `Plan`.  It is constructed with a list of `(Stage, weight)`
pairs.  For every incoming `Signal` it:

1. Calls `evaluate()` on each enabled stage and accumulates the weighted
   scores and the minimum confidence across all stages.
2. Computes a blended score by dividing the weighted sum of scores by the
   sum of weights.
3. Determines the action based on thresholds (`> +0.02 → Buy`, `< –0.02 →
   Sell`, otherwise `Hold`).
4. Calculates the order quantity (`qty`) as a function of the absolute
   blended score times the configured `max_size` (default 10.0).
5. Propagates the `confidence` as the minimum confidence across stages.
6. Fills the rest of the plan parameters with defaults from the `SignalEngine`
   instance (e.g. `ttl_ms`, `max_slippage_bps`).

## Execution layer

The execution layer lives under `src/execution/` and is responsible for
turning a `Plan` into a concrete transaction on the underlying venue.  It
defines a collection of traits and types that decouple pricing, gas estimation,
transaction construction and submission.  This makes it easy to swap any piece
without affecting the rest.

### Execution signals and orders

`execution/types.rs` defines:

* **MetaMap** – a wrapper around a JSON map with helper methods for getting and
  setting `f64`, `u64` and `bool` values.  Engines use the meta map to pass
  resource usage statistics, latency measurements and other state between
  stages.
* **ExecutionSignal** – holds the current `confidence` for the trade and the
  associated `meta` map.  It is the unit flowing through the engine pipeline.
* **OrderRequest** – derived from a `(symbol, Plan)` tuple.  It includes the
  trading symbol, action, quantity, optional limit price, slippage cap, TTL
  and confidence.
* **QuotePack**, **GasFee**, **BuiltTx**, **SubmitResult** – typed wrappers
  representing each stage of the execution.  Quotes include bid/ask/mid and
  estimated slippage.  Gas fees contain the estimated gas cost and a
  priority (in basis points).  `BuiltTx` wraps the final transaction payload.
  `SubmitResult` carries the returned transaction digest, acceptance flag
  and filled quantity.

### Quote provider

The `QuoteProvider` trait in `execution/traits.rs` defines an async `best_quote()`
method that returns a `QuotePack` for a given `OrderRequest`.  The default
implementation is `SuiDexQuotes` in `execution/adapters/quotes.rs`.  It models
a constant product market maker (CPMM) with reserves `r_in` and `r_out` and
takes into account an LP fee.  Given a virtual trade of size one, it computes:

* `mid` = `r_out / r_in` – the pool’s mid price.
* `fee` – 1 minus the fee in basis points (e.g. 0.997 for 30 bps).
* `dx_eff` – effective input after fee.
* `dy` – output amount using the CPMM formula.
* `bid` and `ask` as the inverted ratio.

If either reserve is zero, an error is returned.  The stub returns a constant
slippage estimate; you can enhance it by simulating variable trade sizes or
consulting on‑chain liquidity.

### Gas estimator

The `GasEstimator` trait defines `estimate(req, quotes) -> GasFee`.  The
provided `SuiGasEstimator` in `execution/adapters/gas.rs` currently returns a
placeholder gas price (1e‑6 SUI) multiplied by a user‑specified multiplier.
The priority basis points are derived from the multiplier minus one.  To make
this realistic, you could query the latest Sui reference gas price via JSON‑RPC
and apply surge pricing logic.

### PTB builder

The `PtbBuilder` trait defines `build(req, quotes, gas) -> BuiltTx` which should
construct a transaction payload.  `SuiCetusPtbBuilder` in
`execution/adapters/ptb_builder.rs` produces a JSON object describing a
swap‑exact‑in call to the Cetus router.  It computes `min_out` by either:

* Dividing the quantity by the user’s `limit_px` and applying the slippage cap,
  or
* Multiplying the quantity by the computed quote’s bid and applying the
  slippage cap.

It embeds the router address, symbol, action and gas price.  In a real
implementation this would produce a base64‑encoded `bcs::TransactionPayload`
for Sui or a call to the router’s Move entrypoint.

### Submitter

The `Submitter` trait exposes `submit(tx) -> SubmitResult`.  The stub
`SuiSubmitter` in `execution/adapters/submitter.rs` simply returns a fake
transaction digest and marks it as accepted.  To execute trades for real, you
would call Sui’s `executeTransactionBlock` RPC method using the SDK of your
choice, sign the transaction with your wallet key and parse the result.

### Trade executor

`TradeExecutor` in `execution/executor.rs` wires the adapters together.  It
owns four `Arc` pointers to implementations of `QuoteProvider`, `GasEstimator`,
`PtbBuilder` and `Submitter`.  Its `execute()` method:

1. Fetches a quote.
2. Estimates gas.
3. Builds the transaction.
4. Submits the transaction.
5. Returns the `SubmitResult` or propagates any error.

`bootstrap()` in `src/bootstrap.rs` demonstrates instantiating a `TradeExecutor`
with the provided adapters and passing in the plan from the `SignalEngine`.

## Engine pipeline

The engine pipeline modifies the `ExecutionSignal` before a trade is executed.
Each engine implements the `Engine` trait in `engines/traits.rs`.  Engines can
adjust the `confidence`, update the meta map or short‑circuit execution by
disabling themselves.  The default `EnginePipeline` simply runs all enabled
stages in sequence.

### Latency engine

`LatencyEngine` in `engines/latency.rs` monitors the network latency reported in
the meta map (keys `lat_ms` or `network_latency`).  It applies two penalties:

* If `lat_ms` exceeds `hard_ms`, the confidence is multiplied by
  `1 – hard_penalty`.
* Else if `lat_ms` exceeds `soft_ms`, the confidence is multiplied by
  `1 – soft_penalty`.

The engine writes a boolean flag `latency_warn` to the meta map when latency
exceeds the soft threshold.  Configurable parameters include `enabled`,
`soft_ms`, `hard_ms`, `soft_penalty` and `hard_penalty`.

### Optimization engine

`OptimizationEngine` in `engines/optimization.rs` acts as a self‑healing module.
It reads CPU, GPU, memory and network usage from the meta map (keys
`cpu_usage`, `memory_usage`, `gpu_usage` and `network_usage`), the network
latency (`network_latency`) and the event queue length (`queue_length`).

The engine attempts to reduce resource stress by:

* Lowering CPU and memory usage when memory exceeds its threshold.
* Shifting load from CPU to GPU when CPU exceeds its threshold.
* Reducing network usage based on latency and queue factors but never below
  `min_network_bandwidth`.

After adjustments, it clamps all resource values to `[0, 100]` (or
`[min_network_bandwidth, 100]` for network usage) and computes a composite
stress score.  The confidence is multiplied by `1 – confidence_penalty × stress`.
You can tune thresholds, history window, adjustment step, minimum bandwidth and
penalty via `config/base.toml`.

### Other engines

The repository includes stubs for additional engines:

* **PerformanceEngine** (`engines/performance.rs`) – reserved for throughput
  optimisations.
* **DataHandlingEngine** (`engines/data_handling.rs`) – intended for buffering,
  deduplication or transformation of incoming market data.
* **MarketInsightEngine** (`engines/market_insight.rs`) – could incorporate
  statistical signals, momentum or sentiment.
* **QuantizeEngine** (`engines/quantize.rs`) – slot for quantisation logic such
  as representing values in basis points or compressing tick sizes.
* **ResourceEngine** (`engines/resource.rs`) – general resource manager.

All of the above currently pass the signal through unchanged.  To implement
custom logic, implement the `tick()` method accordingly.

## Plugins

The `plugins/` directory houses optional modules that can be mixed into the
execution pipeline or used by decision stages.  Current placeholders include:

* **gas.rs** – a potential plugin for gas optimisation beyond the default
  `GasEstimator`.
* **path.rs** – reserved for path‑finding across multiple pools.
* **slippage.rs** – reserved for dynamic slippage control.
* **roi.rs** – reserved for return‑on‑investment calculations.

You can flesh these out by defining new traits or by adding methods to the
existing ones.

## Data providers

Real trading bots require continuous market data.  The `data/` module defines
the `DataProvider` trait (in `data/provider.rs`) with a single async method
`next() -> Result<MarketData>`.  Current implementations are stubs:

* `http_client.rs` and `websocket_client.rs` are empty placeholders.
* The `sources/` submodule contains sub‑traits for specific channels (e.g.
  `ws`, `rpc`, `cetus`, `ankr_ws`, `dex`).  Each file currently contains a
  `// stub` comment.

To connect the bot to real exchanges or on‑chain feeds, implement the
`DataProvider` trait for your desired transport and pass live `MarketData` to
your decision stages instead of the static snapshot used in `bootstrap()`.

## Move contracts

The `contracts/` directory is set up for Sui Move packages.  Two source files
are provided as placeholders:

* `flash_loan.move` – would implement a flash‑loan module enabling atomic
  borrowing of tokens within a transaction.
* `router.move` – would route between different pools or execute complex swap
  logic.

Currently these files contain only comments.  To deploy actual on‑chain
components, write your Move code here and use `sui` CLI or a deployment
script to publish the package.

## Extending the bot

This scaffold aims to be extensible.  Here are some suggested paths:

* **Implement real data ingestion**: Write a WebSocket client for Sui or CeFi
  exchange order books, decode messages into `MarketData` and feed them into
  the decision stages.
* **Enhance the decision engine**: Add more rule‑based stages, integrate
  reinforcement learning or statistical models, or tune the weights in
  `SignalEngine`.
* **Add more engines**: Use `engines/performance.rs` to adjust CPU affinity or
  concurrency; add a `RiskEngine` that checks account exposure and rejects
  trades; implement a `BenchmarkEngine` for measuring latency in production.
* **Implement real execution**: Replace the stub adapters with calls into
  wallet libraries and Sui JSON‑RPC; handle rejections, retries and partial
  fills; support multi‑venue routing.
* **Integrate PoS nodes**: Use the bot as part of a liquidity rebalancing
  system across chains; run the optimisation and resource engines to tune
  compute usage.

## Testing

Two simple unit tests are provided under `tests/unit/`:

* `math_test.rs` ensures that `clamp01()` in `core/math.rs` bounds values to the
  range `[0, 1]`.
* `risk_test.rs` checks that `risk_gate()` in `core/risk_management.rs` returns
  `true` for zero (the gate is currently permissive).

You can run the tests with:

```bash
cargo test
```

The `tests/integration/arbitrage_test.rs` file is a placeholder for future
integration tests.

## License

This project is provided as a reference under the MIT license.  See `LICENSE`
for details.  You are free to fork, modify and distribute it.  If you do use
this scaffold as the basis for a production bot, please consider attributing
the original authors and contributing improvements back to the community.
