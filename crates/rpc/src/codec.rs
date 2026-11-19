use borsh::{BorshDeserialize, BorshSerialize};
use revm::primitives::U256;
use serde::{de, Deserializer, Serializer};

pub(crate) fn decode_hex_bytes(input: &str) -> Result<Vec<u8>, String> {
    let normalized = input.strip_prefix("0x").unwrap_or(input);
    hex::decode(normalized).map_err(|_| "invalid hex".into())
}

pub(crate) fn decode_hex_array<const N: usize>(input: &str) -> Result<[u8; N], String> {
    let bytes = decode_hex_bytes(input)?;
    bytes.try_into().map_err(|_| format!("expected {N} bytes"))
}

pub(crate) fn parse_hex_quantity_u64(input: &str) -> Result<u64, String> {
    let normalized = input.strip_prefix("0x").unwrap_or(input);
    if normalized.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(normalized, 16).map_err(|_| "invalid hex quantity".into())
}

pub(crate) fn encode_hex_quantity_u64(value: u64) -> String {
    format!("0x{value:x}")
}

pub(crate) fn encode_hex_quantity_u256(value: U256) -> String {
    format!("0x{value:x}")
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

pub(crate) fn encode_borsh_hex<T: BorshSerialize>(value: &T) -> String {
    encode_hex(&borsh::to_vec(value).expect("borsh serialization should succeed"))
}

pub(crate) fn decode_borsh_hex<T: BorshDeserialize>(input: &str) -> Result<T, String> {
    let bytes = decode_hex_bytes(input)?;
    T::try_from_slice(&bytes).map_err(|_| "invalid borsh payload".into())
}

pub(crate) fn serialize_u128_as_string<S>(value: &u128, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

pub(crate) fn deserialize_u128_from_string_or_number<'de, D>(
    deserializer: D,
) -> Result<u128, D::Error>
where
    D: Deserializer<'de>,
{
    struct U128Visitor;

    impl<'de> de::Visitor<'de> for U128Visitor {
        type Value = u128;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a u128 encoded as a decimal string or non-negative integer")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(u128::from(value))
        }

        fn visit_u128<E>(self, value: u128) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value
                .parse::<u128>()
                .map_err(|_| E::custom("invalid decimal u128 string"))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_any(U128Visitor)
}
