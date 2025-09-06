import "dotenv/config";
import express from "express";
import { z } from "zod";
import { SuiClient } from "@mysten/sui/client";
import { AggregatorClient } from "@cetusprotocol/aggregator-sdk";
import * as CLMM from "@cetusprotocol/cetus-sui-clmm-sdk";
import BN from "bn.js";

const PORT = Number(process.env.PORT ?? 12789);
const SUI_RPC = process.env.SUI_RPC || "https://fullnode.mainnet.sui.io:443";
const NETWORK = (process.env.NETWORK || "mainnet") as "mainnet" | "testnet";

const app = express();
app.disable("x-powered-by");

app.get("/health", (_req, res) =>
  res.json({ ok: true, service: "cetus-sidecar", ts: Date.now() })
);

const QuoteQuery = z.object({
  from: z.string(),
  target: z.string().optional(),
  to: z.string().optional(),
  amount: z.string(),
});
app.get("/quote", async (req, res) => {
  try {
    const q = QuoteQuery.parse(req.query);
    const target = q.target ?? q.to;
    if (!target)
      return res
        .status(400)
        .json({ ok: false, error: "missing target/to" });

    const agg = new AggregatorClient({ network: NETWORK });
    const routers = await agg.findRouters({
      from: q.from,
      target,
      amount: new BN(q.amount),
      byAmountIn: true,
    });

    return res.json({
      ok: true,
      amountIn: routers.amountIn?.toString?.() ?? null,
      amountOut: routers.amountOut?.toString?.() ?? null,
      byAmountIn: routers.byAmountIn ?? true,
      insufficientLiquidity: routers.insufficientLiquidity ?? false,
      deviation: routers.deviationRatio ?? null,
      paths: routers.paths ?? routers.routes ?? null,
      timestamp: Date.now(),
    });
  } catch (e: any) {
    return res
      .status(400)
      .json({ ok: false, error: e?.message || "bad_request" });
  }
});

const PoolQuery = z.object({ poolId: z.string() });
app.get("/pool-snapshot", async (req, res) => {
  try {
    const { poolId } = PoolQuery.parse(req.query);
    const pool = await (CLMM as any).Pool.fetchPool(
      new SuiClient({ url: SUI_RPC }),
      poolId
    );
    return res.json({
      ok: true,
      poolId,
      tick: pool.current_tick_index,
      sqrtPriceX64: pool.current_sqrt_price,
      liquidity: pool.liquidity,
      raw: pool,
      timestamp: Date.now(),
    });
  } catch (e: any) {
    return res
      .status(400)
      .json({ ok: false, error: e?.message || "bad_request" });
  }
});

app.listen(PORT, "127.0.0.1", () => {
  console.log(
    `cetus-sidecar listening on http://127.0.0.1:${PORT}`
  );
});
