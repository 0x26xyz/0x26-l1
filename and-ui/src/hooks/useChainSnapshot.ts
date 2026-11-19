import { useCallback, useEffect, useRef, useState } from "react";
import type {
  ChainClient,
  ChainInfoResponse,
  LiquidationPressureSummaryResponse,
} from "../lib/chainClient";
import { POLL_INTERVAL_MS } from "../config";

export type SnapshotStatus = "idle" | "loading" | "refreshing" | "ready" | "error";

export interface ChainSnapshotState {
  chainInfo: ChainInfoResponse | null;
  summary: LiquidationPressureSummaryResponse | null;
  status: SnapshotStatus;
  error: string | null;
  lastUpdatedMs: number | null;
  refresh: () => Promise<void>;
}

export function useChainSnapshot(client: ChainClient): ChainSnapshotState {
  const [chainInfo, setChainInfo] = useState<ChainInfoResponse | null>(null);
  const [summary, setSummary] =
    useState<LiquidationPressureSummaryResponse | null>(null);
  const [status, setStatus] = useState<SnapshotStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [lastUpdatedMs, setLastUpdatedMs] = useState<number | null>(null);

  const clientRef = useRef(client);
  clientRef.current = client;

  const refresh = useCallback(async () => {
    const current = clientRef.current;
    setStatus((previous) =>
      previous === "ready" || previous === "refreshing" ? "refreshing" : "loading"
    );
    try {
      const [nextChainInfo, nextSummary] = await Promise.all([
        current.chainInfo(),
        current.liquidationPressureSummary(),
      ]);
      // Ignore the result if the client has changed under us.
      if (clientRef.current !== current) {
        return;
      }
      setChainInfo(nextChainInfo);
      setSummary(nextSummary);
      setError(null);
      setLastUpdatedMs(Date.now());
      setStatus("ready");
    } catch (nextError) {
      if (clientRef.current !== current) {
        return;
      }
      setError(
        nextError instanceof Error ? nextError.message : String(nextError)
      );
      setStatus("error");
    }
  }, []);

  useEffect(() => {
    setChainInfo(null);
    setSummary(null);
    setError(null);
    setStatus("loading");
    setLastUpdatedMs(null);

    let cancelled = false;
    const tick = () => {
      if (cancelled) return;
      void refresh();
    };

    tick();
    const timer = window.setInterval(tick, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [client, refresh]);

  return { chainInfo, summary, status, error, lastUpdatedMs, refresh };
}
