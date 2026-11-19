//! RFC 043 — Vaults Discovery Surface.
//!
//! Read-only projection over capital-pool primitives (RFC 026 today; RFC 028
//! and RFC 029 when those land). No new on-chain state, no new signed action;
//! `kombat_listVaults` and `kombat_getVault` re-use existing readers and wrap
//! them in a `VaultRecord` envelope.
//!
//! Implementation status: only `VaultKind::Delegation` is wired up. Other
//! variants are reserved in the enum so the wire shape is stable across
//! v1.1+ → v1.2+.

use alloy_primitives::keccak256;
use kombat_chain_node::{Delegation, DelegationId, DelegationStatus};
use serde_json::{json, Value};

/// USDC asset id used by vault records. v1 has no `AssetId` type yet (RFC 023
/// introduces multi-asset in v1.2+); the field exists in the wire shape so
/// SupplyBorrowPool can land later without breaking clients.
pub const VAULT_USDC_ASSET_ID: u64 = 0;

pub type VaultId = [u8; 16];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultKind {
    Delegation,
    SupplyBorrowPool,
    CoverPool,
}

impl VaultKind {
    pub fn discriminator(self) -> u8 {
        match self {
            VaultKind::Delegation => 0,
            VaultKind::SupplyBorrowPool => 1,
            VaultKind::CoverPool => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            VaultKind::Delegation => "delegation",
            VaultKind::SupplyBorrowPool => "supply_borrow_pool",
            VaultKind::CoverPool => "cover_pool",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "delegation" => Some(VaultKind::Delegation),
            "supply_borrow_pool" => Some(VaultKind::SupplyBorrowPool),
            "cover_pool" => Some(VaultKind::CoverPool),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultStatus {
    Open,
    ClosedToDeposits,
    Winding,
    Triggered,
    Paused,
    Closed,
}

impl VaultStatus {
    pub fn name(self) -> &'static str {
        match self {
            VaultStatus::Open => "open",
            VaultStatus::ClosedToDeposits => "closed_to_deposits",
            VaultStatus::Winding => "winding",
            VaultStatus::Triggered => "triggered",
            VaultStatus::Paused => "paused",
            VaultStatus::Closed => "closed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open" => Some(VaultStatus::Open),
            "closed_to_deposits" => Some(VaultStatus::ClosedToDeposits),
            "winding" => Some(VaultStatus::Winding),
            "triggered" => Some(VaultStatus::Triggered),
            "paused" => Some(VaultStatus::Paused),
            "closed" => Some(VaultStatus::Closed),
            _ => None,
        }
    }
}

/// Exhaustive `DelegationStatus` → `VaultStatus` mapping per RFC 043
/// §"Status normalisation · RFC 026". No default arm; adding a new
/// `DelegationStatus` variant is a build-time failure here.
pub fn delegation_status_to_vault_status(status: DelegationStatus) -> VaultStatus {
    match status {
        DelegationStatus::Open => VaultStatus::Open,
        DelegationStatus::ClosedToDeposits => VaultStatus::ClosedToDeposits,
        DelegationStatus::Winding => VaultStatus::Winding,
        DelegationStatus::Closed => VaultStatus::Closed,
        DelegationStatus::Terminated => VaultStatus::Closed,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeeSummary {
    pub perf_share_units: u16,
    pub headline_apy_bps: i32,
    pub notice_period_secs: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscoveryFlags {
    pub listed: bool,
    pub featured: bool,
}

impl DiscoveryFlags {
    /// Default flags when the indexer has no curation config (RFC 018 out of
    /// scope here). Surface every primitive as `listed`; the frontend can
    /// hide later via its own policy.
    pub const DEFAULT: Self = Self {
        listed: true,
        featured: false,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultDetails {
    Delegation(Delegation),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultRecord {
    pub vault_id: VaultId,
    pub kind: VaultKind,
    pub display_name: String,
    pub operator: Option<u64>,
    pub status: VaultStatus,
    pub capital_currency: u64,
    pub tvl_native: u128,
    pub tvl_micro_usdc: u128,
    pub shares_outstanding: Option<u128>,
    pub fee_summary: FeeSummary,
    pub discovery: DiscoveryFlags,
    pub real_capital: Option<RealCapitalInfo>,
    pub live_trading: Option<VaultLiveTradingInfo>,
    pub details: VaultDetails,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VaultLiveTradingInfo {
    pub trading_account_id: u64,
    pub market_count: usize,
    pub total_collateral_micro_usdc: i128,
    pub total_realized_pnl_micro_usdc: i128,
    pub total_unrealized_pnl_micro_usdc: i128,
    pub total_pnl_micro_usdc: i128,
    pub total_equity_micro_usdc: i128,
    pub total_open_notional_micro_usdc: u128,
    pub total_initial_margin_required_micro_usdc: u128,
    pub total_maintenance_requirement_micro_usdc: u128,
    pub total_margin_surplus_micro_usdc: i128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealCapitalInfo {
    pub mode: String,
    pub pilot_min_serving_agent_tier: Option<String>,
    pub tier0_pilot_exception: bool,
    pub deposit_enabled: bool,
    pub listed_for_discovery: bool,
    pub global_deposit_kill_switch: bool,
    pub per_principal_cap_micro_usdc: u128,
    pub total_vault_tvl_cap_micro_usdc: u128,
    pub protocol_cap_micro_usdc: Option<u128>,
    pub vault_deposited_micro_usdc: u128,
    pub listed_at_block: u64,
    pub last_updated_block: u64,
    pub last_updated_governance_action_hex: String,
}

/// Deterministic vault id per RFC 043 §"VaultRecord":
/// `vault_id = keccak256("vault" || kind_byte || borsh(underlying_id))[..16]`.
/// For `Delegation`, `underlying_id` is the 32-byte `DelegationId`.
pub fn vault_id_for_delegation(delegation_id: &DelegationId) -> VaultId {
    let mut buf = Vec::with_capacity(5 + 1 + 32);
    buf.extend_from_slice(b"vault");
    buf.push(VaultKind::Delegation.discriminator());
    buf.extend_from_slice(delegation_id);
    let hash = keccak256(&buf);
    let mut out = [0u8; 16];
    out.copy_from_slice(&hash.0[..16]);
    out
}

/// Display-name synthesis per RFC 043 §"`display_name` synthesis · Delegation".
fn delegation_display_name(delegation_id: &DelegationId) -> String {
    let mut hex = String::with_capacity(2 + 8);
    hex.push_str("0x");
    for byte in &delegation_id[..4] {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{:02x}", byte);
    }
    format!("Delegation {}", hex)
}

/// Project a `Delegation` row into a `VaultRecord`.
///
/// `tvl_native` / `tvl_micro_usdc` come from the caller (the service has the
/// engine handle to compute equity). `headline_apy_bps` is informational and
/// not consensus-normative; rolling-window NAV sampling is a follow-up, so
/// this function takes 0 today.
pub fn delegation_to_vault_record(
    delegation: Delegation,
    tvl_micro_usdc: u128,
    discovery: DiscoveryFlags,
    real_capital: Option<RealCapitalInfo>,
    live_trading: Option<VaultLiveTradingInfo>,
) -> VaultRecord {
    let vault_id = vault_id_for_delegation(&delegation.delegation_id);
    let status = delegation_status_to_vault_status(delegation.status);
    let fee_summary = FeeSummary {
        perf_share_units: delegation.performance_share_units,
        headline_apy_bps: 0, // TODO: rolling-window NAV; informational, non-consensus.
        notice_period_secs: delegation.notice_period_ms / 1_000,
    };
    let display_name = delegation_display_name(&delegation.delegation_id);
    let operator = Some(delegation.agent);
    let shares_outstanding = Some(delegation.total_shares);
    VaultRecord {
        vault_id,
        kind: VaultKind::Delegation,
        display_name,
        operator,
        status,
        capital_currency: VAULT_USDC_ASSET_ID,
        tvl_native: tvl_micro_usdc,
        tvl_micro_usdc,
        shares_outstanding,
        fee_summary,
        discovery,
        real_capital,
        live_trading,
        details: VaultDetails::Delegation(delegation),
    }
}

/// Default ordering: `tvl_micro_usdc` descending, `vault_id` ascending as
/// the deterministic tiebreaker. RFC 043 §"`kombat_listVaults`".
pub fn order_vault_records(records: &mut [VaultRecord]) {
    records.sort_by(|a, b| {
        b.tvl_micro_usdc
            .cmp(&a.tvl_micro_usdc)
            .then_with(|| a.vault_id.cmp(&b.vault_id))
    });
}

/// Opaque cursor: 16-byte big-endian `tvl_micro_usdc` || 16-byte `vault_id`.
/// Encoded as `0x`-prefixed hex. Pagination semantics: callers receive
/// `next_cursor` pointing at the last record they got; the next call resumes
/// strictly after that `(tvl, vault_id)` in sort order.
pub fn encode_vault_cursor(tvl_micro_usdc: u128, vault_id: VaultId) -> String {
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(&tvl_micro_usdc.to_be_bytes());
    bytes[16..].copy_from_slice(&vault_id);
    format!("0x{}", hex::encode(bytes))
}

pub fn decode_vault_cursor(value: &str) -> Option<(u128, VaultId)> {
    let trimmed = value.strip_prefix("0x").unwrap_or(value);
    let raw = hex::decode(trimmed).ok()?;
    if raw.len() != 32 {
        return None;
    }
    let mut tvl_be = [0u8; 16];
    tvl_be.copy_from_slice(&raw[..16]);
    let tvl = u128::from_be_bytes(tvl_be);
    let mut vault_id = [0u8; 16];
    vault_id.copy_from_slice(&raw[16..]);
    Some((tvl, vault_id))
}

/// `(tvl_a, id_a)` is strictly after `(tvl_b, id_b)` in the default sort
/// order (tvl desc, id asc) iff `tvl_a < tvl_b` OR equal-tvl AND `id_a > id_b`.
pub fn cursor_strictly_after(tvl_a: u128, id_a: &VaultId, tvl_b: u128, id_b: &VaultId) -> bool {
    tvl_a < tvl_b || (tvl_a == tvl_b && id_a > id_b)
}

fn fee_summary_to_json(summary: &FeeSummary) -> Value {
    json!({
        "perf_share_units": summary.perf_share_units,
        "headline_apy_bps": summary.headline_apy_bps,
        "notice_period_secs": summary.notice_period_secs,
    })
}

fn discovery_flags_to_json(flags: &DiscoveryFlags) -> Value {
    json!({
        "listed": flags.listed,
        "featured": flags.featured,
    })
}

fn real_capital_info_to_json(info: &RealCapitalInfo) -> Value {
    json!({
        "mode": info.mode,
        "pilot_min_serving_agent_tier": info.pilot_min_serving_agent_tier,
        "tier0_pilot_exception": info.tier0_pilot_exception,
        "deposit_enabled": info.deposit_enabled,
        "listed_for_discovery": info.listed_for_discovery,
        "global_deposit_kill_switch": info.global_deposit_kill_switch,
        "per_principal_cap_micro_usdc": info.per_principal_cap_micro_usdc.to_string(),
        "total_vault_tvl_cap_micro_usdc": info.total_vault_tvl_cap_micro_usdc.to_string(),
        "protocol_cap_micro_usdc": info.protocol_cap_micro_usdc.map(|value| value.to_string()),
        "vault_deposited_micro_usdc": info.vault_deposited_micro_usdc.to_string(),
        "listed_at_block": info.listed_at_block,
        "last_updated_block": info.last_updated_block,
        "last_updated_governance_action_hex": info.last_updated_governance_action_hex,
    })
}

fn delegation_to_details_json(delegation: &Delegation) -> Value {
    // Mirror the shape of `delegation_response_to_json` in lib.rs so a
    // `details.delegation.*` payload deep-equals the underlying primitive's
    // own read. Kept independent here to avoid a pub helper.
    json!({
        "delegation_id_hex": format!("0x{}", hex::encode(delegation.delegation_id)),
        "agent": delegation.agent,
        "mandate_id_hex": format!("0x{}", hex::encode(delegation.mandate_id)),
        "opened_at_ms": delegation.opened_at_ms,
        "last_updated_at_ms": delegation.last_updated_at_ms,
        "status": match delegation.status {
            DelegationStatus::Open => "open",
            DelegationStatus::ClosedToDeposits => "closed_to_deposits",
            DelegationStatus::Winding => "winding",
            DelegationStatus::Closed => "closed",
            DelegationStatus::Terminated => "terminated",
        },
        "performance_share_units": delegation.performance_share_units,
        "crystallization_cadence_ms": delegation.crystallization_cadence_ms,
        "notice_period_ms": delegation.notice_period_ms,
        "total_shares": delegation.total_shares.to_string(),
        "high_water_mark_per_share": delegation.high_water_mark_per_share.to_string(),
        "last_crystallized_at_ms": delegation.last_crystallized_at_ms,
        "agent_bond": delegation.agent_bond.to_string(),
    })
}

fn live_trading_info_to_json(info: &VaultLiveTradingInfo) -> Value {
    json!({
        // Delegation trading accounts sit in the reserved u64 range, above
        // JavaScript's safe integer limit, so expose the id as a decimal string.
        "trading_account_id": info.trading_account_id.to_string(),
        "market_count": info.market_count,
        "source": "delegation_trading_account_risk_summary",
        "total_collateral_micro_usdc": info.total_collateral_micro_usdc.to_string(),
        "total_realized_pnl_micro_usdc": info.total_realized_pnl_micro_usdc.to_string(),
        "total_unrealized_pnl_micro_usdc": info.total_unrealized_pnl_micro_usdc.to_string(),
        "total_pnl_micro_usdc": info.total_pnl_micro_usdc.to_string(),
        "total_equity_micro_usdc": info.total_equity_micro_usdc.to_string(),
        "total_open_notional_micro_usdc": info.total_open_notional_micro_usdc.to_string(),
        "total_initial_margin_required_micro_usdc": info.total_initial_margin_required_micro_usdc.to_string(),
        "total_maintenance_requirement_micro_usdc": info.total_maintenance_requirement_micro_usdc.to_string(),
        "total_margin_surplus_micro_usdc": info.total_margin_surplus_micro_usdc.to_string(),
    })
}

pub fn vault_record_to_json(record: &VaultRecord) -> Value {
    let details = match &record.details {
        VaultDetails::Delegation(delegation) => json!({
            "kind": "delegation",
            "delegation": delegation_to_details_json(delegation),
        }),
    };
    json!({
        "vault_id_hex": format!("0x{}", hex::encode(record.vault_id)),
        "kind": record.kind.name(),
        "display_name": record.display_name,
        "operator": record.operator,
        "status": record.status.name(),
        "capital_currency": record.capital_currency,
        "tvl_native": record.tvl_native.to_string(),
        "tvl_micro_usdc": record.tvl_micro_usdc.to_string(),
        "shares_outstanding": record.shares_outstanding.map(|v| v.to_string()),
        "fee_summary": fee_summary_to_json(&record.fee_summary),
        "discovery": discovery_flags_to_json(&record.discovery),
        "real_capital": record.real_capital.as_ref().map(real_capital_info_to_json),
        "live_trading": record.live_trading.as_ref().map(live_trading_info_to_json),
        "details": details,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_delegation(status: DelegationStatus) -> Delegation {
        Delegation {
            delegation_id: [7u8; 32],
            agent: 22,
            mandate_id: [3u8; 32],
            opened_at_ms: 1_000,
            last_updated_at_ms: 2_000,
            winding_started_at_ms: (status == DelegationStatus::Winding).then_some(1_500),
            status,
            performance_share_units: 1_500,
            crystallization_cadence_ms: 7 * 24 * 3_600 * 1_000,
            notice_period_ms: 24 * 3_600 * 1_000,
            total_shares: 1_000_000,
            high_water_mark_per_share: 1_000_000,
            last_crystallized_at_ms: 1_500,
            agent_bond: 5_000_000_000,
        }
    }

    #[test]
    fn vault_id_is_deterministic_and_kind_namespaced() {
        let id_a = vault_id_for_delegation(&[1u8; 32]);
        let id_b = vault_id_for_delegation(&[1u8; 32]);
        let id_c = vault_id_for_delegation(&[2u8; 32]);
        assert_eq!(id_a, id_b);
        assert_ne!(id_a, id_c);
        // The kind byte (0) is mixed in, so the hash is not just keccak("vault" || delegation_id).
        let mut bare = Vec::new();
        bare.extend_from_slice(b"vault");
        bare.extend_from_slice(&[1u8; 32]);
        let bare_hash = keccak256(&bare);
        let mut bare16 = [0u8; 16];
        bare16.copy_from_slice(&bare_hash.0[..16]);
        assert_ne!(id_a, bare16);
    }

    #[test]
    fn delegation_status_mapping_is_exhaustive() {
        // Every DelegationStatus variant must produce a VaultStatus.
        // Adding a variant without updating the mapper is a build error.
        for status in [
            DelegationStatus::Open,
            DelegationStatus::ClosedToDeposits,
            DelegationStatus::Winding,
            DelegationStatus::Closed,
            DelegationStatus::Terminated,
        ] {
            let _ = delegation_status_to_vault_status(status);
        }
        assert_eq!(
            delegation_status_to_vault_status(DelegationStatus::Terminated),
            VaultStatus::Closed
        );
        assert_eq!(
            delegation_status_to_vault_status(DelegationStatus::Winding),
            VaultStatus::Winding
        );
    }

    #[test]
    fn delegation_record_carries_expected_fields() {
        let delegation = sample_delegation(DelegationStatus::Open);
        let record = delegation_to_vault_record(
            delegation.clone(),
            12_345_000_000,
            DiscoveryFlags::DEFAULT,
            None,
            None,
        );
        assert_eq!(record.kind, VaultKind::Delegation);
        assert_eq!(record.status, VaultStatus::Open);
        assert_eq!(record.operator, Some(22));
        assert_eq!(record.capital_currency, VAULT_USDC_ASSET_ID);
        assert_eq!(record.tvl_native, 12_345_000_000);
        assert_eq!(record.tvl_micro_usdc, 12_345_000_000);
        assert_eq!(record.shares_outstanding, Some(1_000_000));
        assert_eq!(record.fee_summary.perf_share_units, 1_500);
        assert_eq!(record.fee_summary.notice_period_secs, 86_400);
        assert!(record.discovery.listed);
        assert!(matches!(record.details, VaultDetails::Delegation(d) if d == delegation));
    }

    #[test]
    fn ordering_is_tvl_desc_with_vault_id_tiebreaker() {
        let mut a = delegation_to_vault_record(
            sample_delegation(DelegationStatus::Open),
            100,
            DiscoveryFlags::DEFAULT,
            None,
            None,
        );
        let mut b = delegation_to_vault_record(
            {
                let mut d = sample_delegation(DelegationStatus::Open);
                d.delegation_id = [9u8; 32];
                d
            },
            200,
            DiscoveryFlags::DEFAULT,
            None,
            None,
        );
        let mut c = delegation_to_vault_record(
            {
                let mut d = sample_delegation(DelegationStatus::Open);
                d.delegation_id = [11u8; 32];
                d
            },
            200,
            DiscoveryFlags::DEFAULT,
            None,
            None,
        );
        a.discovery = DiscoveryFlags::DEFAULT;
        b.discovery = DiscoveryFlags::DEFAULT;
        c.discovery = DiscoveryFlags::DEFAULT;
        let mut records = vec![a.clone(), b.clone(), c.clone()];
        order_vault_records(&mut records);
        // tvl 200 records come first; tiebreaker is vault_id ascending.
        assert_eq!(records[0].tvl_micro_usdc, 200);
        assert_eq!(records[1].tvl_micro_usdc, 200);
        assert_eq!(records[2].tvl_micro_usdc, 100);
        assert!(records[0].vault_id < records[1].vault_id);
    }

    #[test]
    fn vault_cursor_round_trip_and_ordering_predicate() {
        let id_a = [1u8; 16];
        let id_b = [2u8; 16];
        let encoded = encode_vault_cursor(123_456, id_a);
        assert!(encoded.starts_with("0x"));
        assert_eq!(decode_vault_cursor(&encoded), Some((123_456, id_a)));
        assert_eq!(decode_vault_cursor("0xdeadbeef"), None);
        assert_eq!(decode_vault_cursor("not hex"), None);

        // (50, id_b) strictly after (100, id_a): tvl smaller wins.
        assert!(cursor_strictly_after(50, &id_b, 100, &id_a));
        // (100, id_b) strictly after (100, id_a): equal tvl, larger id wins.
        assert!(cursor_strictly_after(100, &id_b, 100, &id_a));
        // (100, id_a) NOT strictly after itself.
        assert!(!cursor_strictly_after(100, &id_a, 100, &id_a));
        // (200, _) NOT strictly after (100, _): tvl larger comes earlier.
        assert!(!cursor_strictly_after(200, &id_a, 100, &id_b));
    }

    #[test]
    fn json_round_trip_carries_required_fields() {
        let record = delegation_to_vault_record(
            sample_delegation(DelegationStatus::Winding),
            42,
            DiscoveryFlags {
                listed: true,
                featured: true,
            },
            Some(RealCapitalInfo {
                mode: "capped_pilot".to_string(),
                pilot_min_serving_agent_tier: Some("tier1_operator_attested".to_string()),
                tier0_pilot_exception: false,
                deposit_enabled: true,
                listed_for_discovery: true,
                global_deposit_kill_switch: false,
                per_principal_cap_micro_usdc: 100,
                total_vault_tvl_cap_micro_usdc: 200,
                protocol_cap_micro_usdc: Some(300),
                vault_deposited_micro_usdc: 40,
                listed_at_block: 7,
                last_updated_block: 8,
                last_updated_governance_action_hex: "0x00".to_string(),
            }),
            Some(VaultLiveTradingInfo {
                trading_account_id: 9_223_372_036_854_775_999,
                market_count: 2,
                total_collateral_micro_usdc: 42,
                total_realized_pnl_micro_usdc: 10,
                total_unrealized_pnl_micro_usdc: -3,
                total_pnl_micro_usdc: 7,
                total_equity_micro_usdc: 49,
                total_open_notional_micro_usdc: 1_000,
                total_initial_margin_required_micro_usdc: 100,
                total_maintenance_requirement_micro_usdc: 50,
                total_margin_surplus_micro_usdc: -1,
            }),
        );
        let value = vault_record_to_json(&record);
        assert_eq!(value["kind"], "delegation");
        assert_eq!(value["status"], "winding");
        assert_eq!(value["capital_currency"], 0);
        assert_eq!(value["tvl_native"], "42");
        assert_eq!(value["tvl_micro_usdc"], "42");
        assert_eq!(value["fee_summary"]["perf_share_units"], 1_500);
        assert_eq!(value["fee_summary"]["notice_period_secs"], 86_400);
        assert_eq!(value["discovery"]["featured"], true);
        assert_eq!(value["real_capital"]["mode"], "capped_pilot");
        assert_eq!(value["real_capital"]["protocol_cap_micro_usdc"], "300");
        assert_eq!(
            value["live_trading"]["trading_account_id"],
            "9223372036854775999"
        );
        assert_eq!(value["live_trading"]["market_count"], 2);
        assert_eq!(value["live_trading"]["total_pnl_micro_usdc"], "7");
        assert_eq!(
            value["live_trading"]["total_unrealized_pnl_micro_usdc"],
            "-3"
        );
        assert_eq!(
            value["live_trading"]["source"],
            "delegation_trading_account_risk_summary"
        );
        assert_eq!(value["details"]["kind"], "delegation");
        // delegation_id round-trips as 0x-prefixed hex.
        assert!(value["details"]["delegation"]["delegation_id_hex"]
            .as_str()
            .expect("hex string")
            .starts_with("0x"));
    }
}
