use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json;

#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
pub struct QuoteResp {
    pub ok: bool,
    pub amountIn: Option<String>,
    pub amountOut: Option<String>,
    pub byAmountIn: Option<bool>,
    pub insufficientLiquidity: Option<bool>,
    // accept deviation as a string or number
    pub deviation: Option<serde_json::Value>,
    pub paths: Option<serde_json::Value>,
    pub timestamp: u64,
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
pub struct PoolSnapshotResp {
    pub ok: bool,
    pub poolId: Option<String>,
    pub tick: Option<i64>,
    pub sqrtPriceX64: Option<String>,
    pub liquidity: Option<String>,
    pub raw: Option<serde_json::Value>,
    pub timestamp: u64,
}

pub struct CetusSidecar {
    base: String,
    client: reqwest::Client,
}

impl CetusSidecar {
    pub fn new(base: &str) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("flash-trader/adapters_cetus")
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .build()
            .context("building reqwest client")?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            client,
        })
    }

    /// Quote amountOut for a given input amount (atomic units as string).
    pub async fn quote(&self, from: &str, to: &str, amount_atomic: &str) -> Result<QuoteResp> {
        let url = format!("{}/quote", self.base);
        let resp = self.client
            .get(&url)
            .query(&[("from", from), ("to", to), ("amount", amount_atomic)])
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .context("sidecar /quote send failed")?;

        let status = resp.status();
        let body = resp.text().await.context("sidecar /quote read failed")?;
        if !status.is_success() {
            anyhow::bail!("sidecar /quote HTTP {}: {}", status, body);
        }
        let parsed: QuoteResp = serde_json::from_str(&body)
            .context("sidecar /quote json parse failed")?;
        if !parsed.ok {
            anyhow::bail!("sidecar /quote returned ok=false: {}", body);
        }
        Ok(parsed)
    }

    /// Fetch a pool snapshot by poolId.
    pub async fn pool_snapshot(&self, pool_id: &str) -> Result<PoolSnapshotResp> {
        let url = format!("{}/pool-snapshot", self.base);
        let resp = self.client
            .get(&url)
            .query(&[("poolId", pool_id)])
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .context("sidecar /pool-snapshot send failed")?;

        let status = resp.status();
        let body = resp.text().await.context("sidecar /pool-snapshot read failed")?;
        if !status.is_success() {
            anyhow::bail!("sidecar /pool-snapshot HTTP {}: {}", status, body);
        }
        let parsed: PoolSnapshotResp = serde_json::from_str(&body)
            .context("sidecar /pool-snapshot json parse failed")?;
        if !parsed.ok {
            anyhow::bail!("sidecar /pool-snapshot returned ok=false: {}", body);
        }
        Ok(parsed)
    }
}
