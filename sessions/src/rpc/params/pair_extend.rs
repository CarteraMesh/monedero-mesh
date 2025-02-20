//! https://specs.walletconnect.com/2.0/specs/clients/core/pairing/rpc-methods

use {
    super::IrnMetadata,
    crate::rpc::{ErrorParams, IntoUnknownError, ResponseParamsError},
    serde::{Deserialize, Serialize},
};

pub(super) const IRN_REQUEST_METADATA: IrnMetadata = IrnMetadata {
    tag: crate::rpc::TAG_PAIR_EXTEND_REQUEST,
    ttl: 30,
    prompt: false,
};

pub(super) const IRN_RESPONSE_METADATA: IrnMetadata = IrnMetadata {
    tag: crate::rpc::TAG_PAIR_EXTEND_RESPONSE,
    ttl: 30,
    prompt: false,
};

#[derive(Debug, Serialize, PartialEq, Eq, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PairExtendRequest {
    // Epoch UTC
    pub expiry: u64,
}

impl IntoUnknownError for PairExtendRequest {
    fn unknown(&self) -> ResponseParamsError {
        ResponseParamsError::PairDelete(ErrorParams::unknown())
    }
}

#[cfg(test)]
mod tests {
    use {
        super::{super::tests::param_serde_test, *},
        anyhow::Result,
    };

    #[test]
    fn test_serde_pair_extend_request() -> Result<()> {
        let json = r#"{"expiry": 111233211}"#;

        param_serde_test::<PairExtendRequest>(json)
    }
}
