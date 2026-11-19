import { Activity, AlertTriangle, ShieldAlert, Waves } from "lucide-react";
import type { LiquidationPressureSummaryResponse } from "../lib/chainClient";

interface PressureSummaryCardProps {
  summary: LiquidationPressureSummaryResponse | null;
  loading: boolean;
}

function show(value: number | string | null | undefined, loading: boolean) {
  if (value == null) {
    return loading ? "…" : "—";
  }
  return value.toString();
}

export function PressureSummaryCard({
  summary,
  loading,
}: PressureSummaryCardProps) {
  return (
    <section className="card">
      <div className="section-title">Pressure summary</div>
      <div className="stack">
        <div className="mini-stat">
          <ShieldAlert size={16} />
          <div>
            <strong>{show(summary?.stressed_market_count, loading)}</strong>
            <span>stressed markets</span>
          </div>
        </div>
        <div className="mini-stat">
          <AlertTriangle size={16} />
          <div>
            <strong>
              {show(summary?.imminent_freeze_account_count, loading)}
            </strong>
            <span>imminent freezes</span>
          </div>
        </div>
        <div className="mini-stat">
          <Activity size={16} />
          <div>
            <strong>
              {show(summary?.total_liquidation_candidate_count, loading)}
            </strong>
            <span>liquidation candidates</span>
          </div>
        </div>
        <div className="mini-stat">
          <Waves size={16} />
          <div>
            <strong>{show(summary?.worst_market_id, loading)}</strong>
            <span>
              worst market · gap {show(summary?.worst_market_gap, loading)}
            </span>
          </div>
        </div>
      </div>
    </section>
  );
}
