import type { MarketStateResponse } from "../lib/chainClient";

interface StressedMarketsCardProps {
  markets: MarketStateResponse[] | undefined;
  loading: boolean;
}

export function StressedMarketsCard({
  markets,
  loading,
}: StressedMarketsCardProps) {
  const rows = markets?.slice(0, 5) ?? [];
  return (
    <section className="card">
      <div className="section-title">Top stressed markets</div>
      <div className="list">
        {rows.length ? (
          rows.map((market) => (
            <div key={market.market_id} className="list-row">
              <div>
                <strong>Market {market.market_id}</strong>
                <span>
                  {market.liquidation_candidate_count} candidates ·{" "}
                  {market.imminent_freeze_count} imminent freezes
                </span>
              </div>
              <strong>{market.total_liquidation_gap}</strong>
            </div>
          ))
        ) : (
          <div className="empty-state">
            {loading
              ? "Loading stressed markets…"
              : "No stressed markets in the summary."}
          </div>
        )}
      </div>
    </section>
  );
}
