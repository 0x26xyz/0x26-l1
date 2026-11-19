import type { MarketRiskTransition } from "../lib/chainClient";

interface TransitionCardProps {
  transition: MarketRiskTransition | null;
  loading: boolean;
}

function formatTimestamp(timestampMs: number | null | undefined) {
  if (timestampMs == null) {
    return "No transition yet";
  }
  return new Date(timestampMs).toLocaleString();
}

export function TransitionCard({ transition, loading }: TransitionCardProps) {
  return (
    <section className="card">
      <div className="section-title">Recent transition</div>
      <div className="transition-block">
        {transition ? (
          <>
            <strong>{`${transition.kind} via ${transition.source}`}</strong>
            <span>
              market {transition.market_id}, account {transition.account_id}
            </span>
            <small>{formatTimestamp(transition.timestamp_ms)}</small>
          </>
        ) : (
          <>
            <strong>
              {loading ? "Loading…" : "No market transition yet"}
            </strong>
            <span>
              {loading
                ? "Fetching pressure summary"
                : "Waiting for stressed-market activity"}
            </span>
            <small>{formatTimestamp(null)}</small>
          </>
        )}
      </div>
    </section>
  );
}
