use kombat_chain_consensus::{mahi::MahiBlock, BlockItem};
use kombat_chain_node::execution::{summarize_block_item_schedule, ExecutionScheduleSummary};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MahiExecutionScheduleProfile {
    pub(crate) total_item_count: usize,
    pub(crate) truncated: bool,
    pub(crate) schedule: ExecutionScheduleSummary,
}

pub(crate) fn summarize_mahi_execution_schedule(
    blocks: &[MahiBlock],
    max_items: usize,
) -> MahiExecutionScheduleProfile {
    let max_items = max_items.max(1);
    let total_item_count = blocks
        .iter()
        .map(|block| {
            block
                .oracle_prices
                .len()
                .saturating_add(block.intents.len())
                .saturating_add(block.faucets.len())
                .saturating_add(block.real_capital_governance.len())
                .saturating_add(block.delegations.len())
                .saturating_add(block.market_maker_programs.len())
                .saturating_add(block.tee_governance.len())
        })
        .sum::<usize>();
    let items = blocks
        .iter()
        .flat_map(|block| {
            block
                .oracle_prices
                .iter()
                .cloned()
                .map(BlockItem::OraclePrice)
                .chain(block.intents.iter().cloned().map(BlockItem::Intent))
                .chain(block.faucets.iter().cloned().map(BlockItem::Faucet))
                .chain(
                    block
                        .real_capital_governance
                        .iter()
                        .cloned()
                        .map(BlockItem::RealCapitalGovernance),
                )
                .chain(block.delegations.iter().cloned().map(BlockItem::Delegation))
                .chain(
                    block
                        .market_maker_programs
                        .iter()
                        .cloned()
                        .map(BlockItem::MarketMakerProgram),
                )
                .chain(
                    block
                        .tee_governance
                        .iter()
                        .cloned()
                        .map(BlockItem::TeeGovernance),
                )
        })
        .take(max_items)
        .collect::<Vec<_>>();
    MahiExecutionScheduleProfile {
        total_item_count,
        truncated: total_item_count > items.len(),
        schedule: summarize_block_item_schedule(&items),
    }
}
