import type { AccountRiskSummaryResponse } from "../lib/chainClient";

interface StressedAccountsCardProps {
  accounts: AccountRiskSummaryResponse[] | undefined;
  loading: boolean;
}

export function StressedAccountsCard({
  accounts,
  loading,
}: StressedAccountsCardProps) {
  const rows = accounts?.slice(0, 5) ?? [];
  return (
    <section className="card">
      <div className="section-title">Top stressed accounts</div>
      <div className="list">
        {rows.length ? (
          rows.map((account) => (
            <div key={account.account_id} className="list-row">
              <div>
                <strong>Account {account.account_id}</strong>
                <span>
                  {account.under_maintenance_market_count} stressed markets ·{" "}
                  {account.imminent_freeze_market_count} imminent freezes
                </span>
              </div>
              <strong>{account.total_liquidation_gap}</strong>
            </div>
          ))
        ) : (
          <div className="empty-state">
            {loading
              ? "Loading stressed accounts…"
              : "No stressed accounts in the summary."}
          </div>
        )}
      </div>
    </section>
  );
}
