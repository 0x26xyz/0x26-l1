import { useMemo } from "react";
import { ChainClient } from "./lib/chainClient";
import { POLL_INTERVAL_MS } from "./config";
import { useRpcEndpoint } from "./hooks/useRpcEndpoint";
import { useChainSnapshot } from "./hooks/useChainSnapshot";
import { useEventStream } from "./hooks/useEventStream";
import { AdminHeader } from "./components/AdminHeader";
import { ChainSnapshotCard } from "./components/ChainSnapshotCard";
import { PressureSummaryCard } from "./components/PressureSummaryCard";
import { TransitionCard } from "./components/TransitionCard";
import { EventStreamCard } from "./components/EventStreamCard";
import { StressedMarketsCard } from "./components/StressedMarketsCard";
import { StressedAccountsCard } from "./components/StressedAccountsCard";
import { ErrorBanner } from "./components/ErrorBanner";

function footerStatus(
  status: string,
  lastUpdatedMs: number | null
): string {
  if (status === "refreshing") return "Refreshing…";
  if (status === "loading") return "Loading snapshot…";
  if (status === "error") return "Snapshot failed — retrying on next poll";
  if (lastUpdatedMs == null) {
    return `Polling every ${Math.round(POLL_INTERVAL_MS / 1000)}s`;
  }
  const ts = new Date(lastUpdatedMs).toLocaleTimeString();
  return `Last update ${ts} · polling every ${Math.round(
    POLL_INTERVAL_MS / 1000
  )}s`;
}

function App() {
  const { rpcUrl, commit } = useRpcEndpoint();
  const client = useMemo(() => new ChainClient(rpcUrl), [rpcUrl]);

  const snapshot = useChainSnapshot(client);
  const stream = useEventStream(client);

  const loading = snapshot.status === "loading";
  const refreshing =
    snapshot.status === "loading" || snapshot.status === "refreshing";
  const hasData = snapshot.chainInfo != null || snapshot.summary != null;
  const showError = snapshot.error != null && !hasData
    ? snapshot.error
    : snapshot.status === "error" && snapshot.error
      ? snapshot.error
      : null;

  return (
    <div className="page-shell">
      <AdminHeader
        rpcUrl={rpcUrl}
        onCommitEndpoint={commit}
        onRefresh={() => void snapshot.refresh()}
        refreshing={refreshing}
      />

      {showError ? <ErrorBanner message={showError} /> : null}

      <main className="grid-layout">
        <ChainSnapshotCard
          chainInfo={snapshot.chainInfo}
          cursor={stream.cursor}
          loading={loading}
        />
        <PressureSummaryCard summary={snapshot.summary} loading={loading} />
        <TransitionCard
          transition={snapshot.summary?.latest_market_risk_transition ?? null}
          loading={loading}
        />
        <EventStreamCard events={stream.events} status={stream.status} />
        <StressedMarketsCard
          markets={snapshot.summary?.top_markets}
          loading={loading}
        />
        <StressedAccountsCard
          accounts={snapshot.summary?.top_accounts}
          loading={loading}
        />
      </main>

      <footer className="footer">
        <span>Endpoint: {rpcUrl}</span>
        <span>{footerStatus(snapshot.status, snapshot.lastUpdatedMs)}</span>
      </footer>
    </div>
  );
}

export default App;
