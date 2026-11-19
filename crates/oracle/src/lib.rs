pub const CRATE_NAME: &str = "oracle";
const DEFAULT_MAX_STALENESS_MS: u64 = 5_000;
const RATE_SCALE_PPM: i128 = 1_000_000;
const FUNDING_PREMIUM_CLAMP_PPM: i128 = 500;
const FUNDING_RATE_CLAMP_PPM: i128 = 7_500;
const DEFAULT_INTEREST_RATE_PPM_8H: i128 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct OraclePrice {
    pub price_ticks: u64,
    pub timestamp_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct OracleConfig {
    pub max_staleness_ms: u64,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            max_staleness_ms: DEFAULT_MAX_STALENESS_MS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PriceFeedStatus {
    pub price_ticks: u64,
    pub timestamp_ms: u64,
    pub stale: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct OracleSample {
    pub price_ticks: u64,
    pub sources_used: u8,
    pub confidence: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OracleAggregationConfig {
    pub min_sources: u8,
    pub min_confidence: u8,
    pub min_samples: usize,
}

impl Default for OracleAggregationConfig {
    fn default() -> Self {
        Self {
            min_sources: 1,
            min_confidence: 1,
            min_samples: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OracleAggregation {
    pub price_ticks: u64,
    pub accepted_samples: usize,
    pub rejected_samples: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OracleAggregationError {
    InvalidConfig,
    InsufficientSamples {
        accepted_samples: usize,
        required_samples: usize,
    },
}

pub fn component_name() -> &'static str {
    CRATE_NAME
}

pub fn aggregate_median_price<I>(
    samples: I,
    config: OracleAggregationConfig,
) -> Result<OracleAggregation, OracleAggregationError>
where
    I: IntoIterator<Item = OracleSample>,
{
    if config.min_sources == 0 || config.min_confidence == 0 || config.min_samples == 0 {
        return Err(OracleAggregationError::InvalidConfig);
    }

    let mut accepted = Vec::new();
    let mut rejected_samples = 0;
    for sample in samples {
        if sample.price_ticks == 0
            || sample.sources_used < config.min_sources
            || sample.confidence < config.min_confidence
        {
            rejected_samples += 1;
            continue;
        }
        accepted.push(sample.price_ticks);
    }
    if accepted.len() < config.min_samples {
        return Err(OracleAggregationError::InsufficientSamples {
            accepted_samples: accepted.len(),
            required_samples: config.min_samples,
        });
    }

    accepted.sort_unstable();
    let midpoint = accepted.len() / 2;
    let price_ticks = if accepted.len() % 2 == 1 {
        accepted[midpoint]
    } else {
        ((u128::from(accepted[midpoint - 1]) + u128::from(accepted[midpoint])) / 2) as u64
    };

    Ok(OracleAggregation {
        price_ticks,
        accepted_samples: accepted.len(),
        rejected_samples,
    })
}

pub fn price_is_stale(
    now_ms: u64,
    price: Option<OraclePrice>,
    config: OracleConfig,
) -> Option<bool> {
    price.map(|price| now_ms.saturating_sub(price.timestamp_ms) > config.max_staleness_ms)
}

pub fn price_status(
    now_ms: u64,
    price: Option<OraclePrice>,
    config: OracleConfig,
) -> Option<PriceFeedStatus> {
    price.map(|price| PriceFeedStatus {
        price_ticks: price.price_ticks,
        timestamp_ms: price.timestamp_ms,
        stale: now_ms.saturating_sub(price.timestamp_ms) > config.max_staleness_ms,
    })
}

pub fn premium_rate_ppm(mark_price_ticks: u64, index_price_ticks: u64) -> Option<i64> {
    if index_price_ticks == 0 {
        return None;
    }

    let premium = (i128::from(mark_price_ticks) - i128::from(index_price_ticks))
        .saturating_mul(RATE_SCALE_PPM)
        / i128::from(index_price_ticks);
    Some(i128_to_i64_saturating(premium))
}

pub fn indicative_funding_rate_ppm(mark_price_ticks: u64, index_price_ticks: u64) -> Option<i64> {
    let premium = i128::from(premium_rate_ppm(mark_price_ticks, index_price_ticks)?);
    let interest_minus_premium = DEFAULT_INTEREST_RATE_PPM_8H.saturating_sub(premium);
    let bounded_adjustment =
        interest_minus_premium.clamp(-FUNDING_PREMIUM_CLAMP_PPM, FUNDING_PREMIUM_CLAMP_PPM);
    let rate = premium
        .saturating_add(bounded_adjustment)
        .clamp(-FUNDING_RATE_CLAMP_PPM, FUNDING_RATE_CLAMP_PPM);
    Some(i128_to_i64_saturating(rate))
}

pub fn funding_index_delta(index_price_ticks: u64, funding_rate_ppm: i64) -> i128 {
    i128::from(index_price_ticks).saturating_mul(i128::from(funding_rate_ppm)) / RATE_SCALE_PPM
}

fn i128_to_i64_saturating(value: i128) -> i64 {
    value.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

#[cfg(test)]
mod tests {
    use super::{
        aggregate_median_price, component_name, funding_index_delta, indicative_funding_rate_ppm,
        premium_rate_ppm, price_is_stale, price_status, OracleAggregation, OracleAggregationConfig,
        OracleAggregationError, OracleConfig, OraclePrice, OracleSample, PriceFeedStatus,
    };

    #[test]
    fn reports_component_name() {
        assert_eq!(component_name(), "oracle");
    }

    #[test]
    fn derives_staleness_from_chain_time() {
        let price = OraclePrice {
            price_ticks: 101,
            timestamp_ms: 1_000,
        };
        let config = OracleConfig {
            max_staleness_ms: 500,
        };

        assert_eq!(price_is_stale(1_499, Some(price), config), Some(false));
        assert_eq!(price_is_stale(1_501, Some(price), config), Some(true));
    }

    #[test]
    fn returns_structured_price_status() {
        let price = OraclePrice {
            price_ticks: 101,
            timestamp_ms: 1_000,
        };
        let config = OracleConfig {
            max_staleness_ms: 500,
        };

        assert_eq!(
            price_status(1_600, Some(price), config),
            Some(PriceFeedStatus {
                price_ticks: 101,
                timestamp_ms: 1_000,
                stale: true,
            })
        );
    }

    #[test]
    fn computes_premium_and_indicative_funding_rates() {
        assert_eq!(premium_rate_ppm(10_100, 10_000), Some(10_000));
        assert_eq!(indicative_funding_rate_ppm(10_100, 10_000), Some(7_500));
        assert_eq!(premium_rate_ppm(9_900, 10_000), Some(-10_000));
        assert_eq!(indicative_funding_rate_ppm(9_900, 10_000), Some(-7_500));
        assert_eq!(funding_index_delta(10_000, 7_500), 75);
    }

    #[test]
    fn aggregates_deterministic_filtered_median() {
        let config = OracleAggregationConfig {
            min_sources: 2,
            min_confidence: 50,
            min_samples: 3,
        };
        let samples = [
            OracleSample {
                price_ticks: 105,
                sources_used: 2,
                confidence: 80,
            },
            OracleSample {
                price_ticks: 100,
                sources_used: 3,
                confidence: 90,
            },
            OracleSample {
                price_ticks: 300,
                sources_used: 1,
                confidence: 90,
            },
            OracleSample {
                price_ticks: 110,
                sources_used: 2,
                confidence: 60,
            },
        ];

        assert_eq!(
            aggregate_median_price(samples, config),
            Ok(OracleAggregation {
                price_ticks: 105,
                accepted_samples: 3,
                rejected_samples: 1,
            })
        );
    }

    #[test]
    fn rejects_when_aggregation_lacks_quorum() {
        let config = OracleAggregationConfig {
            min_sources: 2,
            min_confidence: 50,
            min_samples: 2,
        };
        let samples = [OracleSample {
            price_ticks: 100,
            sources_used: 1,
            confidence: 90,
        }];

        assert_eq!(
            aggregate_median_price(samples, config),
            Err(OracleAggregationError::InsufficientSamples {
                accepted_samples: 0,
                required_samples: 2,
            })
        );
    }
}
