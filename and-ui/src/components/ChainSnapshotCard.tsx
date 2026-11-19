import type { ChainInfoResponse } from "../lib/chainClient";

interface ChainSnapshotCardProps {
  chainInfo: ChainInfoResponse | null;
  cursor: number | null;
  loading: boolean;
}

function shortHash(hashHex: string | undefined): string {
  if (!hashHex) return "awaiting head";
  return hashHex.length > 18 ? hashHex.slice(0, 18) : hashHex;
}

function display(value: number | null | undefined, loading: boolean) {
  if (value == null) {
    return loading ? "…" : "—";
  }
  return value.toString();
}

export function ChainSnapshotCard({
  chainInfo,
  cursor,
  loading,
}: ChainSnapshotCardProps) {
  return (
    <section className="card metrics-card">
      <div className="section-title">Chain snapshot</div>
      <div className="metric-grid">
        <div className="metric-tile">
          <span>Chain ID</span>
          <strong>{display(chainInfo?.chain_id, loading)}</strong>
          <small>current network identity</small>
        </div>
        <div className="metric-tile">
          <span>Head height</span>
          <strong>{display(chainInfo?.head_height, loading)}</strong>
          <small>{shortHash(chainInfo?.head_hash_hex)}</small>
        </div>
        <div className="metric-tile">
          <span>Quarantined</span>
          <strong>
            {display(chainInfo?.quarantined_block_count, loading)}
          </strong>
          <small>blocks awaiting operator review</small>
        </div>
        <div className="metric-tile">
          <span>Event cursor</span>
          <strong>{display(cursor, loading)}</strong>
          <small>live runtime sequence</small>
        </div>
      </div>
    </section>
  );
}
