use crate::config::config::load_base;
use crate::decision::types::Signal;
use crate::decision::arbitrage_detection::ArbitrageDetectionStage;
use crate::decision::signal_engine::SignalEngine;
use crate::decision::execution_planning::shape_for_execution;
use crate::decision::phi3_stage::Phi3DecisionStage;
use crate::llm::phi3_onnx::Phi3OnnxClient;

use crate::execution::types::{ExecutionSignal, OrderRequest};
use crate::execution::engine::EnginePipeline;
use crate::engines::{Engine, EngineContext};
use crate::engines::latency::LatencyEngine;
use crate::engines::optimization::OptimizationEngine;

use crate::execution::executor::TradeExecutor;
use crate::execution::adapters::{quotes::SuiDexQuotes, gas::SuiGasEstimator, ptb_builder::SuiCetusPtbBuilder, submitter::SuiSubmitter};

use std::sync::Arc;
use tracing::info;

use crate::secrets;
use crate::env_required;

pub async fn bootstrap() -> anyhow::Result<()> {
    // 🔐 Ensure secrets are loaded at startup
    secrets::load_encrypted_env().await?;

    let cfg = load_base().ok();

    // 1) Market signal (toy)
    let mut features = serde_json::Map::new();
    features.insert("venueA_bid".into(), 1.002.into());
    features.insert("venueB_ask".into(), 0.998.into());
    features.insert("tx_cost_bps".into(), 5.0.into());

    let s = Signal {
        ts_ms: 0,
        symbol: "SUI-USDC".into(),
        mid: 1.0, bid: 0.999, ask: 1.001,
        features,
        meta: serde_json::Map::new(),
    };

    // 2) Decision stages: arbitrage + Phi-3
    let arb = Arc::new(ArbitrageDetectionStage { min_edge_bps: 5.0 });

    let phi3_url = cfg.as_ref()
        .and_then(|c| c.llm.as_ref())
        .and_then(|l| l.onnx.as_ref())
        .and_then(|o| o.url.clone())
        .unwrap_or_else(|| env_required!("PHI3_URL")); // now comes from .env
    let phi3 = Arc::new(Phi3DecisionStage::new("phi3", Phi3OnnxClient::new(phi3_url)));

    let se = SignalEngine::new(vec![
        (arb as Arc<_>, 1.0),
        (phi3 as Arc<_>, 1.0),
    ]);
    let plan = se.decide(&s).await?;
    let plan = shape_for_execution(plan, Some(12.0))?;

    // 3) ExecutionSignal → engines
    let mut sig = ExecutionSignal::default();
    sig.confidence = plan.confidence;
    sig.meta.set_f64("lat_ms", 42.0);

    let ctx = EngineContext { id: "default".into() };
    let stages: Vec<Arc<dyn Engine>> = vec![
        Arc::new(LatencyEngine::new("latency", None)),
        Arc::new(OptimizationEngine::new("optimization", None)),
    ];
    let pipeline = EnginePipeline::new(ctx, stages);
    let sig = pipeline.run(sig).await?;

    // 4) DEX-only execution
    if sig.confidence >= 0.4 {
        let order: OrderRequest = (s.symbol.as_str(), &plan).into();
        let exec = TradeExecutor::new(
            Arc::new(SuiDexQuotes { fee_bps: 30, pool_reserve_in: 1_000_000.0, pool_reserve_out: 1_000_000.0 }),
            Arc::new(SuiGasEstimator { 
                rpc_url: env_required!("SUI_FULLNODE_URL"), 
                multiplier: 1.10 
            }),
            Arc::new(SuiCetusPtbBuilder { 
                router_addr: env_required!("CETUS_ROUTER_ADDR") 
            }),
            Arc::new(SuiSubmitter { 
                rpc_url: env_required!("SUI_FULLNODE_URL") 
            }),
        );
        let res = exec.execute(&sig, &order).await?;
        info!(?order, ?res, "order executed via dex adapters (stubbed)");
    } else {
        info!(conf = sig.confidence, "skipping execution due to low confidence");
    }

    Ok(())
}