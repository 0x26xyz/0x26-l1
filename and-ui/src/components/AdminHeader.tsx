import { useEffect, useState } from "react";
import { Blocks, RefreshCw } from "lucide-react";

interface AdminHeaderProps {
  rpcUrl: string;
  onCommitEndpoint: (next: string) => void;
  onRefresh: () => void;
  refreshing: boolean;
}

export function AdminHeader({
  rpcUrl,
  onCommitEndpoint,
  onRefresh,
  refreshing,
}: AdminHeaderProps) {
  const [draft, setDraft] = useState(rpcUrl);

  useEffect(() => {
    setDraft(rpcUrl);
  }, [rpcUrl]);

  const isDirty = draft.trim() !== rpcUrl;

  return (
    <header className="hero">
      <div>
        <div className="eyebrow">
          <Blocks size={14} />
          &amp; Independent Admin
        </div>
        <h1>Control plane for the chain, not for Kombat.</h1>
        <p>
          Standalone deployable UI for head state, liquidation pressure, and
          live runtime signals.
        </p>
      </div>

      <form
        className="endpoint-panel"
        onSubmit={(event) => {
          event.preventDefault();
          onCommitEndpoint(draft);
        }}
      >
        <label htmlFor="rpc-url">RPC endpoint</label>
        <div className="endpoint-row">
          <input
            id="rpc-url"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            spellCheck={false}
            autoComplete="off"
          />
          <button type="submit" disabled={!draft.trim() || !isDirty}>
            Connect
          </button>
        </div>
        <button
          type="button"
          className="ghost-button"
          onClick={onRefresh}
          disabled={refreshing}
        >
          <RefreshCw
            size={14}
            className={refreshing ? "spin" : undefined}
          />
          {refreshing ? "Refreshing…" : "Refresh now"}
        </button>
      </form>
    </header>
  );
}
