export interface ChainInfoResponse {
  chain_id: number;
  head_height: number;
  head_hash_hex: string;
  committed_state_root_hex: string;
  quarantined_block_count: number;
}

export interface AccountRiskSummaryResponse {
  account_id: number;
  market_count: number;
  frozen: boolean;
  max_liquidation_stall_count: number;
  imminent_freeze_market_count: number;
  worst_market_id: number | null;
  worst_liquidation_gap: string;
  total_liquidation_gap: string;
  under_maintenance_market_count: number;
}

export interface MarketRiskTransition {
  market_id: number;
  account_id: number;
  kind: string;
  source: string;
  seq: number;
  block_height: number | null;
  timestamp_ms: number;
}

export interface MarketStateResponse {
  market_id: number;
  liquidation_candidate_count: number;
  imminent_freeze_count: number;
  total_liquidation_gap: string;
}

export interface LiquidationPressureSummaryResponse {
  market_count: number;
  stressed_market_count: number;
  stressed_account_count: number;
  imminent_freeze_market_count: number;
  imminent_freeze_account_count: number;
  total_liquidation_candidate_count: number;
  total_liquidation_gap: string;
  worst_market_id: number | null;
  worst_market_gap: string;
  worst_account_id: number | null;
  worst_account_gap: string;
  suggested_clear_count: number;
  suggested_still_under_count: number;
  suggested_freeze_count: number;
  suggested_bad_debt_count: number;
  total_suggested_bad_debt: string;
  latest_market_risk_transition: MarketRiskTransition | null;
  recent_market_risk_transitions: MarketRiskTransition[];
  top_markets: MarketStateResponse[];
  top_accounts: AccountRiskSummaryResponse[];
}

export interface RuntimeEventRecord {
  seq: number;
  block_height: number | null;
  timestamp_ms: number;
  kind: Record<string, unknown>;
}

interface JsonRpcSuccess<T> {
  jsonrpc: "2.0";
  id: number;
  result: T;
}

interface JsonRpcFailure {
  jsonrpc: "2.0";
  id: number;
  error: {
    code: number;
    message: string;
    data?: unknown;
  };
}

type JsonRpcResponse<T> = JsonRpcSuccess<T> | JsonRpcFailure;

export class ChainRpcError extends Error {
  code: number;
  data?: unknown;

  constructor(code: number, message: string, data?: unknown) {
    super(message);
    this.name = "ChainRpcError";
    this.code = code;
    this.data = data;
  }
}

export class ChainClient {
  constructor(private readonly baseUrl: string) {}

  async chainInfo() {
    return this.call<ChainInfoResponse>("kombat_chainInfo");
  }

  async liquidationPressureSummary() {
    return this.call<LiquidationPressureSummaryResponse>(
      "kombat_getLiquidationPressureSummary"
    );
  }

  openEventStream(
    handlers: {
      onEvent?: (event: RuntimeEventRecord) => void;
      onCursor?: (nextSeq: number) => void;
      onError?: () => void;
    },
    query: { limit?: number; follow?: boolean } = {}
  ) {
    const trimmed = this.baseUrl.replace(/\/$/, "");
    const url = new URL(
      `${trimmed}/events`,
      typeof window !== "undefined" ? window.location.href : undefined
    );
    if (query.limit != null) {
      url.searchParams.set("limit", String(query.limit));
    }
    url.searchParams.set("follow", String(query.follow ?? true));

    const source = new EventSource(url.toString());

    source.addEventListener("cursor", (event) => {
      if (!(event instanceof MessageEvent)) {
        return;
      }
      try {
        const payload = JSON.parse(event.data) as { next_seq?: number };
        if (typeof payload.next_seq === "number") {
          handlers.onCursor?.(payload.next_seq);
        }
      } catch {
        handlers.onError?.();
      }
    });

    const topics = [
      "account_created",
      "policy_created",
      "policy_revoked",
      "intent_accepted",
      "intent_promoted",
      "intent_executed",
      "intent_rejected",
      "owner_call_executed",
      "market_updated",
      "order_accepted",
      "order_filled",
      "order_rejected",
      "liquidation_executed",
      "liquidation_outcome",
      "liquidation_escalation_updated",
      "bad_debt_created",
      "bad_debt_recovered",
      "insurance_fund_updated",
      "account_frozen",
      "account_unfrozen",
      "snapshot_saved",
      "snapshot_loaded",
    ];

    for (const topic of topics) {
      source.addEventListener(topic, (event) => {
        if (!(event instanceof MessageEvent)) {
          return;
        }
        try {
          handlers.onEvent?.(JSON.parse(event.data) as RuntimeEventRecord);
        } catch {
          handlers.onError?.();
        }
      });
    }

    source.addEventListener("error", () => {
      handlers.onError?.();
    });

    return source;
  }

  private async call<T>(method: string, params?: unknown): Promise<T> {
    const response = await fetch(this.baseUrl, {
      method: "POST",
      headers: {
        "content-type": "application/json",
      },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method,
        params: params ?? {},
      }),
    });

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }

    const payload = (await response.json()) as JsonRpcResponse<T>;

    if ("error" in payload) {
      throw new ChainRpcError(
        payload.error.code,
        payload.error.message,
        payload.error.data
      );
    }

    return payload.result;
  }
}
